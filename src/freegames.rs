//! Free games from [Homebrew Hub](https://hh.gbdev.io): homebrew their makers
//! share for free, for Game Boy, Game Boy Color, Game Boy Advance and NES.
//!
//! The list comes from its public API (`/api/search`, ten games a page); files and
//! pictures live at `static/<basepath>/entries/<slug>/<file>`.

use crate::platform::Platform;
use eframe::egui;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

const API: &str = "https://hh3.gbdev.io/api/search";
const STATIC: &str = "https://hh3.gbdev.io/static";
/// Homebrew is small; anything bigger than this isn't what we expect.
const MAX_GAME_BYTES: u64 = 256 * 1024 * 1024;
/// Stops a runaway listing.
const MAX_PAGES: u32 = 200;

/// The systems Homebrew Hub has, in the order shown.
pub const SYSTEMS: [Platform; 4] = [Platform::Gb, Platform::Gbc, Platform::Gba, Platform::Nes];

fn api_code(p: Platform) -> Option<&'static str> {
    Some(match p {
        Platform::Gb => "GB",
        Platform::Gbc => "GBC",
        Platform::Gba => "GBA",
        Platform::Nes => "NES",
        _ => return None,
    })
}

#[derive(Clone)]
pub struct FreeFile {
    pub filename: String,
    pub default: bool,
    pub playable: bool,
}

#[derive(Clone)]
pub struct FreeGame {
    pub slug: String,
    pub title: String,
    pub developer: Option<String>,
    pub platform: Platform,
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub website: Option<String>,
    basepath: String,
    cover: Option<String>,
    pub files: Vec<FreeFile>,
}

impl FreeGame {
    fn entry_url(&self, file: &str) -> String {
        format!("{STATIC}/{}/entries/{}/{}", self.basepath, self.slug, file)
    }

    pub fn cover_url(&self) -> Option<String> {
        self.cover.as_deref().map(|c| self.entry_url(c))
    }

    /// The file to download: the one marked default, else the first playable one.
    pub fn main_file(&self) -> Option<&FreeFile> {
        self.files
            .iter()
            .find(|f| f.default)
            .or_else(|| self.files.iter().find(|f| f.playable))
            .or_else(|| self.files.first())
    }

    /// Name the game gets on disk (`files/game.gba` → `game.gba`).
    pub fn file_name(&self) -> Option<String> {
        let f = self.main_file()?;
        Path::new(&f.filename)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty() && !n.starts_with('.'))
    }

    /// Searchable text: title, developer and tags.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        q.is_empty()
            || self.title.to_lowercase().contains(&q)
            || self
                .developer
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(&q))
            || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }
}

fn parse_entry(e: &serde_json::Value, platform: Platform) -> Option<FreeGame> {
    let s = |key: &str| {
        e.get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|v| !v.is_empty())
    };
    let files: Vec<FreeFile> = e
        .get("files")?
        .as_array()?
        .iter()
        .filter_map(|f| {
            Some(FreeFile {
                filename: f.get("filename")?.as_str()?.to_string(),
                default: f.get("default").and_then(|v| v.as_bool()).unwrap_or(false),
                playable: f.get("playable").and_then(|v| v.as_bool()).unwrap_or(false),
            })
        })
        .collect();
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
        .cloned();
    Some(FreeGame {
        slug: s("slug")?,
        title: s("title")?,
        developer: s("developer"),
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
        basepath: s("basepath")?,
        cover,
        files,
    })
    .filter(|g| !g.files.is_empty())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .user_agent("GameShelf")
        .build()
}

pub enum CatalogMsg {
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

/// Loads every game of `platform`, a page at a time, on a background thread.
pub fn load(platform: Platform, tx: Sender<CatalogMsg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let send = |m: CatalogMsg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        let Some(code) = api_code(platform) else {
            send(CatalogMsg::Failed {
                platform,
                error: "Homebrew Hub doesn't have this system".into(),
            });
            return;
        };
        let agent = agent();
        let mut page = 1;
        loop {
            let url = format!("{API}?platform={code}&typetag=game&page={page}");
            let json: Result<serde_json::Value, String> = agent
                .get(&url)
                .call()
                .map_err(|e| e.to_string())
                .and_then(|r| {
                    let mut text = String::new();
                    r.into_reader()
                        .take(8 * 1024 * 1024)
                        .read_to_string(&mut text)
                        .map_err(|e| e.to_string())?;
                    serde_json::from_str(&text).map_err(|e| e.to_string())
                });
            let json = match json {
                Ok(j) => j,
                Err(error) => {
                    send(CatalogMsg::Failed { platform, error });
                    return;
                }
            };
            let total = json.get("results").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let pages = json.get("page_total").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let games = json
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|e| parse_entry(e, platform)).collect())
                .unwrap_or_default();
            send(CatalogMsg::Page {
                platform,
                games,
                total,
            });
            if page >= pages || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }
        send(CatalogMsg::Done { platform });
    });
}

