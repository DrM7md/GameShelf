//! Keyboard bindings for the standalone emulators (DuckStation, PCSX2, PPSSPP,
//! Dolphin, RPCS3, Flycast), read from and written to each one's own settings file.
//!
//! Safety rules: only lines that already exist are changed (so a key name we got
//! wrong just isn't shown, rather than written), gamepad bindings are left alone,
//! and the original file is backed up before the first change.

use crate::controls::{Binding, Controls};
use eframe::egui::Key;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq)]
enum Format {
    /// DuckStation / PCSX2: `Cross = Keyboard/K` (one line per binding).
    Duck,
    /// PPSSPP: `Cross = 1-39,10-189` (device-keycode list; keyboard is device 1).
    Ppsspp,
    /// Dolphin: `Buttons/A = X` or `` `X` ``.
    Dolphin,
    /// RPCS3 YAML: `    Cross: X` under `Player 1 Input:`.
    Rpcs3,
    /// Flycast: `bind3 = 27:btn_a` (USB scancode : action).
    Flycast,
}

struct Entry {
    action: &'static str,
    section: &'static str,
    key: &'static str,
    careful: bool,
    button: bool,
}

const fn b(action: &'static str, section: &'static str, key: &'static str) -> Entry {
    Entry {
        action,
        section,
        key,
        careful: false,
        button: true,
    }
}
const fn h(action: &'static str, section: &'static str, key: &'static str, careful: bool) -> Entry {
    Entry {
        action,
        section,
        key,
        careful,
        button: false,
    }
}

const PS_PAD: &[Entry] = &[
    b("Up", "Pad1", "Up"),
    b("Down", "Pad1", "Down"),
    b("Left", "Pad1", "Left"),
    b("Right", "Pad1", "Right"),
    b("✕ Cross", "Pad1", "Cross"),
    b("○ Circle", "Pad1", "Circle"),
    b("□ Square", "Pad1", "Square"),
    b("△ Triangle", "Pad1", "Triangle"),
    b("L1", "Pad1", "L1"),
    b("R1", "Pad1", "R1"),
    b("L2", "Pad1", "L2"),
    b("R2", "Pad1", "R2"),
    b("Start", "Pad1", "Start"),
    b("Select", "Pad1", "Select"),
];

const DUCKSTATION_HOTKEYS: &[Entry] = &[
    h("Fast-forward (hold)", "Hotkeys", "FastForward", true),
    h(
        "Fast-forward (on/off)",
        "Hotkeys",
        "ToggleFastForward",
        true,
    ),
    h("Pause menu", "Hotkeys", "OpenPauseMenu", false),
    h("Pause", "Hotkeys", "TogglePause", false),
    h("Save state", "Hotkeys", "SaveSelectedSaveState", false),
    h("Load state", "Hotkeys", "LoadSelectedSaveState", false),
    h("Screenshot", "Hotkeys", "TakeScreenshot", false),
    h("Fullscreen", "Hotkeys", "ToggleFullscreen", false),
];

const PCSX2_HOTKEYS: &[Entry] = &[
    h("Turbo (on/off)", "Hotkeys", "ToggleTurbo", true),
    h("Turbo (hold)", "Hotkeys", "HoldTurbo", true),
    h("Slow motion", "Hotkeys", "ToggleSlowMotion", true),
    h("Pause menu", "Hotkeys", "OpenPauseMenu", false),
    h("Pause", "Hotkeys", "TogglePause", false),
    h("Save state", "Hotkeys", "SaveStateToSlot", false),
    h("Load state", "Hotkeys", "LoadStateFromSlot", false),
    h("Screenshot", "Hotkeys", "Screenshot", false),
    h("Fullscreen", "Hotkeys", "ToggleFullscreen", false),
];

