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

// ---------------------------------------------------------------------------
// Standalone emulators (DuckStation, PCSX2, PPSSPP, Dolphin, RPCS3, Flycast).
//
// Their saves aren't moved and their settings aren't changed: GameShelf links
// `Saves\<Emulator>` and `States\<Emulator>` to the real folders (a Windows
// "junction"), so everything shows up in one place and each emulator keeps
// working exactly as before.
// ---------------------------------------------------------------------------

pub const STANDALONE: [&str; 6] = [
    "DuckStation",
    "PCSX2",
    "PPSSPP",
    "Dolphin",
    "RPCS3",
    "Flycast",
];

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// In-game saves: memory cards, save data.
    Saves,
    /// Save states.
    States,
}

pub struct SaveFolder {
    pub kind: Kind,
    pub label: &'static str,
    pub path: PathBuf,
}

/// Where one emulator keeps progress (only folders that exist).
pub struct EmuSaves {
    pub emulator: &'static str,
    pub folders: Vec<SaveFolder>,
}

fn ini_value(path: &Path, section: &str, key: &str) -> Option<String> {
    let small = fs::metadata(path).is_ok_and(|m| m.len() <= MAX_CFG_BYTES);
    let text = small.then(|| fs::read_to_string(path).ok()).flatten()?;
    let header = format!("[{section}]");
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line.eq_ignore_ascii_case(&header);
            continue;
        }
        if inside {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key && !v.trim().is_empty() {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// A folder setting: absolute, or relative to `base`.
fn folder(base: &Path, value: Option<String>, default: &str) -> PathBuf {
    match value {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        Some(v) => base.join(v),
        None => base.join(default),
    }
}

pub fn standalone_saves(emulator: &'static str, exe: &Path) -> EmuSaves {
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let settings = crate::emukeys::settings_files(emulator, exe);
    let first = settings.first().cloned();
    let docs = directories::UserDirs::new().and_then(|u| u.document_dir().map(Path::to_path_buf));
    let base = directories::BaseDirs::new();
    let roaming = base.as_ref().map(|b| b.config_dir().to_path_buf());
    let local = base.as_ref().map(|b| b.data_local_dir().to_path_buf());
    // The first of these folders that exists.
    let pick = |dirs: Vec<Option<PathBuf>>| dirs.into_iter().flatten().find(|d| d.is_dir());
    let mut found: Vec<SaveFolder> = Vec::new();
    let mut add = |kind, label, path: PathBuf| found.push(SaveFolder { kind, label, path });

    match emulator {
        "DuckStation" => {
            let user = first
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .or_else(|| {
                    pick(vec![
                        Some(exe_dir.clone()).filter(|d| d.join("settings.ini").is_file()),
                        local.as_ref().map(|l| l.join("DuckStation")),
                        docs.as_ref().map(|d| d.join("DuckStation")),
                    ])
                });
            if let Some(user) = user {
                let ini = user.join("settings.ini");
                add(
                    Kind::Saves,
                    "Memory cards",
                    folder(
                        &user,
                        ini_value(&ini, "MemoryCards", "Directory"),
                        "memcards",
                    ),
                );
                add(
                    Kind::States,
                    "Save states",
                    folder(
                        &user,
                        ini_value(&ini, "Folders", "SaveStates"),
                        "savestates",
                    ),
                );
            }
        }
        "PCSX2" => {
            // <user>\inis\PCSX2.ini
            let user = first
                .as_deref()
                .and_then(|p| p.ancestors().nth(2))
                .map(Path::to_path_buf)
                .or_else(|| {
                    pick(vec![
                        Some(exe_dir.clone()).filter(|d| d.join("inis").is_dir()),
                        docs.as_ref().map(|d| d.join("PCSX2")),
                    ])
                });
            if let Some(user) = user {
                let ini = user.join("inis").join("PCSX2.ini");
                add(
                    Kind::Saves,
                    "Memory cards",
                    folder(&user, ini_value(&ini, "Folders", "MemoryCards"), "memcards"),
                );
                add(
                    Kind::States,
                    "Save states",
                    folder(&user, ini_value(&ini, "Folders", "Savestates"), "sstates"),
                );
            }
        }
        "PPSSPP" => {
            // <memstick>\PSP\SYSTEM\controls.ini
            let memstick = first
                .as_deref()
                .and_then(|p| p.ancestors().nth(3))
                .map(Path::to_path_buf)
                .or_else(|| {
                    pick(vec![
                        Some(exe_dir.join("memstick")),
                        docs.as_ref().map(|d| d.join("PPSSPP")),
                    ])
                });
            if let Some(m) = memstick {
                add(Kind::Saves, "Save data", m.join("PSP").join("SAVEDATA"));
                add(
                    Kind::States,
                    "Save states",
                    m.join("PSP").join("PPSSPP_STATE"),
                );
            }
        }
        "Dolphin" => {
            // <user>\Config\GCPadNew.ini
            let user = first
                .as_deref()
                .and_then(|p| p.ancestors().nth(2))
                .map(Path::to_path_buf)
                .or_else(|| {
                    pick(vec![
                        Some(exe_dir.join("User")),
                        roaming.as_ref().map(|r| r.join("Dolphin Emulator")),
                        docs.as_ref().map(|d| d.join("Dolphin Emulator")),
                    ])
                });
            if let Some(user) = user {
                add(Kind::Saves, "Memory cards", user.join("GC"));
                add(Kind::States, "Save states", user.join("StateSaves"));
            }
        }
        "RPCS3" => {
            add(
                Kind::Saves,
                "Save data",
                exe_dir
                    .join("dev_hdd0")
                    .join("home")
                    .join("00000001")
                    .join("savedata"),
            );
            add(Kind::States, "Save states", exe_dir.join("savestates"));
        }
        "Flycast" => {
            if let Some(data) = pick(vec![
                Some(exe_dir.join("data")),
                roaming.as_ref().map(|r| r.join("flycast").join("data")),
            ]) {
                add(Kind::Saves, "Saves (VMU)", data);
            }
        }
        _ => {}
    }
    found.retain(|f| f.path.is_dir());
    EmuSaves {
        emulator,
        folders: found,
    }
}

/// `GameShelf\Saves\<Emulator>` or `GameShelf\States\<Emulator>`.
pub fn link_path(root: &Path, emulator: &str, kind: Kind) -> PathBuf {
    let top = match kind {
        Kind::Saves => workspace::SAVES,
        Kind::States => workspace::STATES,
    };
    root.join(top).join(emulator)
}

/// Is `link` already pointing at `target`?
pub fn is_linked(link: &Path, target: &Path) -> bool {
    fs::symlink_metadata(link).is_ok()
        && matches!((fs::canonicalize(link), fs::canonicalize(target)), (Ok(a), Ok(b)) if a == b)
}

/// Links every emulator's save folders into the GameShelf folder.
/// Returns how many links were made; existing folders are never touched.
pub fn link_all(root: &Path, emulators: &[EmuSaves]) -> Result<usize, String> {
    workspace::create(root)?;
    let mut made = 0;
    for emu in emulators {
        for f in &emu.folders {
            let link = link_path(root, emu.emulator, f.kind);
            if fs::symlink_metadata(&link).is_ok() {
                continue; // already linked, or a real folder we won't replace
            }
            make_junction(&link, &f.path)?;
            made += 1;
        }
    }
    Ok(made)
}

/// A directory junction (no admin rights needed). Deleting it later removes only
/// the link, never the saves behind it.
fn make_junction(link: &Path, target: &Path) -> Result<(), String> {
    let (l, t) = (link.display().to_string(), target.display().to_string());
    // cmd.exe would read these as commands; such paths are rare, so just refuse.
    if format!("{l}{t}").contains(['&', '|', '<', '>', '^', '"', '%']) {
        return Err(format!(
            "Can't link folders whose names contain & | < > ^ \" or %: {t}"
        ));
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "mklink", "/J", &l, &t]);
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
            "Could not link {t}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

// ---------------------------------------------------------------------------
// Backup: every save folder into one zip.
// ---------------------------------------------------------------------------

/// Every save folder, named for the zip (`RetroArch/Saves`, `DuckStation/Memory cards`…).
pub fn backup_sources(
    retroarch: Option<&RaDirs>,
    emulators: &[EmuSaves],
) -> Vec<(String, PathBuf)> {
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    if let Some(ra) = retroarch {
        sources.push(("RetroArch/Saves".into(), ra.saves.clone()));
        sources.push(("RetroArch/States".into(), ra.states.clone()));
    }
    for emu in emulators {
        for f in &emu.folders {
            sources.push((format!("{}/{}", emu.emulator, f.label), f.path.clone()));
        }
    }
    // Drop missing folders and any folder already inside another one.
    let real: Vec<(String, PathBuf)> = sources
        .into_iter()
        .filter_map(|(n, p)| fs::canonicalize(&p).ok().map(|c| (n, c)))
        .collect();
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for (name, path) in real {
        if !out
            .iter()
            .any(|(_, p)| path.starts_with(p) || p.starts_with(&path))
        {
            out.push((name, path));
        }
    }
    out
}

/// Zips the sources into `out_dir\GameShelf saves <date>.zip`. Returns the zip and file count.
pub fn make_backup(
    sources: &[(String, PathBuf)],
    out_dir: &Path,
) -> Result<(PathBuf, usize), String> {
    use std::io::Write;
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let target = out_dir.join(format!("GameShelf saves {}.zip", timestamp()));
    let tmp = target.with_extension("zip.tmp");
    let file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    let mut count = 0;
    for (name, dir) in sources {
        add_dir(&mut zip, options, dir, name, 0, &mut count)?;
    }
    let readme = "Made by GameShelf. Each folder is one emulator's saves; copy a folder back to restore it.\r\n";
    zip.start_file("README.txt", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(readme.as_bytes())
        .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    Ok((target, count))
}

fn add_dir(
    zip: &mut zip::ZipWriter<fs::File>,
    options: zip::write::SimpleFileOptions,
    dir: &Path,
    prefix: &str,
    depth: u32,
    count: &mut usize,
) -> Result<(), String> {
    if depth > 16 {
        return Ok(());
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        if path.is_dir() {
            add_dir(zip, options, &path, &name, depth + 1, count)?;
        } else {
            let mut src = fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            std::io::copy(&mut src, zip).map_err(|e| e.to_string())?;
            *count += 1;
        }
    }
    Ok(())
}

/// `2026-10-15 1432` (UTC), safe for file names.
fn timestamp() -> String {
    let secs = crate::library::now_secs();
    let days = (secs / 86_400) as i64;
    let (h, m) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02} {h:02}{m:02}")
}

// ---------------------------------------------------------------------------
// Restore: put a backup zip back where each emulator keeps its saves today.
// ---------------------------------------------------------------------------

/// One folder of the backup and where it goes back to.
pub struct RestoreItem {
    pub name: String,
    pub target: PathBuf,
    pub files: usize,
}

pub struct RestorePlan {
    pub zip: PathBuf,
    pub items: Vec<RestoreItem>,
    /// Backup folders with no matching emulator set up now (name, file count).
    pub skipped: Vec<(String, usize)>,
}

/// The backup folder (e.g. `RetroArch/Saves`) an entry belongs to, and the rest of its path.
fn split_entry(entry: &Path) -> Option<(String, PathBuf)> {
    let parts: Vec<String> = entry
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    // `Emulator/Folder name/rest…`
    let (emu, folder) = (parts.first()?, parts.get(1)?);
    let name = format!("{emu}/{folder}");
    let rest: PathBuf = parts.iter().skip(2).collect();
    (rest.components().count() > 0).then_some((name, rest))
}

/// Reads a backup and matches each of its folders to today's save folders.
pub fn plan_restore(zip_path: &Path, sources: &[(String, PathBuf)]) -> Result<RestorePlan, String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Not a GameShelf backup: {e}"))?;
    let mut items: Vec<RestoreItem> = Vec::new();
    let mut skipped: Vec<(String, usize)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let Some((name, _)) = split_entry(&path) else {
            continue;
        };
        match sources.iter().find(|(n, _)| *n == name) {
            Some((_, target)) => match items.iter_mut().find(|it| it.name == name) {
                Some(it) => it.files += 1,
                None => items.push(RestoreItem {
                    name,
                    target: target.clone(),
                    files: 1,
                }),
            },
            None => match skipped.iter_mut().find(|(n, _)| *n == name) {
                Some((_, count)) => *count += 1,
                None => skipped.push((name, 1)),
            },
        }
    }
    if items.is_empty() && skipped.is_empty() {
        return Err("This zip doesn't look like a GameShelf backup.".into());
    }
    Ok(RestorePlan {
        zip: zip_path.to_path_buf(),
        items,
        skipped,
    })
}

