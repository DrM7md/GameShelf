//! Free games from two sources:
//!
//! - **My list**: GameShelf's own `catalog/free-games.json`, read from the GameShelf
//!   repository on GitHub (a copy is built in, for when it can't be reached). Each game
//!   downloads straight from its maker's GitHub release.
//! - **[Homebrew Hub](https://hh.gbdev.io)**: hundreds of homebrew games for Game Boy,
//!   Game Boy Color, Game Boy Advance and NES, from its public API (`/api/search`, ten
//!   games a page; files and pictures at `static/<basepath>/entries/<slug>/<file>`).

use crate::platform::Platform;
use eframe::egui;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

const API: &str = "https://hh3.gbdev.io/api/search";
const STATIC: &str = "https://hh3.gbdev.io/static";
/// Where GameShelf reads "My list" from by default.
pub const DEFAULT_LIST_URL: &str =
    "https://raw.githubusercontent.com/DrM7md/GameShelf/main/catalog/free-games.json";
/// The list as it was when this GameShelf was built.
const BUILT_IN_LIST: &str = include_str!("../catalog/free-games.json");
/// Homebrew is small; anything bigger than this isn't what we expect.
const MAX_GAME_BYTES: u64 = 512 * 1024 * 1024;
/// Stops a runaway listing.
const MAX_PAGES: u32 = 200;

/// The systems Homebrew Hub has.
pub const HUB_SYSTEMS: [Platform; 4] = [Platform::Gb, Platform::Gbc, Platform::Gba, Platform::Nes];