const PPSSPP_KEYS: &[Entry] = &[
    b("Up", "ControlMapping", "Up"),
    b("Down", "ControlMapping", "Down"),
    b("Left", "ControlMapping", "Left"),
    b("Right", "ControlMapping", "Right"),
    b("✕ Cross", "ControlMapping", "Cross"),
    b("○ Circle", "ControlMapping", "Circle"),
    b("□ Square", "ControlMapping", "Square"),
    b("△ Triangle", "ControlMapping", "Triangle"),
    b("L", "ControlMapping", "L"),
    b("R", "ControlMapping", "R"),
    b("Start", "ControlMapping", "Start"),
    b("Select", "ControlMapping", "Select"),
    h("Fast-forward", "ControlMapping", "Fast-forward", true),
    h("Fast-forward", "ControlMapping", "Unthrottle", true),
    h("Pause", "ControlMapping", "Pause", false),
    h("Save state", "ControlMapping", "Save State", false),
    h("Load state", "ControlMapping", "Load State", false),
    h("Screenshot", "ControlMapping", "Screenshot", false),
];

const DOLPHIN_PAD: &[Entry] = &[
    b("A", "GCPad1", "Buttons/A"),
    b("B", "GCPad1", "Buttons/B"),
    b("X", "GCPad1", "Buttons/X"),
    b("Y", "GCPad1", "Buttons/Y"),
    b("Z", "GCPad1", "Buttons/Z"),
    b("Start", "GCPad1", "Buttons/Start"),
    b("D-pad Up", "GCPad1", "D-Pad/Up"),
    b("D-pad Down", "GCPad1", "D-Pad/Down"),
    b("D-pad Left", "GCPad1", "D-Pad/Left"),
    b("D-pad Right", "GCPad1", "D-Pad/Right"),
    b("Stick Up", "GCPad1", "Main Stick/Up"),
    b("Stick Down", "GCPad1", "Main Stick/Down"),
    b("Stick Left", "GCPad1", "Main Stick/Left"),
    b("Stick Right", "GCPad1", "Main Stick/Right"),
    b("L", "GCPad1", "Triggers/L"),
    b("R", "GCPad1", "Triggers/R"),
];

const DOLPHIN_HOTKEYS: &[Entry] = &[
    h(
        "Unlimited speed",
        "Hotkeys",
        "Emulation Speed/Disable Emulation Speed Limit",
        true,
    ),
    h("Pause", "Hotkeys", "General/Toggle Pause", false),
    h("Fullscreen", "Hotkeys", "General/Toggle Fullscreen", false),
    h("Screenshot", "Hotkeys", "General/Take Screenshot", false),
    h(
        "Save state",
        "Hotkeys",
        "Save State/Save to Selected Slot",
        false,
    ),
    h(
        "Load state",
        "Hotkeys",
        "Load State/Load from Selected Slot",
        false,
    ),
];

const RPCS3_KEYS: &[Entry] = &[
    b("Up", "Player 1 Input", "Up"),
    b("Down", "Player 1 Input", "Down"),
    b("Left", "Player 1 Input", "Left"),
    b("Right", "Player 1 Input", "Right"),
    b("✕ Cross", "Player 1 Input", "Cross"),
    b("○ Circle", "Player 1 Input", "Circle"),
    b("□ Square", "Player 1 Input", "Square"),
    b("△ Triangle", "Player 1 Input", "Triangle"),
    b("L1", "Player 1 Input", "L1"),
    b("R1", "Player 1 Input", "R1"),
    b("L2", "Player 1 Input", "L2"),
    b("R2", "Player 1 Input", "R2"),
    b("Start", "Player 1 Input", "Start"),
    b("Select", "Player 1 Input", "Select"),
    b("PS button", "Player 1 Input", "PS Button"),
];

const FLYCAST_KEYS: &[Entry] = &[
    b("Up", "digital", "btn_dpad1_up"),
    b("Down", "digital", "btn_dpad1_down"),
    b("Left", "digital", "btn_dpad1_left"),
    b("Right", "digital", "btn_dpad1_right"),
    b("A", "digital", "btn_a"),
    b("B", "digital", "btn_b"),
    b("X", "digital", "btn_x"),
    b("Y", "digital", "btn_y"),
    b("L trigger", "digital", "btn_trigger_left"),
    b("R trigger", "digital", "btn_trigger_right"),
    b("Start", "digital", "btn_start"),
    h("Fast-forward", "digital", "btn_fforward", true),
    h("Menu", "digital", "btn_menu", false),
    h("Quit", "digital", "btn_escape", false),
];