/// Writes the planned folders back. Files in the backup replace the current ones;
/// other current files are left alone. Returns how many files were restored.
pub fn restore(plan: &RestorePlan) -> Result<usize, String> {
    let file = fs::File::open(&plan.zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut restored = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // `enclosed_name` rejects `..` and absolute paths, so nothing escapes the target.
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let Some((name, rest)) = split_entry(&path) else {
            continue;
        };
        let Some(item) = plan.items.iter().find(|it| it.name == name) else {
            continue;
        };
        let dest = item.target.join(&rest);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = dest.with_file_name(format!(
            "{}.gameshelf-restore",
            dest.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
        let mut out = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", dest.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        drop(out);
        fs::rename(&tmp, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        restored += 1;
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_then_restore_brings_saves_back() {
        let base = std::env::temp_dir().join(format!("gameshelf-restore-{}", std::process::id()));
        let saves = base.join("saves");
        fs::create_dir_all(saves.join("mGBA")).unwrap();
        fs::write(saves.join("mGBA/Game.srm"), b"old progress").unwrap();
        let sources = vec![("RetroArch/Saves".to_string(), saves.clone())];
        let (zip_path, _) = make_backup(&sources, &base.join("Backups")).unwrap();

        // Progress gets lost or overwritten…
        fs::write(saves.join("mGBA/Game.srm"), b"broken").unwrap();
        fs::write(saves.join("mGBA/Other.srm"), b"keep me").unwrap();

        // …and a backup for an emulator that isn't set up any more is reported, not written.
        let plan = plan_restore(&zip_path, &sources).unwrap();
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].files, 1);
        assert!(plan.skipped.is_empty());
        assert_eq!(restore(&plan).unwrap(), 1);
        assert_eq!(
            fs::read(saves.join("mGBA/Game.srm")).unwrap(),
            b"old progress"
        );
        assert_eq!(fs::read(saves.join("mGBA/Other.srm")).unwrap(), b"keep me");

        let none: Vec<(String, PathBuf)> = Vec::new();
        let plan = plan_restore(&zip_path, &none).unwrap();
        assert!(plan.items.is_empty());
        assert_eq!(plan.skipped, vec![("RetroArch/Saves".to_string(), 1)]);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn finds_links_and_backs_up_standalone_saves() {
        let base = std::env::temp_dir().join(format!("gameshelf-emusaves-{}", std::process::id()));
        let emu = base.join("RPCS3");
        let savedata = emu.join("dev_hdd0/home/00000001/savedata/BLUS00001");
        fs::create_dir_all(&savedata).unwrap();
        fs::write(savedata.join("SAVE.DAT"), b"progress").unwrap();
        let saves = standalone_saves("RPCS3", &emu.join("rpcs3.exe"));
        assert_eq!(saves.folders.len(), 1, "only folders that exist");

        let root = base.join("GameShelf");
        assert_eq!(link_all(&root, std::slice::from_ref(&saves)).unwrap(), 1);
        let link = link_path(&root, "RPCS3", Kind::Saves);
        assert!(is_linked(&link, &saves.folders[0].path));
        assert!(
            link.join("BLUS00001/SAVE.DAT").is_file(),
            "saves visible through the link"
        );
        assert_eq!(
            link_all(&root, std::slice::from_ref(&saves)).unwrap(),
            0,
            "linking twice is a no-op"
        );

        let sources = backup_sources(None, std::slice::from_ref(&saves));
        let (zip_path, files) = make_backup(&sources, &base.join("Backups")).unwrap();
        assert_eq!(files, 1);
        assert!(zip_path.is_file());

        // Removing the link leaves the saves alone.
        fs::remove_dir(&link).unwrap();
        assert!(savedata.join("SAVE.DAT").is_file());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn timestamp_is_a_date() {
        let t = timestamp();
        assert_eq!(t.len(), 15);
        assert!(t.starts_with("20"));
    }

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
