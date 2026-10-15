//! The game library: what's in it, saving it to disk, and launching games.

use crate::platform::{self, Platform, ALL};
use crate::workspace;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SCAN_DEPTH: u32 = 8;

#[derive(Serialize, Deserialize, Clone)]
pub struct Game {
    pub title: String,
    pub path: PathBuf,
    pub platform: Platform,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub last_played: Option<u64>,
    #[serde(default)]
    pub play_count: u32,
    #[serde(default)]
    pub added: u64,
    /// Box art on disk (downloaded or picked by the user).
    #[serde(default)]
    pub cover: Option<PathBuf>,
    /// Looked online and found nothing; don't ask again on every start.
    #[serde(default)]
    pub cover_missing: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct EmuConfig {
    pub exe: String,
    pub args: String,
}

impl EmuConfig {
    pub fn exe_path(&self) -> PathBuf {
        PathBuf::from(self.exe.trim().trim_matches('"'))
    }

    pub fn is_ready(&self) -> bool {
        !self.exe.trim().is_empty() && self.exe_path().is_file()
    }
}

#[derive(Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub games: Vec<Game>,
    #[serde(default)]
    pub emulators: HashMap<Platform, EmuConfig>,
    #[serde(default = "default_true")]
    pub dark_mode: bool,
    /// The GameShelf folder (Games, Emulators, BIOS).
    #[serde(default)]
    pub root: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub scan_on_start: bool,
    #[serde(default = "default_true")]
    pub auto_covers: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Library {
    fn default() -> Self {
        Self {
            games: Vec::new(),
            emulators: HashMap::new(),
            dark_mode: true,
            root: None,
            scan_on_start: true,
            auto_covers: true,
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Library {
    fn file_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "GameShelf", "GameShelf")
            .map(|d| d.config_dir().join("library.json"))
    }

    pub fn covers_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "GameShelf", "GameShelf")
            .map(|d| d.config_dir().join("covers"))
    }

    /// Loads the saved library. A damaged file is kept as `library.json.bak`
    /// and an empty library is returned along with a warning.
    pub fn load() -> (Self, Option<String>) {
        let mut warning = None;
        let mut lib = match Self::file_path() {
            Some(path) if path.exists() => match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<Library>(&s).map_err(|e| e.to_string()))
            {
                Ok(lib) => lib,
                Err(e) => {
                    let _ = fs::rename(&path, path.with_extension("json.bak"));
                    warning = Some(format!(
                        "Library file was damaged ({e}); a backup was kept."
                    ));
                    Library::default()
                }
            },
            _ => Library::default(),
        };
        for p in ALL {
            if let Some(info) = p.emulator() {
                lib.emulators.entry(p).or_insert_with(|| EmuConfig {
                    exe: String::new(),
                    args: info.default_args.to_string(),
                });
            }
        }
        (lib, warning)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::file_path().ok_or("No folder to save the library in")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Write a temp file then swap it in, so a crash never leaves half a file.
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    /// Adds a file or every game inside a folder. Returns how many were new.
    pub fn add_path(&mut self, path: &Path) -> usize {
        let mut added = 0;
        if path.is_dir() {
            self.scan_dir(path, 0, &mut added);
        } else if self.add_file(path) {
            added += 1;
        }
        added
    }

    fn scan_dir(&mut self, dir: &Path, depth: u32, added: &mut usize) {
        if depth > MAX_SCAN_DEPTH {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir(&path, depth + 1, added);
            } else if self.add_file(&path) {
                *added += 1;
            }
        }
    }

    fn add_file(&mut self, path: &Path) -> bool {
        if self.games.iter().any(|g| g.path == path) {
            return false;
        }
        let Some(detected) = platform::detect(path) else {
            return false;
        };
        // A system folder (e.g. "PlayStation 2") beats guessing from the extension.
        let platform = platform::folder_hint(path).unwrap_or(detected);
        self.games.push(Game {
            title: title_for(path),
            path: path.to_path_buf(),
            platform,
            favorite: false,
            last_played: None,
            play_count: 0,
            added: now_secs(),
            cover: None,
            cover_missing: false,
        });
        true
    }

    /// Picks up new games and emulators from the GameShelf folder.
    /// Returns (games added, emulators newly set up).
    pub fn sync_root(&mut self) -> (usize, Vec<&'static str>) {
        let Some(root) = self.root.clone() else {
            return (0, Vec::new());
        };
        let games_dir = root.join(workspace::GAMES);
        let added = if games_dir.is_dir() {
            self.add_path(&games_dir)
        } else {
            0
        };
        let mut set_up = Vec::new();
        for (name, exe) in workspace::find_emulators(&root.join(workspace::EMULATORS)) {
            let systems: Vec<Platform> = ALL
                .into_iter()
                .filter(|p| p.emulator().is_some_and(|i| i.name == name))
                .collect();
            // Never replace an emulator the user already set up elsewhere.
            if systems.iter().any(|p| self.emulator_ready(*p)) {
                continue;
            }
            for p in systems {
                self.emulators.entry(p).or_default().exe = exe.display().to_string();
            }
            set_up.push(name);
        }
        (added, set_up)
    }

    pub fn emulator_ready(&self, platform: Platform) -> bool {
        self.emulators
            .get(&platform)
            .is_some_and(EmuConfig::is_ready)
    }

    /// Starts the game in its emulator.
    pub fn launch(&mut self, index: usize) -> Result<String, String> {
        let game = self.games.get(index).ok_or("Game not found")?.clone();
        if game.platform == Platform::Unknown {
            return Err("Pick a system for this game first (the ⋯ button on its card).".into());
        }
        if !game.path.exists() {
            return Err(format!("File not found: {}", game.path.display()));
        }
        let cfg = self
            .emulators
            .get(&game.platform)
            .cloned()
            .unwrap_or_default();
        if !cfg.is_ready() {
            return Err(format!(
                "No emulator set for {}. Set it in the Emulators tab.",
                game.platform.name()
            ));
        }
        let exe = cfg.exe_path();
        let mut cmd = Command::new(&exe);
        let mut has_rom = false;
        for token in cfg.args.split_whitespace() {
            if token == "{rom}" {
                cmd.arg(&game.path);
                has_rom = true;
            } else {
                cmd.arg(token);
            }
        }
        if !has_rom {
            cmd.arg(&game.path);
        }
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        cmd.spawn()
            .map_err(|e| format!("Could not start the emulator: {e}"))?;

        if let Some(g) = self.games.get_mut(index) {
            g.last_played = Some(now_secs());
            g.play_count = g.play_count.saturating_add(1);
        }
        Ok(format!("Started {}", game.title))
    }
}

/// A readable title from the file name (the game folder's name for PS3).
fn title_for(path: &Path) -> String {
    let raw = if platform::is_ps3_eboot(path) {
        path.ancestors().nth(3).and_then(Path::file_name)
    } else {
        path.file_stem()
    };
    raw.map(|s| s.to_string_lossy().replace('_', " ").trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Untitled".to_string())
}