/// One settings file and the bindings it holds.
struct FileSpec {
    /// Possible locations, most likely first; the first that exists is used.
    candidates: Vec<PathBuf>,
    format: Format,
    entries: &'static [&'static [Entry]],
}

struct Spec {
    files: Vec<FileSpec>,
    where_to_change: &'static str,
    first_run_hint: &'static str,
}

fn spec(emulator: &str, exe: &Path) -> Option<Spec> {
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let docs = directories::UserDirs::new().and_then(|u| u.document_dir().map(Path::to_path_buf));
    let base = directories::BaseDirs::new();
    let roaming = base.as_ref().map(|b| b.config_dir().to_path_buf());
    let local = base.as_ref().map(|b| b.data_local_dir().to_path_buf());
    let under = |root: &Option<PathBuf>, rel: &str| root.as_ref().map(|r| r.join(rel));
    let list = |paths: Vec<Option<PathBuf>>| paths.into_iter().flatten().collect::<Vec<_>>();

    Some(match emulator {
        "DuckStation" => Spec {
            files: vec![FileSpec {
                candidates: list(vec![
                    Some(dir.join("settings.ini")),
                    under(&local, "DuckStation/settings.ini"),
                    under(&docs, "DuckStation/settings.ini"),
                ]),
                format: Format::Duck,
                entries: &[PS_PAD, DUCKSTATION_HOTKEYS],
            }],
            where_to_change: "here, or DuckStation: Settings › Controllers / Hotkeys",
            first_run_hint: "Open DuckStation once (and close it) so it creates its settings file.",
        },
        "PCSX2" => Spec {
            files: vec![FileSpec {
                candidates: list(vec![
                    Some(dir.join("inis/PCSX2.ini")),
                    under(&docs, "PCSX2/inis/PCSX2.ini"),
                ]),
                format: Format::Duck,
                entries: &[PS_PAD, PCSX2_HOTKEYS],
            }],
            where_to_change: "here, or PCSX2: Settings › Controllers / Hotkeys",
            first_run_hint: "Open PCSX2 once and finish its setup so it creates its settings file.",
        },
        "PPSSPP" => Spec {
            files: vec![FileSpec {
                candidates: list(vec![
                    Some(dir.join("memstick/PSP/SYSTEM/controls.ini")),
                    under(&docs, "PPSSPP/PSP/SYSTEM/controls.ini"),
                ]),
                format: Format::Ppsspp,
                entries: &[PPSSPP_KEYS],
            }],
            where_to_change: "here, or PPSSPP: Settings › Controls › Control mapping",
            first_run_hint: "In PPSSPP, open Settings › Controls › Control mapping once so it saves controls.ini.",
        },
        "Dolphin" => {
            let config = |file: &str| {
                list(vec![
                    Some(dir.join("User/Config").join(file)),
                    under(&roaming, "Dolphin Emulator/Config").map(|p| p.join(file)),
                    under(&docs, "Dolphin Emulator/Config").map(|p| p.join(file)),
                ])
            };
            Spec {
                files: vec![
                    FileSpec { candidates: config("GCPadNew.ini"), format: Format::Dolphin, entries: &[DOLPHIN_PAD] },
                    FileSpec { candidates: config("Hotkeys.ini"), format: Format::Dolphin, entries: &[DOLPHIN_HOTKEYS] },
                ],
                where_to_change: "here, or Dolphin: Controllers / Options › Hotkey Settings",
                first_run_hint: "In Dolphin, open Controllers, set Port 1 to Standard Controller and press Configure once.",
            }
        }
        "RPCS3" => Spec {
            files: vec![FileSpec {
                candidates: vec![
                    dir.join("config/input_configs/global/Default.yml"),
                    dir.join("input_configs/global/Default.yml"),
                ],
                format: Format::Rpcs3,
                entries: &[RPCS3_KEYS],
            }],
            where_to_change: "here, or RPCS3: Pads (toolbar)",
            first_run_hint: "In RPCS3, open Pads, pick Keyboard for Player 1 and press Save once.",
        },
        "Flycast" => Spec {
            files: vec![FileSpec {
                candidates: list(vec![
                    Some(dir.join("mappings/SDL_Keyboard.cfg")),
                    under(&roaming, "flycast/mappings/SDL_Keyboard.cfg"),
                ]),
                format: Format::Flycast,
                entries: &[FLYCAST_KEYS],
            }],
            where_to_change: "here, or Flycast: Settings › Controls › Keyboard › Map",
            first_run_hint: "In Flycast, open Settings › Controls and map the keyboard once so it saves its mapping file.",
        },
        _ => return None,
    })
}