fn api_code(p: Platform) -> Option<&'static str> {
    Some(match p {
        Platform::Gb => "GB",
        Platform::Gbc => "GBC",
        Platform::Gba => "GBA",
        Platform::Nes => "NES",
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    MyList,
    HomebrewHub,
}

#[derive(Clone)]
pub struct FreeGame {
    /// Unique within its source.
    pub id: String,
    pub source: Source,
    pub title: String,
    pub developer: Option<String>,
    pub description: Option<String>,
    pub platform: Platform,
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub website: Option<String>,
    pub cover_url: Option<String>,
    /// Download links; the first is the game itself (e.g. the `.cue` of a `.cue`/`.bin`).
    pub downloads: Vec<String>,
}

/// The file name at the end of a link (`…/v1.0/My%20Game.gba` → `My Game.gba`).
fn url_file_name(url: &str) -> Option<String> {
    let last = url.split(['?', '#']).next()?.rsplit('/').next()?;
    let name = percent_decode(last);
    let ok = !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\', ':']);
    ok.then_some(name)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b, hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn percent_encode_path(s: &str) -> String {
    s.split('/')
        .map(|part| {
            part.bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                        char::from(b).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

impl FreeGame {
    /// A key that's unique across sources.
    pub fn key(&self) -> String {
        match self.source {
            Source::MyList => format!("list-{}", self.id),
            Source::HomebrewHub => format!("hub-{}", self.id),
        }
    }

    /// Name the game file gets on disk.
    pub fn file_name(&self) -> Option<String> {
        url_file_name(self.downloads.first()?)
    }

    /// Several files, or a zip, go into a folder named after the game.
    fn needs_folder(&self) -> bool {
        self.downloads.len() > 1
            || self
                .file_name()
                .is_some_and(|n| n.to_ascii_lowercase().ends_with(".zip"))
    }

    /// Searchable text: title, developer, tags and description.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        let has = |s: &str| s.to_lowercase().contains(&q);
        q.is_empty()
            || has(&self.title)
            || self.developer.as_deref().is_some_and(has)
            || self.description.as_deref().is_some_and(has)
            || self.tags.iter().any(|t| has(t))
    }
}

/// A folder name without characters Windows doesn't allow.
fn safe_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() {
        "Game".into()
    } else {
        trimmed
    }
}

/// Where a downloaded game ends up inside its system folder.
pub fn destination(game: &FreeGame, system_dir: &Path) -> Option<PathBuf> {
    if game.needs_folder() {
        Some(system_dir.join(safe_name(&game.title)))
    } else {
        game.file_name().map(|n| system_dir.join(n))
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .user_agent("GameShelf")
        .build()
}

fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut text = String::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    Ok(text)
}

// --------------------------------------------------------------------------
// My list
// --------------------------------------------------------------------------

/// Reads a `free-games.json`. Entries that don't make sense are skipped.
pub fn parse_list(text: &str) -> Result<Vec<FreeGame>, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("The list isn't valid JSON: {e}"))?;
    let games = json
        .get("games")
        .and_then(|g| g.as_array())
        .ok_or("The list has no \"games\"")?;
    Ok(games
        .iter()
        .filter_map(|e| {
            let s = |key: &str| {
                e.get(key)
                    .and_then(|v| v.as_str())
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            let platform: Platform = serde_json::from_value(e.get("system")?.clone()).ok()?;
            let downloads: Vec<String> = e
                .get("files")?
                .as_array()?
                .iter()
                .filter_map(|f| f.as_str().map(str::to_string))
                .filter(|u| u.starts_with("https://"))
                .collect();
            let game = FreeGame {
                id: s("id")?,
                source: Source::MyList,
                title: s("title")?,
                developer: s("developer"),
                description: s("description"),
                platform,
                tags: e
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                license: s("license"),
                website: s("website"),
                cover_url: s("cover").filter(|u| u.starts_with("https://")),
                downloads,
            };
            game.file_name().is_some().then_some(game)
        })
        .collect())
}

pub enum ListMsg {
    /// The list (the built-in copy, with a note, when the online one couldn't be read).
    Loaded {
        games: Vec<FreeGame>,
        note: Option<String>,
    },
}

/// Reads "My list" from `url` in the background; falls back to the built-in copy.
pub fn load_list(url: String, tx: Sender<ListMsg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let online = get_text(&agent(), &url).and_then(|t| parse_list(&t));
        let msg = match online {
            Ok(games) => ListMsg::Loaded { games, note: None },
            Err(e) => ListMsg::Loaded {
                games: parse_list(BUILT_IN_LIST).unwrap_or_default(),
                note: Some(format!(
                    "Couldn't read the online list ({e}); showing the built-in copy."
                )),
            },
        };
        let _ = tx.send(msg);
        ctx.request_repaint();
    });
}

// --------------------------------------------------------------------------
// Homebrew Hub
// --------------------------------------------------------------------------

fn parse_hub_entry(e: &serde_json::Value, platform: Platform) -> Option<FreeGame> {
    let s = |key: &str| {
        e.get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|v| !v.is_empty())
    };
    let slug = s("slug")?;
    let basepath = s("basepath")?;
    let entry_url = |file: &str| {
        format!(
            "{STATIC}/{}/entries/{}/{}",
            basepath,
            percent_encode_path(&slug),
            percent_encode_path(file)
        )
    };
    let files = e.get("files")?.as_array()?;
    let pick = |pred: &dyn Fn(&serde_json::Value) -> bool| {
        files
            .iter()
            .find(|f| pred(f))
            .and_then(|f| f.get("filename")?.as_str().map(str::to_string))
    };
    // The file marked default, else the first playable one, else the first.
    let main = pick(&|f| f.get("default").and_then(|v| v.as_bool()) == Some(true))
        .or_else(|| pick(&|f| f.get("playable").and_then(|v| v.as_bool()) == Some(true)))
        .or_else(|| pick(&|_| true))?;
    let screenshots: Vec<String> = e
        .get("screenshots")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // Prefer a file called cover.*, else the first screenshot.
    let cover = screenshots
        .iter()
        .find(|s| s.to_lowercase().starts_with("cover."))
        .or_else(|| screenshots.first())
        .map(|c| entry_url(c));
    let game = FreeGame {
        id: slug.clone(),
        source: Source::HomebrewHub,
        title: s("title")?,
        developer: s("developer"),
        description: None,
        platform,
        tags: e
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        license: s("license"),
        website: s("website"),
        cover_url: cover,
        downloads: vec![entry_url(&main)],
    };
    game.file_name().is_some().then_some(game)
}

