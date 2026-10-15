//! The GameShelf folder: one place for games (sorted by system), emulators and BIOS files.
//!
//! ```text
//! GameShelf\
//!   Games\
//!     PlayStation 2\      <- the folder name tells GameShelf the system
//!     Game Boy Advance\
//!     ...
//!   Emulators\            <- unzip emulators here; GameShelf finds their .exe
//!   BIOS\
//! ```

use crate::platform::{self, Platform, ALL};
use std::fs;
use std::path::{Path, PathBuf};

pub const GAMES: &str = "Games";
pub const EMULATORS: &str = "Emulators";
pub const BIOS: &str = "BIOS";
const MAX_DEPTH: u32 = 4;

const README: &str = "GameShelf folder\r\n\
================\r\n\
\r\n\
Games\\       Put each game in the folder of its system, e.g. Games\\PlayStation 2\\.\r\n\
             GameShelf uses the folder name to know the system.\r\n\
Emulators\\   Unzip or install emulators here (one folder each).\r\n\
             GameShelf finds them and sets them up for you.\r\n\
BIOS\\        Keep BIOS files dumped from your own consoles here.\r\n\
\r\n\
Click \"Scan now\" in GameShelf settings after adding files.\r\n";

/// `Documents\GameShelf`.
pub fn default_root() -> Option<PathBuf> {
    directories::UserDirs::new()?
        .document_dir()
        .map(|d| d.join("GameShelf"))
}

/// Folder for one system's games.
pub fn system_dir(root: &Path, platform: Platform) -> PathBuf {
    root.join(GAMES).join(platform.name())
}

/// Creates the folders (and a README). Existing files are left alone.
pub fn create(root: &Path) -> Result<(), String> {
    let mkdir = |p: &Path| fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()));
    for p in ALL {
        if p != Platform::Unknown {
            mkdir(&system_dir(root, p))?;
        }
    }
    mkdir(&root.join(EMULATORS))?;
    mkdir(&root.join(BIOS))?;
    let readme = root.join("README.txt");
    if !readme.exists() {
        fs::write(&readme, README).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// What's in the folder, for the settings screen.
pub struct FolderInfo {
    pub has_structure: bool,
    pub games_per_system: Vec<(Platform, usize)>,
    pub emulators: Vec<(&'static str, PathBuf)>,
}

pub fn inspect(root: &Path) -> FolderInfo {
    let games_per_system = ALL
        .into_iter()
        .filter(|p| *p != Platform::Unknown)
        .map(|p| (p, count_games(&system_dir(root, p), 0)))
        .collect();
    FolderInfo {
        has_structure: root.join(GAMES).is_dir() && root.join(EMULATORS).is_dir(),
        games_per_system,
        emulators: find_emulators(&root.join(EMULATORS)),
    }
}

fn count_games(dir: &Path, depth: u32) -> usize {
    if depth > MAX_DEPTH {
        return 0;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let path = e.path();
            if path.is_dir() {
                count_games(&path, depth + 1)
            } else {
                usize::from(platform::detect(&path).is_some())
            }
        })
        .sum()
}

/// Known emulator programs inside `dir` (a few levels deep).
pub fn find_emulators(dir: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut found: Vec<(&'static str, PathBuf)> = Vec::new();
    walk_for_exes(dir, 0, &mut found);
    found
}

fn walk_for_exes(dir: &Path, depth: u32, found: &mut Vec<(&'static str, PathBuf)>) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_for_exes(&path, depth + 1, found);
            continue;
        }
        let Some(file) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let file = file.to_ascii_lowercase();
        let Some(name) = emulator_for_exe(&file) else {
            continue;
        };
        match found.iter_mut().find(|(n, _)| *n == name) {
            // PPSSPP ships a 32- and a 64-bit exe; prefer 64.
            Some(existing) if file.contains("64") => existing.1 = path,
            Some(_) => {}
            None => found.push((name, path)),
        }
    }
}

fn emulator_for_exe(file: &str) -> Option<&'static str> {
    if !file.ends_with(".exe") || file.contains("update") || file.contains("uninst") {
        return None;
    }
    Some(match file {
        "retroarch.exe" => "RetroArch",
        "rpcs3.exe" => "RPCS3",
        "dolphin.exe" => "Dolphin",
        "flycast.exe" => "Flycast",
        f if f.starts_with("duckstation") => "DuckStation",
        f if f.starts_with("pcsx2") => "PCSX2",
        f if f.starts_with("ppssppwindows") => "PPSSPP",
        _ => return None,
    })
}

/// Shows a folder in File Explorer.
pub fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open the folder: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_name_decides_the_system() {
        let p = Path::new(r"D:\GameShelf\Games\PlayStation 2\Okami (USA).iso");
        assert_eq!(platform::folder_hint(p), Some(Platform::Ps2));
        let p = Path::new(r"D:\GameShelf\Games\Sega Genesis\Sonic.bin");
        assert_eq!(platform::folder_hint(p), Some(Platform::Genesis));
        assert_eq!(
            platform::folder_hint(Path::new(r"D:\Downloads\x.iso")),
            None
        );
    }

    #[test]
    fn recognises_emulator_programs() {
        assert_eq!(emulator_for_exe("retroarch.exe"), Some("RetroArch"));
        assert_eq!(
            emulator_for_exe("duckstation-qt-x64-releaseltcg.exe"),
            Some("DuckStation")
        );
        assert_eq!(emulator_for_exe("pcsx2-qt.exe"), Some("PCSX2"));
        assert_eq!(emulator_for_exe("ppssppwindows64.exe"), Some("PPSSPP"));
        assert_eq!(emulator_for_exe("duckstation-updater.exe"), None);
        assert_eq!(emulator_for_exe("readme.txt"), None);
    }

    #[test]
    fn creates_and_reads_the_folder() {
        let root = std::env::temp_dir().join(format!("gameshelf-test-{}", std::process::id()));
        create(&root).unwrap();
        fs::write(system_dir(&root, Platform::Gba).join("Game.gba"), b"x").unwrap();
        let emu = root.join(EMULATORS).join("RetroArch");
        fs::create_dir_all(&emu).unwrap();
        fs::write(emu.join("retroarch.exe"), b"x").unwrap();

        let info = inspect(&root);
        assert!(info.has_structure);
        assert!(info.games_per_system.contains(&(Platform::Gba, 1)));
        assert_eq!(info.emulators.len(), 1);
        fs::remove_dir_all(&root).unwrap();
    }
}
