//! Keyboard controls for each emulator. For RetroArch they're read from, and
//! changed in, its own `retroarch.cfg`, so the guide always shows the keys in use.

use crate::emukeys;
use eframe::egui;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_CFG_BYTES: u64 = 4 * 1024 * 1024;

pub struct Binding {
    pub action: &'static str,
    /// As printed on the keyboard ("Space", "F2", "↑").
    pub key: String,
    /// RetroArch's own name ("space", "f2", "up").
    pub raw: String,
    /// A key people press by accident (e.g. fast-forward).
    pub careful: bool,
    /// The retroarch.cfg setting, when the key can be changed from GameShelf.
    pub setting: Option<&'static str>,
}

pub struct Controls {
    pub buttons: Vec<Binding>,
    pub hotkeys: Vec<Binding>,
    /// Shown first: the "why is my game fast?" answer.
    pub tip: Option<String>,
    pub where_to_change: &'static str,
    /// Where the keys came from.
    pub source: String,
    /// Keys can be changed from GameShelf.
    pub editable: bool,
}

impl Controls {
    pub fn all(&self) -> impl Iterator<Item = &Binding> {
        self.buttons.iter().chain(&self.hotkeys)
    }
}

pub fn for_emulator(name: &str, exe: Option<&Path>) -> Controls {
    if name != "RetroArch" {
        // Read the emulator's own settings file when it's set up.
        if let Some(controls) = exe.and_then(|exe| emukeys::load(name, exe)) {
            if controls.editable || exe.is_some_and(|e| e.is_file()) {
                return controls;
            }
        }
    }
    match name {
        "RetroArch" => retroarch(exe),
        "DuckStation" => other(
            "Settings › Controllers (keys) and Settings › Hotkeys",
            Some("Tab (hold)"),
        ),
        "PCSX2" => other(
            "Settings › Controllers and Settings › Hotkeys",
            Some("Tab (toggle turbo)"),
        ),
        "PPSSPP" => other("Settings › Controls › Control mapping", Some("Tab (hold)")),
        "Dolphin" => other(
            "Controllers (toolbar) and Options › Hotkey Settings",
            Some("Tab (toggle speed limit)"),
        ),
        "RPCS3" => other("Pads (toolbar)", None),
        "Flycast" => other("Settings › Controls", None),
        _ => other("the emulator's settings", None),
    }
}

fn other(where_to_change: &'static str, fast_forward: Option<&str>) -> Controls {
    let hotkeys = fast_forward
        .map(|key| {
            vec![Binding {
                action: "Fast-forward",
                key: key.to_string(),
                raw: String::new(),
                careful: true,
                setting: None,
            }]
        })
        .unwrap_or_default();
    let tip = fast_forward.map(|key| {
        format!("Game suddenly fast? That's fast-forward, usually {key}. Press it again or let go.")
    });
    Controls {
        buttons: Vec::new(),
        hotkeys,
        tip,
        where_to_change,
        source: "This emulator's usual defaults. Set your keys or gamepad in its own settings."
            .into(),
        editable: false,
    }
}

/// One RetroArch keyboard setting GameShelf shows (and can change).
struct RaKey {
    action: &'static str,
    setting: &'static str,
    default: &'static str,
    careful: bool,
    button: bool,
}

const fn ra(
    action: &'static str,
    setting: &'static str,
    default: &'static str,
    careful: bool,
    button: bool,
) -> RaKey {
    RaKey {
        action,
        setting,
        default,
        careful,
        button,
    }
}

