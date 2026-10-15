//! Where RetroArch keeps game progress (saves, save states) and cheats, and moving
//! that into the GameShelf folder (`Saves\`, `States\`, `Cheats\`).

use crate::controls;
use crate::workspace;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_CFG_BYTES: u64 = 4 * 1024 * 1024;

pub struct RaDirs {
    pub saves: PathBuf,
    pub states: PathBuf,
    pub cheats: PathBuf,
    /// RetroArch saves a state when a game closes and loads it next time.
    pub resume: bool,
}

impl RaDirs {
    pub fn inside(&self, root: &Path) -> bool {
        self.saves.starts_with(root)
            && self.states.starts_with(root)
            && self.cheats.starts_with(root)
    }
}

fn cfg_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// RetroArch writes `:\saves` for "next to retroarch.exe" and `default` for its default.
fn resolve(value: Option<String>, exe_dir: &Path, default: &str) -> PathBuf {
    match value.as_deref() {
        None | Some("") | Some("default") => exe_dir.join(default),
        Some(v) => match v.strip_prefix(':') {
            Some(rest) => exe_dir.join(rest.trim_start_matches(['\\', '/'])),
            None => PathBuf::from(v),
        },
    }
}

pub fn retroarch_dirs(exe: &Path) -> Option<RaDirs> {
    let dir = exe.parent()?;
    let cfg = controls::retroarch_cfg(exe)?;
    let small = fs::metadata(&cfg).is_ok_and(|m| m.len() <= MAX_CFG_BYTES);
    let text = small
        .then(|| fs::read_to_string(&cfg).ok())
        .flatten()
        .unwrap_or_default();
    Some(RaDirs {
        saves: resolve(cfg_value(&text, "savefile_directory"), dir, "saves"),
        states: resolve(cfg_value(&text, "savestate_directory"), dir, "states"),
        cheats: resolve(cfg_value(&text, "cheat_database_path"), dir, "cheats"),
        resume: cfg_value(&text, "savestate_auto_save").as_deref() == Some("true")
            && cfg_value(&text, "savestate_auto_load").as_deref() == Some("true"),
    })
}

/// Points RetroArch at the GameShelf folder, copying existing saves, states and
/// cheats there first (originals are kept). Returns how many files were copied.
pub fn use_gameshelf_folder(exe: &Path, root: &Path) -> Result<usize, String> {
    let old = retroarch_dirs(exe).ok_or("RetroArch's folder wasn't found")?;
    workspace::create(root)?;
    let new = [
        (old.saves, root.join(workspace::SAVES), "savefile_directory"),
        (
            old.states,
            root.join(workspace::STATES),
            "savestate_directory",
        ),
        (
            old.cheats,
            root.join(workspace::CHEATS),
            "cheat_database_path",
        ),
    ];
    let mut copied = 0;
    for (from, to, _) in &new {
        if from != to && from.is_dir() {
            copied += copy_missing(from, to, 0)?;
        }
    }
    let changes: Vec<(&str, String)> = new
        .iter()
        .map(|(_, to, key)| (*key, to.display().to_string()))
        .collect();
    controls::write_retroarch(exe, &changes)?;
    Ok(copied)
}

/// "Resume where I left off": save a state when a game closes, load it next time.
pub fn set_resume(exe: &Path, on: bool) -> Result<(), String> {
    let value = if on { "true" } else { "false" }.to_string();
    controls::write_retroarch(
        exe,
        &[
            ("savestate_auto_save", value.clone()),
            ("savestate_auto_load", value),
        ],
    )
}

/// Copies files that aren't at the destination yet. Never overwrites.
fn copy_missing(from: &Path, to: &Path, depth: u32) -> Result<usize, String> {
    if depth > 16 {
        return Ok(0);
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut copied = 0;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copied += copy_missing(&src, &dst, depth + 1)?;
        } else if !dst.exists() {
            fs::copy(&src, &dst).map_err(|e| e.to_string())?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_saves_into_the_gameshelf_folder() {
        let base = std::env::temp_dir().join(format!("gameshelf-saves-{}", std::process::id()));
        let ra = base.join("RetroArch");
        let root = base.join("GameShelf");
        fs::create_dir_all(ra.join("saves/mGBA")).unwrap();
        fs::write(ra.join("saves/mGBA/Game.srm"), b"progress").unwrap();
        fs::write(
            ra.join("retroarch.cfg"),
            "savefile_directory = \":\\saves\"\nsavestate_directory = \":\\states\"\nvideo_driver = \"gl\"\n",
        )
        .unwrap();
        let exe = ra.join("retroarch.exe");

        let dirs = retroarch_dirs(&exe).unwrap();
        assert_eq!(dirs.saves, ra.join("saves"));
        assert!(!dirs.inside(&root));

        assert_eq!(use_gameshelf_folder(&exe, &root).unwrap(), 1);
        assert!(root.join("Saves/mGBA/Game.srm").is_file());
        assert!(ra.join("saves/mGBA/Game.srm").is_file(), "original kept");
        assert!(retroarch_dirs(&exe).unwrap().inside(&root));

        set_resume(&exe, true).unwrap();
        assert!(retroarch_dirs(&exe).unwrap().resume);
        fs::remove_dir_all(&base).unwrap();
    }
}