/// Where a downloaded game is saved: `<system folder>\<file name>`.
pub fn destination(game: &FreeGame, system_dir: &Path) -> Option<PathBuf> {
    game.file_name().map(|n| system_dir.join(n))
}

pub struct GetResult {
    pub slug: String,
    /// The game file in the library folder.
    pub result: Result<PathBuf, String>,
}

/// Downloads a game into its system folder (and unzips it if needed), in the background.
pub fn get(game: FreeGame, system_dir: PathBuf, tx: Sender<GetResult>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = download_game(&game, &system_dir);
        let _ = tx.send(GetResult {
            slug: game.slug.clone(),
            result,
        });
        ctx.request_repaint();
    });
}

fn download_game(game: &FreeGame, system_dir: &Path) -> Result<PathBuf, String> {
    let file = game
        .main_file()
        .ok_or("This entry has no file to download")?;
    let dest = destination(game, system_dir).ok_or("This entry's file name isn't usable")?;
    if dest.exists() {
        return Err(format!("{} is already there", dest.display()));
    }
    fs::create_dir_all(system_dir).map_err(|e| e.to_string())?;
    let url = game.entry_url(&file.filename);
    let resp = agent()
        .get(&url)
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
    if copied > MAX_GAME_BYTES || copied == 0 {
        let _ = fs::remove_file(&part);
        return Err("The download wasn't a game file.".into());
    }
    fs::rename(&part, &dest).map_err(|e| e.to_string())?;

    // A .zip: unpack it next to it, into a folder named after the game.
    let is_zip = dest
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip {
        return Ok(dest);
    }
    let folder = system_dir.join(safe_name(&game.title));
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("tar");
    cmd.arg("-xf").arg(&dest).arg("-C").arg(&folder);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&dest);
    if !out.status.success() {
        return Err(format!(
            "Could not unpack: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(folder)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> serde_json::Value {
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
                {"filename": "files/CoolGame.gba", "default": true, "playable": true}
            ]
        })
    }

    #[test]
    fn reads_an_entry() {
        let g = parse_entry(&entry(), Platform::Gba).unwrap();
        assert_eq!(g.file_name().as_deref(), Some("CoolGame.gba"));
        assert_eq!(
            g.cover_url().as_deref(),
            Some("https://hh3.gbdev.io/static/database-gba/entries/dev_cool-game/cover.png")
        );
        assert!(g.matches("platform"));
        assert!(g.matches("dev"));
        assert!(!g.matches("racing"));
    }

    #[test]
    fn rejects_bad_file_names() {
        let mut e = entry();
        e["files"] = serde_json::json!([{"filename": "files/..", "default": true}]);
        let g = parse_entry(&e, Platform::Gba).unwrap();
        assert!(g.file_name().is_none());
        assert_eq!(safe_name("A/B: C?"), "A_B_ C_");
    }

    /// Needs the internet: downloads the first free NES game into a temp folder.
    #[test]
    #[ignore]
    fn downloads_a_real_game() {
        let (tx, rx) = std::sync::mpsc::channel();
        load(Platform::Nes, tx, egui::Context::default());
        let game = rx
            .iter()
            .find_map(|m| match m {
                CatalogMsg::Page { games, .. } => games.into_iter().find(|g| {
                    g.file_name()
                        .is_some_and(|n| n.to_lowercase().ends_with(".nes"))
                }),
                CatalogMsg::Failed { error, .. } => panic!("{error}"),
                CatalogMsg::Done { .. } => None,
            })
            .unwrap();
        let dir = std::env::temp_dir().join(format!("gameshelf-free-{}", std::process::id()));
        let path = download_game(&game, &dir).unwrap();
        let size = fs::metadata(&path).unwrap().len();
        println!("{} -> {} ({size} bytes)", game.title, path.display());
        assert_eq!(crate::platform::detect(&path), Some(Platform::Nes));
        assert!(download_game(&game, &dir).is_err(), "never overwrites");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Needs the internet: `cargo test -- --ignored lists_real_games`.
    #[test]
    #[ignore]
    fn lists_real_games() {
        let (tx, rx) = std::sync::mpsc::channel();
        load(Platform::Nes, tx, egui::Context::default());
        let mut count = 0;
        for msg in rx {
            match msg {
                CatalogMsg::Page { games, total, .. } => {
                    count += games.len();
                    assert!(total > 0);
                }
                CatalogMsg::Done { .. } => break,
                CatalogMsg::Failed { error, .. } => panic!("{error}"),
            }
        }
        println!("NES free games: {count}");
        assert!(count > 0);
    }
}