/// RetroArch's own defaults.
const RETROARCH_KEYS: &[RaKey] = &[
    ra("Up", "input_player1_up", "up", false, true),
    ra("Down", "input_player1_down", "down", false, true),
    ra("Left", "input_player1_left", "left", false, true),
    ra("Right", "input_player1_right", "right", false, true),
    ra("A", "input_player1_a", "x", false, true),
    ra("B", "input_player1_b", "z", false, true),
    ra("X", "input_player1_x", "s", false, true),
    ra("Y", "input_player1_y", "a", false, true),
    ra("L", "input_player1_l", "q", false, true),
    ra("R", "input_player1_r", "w", false, true),
    ra("Start", "input_player1_start", "enter", false, true),
    ra("Select", "input_player1_select", "rshift", false, true),
    ra("Menu", "input_menu_toggle", "f1", false, false),
    ra(
        "Fast-forward (on/off)",
        "input_toggle_fast_forward",
        "space",
        true,
        false,
    ),
    ra(
        "Fast-forward (hold)",
        "input_hold_fast_forward",
        "l",
        true,
        false,
    ),
    ra(
        "Slow motion (hold)",
        "input_hold_slowmotion",
        "e",
        true,
        false,
    ),
    ra("Pause", "input_pause_toggle", "p", false, false),
    ra("Save state", "input_save_state", "f2", false, false),
    ra("Load state", "input_load_state", "f4", false, false),
    ra(
        "State slot −",
        "input_state_slot_decrease",
        "f6",
        false,
        false,
    ),
    ra(
        "State slot +",
        "input_state_slot_increase",
        "f7",
        false,
        false,
    ),
    ra("Reset game", "input_reset", "h", false, false),
    ra("Screenshot", "input_screenshot", "f8", false, false),
    ra("Fullscreen", "input_toggle_fullscreen", "f", false, false),
    ra("Mute", "input_audio_mute", "f9", false, false),
    ra(
        "Quit (press twice)",
        "input_exit_emulator",
        "escape",
        false,
        false,
    ),
];

pub fn retroarch_cfg(exe: &Path) -> Option<PathBuf> {
    exe.parent().map(|d| d.join("retroarch.cfg"))
}

fn retroarch(exe: Option<&Path>) -> Controls {
    let cfg_path = exe.and_then(retroarch_cfg);
    let cfg = cfg_path.as_deref().map(read_cfg).unwrap_or_default();
    let source = match &cfg_path {
        Some(p) if !cfg.is_empty() => format!("Read from {}", p.display()),
        _ => "RetroArch's default keys (its settings file wasn't found).".into(),
    };
    let (mut buttons, mut hotkeys) = (Vec::new(), Vec::new());
    for k in RETROARCH_KEYS {
        let raw = cfg
            .get(k.setting)
            .cloned()
            .unwrap_or_else(|| k.default.to_string());
        let binding = Binding {
            action: k.action,
            key: pretty_key(&raw),
            raw,
            careful: k.careful,
            setting: exe.is_some().then_some(k.setting),
        };
        if k.button {
            buttons.push(binding);
        } else {
            hotkeys.push(binding);
        }
    }
    let ff = hotkeys
        .iter()
        .find(|b| {
            b.setting == Some("input_toggle_fast_forward")
                || b.action.starts_with("Fast-forward (on")
        })
        .map(|b| b.key.clone())
        .unwrap_or_else(|| "Space".into());
    Controls {
        buttons,
        hotkeys,
        tip: Some(format!(
            "Game suddenly fast? You pressed {ff} (fast-forward). Press {ff} again for normal speed."
        )),
        where_to_change: "here (click a key), or RetroArch: F1 › Settings › Input",
        source,
        editable: exe.is_some(),
    }
}

/// Every key GameShelf manages, back to RetroArch's defaults.
pub fn retroarch_defaults() -> Vec<(&'static str, String)> {
    RETROARCH_KEYS
        .iter()
        .map(|k| (k.setting, k.default.to_string()))
        .collect()
}

/// `key = "value"` lines of retroarch.cfg.
fn read_cfg(path: &Path) -> HashMap<String, String> {
    let Some(text) = read_small(path) else {
        return HashMap::new();
    };
    text.lines()
        .filter_map(|line| {
            let (k, v) = line.split_once('=')?;
            let k = k.trim();
            k.starts_with("input_")
                .then(|| (k.to_string(), v.trim().trim_matches('"').to_string()))
        })
        .collect()
}

fn read_small(path: &Path) -> Option<String> {
    let small = fs::metadata(path).is_ok_and(|m| m.len() <= MAX_CFG_BYTES);
    small.then(|| fs::read_to_string(path).ok()).flatten()
}