/// Where a binding lives in a file, and its current keyboard value.
struct Found {
    line: usize,
    /// The keyboard part only: `Keyboard/K`, `39` (PPSSPP keycode), `` `X` ``, `X`, `27`.
    value: String,
}

fn read_lines(path: &Path) -> Option<Vec<String>> {
    let small = fs::metadata(path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES);
    let text = small.then(|| fs::read_to_string(path).ok()).flatten()?;
    Some(text.lines().map(str::to_string).collect())
}

/// Line range of an INI `[section]` (exclusive of the header).
fn ini_section(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let header = format!("[{section}]");
    let start = lines
        .iter()
        .position(|l| l.trim().eq_ignore_ascii_case(&header))?
        + 1;
    let end = lines
        .iter()
        .skip(start)
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |p| start + p);
    Some((start, end))
}

/// Line range of an RPCS3 `Player 1 Input:` block.
fn yaml_block(lines: &[String], name: &str) -> Option<(usize, usize)> {
    let header = format!("{name}:");
    let start = lines.iter().position(|l| l.trim_end() == header)? + 1;
    let end = lines
        .iter()
        .skip(start)
        .position(|l| !l.is_empty() && !l.starts_with(' '))
        .map_or(lines.len(), |p| start + p);
    Some((start, end))
}

fn find(lines: &[String], format: Format, e: &Entry) -> Option<Found> {
    let (start, end) = match format {
        Format::Rpcs3 => yaml_block(lines, e.section)?,
        _ => ini_section(lines, e.section)?,
    };
    let block = lines.get(start..end)?;
    if format == Format::Dolphin && e.section.starts_with("GCPad") {
        // Only a keyboard-driven controller has keyboard keys to change.
        let keyboard = block.iter().any(|l| {
            l.split_once('=').is_some_and(|(k, v)| {
                k.trim() == "Device" && v.to_ascii_lowercase().contains("keyboard")
            })
        });
        if !keyboard {
            return None;
        }
    }
    if format == Format::Rpcs3 {
        let keyboard = block.iter().any(|l| l.trim() == "Handler: Keyboard");
        if !keyboard {
            return None;
        }
    }
    block.iter().enumerate().find_map(|(i, line)| {
        let line_no = start + i;
        match format {
            Format::Duck => {
                let (k, v) = line.split_once('=')?;
                let v = v.trim();
                (k.trim() == e.key && v.starts_with("Keyboard/")).then(|| Found {
                    line: line_no,
                    value: v.to_string(),
                })
            }
            Format::Dolphin => {
                let (k, v) = line.split_once('=')?;
                (k.trim() == e.key).then(|| Found {
                    line: line_no,
                    value: v.trim().to_string(),
                })
            }
            Format::Ppsspp => {
                let (k, v) = line.split_once('=')?;
                if k.trim() != e.key {
                    return None;
                }
                let keyboard = v
                    .split(',')
                    .find_map(|item| item.trim().strip_prefix("1-").map(str::to_string));
                Some(Found {
                    line: line_no,
                    value: keyboard.unwrap_or_default(),
                })
            }
            Format::Rpcs3 => {
                let rest = line.trim_start().strip_prefix(e.key)?.strip_prefix(':')?;
                Some(Found {
                    line: line_no,
                    value: rest.trim().to_string(),
                })
            }
            Format::Flycast => {
                let (_, v) = line.split_once('=')?;
                let (code, action) = v.trim().split_once(':')?;
                (action.trim() == e.key).then(|| Found {
                    line: line_no,
                    value: code.trim().to_string(),
                })
            }
        }
    })
}