pub enum HubMsg {
    /// One more page of games, and how many there are in all.
    Page {
        platform: Platform,
        games: Vec<FreeGame>,
        total: usize,
    },
    Done {
        platform: Platform,
    },
    Failed {
        platform: Platform,
        error: String,
    },
}

/// Loads every Homebrew Hub game of `platform`, a page at a time, in the background.
pub fn load_hub(platform: Platform, tx: Sender<HubMsg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let send = |m: HubMsg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        let Some(code) = api_code(platform) else {
            send(HubMsg::Done { platform });
            return;
        };
        let agent = agent();
        let mut page = 1;
        loop {
            let url = format!("{API}?platform={code}&typetag=game&page={page}");
            let json = get_text(&agent, &url).and_then(|t| {
                serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())
            });
            let json = match json {
                Ok(j) => j,
                Err(error) => {
                    send(HubMsg::Failed { platform, error });
                    return;
                }
            };
            let total = json.get("results").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let pages = json.get("page_total").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let games = json
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|e| parse_hub_entry(e, platform))
                        .collect()
                })
                .unwrap_or_default();
            send(HubMsg::Page {
                platform,
                games,
                total,
            });
            if page >= pages || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }
        send(HubMsg::Done { platform });
    });
}

// --------------------------------------------------------------------------
// Getting a game
// --------------------------------------------------------------------------

pub struct GetResult {
    pub key: String,
    pub title: String,
    /// The game file, or its folder, in the library.
    pub result: Result<PathBuf, String>,
}

/// Downloads a game into its system folder (unzipping if needed), in the background.
pub fn get(game: FreeGame, system_dir: PathBuf, tx: Sender<GetResult>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = download_game(&game, &system_dir);
        let _ = tx.send(GetResult {
            key: game.key(),
            title: game.title.clone(),
            result,
        });
        ctx.request_repaint();
    });
}

