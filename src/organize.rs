//! "Organize my games": move each game into `Games\<System>\` of the GameShelf folder.
//!
//! A plan is made first and shown to the user; nothing moves until they confirm.
//! Moves never overwrite, carry a game's companion files (.cue tracks, PS3 game
//! folders) along, and run on a background thread.

use crate::library::Library;
use crate::platform::{self, Platform};
use crate::workspace;
use eframe::egui;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;

/// Index files larger than this aren't track lists; don't read them.
const MAX_INDEX_FILE: u64 = 1024 * 1024;

pub struct Move {
    pub title: String,
    pub platform: Platform,
    /// The game's current path (its key in the library).
    pub old_path: PathBuf,
    /// Where the game's main file ends up.
    pub new_path: PathBuf,
    /// Every file or folder to move: (from, to).
    pub items: Vec<(PathBuf, PathBuf)>,
}

#[derive(Default)]
pub struct Plan {
    pub moves: Vec<Move>,
    pub in_place: usize,
    pub unknown: usize,
    pub missing: usize,
    /// Titles skipped because something already sits where they would go.
    pub conflicts: Vec<String>,
}

pub fn plan(lib: &Library, root: &Path) -> Plan {
    let mut plan = Plan::default();
    let mut taken: HashSet<PathBuf> = HashSet::new();
    for game in &lib.games {
        if game.platform == Platform::Unknown {
            plan.unknown += 1;
            continue;
        }
        if !game.path.exists() {
            plan.missing += 1;
            continue;
        }
        let target = workspace::system_dir(root, game.platform);
        if game.path.starts_with(&target) {
            plan.in_place += 1;
            continue;
        }

        let (items, new_path) = if platform::is_ps3_eboot(&game.path) {
            // Move the whole game folder: <Game>\PS3_GAME\USRDIR\EBOOT.BIN.
            let Some(game_dir) = game.path.ancestors().nth(3) else {
                plan.conflicts.push(game.title.clone());
                continue;
            };
            let (Some(name), Ok(inner)) = (game_dir.file_name(), game.path.strip_prefix(game_dir))
            else {
                plan.conflicts.push(game.title.clone());
                continue;
            };
            let dest = target.join(name);
            (
                vec![(game_dir.to_path_buf(), dest.clone())],
                dest.join(inner),
            )
        } else {
            let mut items = Vec::new();
            for file in with_companions(&game.path) {
                if let Some(name) = file.file_name() {
                    let dest = target.join(name);
                    items.push((file, dest));
                }
            }
            let Some(name) = game.path.file_name() else {
                plan.conflicts.push(game.title.clone());
                continue;
            };
            (items, target.join(name))
        };

        let blocked = items
            .iter()
            .any(|(_, to)| to.exists() || taken.contains(to));
        if blocked || items.is_empty() {
            plan.conflicts.push(game.title.clone());
            continue;
        }
        taken.extend(items.iter().map(|(_, to)| to.clone()));
        plan.moves.push(Move {
            title: game.title.clone(),
            platform: game.platform,
            old_path: game.path.clone(),
            new_path,
            items,
        });
    }
    plan
}

/// The game file plus the files it points to (.cue / .gdi / .m3u track lists).
fn with_companions(path: &Path) -> Vec<PathBuf> {
    let mut files = vec![path.to_path_buf()];
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !matches!(ext.as_str(), "cue" | "gdi" | "m3u") {
        return files;
    }
    let Some(dir) = path.parent() else {
        return files;
    };
    let small = fs::metadata(path).is_ok_and(|m| m.len() <= MAX_INDEX_FILE);
    let Some(text) = small.then(|| fs::read_to_string(path).ok()).flatten() else {
        return files;
    };
    for line in text.lines() {
        let line = line.trim();
        let name = match ext.as_str() {
            "cue" if line.to_ascii_uppercase().starts_with("FILE") => quoted_or_word(line, 1),
            "gdi" => quoted_or_word(line, 4),
            "m3u" if !line.is_empty() && !line.starts_with('#') => Some(line.to_string()),
            _ => None,
        };
        let Some(name) = name else { continue };
        let file = dir.join(&name);
        // Only plain names next to the index file; never paths elsewhere.
        let plain = Path::new(&name).components().count() == 1;
        if plain && file.is_file() && !files.contains(&file) {
            files.push(file);
        }
    }
    files
}

/// The quoted part of a line, or else its `word`-th whitespace-separated word.
fn quoted_or_word(line: &str, word: usize) -> Option<String> {
    if let Some(start) = line.find('"') {
        let rest = line.get(start + 1..)?;
        let end = rest.find('"')?;
        return rest.get(..end).map(str::to_string);
    }
    line.split_whitespace().nth(word).map(str::to_string)
}