/// The new text of a line, with `value` (already encoded) as its keyboard binding.
fn rewrite(line: &str, format: Format, e: &Entry, value: &str) -> String {
    match format {
        Format::Duck => format!("{} = {value}", e.key),
        Format::Dolphin => {
            let old = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
            let quoted = old.starts_with('`');
            if quoted {
                format!("{} = `{value}`", e.key)
            } else {
                format!("{} = {value}", e.key)
            }
        }
        Format::Ppsspp => {
            let old = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
            let mut items: Vec<String> = old
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && !s.starts_with("1-"))
                .collect();
            items.insert(0, format!("1-{value}"));
            format!("{} = {}", e.key, items.join(","))
        }
        Format::Rpcs3 => {
            let indent: String = line.chars().take_while(|c| *c == ' ').collect();
            format!("{indent}{}: {value}", e.key)
        }
        Format::Flycast => {
            let name = line
                .split_once('=')
                .map(|(k, _)| k.trim())
                .unwrap_or("bind0");
            format!("{name} = {value}:{}", e.key)
        }
    }
}

/// Keys that can be picked (modifiers alone and Esc can't).
const PICKABLE: &[Key] = &[
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
    Key::Num0,
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
    Key::ArrowUp,
    Key::ArrowDown,
    Key::ArrowLeft,
    Key::ArrowRight,
    Key::Enter,
    Key::Space,
    Key::Tab,
    Key::Backspace,
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
];

/// `A`, `1`, `Up`, `Return`, `F1`: the names Qt-based emulators use.
fn qt_name(key: Key) -> Option<String> {
    Some(match key {
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::Enter => "Return".into(),
        Key::Space => "Space".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "Backspace".into(),
        k => {
            let name = k.name();
            if let Some(digit) = name.strip_prefix("Num") {
                digit.to_string()
            } else {
                name.to_string()
            }
        }
    })
}

/// DirectInput names Dolphin uses on Windows.
fn dinput_name(key: Key) -> Option<String> {
    Some(match key {
        Key::ArrowUp => "UP".into(),
        Key::ArrowDown => "DOWN".into(),
        Key::ArrowLeft => "LEFT".into(),
        Key::ArrowRight => "RIGHT".into(),
        Key::Enter => "RETURN".into(),
        Key::Space => "SPACE".into(),
        Key::Tab => "TAB".into(),
        Key::Backspace => "BACK".into(),
        k => qt_name(k)?.to_ascii_uppercase(),
    })
}

/// Android key codes, which PPSSPP uses for the keyboard too.
fn android_code(key: Key) -> Option<u32> {
    let letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let name = key.name();
    if name.len() == 1 {
        if let Some(i) = letters.find(name) {
            return Some(29 + i as u32);
        }
    }
    if let Some(d) = name.strip_prefix("Num").and_then(|d| d.parse::<u32>().ok()) {
        return Some(7 + d);
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=12).contains(&n).then_some(130 + n);
    }
    Some(match key {
        Key::ArrowUp => 19,
        Key::ArrowDown => 20,
        Key::ArrowLeft => 21,
        Key::ArrowRight => 22,
        Key::Enter => 66,
        Key::Space => 62,
        Key::Tab => 61,
        Key::Backspace => 67,
        _ => return None,
    })
}