fn download_to(agent: &ureq::Agent, url: &str, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        return Err(format!("{} is already there", dest.display()));
    }
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let part = dest.with_file_name(format!(
        "{}.part",
        dest.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    let mut out = fs::File::create(&part).map_err(|e| e.to_string())?;
    let copied = std::io::copy(&mut resp.into_reader().take(MAX_GAME_BYTES + 1), &mut out)
        .map_err(|e| format!("Download interrupted: {e}"))?;
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if copied > MAX_GAME_BYTES {
        let _ = fs::remove_file(&part);
        return Err("The download is unexpectedly large; stopped.".into());
    }
    fs::rename(&part, dest).map_err(|e| e.to_string())
}

fn download_game(game: &FreeGame, system_dir: &Path) -> Result<PathBuf, String> {
    let dest = destination(game, system_dir).ok_or("This game's file name isn't usable")?;
    if dest.exists() {
        return Err(format!("{} is already there", dest.display()));
    }
    let agent = agent();
    if !game.needs_folder() {
        fs::create_dir_all(system_dir).map_err(|e| e.to_string())?;
        let url = game.downloads.first().ok_or("This game has no file")?;
        download_to(&agent, url, &dest)?;
        return Ok(dest);
    }

    // Several files (e.g. .cue + .bin), or a zip: everything goes in one folder.
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let result = (|| {
        for url in &game.downloads {
            let name = url_file_name(url).ok_or("A file name isn't usable")?;
            let file = dest.join(&name);
            download_to(&agent, url, &file)?;
            if name.to_ascii_lowercase().ends_with(".zip") {
                unzip(&file, &dest)?;
                let _ = fs::remove_file(&file);
            }
        }
        Ok::<(), String>(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&dest);
        return Err(e);
    }
    Ok(dest)
}

fn unzip(archive: &Path, into: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new("tar");
    cmd.arg("-xf").arg(archive).arg("-C").arg(into);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not unpack: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub_entry() -> serde_json::Value {
        serde_json::json!({
            "slug": "dev_cool-game",
            "title": "Cool Game",
            "developer": "Dev",
            "platform": "GBA",
            "typetag": "game",
            "tags": ["platformer"],
            "license": "MIT",
            "basepath": "database-gba",
            "screenshots": ["screenshots/a.png", "cover.png"],
            "files": [
                {"filename": "files/manual.pdf", "playable": false},
                {"filename": "files/Cool Game.gba", "default": true, "playable": true}
            ]
        })
    }

    #[test]
    fn reads_a_hub_entry() {
        let g = parse_hub_entry(&hub_entry(), Platform::Gba).unwrap();
        assert_eq!(g.file_name().as_deref(), Some("Cool Game.gba"));
        assert_eq!(
            g.downloads[0],
            "https://hh3.gbdev.io/static/database-gba/entries/dev_cool-game/files/Cool%20Game.gba"
        );
        assert_eq!(
            g.cover_url.as_deref(),
            Some("https://hh3.gbdev.io/static/database-gba/entries/dev_cool-game/cover.png")
        );
        assert!(g.matches("platform") && g.matches("dev") && !g.matches("racing"));
    }

    #[test]
    fn the_built_in_list_is_valid() {
        let games = parse_list(BUILT_IN_LIST).unwrap();
        assert!(games.len() >= 10, "{} games", games.len());
        let tetrade = games.iter().find(|g| g.id == "tetrade").unwrap();
        assert_eq!(tetrade.platform, Platform::Ps1);
        assert_eq!(tetrade.file_name().as_deref(), Some("TETRADE_PSX.cue"));
        assert!(tetrade.needs_folder());
        let penguin = games.iter().find(|g| g.id == "penguin-world").unwrap();
        assert_eq!(
            penguin.file_name().as_deref(),
            Some("Penguin-World_V.15-12-21.-.DiagV1.SP.bin")
        );
    }

    #[test]
    fn rejects_bad_entries_and_names() {
        let list = r#"{"games": [
            {"id": "a", "title": "No system", "files": ["https://x/a.gba"]},
            {"id": "b", "title": "Bad link", "system": "Gba", "files": ["http://x/b.gba"]},
            {"id": "c", "title": "Sneaky", "system": "Gba", "files": ["https://x/..%2F..%2Fc.gba"]},
            {"id": "d", "title": "Fine", "system": "Gba", "files": ["https://x/d.gba"]}
        ]}"#;
        let games = parse_list(list).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].id, "d");
        assert_eq!(safe_name("A/B: C?"), "A_B_ C_");
    }

    /// Needs the internet: downloads Tetrade (.cue + .bin) into a temp folder.
    #[test]
    #[ignore]
    fn downloads_a_multi_file_game() {
        let games = parse_list(BUILT_IN_LIST).unwrap();
        let game = games.iter().find(|g| g.id == "tetrade").unwrap();
        let dir = std::env::temp_dir().join(format!("gameshelf-free-{}", std::process::id()));
        let folder = download_game(game, &dir).unwrap();
        assert!(folder.join("TETRADE_PSX.cue").is_file());
        assert!(folder.join("TETRADE_PSX.bin").is_file());
        assert!(download_game(game, &dir).is_err(), "never overwrites");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Needs the internet: the online list can be read.
    #[test]
    #[ignore]
    fn reads_the_online_list() {
        let (tx, rx) = std::sync::mpsc::channel();
        load_list(DEFAULT_LIST_URL.into(), tx, egui::Context::default());
        let ListMsg::Loaded { games, note } = rx.recv().unwrap();
        println!("{} games, {note:?}", games.len());
        assert!(!games.is_empty());
    }
}