pub enum Progress {
    Working {
        done: usize,
        total: usize,
        title: String,
    },
    Moved {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    Failed {
        title: String,
        error: String,
    },
    Finished,
}

/// Moves everything in the background, reporting as it goes.
pub fn run(moves: Vec<Move>, tx: Sender<Progress>, ctx: egui::Context) {
    thread::spawn(move || {
        let total = moves.len();
        for (done, m) in moves.into_iter().enumerate() {
            let send = |p: Progress| {
                let _ = tx.send(p);
                ctx.request_repaint();
            };
            send(Progress::Working {
                done,
                total,
                title: m.title.clone(),
            });
            match apply(&m) {
                Ok(()) => send(Progress::Moved {
                    old_path: m.old_path,
                    new_path: m.new_path,
                }),
                Err(error) => send(Progress::Failed {
                    title: m.title,
                    error,
                }),
            }
        }
        let _ = tx.send(Progress::Finished);
        ctx.request_repaint();
    });
}

/// Moves one game's files (used by the background mover and `--organize`).
pub fn apply(m: &Move) -> Result<(), String> {
    m.items
        .iter()
        .try_for_each(|(from, to)| move_path(from, to))
}

/// Rename when possible; across drives, copy, check, then delete the original.
fn move_path(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        return Err(format!("{} already exists", to.display()));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, to, 0)?;
        fs::remove_dir_all(from)
            .map_err(|e| format!("Copied, but could not remove the original: {e}"))
    } else {
        let copied = fs::copy(from, to).map_err(|e| e.to_string())?;
        let original = fs::metadata(from).map_err(|e| e.to_string())?.len();
        if copied != original {
            let _ = fs::remove_file(to);
            return Err("The copy came out a different size; the original was kept.".into());
        }
        fs::remove_file(from).map_err(|e| format!("Copied, but could not remove the original: {e}"))
    }
}

fn copy_dir(from: &Path, to: &Path, depth: u32) -> Result<(), String> {
    if depth > 32 {
        return Err("Folder is nested too deeply".into());
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst, depth + 1)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Game;

    fn game(path: PathBuf, platform: Platform) -> Game {
        Game {
            title: "T".into(),
            path,
            platform,
            favorite: false,
            last_played: None,
            play_count: 0,
            added: 0,
            cover: None,
            cover_missing: false,
        }
    }

    #[test]
    fn plans_and_moves_a_cue_with_its_tracks() {
        let base = std::env::temp_dir().join(format!("gameshelf-org-{}", std::process::id()));
        let loose = base.join("Downloads");
        let root = base.join("GameShelf");
        fs::create_dir_all(&loose).unwrap();
        workspace::create(&root).unwrap();
        fs::write(
            loose.join("Game.cue"),
            "FILE \"Game (Track 1).bin\" BINARY\n",
        )
        .unwrap();
        fs::write(loose.join("Game (Track 1).bin"), b"data").unwrap();
        fs::write(loose.join("Other.gba"), b"rom").unwrap();
        let placed = workspace::system_dir(&root, Platform::Gba).join("Placed.gba");
        fs::write(&placed, b"rom").unwrap();

        let mut lib = Library::default();
        lib.games.push(game(loose.join("Game.cue"), Platform::Ps1));
        lib.games.push(game(loose.join("Other.gba"), Platform::Gba));
        lib.games.push(game(placed, Platform::Gba));
        lib.games
            .push(game(loose.join("Mystery.iso"), Platform::Unknown));

        let plan = plan(&lib, &root);
        assert_eq!(plan.moves.len(), 2);
        assert_eq!(plan.in_place, 1);
        assert_eq!(plan.unknown, 1);
        assert_eq!(plan.moves[0].items.len(), 2, "cue moves with its track");

        for m in &plan.moves {
            for (from, to) in &m.items {
                move_path(from, to).unwrap();
            }
        }
        let ps1 = workspace::system_dir(&root, Platform::Ps1);
        assert!(ps1.join("Game.cue").is_file());
        assert!(ps1.join("Game (Track 1).bin").is_file());
        assert!(!loose.join("Game.cue").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn never_overwrites() {
        let base = std::env::temp_dir().join(format!("gameshelf-ow-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("a"), b"1").unwrap();
        fs::write(base.join("b"), b"2").unwrap();
        assert!(move_path(&base.join("a"), &base.join("b")).is_err());
        assert_eq!(fs::read(base.join("b")).unwrap(), b"2");
        fs::remove_dir_all(&base).unwrap();
    }
}