/// Writes key settings into retroarch.cfg, keeping every other line as it was.
/// The first time, the original is kept as `retroarch.cfg.gameshelf-backup`.
pub fn write_retroarch(exe: &Path, changes: &[(&str, String)]) -> Result<(), String> {
    let path = retroarch_cfg(exe).ok_or("RetroArch's folder wasn't found")?;
    let text = if path.exists() {
        read_small(&path).ok_or("Could not read retroarch.cfg")?
    } else {
        String::new()
    };
    let backup = path.with_extension("cfg.gameshelf-backup");
    if path.exists() && !backup.exists() {
        fs::copy(&path, &backup).map_err(|e| format!("Could not back up retroarch.cfg: {e}"))?;
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for (setting, value) in changes {
        let line = format!("{setting} = \"{value}\"");
        let existing = lines
            .iter_mut()
            .find(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == *setting));
        match existing {
            Some(l) => *l = line,
            None => lines.push(line),
        }
    }
    let mut out = lines.join(newline);
    out.push_str(newline);
    let tmp = path.with_extension("cfg.tmp");
    fs::write(&tmp, out).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Changes one key of any emulator. If another action already used that key,
/// the two swap. Returns a short description of what changed.
pub fn set_key(
    emulator: &str,
    exe: &Path,
    id: &'static str,
    key: egui::Key,
) -> Result<String, String> {
    if emulator != "RetroArch" {
        return emukeys::set_key(emulator, exe, id, key);
    }
    let raw = retroarch_key_name(key).ok_or("That key can't be used here. Try another one.")?;
    let controls = retroarch(Some(exe));
    let current = controls.all().find(|b| b.setting == Some(id));
    let action = current.map_or("Key", |b| b.action);
    let old = current.map(|b| b.raw.clone());
    let mut changes = vec![(id, raw.clone())];
    let mut swapped = None;
    let clash = controls
        .all()
        .find(|b| b.setting.is_some() && b.setting != Some(id) && b.raw.eq_ignore_ascii_case(&raw));
    if let (Some(other), Some(old)) = (clash, old) {
        if let Some(other_setting) = other.setting {
            changes.push((other_setting, old.clone()));
            swapped = Some((other.action, pretty_key(&old)));
        }
    }
    write_retroarch(exe, &changes)?;
    let mut msg = format!("✔ {action} → {}", pretty_key(&raw));
    if let Some((other, key)) = swapped {
        msg.push_str(&format!("   ·   {other} moved to {key}"));
    }
    Ok(msg)
}

/// Whether there's a backup from before GameShelf's first key change.
pub fn has_backup(emulator: &str, exe: &Path) -> bool {
    if emulator == "RetroArch" {
        retroarch_cfg(exe).is_some_and(|p| p.with_extension("cfg.gameshelf-backup").is_file())
    } else {
        emukeys::has_backup(emulator, exe)
    }
}

/// Puts the settings back as they were before GameShelf changed any key.
pub fn restore_backup(emulator: &str, exe: &Path) -> Result<(), String> {
    if emulator != "RetroArch" {
        return emukeys::restore_backup(emulator, exe).map(|_| ());
    }
    let path = retroarch_cfg(exe).ok_or("RetroArch's folder wasn't found")?;
    let backup = path.with_extension("cfg.gameshelf-backup");
    fs::copy(&backup, &path).map_err(|e| e.to_string())?;
    fs::remove_file(&backup).map_err(|e| e.to_string())
}

/// Is this emulator open? Emulators save their settings when they close, which
/// would undo a change made while they run.
pub fn is_running(exe: &Path) -> bool {
    let Some(name) = exe.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let mut cmd = std::process::Command::new("tasklist");
    cmd.args(["/FI", &format!("IMAGENAME eq {name}"), "/NH"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains(&name.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

/// egui's key → RetroArch's name for it. Modifier keys alone (Shift, Ctrl, Alt)
/// don't arrive as key presses, so they can't be picked this way.
pub fn retroarch_key_name(key: egui::Key) -> Option<String> {
    use egui::Key as K;
    let name = match key {
        K::ArrowUp => "up",
        K::ArrowDown => "down",
        K::ArrowLeft => "left",
        K::ArrowRight => "right",
        K::Enter => "enter",
        K::Space => "space",
        K::Tab => "tab",
        K::Backspace => "backspace",
        K::Insert => "insert",
        K::Delete => "del",
        K::Home => "home",
        K::End => "end",
        K::PageUp => "pageup",
        K::PageDown => "pagedown",
        K::Minus => "minus",
        K::Equals => "equals",
        K::Comma => "comma",
        K::Period => "period",
        K::Slash => "slash",
        K::Backslash => "backslash",
        K::Semicolon => "semicolon",
        K::Quote => "quote",
        K::OpenBracket => "leftbracket",
        K::CloseBracket => "rightbracket",
        K::Backtick => "tilde",
        K::Num0 => "num0",
        K::Num1 => "num1",
        K::Num2 => "num2",
        K::Num3 => "num3",
        K::Num4 => "num4",
        K::Num5 => "num5",
        K::Num6 => "num6",
        K::Num7 => "num7",
        K::Num8 => "num8",
        K::Num9 => "num9",
        other => {
            let name = other.name();
            // Letters and F1–F12 map to their lowercase names.
            let letter = name.len() == 1 && name.chars().all(|c| c.is_ascii_alphabetic());
            let function = name.starts_with('F')
                && name.len() <= 3
                && name.chars().skip(1).all(|c| c.is_ascii_digit())
                && name.len() > 1;
            if letter || function {
                return Some(name.to_ascii_lowercase());
            }
            return None;
        }
    };
    Some(name.to_string())
}

/// RetroArch key names → what's printed on the keyboard.
pub fn pretty_key(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "nul" | "" => "—".to_string(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "enter" => "Enter".into(),
        "space" => "Space".into(),
        "escape" => "Esc".into(),
        "rshift" => "Right Shift".into(),
        "shift" | "lshift" => "Shift".into(),
        "rctrl" => "Right Ctrl".into(),
        "ctrl" | "lctrl" => "Ctrl".into(),
        "tab" => "Tab".into(),
        "backspace" => "Backspace".into(),
        "del" => "Delete".into(),
        "pageup" => "Page Up".into(),
        "pagedown" => "Page Down".into(),
        "leftbracket" => "[".into(),
        "rightbracket" => "]".into(),
        "minus" => "-".into(),
        "equals" => "=".into(),
        "comma" => ",".into(),
        "period" => ".".into(),
        "slash" => "/".into(),
        "backslash" => "\\".into(),
        "semicolon" => ";".into(),
        "quote" => "'".into(),
        "tilde" => "`".into(),
        k if k.starts_with("num") && k.len() == 4 => k[3..].to_string(),
        k if k.chars().count() == 1 => k.to_ascii_uppercase(),
        k if k.starts_with('f')
            && k.len() <= 3
            && k.chars().skip(1).all(|c| c.is_ascii_digit()) =>
        {
            k.to_ascii_uppercase()
        }
        k => {
            let mut c = k.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_keys() {
        assert_eq!(pretty_key("space"), "Space");
        assert_eq!(pretty_key("f2"), "F2");
        assert_eq!(pretty_key("x"), "X");
        assert_eq!(pretty_key("rshift"), "Right Shift");
        assert_eq!(pretty_key("num5"), "5");
        assert_eq!(pretty_key("nul"), "—");
    }

    #[test]
    fn egui_keys_map_to_retroarch_names() {
        assert_eq!(retroarch_key_name(egui::Key::K).as_deref(), Some("k"));
        assert_eq!(retroarch_key_name(egui::Key::F5).as_deref(), Some("f5"));
        assert_eq!(
            retroarch_key_name(egui::Key::Space).as_deref(),
            Some("space")
        );
        assert_eq!(retroarch_key_name(egui::Key::Num3).as_deref(), Some("num3"));
    }

    #[test]
    fn reads_and_writes_retroarch_cfg() {
        let dir = std::env::temp_dir().join(format!("gameshelf-ra-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("retroarch.exe");
        fs::write(
            dir.join("retroarch.cfg"),
            "input_toggle_fast_forward = \"space\"\nvideo_driver = \"gl\"\n",
        )
        .unwrap();

        write_retroarch(
            &exe,
            &[
                ("input_toggle_fast_forward", "tab".into()),
                ("input_player1_a", "k".into()),
            ],
        )
        .unwrap();
        let text = fs::read_to_string(dir.join("retroarch.cfg")).unwrap();
        assert!(text.contains("input_toggle_fast_forward = \"tab\""));
        assert!(
            text.contains("input_player1_a = \"k\""),
            "missing settings are added"
        );
        assert!(
            text.contains("video_driver = \"gl\""),
            "other lines are kept"
        );
        assert!(dir.join("retroarch.cfg.gameshelf-backup").is_file());

        let c = retroarch(Some(&exe));
        let ff = c
            .hotkeys
            .iter()
            .find(|b| b.action.starts_with("Fast-forward (on"))
            .unwrap();
        assert_eq!(ff.key, "Tab");
        assert!(c.tip.unwrap().contains("Tab"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