/// USB HID scancodes (what SDL, and so Flycast, stores).
fn scancode(key: Key) -> Option<u32> {
    let letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let name = key.name();
    if name.len() == 1 {
        if let Some(i) = letters.find(name) {
            return Some(4 + i as u32);
        }
    }
    if let Some(d) = name.strip_prefix("Num").and_then(|d| d.parse::<u32>().ok()) {
        return Some(if d == 0 { 39 } else { 29 + d });
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=12).contains(&n).then_some(57 + n);
    }
    Some(match key {
        Key::Enter => 40,
        Key::Backspace => 42,
        Key::Tab => 43,
        Key::Space => 44,
        Key::ArrowRight => 79,
        Key::ArrowLeft => 80,
        Key::ArrowDown => 81,
        Key::ArrowUp => 82,
        _ => return None,
    })
}

/// A key in a file's own notation.
fn encode(format: Format, key: Key) -> Option<String> {
    if !PICKABLE.contains(&key) {
        return None;
    }
    match format {
        Format::Duck => qt_name(key).map(|n| format!("Keyboard/{n}")),
        Format::Rpcs3 => qt_name(key),
        Format::Dolphin => dinput_name(key),
        Format::Ppsspp => android_code(key).map(|c| c.to_string()),
        Format::Flycast => scancode(key).map(|c| c.to_string()),
    }
}

/// A stored value, as printed on the keyboard.
fn pretty(format: Format, value: &str) -> String {
    let bare = value.trim().trim_matches('`');
    if bare.is_empty() {
        return "—".into();
    }
    // Numbers (PPSSPP, Flycast): find the key with that code.
    if matches!(format, Format::Ppsspp | Format::Flycast) {
        return PICKABLE
            .iter()
            .find(|k| encode(format, **k).as_deref() == Some(bare))
            .map_or_else(|| format!("#{bare}"), |k| key_label(*k));
    }
    let name = bare.strip_prefix("Keyboard/").unwrap_or(bare);
    match name.to_ascii_uppercase().as_str() {
        "UP" => "↑".into(),
        "DOWN" => "↓".into(),
        "LEFT" => "←".into(),
        "RIGHT" => "→".into(),
        "RETURN" | "ENTER" => "Enter".into(),
        "SPACE" => "Space".into(),
        "TAB" => "Tab".into(),
        "BACK" | "BACKSPACE" => "Backspace".into(),
        "ESCAPE" => "Esc".into(),
        _ if name.chars().count() == 1 => name.to_ascii_uppercase(),
        _ => name.to_string(),
    }
}

fn key_label(key: Key) -> String {
    match key {
        Key::ArrowUp => "↑".into(),
        Key::ArrowDown => "↓".into(),
        Key::ArrowLeft => "←".into(),
        Key::ArrowRight => "→".into(),
        k => {
            let name = k.name();
            name.strip_prefix("Num").unwrap_or(name).to_string()
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.gameshelf-backup"))
}

/// The settings files of `emulator` that exist, with their spec.
fn existing_files(spec: &Spec) -> Vec<(PathBuf, &FileSpec)> {
    spec.files
        .iter()
        .filter_map(|f| {
            f.candidates
                .iter()
                .find(|p| p.is_file())
                .map(|p| (p.clone(), f))
        })
        .collect()
}

/// The emulator's controls, or `None` for emulators this module doesn't handle.
pub fn load(emulator: &str, exe: &Path) -> Option<Controls> {
    let spec = spec(emulator, exe)?;
    let files = existing_files(&spec);
    let (mut buttons, mut hotkeys) = (Vec::new(), Vec::new());
    for (path, file) in &files {
        let Some(lines) = read_lines(path) else {
            continue;
        };
        for entry in file.entries.iter().flat_map(|group| group.iter()) {
            let Some(found) = find(&lines, file.format, entry) else {
                continue;
            };
            let binding = Binding {
                action: entry.action,
                key: pretty(file.format, &found.value),
                raw: found.value.trim().trim_matches('`').to_ascii_lowercase(),
                careful: entry.careful,
                setting: Some(entry.key),
            };
            if entry.button {
                buttons.push(binding);
            } else {
                hotkeys.push(binding);
            }
        }
    }
    let found_any = !buttons.is_empty() || !hotkeys.is_empty();
    let fast = hotkeys
        .iter()
        .find(|b| b.careful)
        .map(|b| (b.action, b.key.clone()));
    let tip = fast.map(|(action, key)| {
        format!("Game suddenly fast? That's {action}: {key}. Press it again or let go for normal speed.")
    });
    let source = match (files.first(), found_any) {
        (Some((path, _)), true) => format!("Read from {}", path.display()),
        (Some(_), false) => format!(
            "Its settings file has no keyboard keys yet. {}",
            spec.first_run_hint
        ),
        (None, _) => format!("Settings file not found. {}", spec.first_run_hint),
    };
    Some(Controls {
        buttons,
        hotkeys,
        tip,
        where_to_change: spec.where_to_change,
        source,
        editable: found_any,
    })
}

/// Sets one binding to `key`. If another action used that key, they swap.
/// Returns a short description of what changed.
pub fn set_key(emulator: &str, exe: &Path, id: &str, key: Key) -> Result<String, String> {
    let spec = spec(emulator, exe).ok_or("This emulator isn't supported")?;
    for (path, file) in existing_files(&spec) {
        let entries: Vec<&Entry> = file.entries.iter().flat_map(|g| g.iter()).collect();
        let Some(entry) = entries.iter().find(|e| e.key == id) else {
            continue;
        };
        let mut lines = read_lines(&path).ok_or("Could not read the settings file")?;
        let target = find(&lines, file.format, entry)
            .ok_or("That key isn't in the settings file any more")?;
        let new_value =
            encode(file.format, key).ok_or("That key can't be used here. Try another one.")?;

        // Swap with whatever already uses the new key.
        let mut swapped = None;
        let same = |v: &str| v.trim().trim_matches('`').eq_ignore_ascii_case(&new_value);
        for other in &entries {
            if other.key == entry.key {
                continue;
            }
            if let Some(found) = find(&lines, file.format, other).filter(|f| same(&f.value)) {
                if !target.value.is_empty() {
                    let old = target.value.trim().trim_matches('`').to_string();
                    if let Some(line) = lines.get_mut(found.line) {
                        *line = rewrite(line, file.format, other, &old);
                    }
                    swapped = Some((other.action, pretty(file.format, &old)));
                }
            }
        }
        if let Some(line) = lines.get_mut(target.line) {
            *line = rewrite(line, file.format, entry, &new_value);
        }
        write_lines(&path, &lines)?;

        let mut msg = format!("✔ {} → {}", entry.action, pretty(file.format, &new_value));
        if let Some((other, old)) = swapped {
            msg.push_str(&format!("   ·   {other} moved to {old}"));
        }
        return Ok(msg);
    }
    Err("That key isn't in the settings file any more".into())
}

fn write_lines(path: &Path, lines: &[String]) -> Result<(), String> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let backup = backup_path(path);
    if !backup.exists() {
        fs::copy(path, &backup).map_err(|e| format!("Could not back up the settings file: {e}"))?;
    }
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = lines.join(newline);
    if original.ends_with('\n') {
        out.push_str(newline);
    }
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    fs::write(&tmp, out).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Puts back the files as they were before GameShelf first changed them.
pub fn restore_backup(emulator: &str, exe: &Path) -> Result<usize, String> {
    let spec = spec(emulator, exe).ok_or("This emulator isn't supported")?;
    let mut restored = 0;
    for (path, _) in existing_files(&spec) {
        let backup = backup_path(&path);
        if backup.is_file() {
            fs::copy(&backup, &path).map_err(|e| e.to_string())?;
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
            restored += 1;
        }
    }
    Ok(restored)
}

/// Whether GameShelf has changed (and backed up) any of this emulator's files.
pub fn has_backup(emulator: &str, exe: &Path) -> bool {
    spec(emulator, exe).is_some_and(|spec| {
        existing_files(&spec)
            .iter()
            .any(|(path, _)| backup_path(path).is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str, rel: &str, text: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("gameshelf-keys-{name}-{}", std::process::id()));
        let file = dir.join(rel);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, text).unwrap();
        (dir.join("emu.exe"), file)
    }

    #[test]
    fn duckstation_swaps_and_keeps_gamepad_lines() {
        let (exe, file) = setup(
            "duck",
            "settings.ini",
            "[Pad1]\nType = AnalogController\nCross = Keyboard/K\nCross = SDL-0/A\nCircle = Keyboard/L\n[Hotkeys]\nFastForward = Keyboard/Tab\n",
        );
        let c = load("DuckStation", &exe).unwrap();
        assert!(c.editable);
        assert_eq!(
            c.buttons
                .iter()
                .find(|b| b.setting == Some("Cross"))
                .unwrap()
                .key,
            "K"
        );
        assert!(c.tip.unwrap().contains("Tab"));

        let msg = set_key("DuckStation", &exe, "Cross", Key::L).unwrap();
        assert!(msg.contains("moved to K"), "{msg}");
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("Cross = Keyboard/L"));
        assert!(text.contains("Circle = Keyboard/K"));
        assert!(text.contains("Cross = SDL-0/A"), "gamepad binding kept");
        assert!(has_backup("DuckStation", &exe));
        assert_eq!(restore_backup("DuckStation", &exe).unwrap(), 1);
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("Cross = Keyboard/K"));
        fs::remove_dir_all(exe.parent().unwrap()).unwrap();
    }

    #[test]
    fn ppsspp_keycodes() {
        let (exe, file) = setup(
            "ppsspp",
            "memstick/PSP/SYSTEM/controls.ini",
            "[ControlMapping]\nCross = 1-52,10-96\nFast-forward = 1-61\n",
        );
        let c = load("PPSSPP", &exe).unwrap();
        assert_eq!(c.buttons[0].key, "X");
        set_key("PPSSPP", &exe, "Cross", Key::K).unwrap();
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("Cross = 1-39,10-96"));
        fs::remove_dir_all(exe.parent().unwrap()).unwrap();
    }

    #[test]
    fn rpcs3_and_flycast_and_dolphin() {
        let (exe, file) = setup(
            "rpcs3",
            "config/input_configs/global/Default.yml",
            "Player 1 Input:\n  Handler: Keyboard\n  Config:\n    Cross: X\n    Left Stick Up: W\nPlayer 2 Input:\n  Handler: Null\n",
        );
        set_key("RPCS3", &exe, "Cross", Key::J).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("    Cross: J") && text.contains("Left Stick Up: W"));
        fs::remove_dir_all(exe.parent().unwrap()).unwrap();

        let (exe, file) = setup(
            "fly",
            "mappings/SDL_Keyboard.cfg",
            "[digital]\nbind0 = 27:btn_a\nbind1 = 44:btn_fforward\n",
        );
        assert_eq!(load("Flycast", &exe).unwrap().buttons[0].key, "X");
        set_key("Flycast", &exe, "btn_fforward", Key::F10).unwrap();
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("bind1 = 67:btn_fforward"));
        fs::remove_dir_all(exe.parent().unwrap()).unwrap();

        let (exe, file) = setup(
            "dolphin",
            "User/Config/GCPadNew.ini",
            "[GCPad1]\nDevice = DInput/0/Keyboard Mouse\nButtons/A = `X`\nButtons/Start = RETURN\n",
        );
        set_key("Dolphin", &exe, "Buttons/A", Key::C).unwrap();
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("Buttons/A = `C`"));
        fs::remove_dir_all(exe.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_file_is_not_editable() {
        let exe = std::env::temp_dir()
            .join("gameshelf-none")
            .join("rpcs3.exe");
        let c = load("RPCS3", &exe).unwrap();
        assert!(!c.editable);
        assert!(c.source.contains("not found"));
    }
}
