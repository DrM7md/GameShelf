//! Game systems: names, colours, recommended emulators and file detection.

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Platform {
    Ps1,
    Ps2,
    Ps3,
    Psp,
    GameCube,
    Nes,
    Snes,
    N64,
    Gb,
    Gbc,
    Gba,
    Nds,
    Genesis,
    Dreamcast,
    Unknown,
}

pub const ALL: [Platform; 15] = [
    Platform::Ps1,
    Platform::Ps2,
    Platform::Ps3,
    Platform::Psp,
    Platform::GameCube,
    Platform::Nes,
    Platform::Snes,
    Platform::N64,
    Platform::Gb,
    Platform::Gbc,
    Platform::Gba,
    Platform::Nds,
    Platform::Genesis,
    Platform::Dreamcast,
    Platform::Unknown,
];

/// Extensions offered in the "Add games" dialog.
pub const GAME_EXTENSIONS: &[&str] = &[
    "iso", "cue", "bin", "chd", "pbp", "cso", "gcm", "rvz", "gcz", "ciso", "nes", "sfc", "smc",
    "n64", "z64", "v64", "gb", "gbc", "gba", "nds", "md", "gen", "smd", "gdi", "cdi",
];

pub const RETROARCH: &str = "RetroArch";

/// The emulator we suggest for a system.
pub struct EmulatorInfo {
    pub name: &'static str,
    pub url: &'static str,
    pub exe_hint: &'static str,
    /// Command-line arguments; `{rom}` is replaced by the game file.
    pub default_args: &'static str,
    pub note: &'static str,
}

impl Platform {
    pub fn name(self) -> &'static str {
        match self {
            Platform::Ps1 => "PlayStation",
            Platform::Ps2 => "PlayStation 2",
            Platform::Ps3 => "PlayStation 3",
            Platform::Psp => "PSP",
            Platform::GameCube => "GameCube",
            Platform::Nes => "NES",
            Platform::Snes => "Super Nintendo",
            Platform::N64 => "Nintendo 64",
            Platform::Gb => "Game Boy",
            Platform::Gbc => "Game Boy Color",
            Platform::Gba => "Game Boy Advance",
            Platform::Nds => "Nintendo DS",
            Platform::Genesis => "Sega Genesis",
            Platform::Dreamcast => "Sega Dreamcast",
            Platform::Unknown => "Unknown",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Platform::Ps1 => "PS1",
            Platform::Ps2 => "PS2",
            Platform::Ps3 => "PS3",
            Platform::Psp => "PSP",
            Platform::GameCube => "GC",
            Platform::Nes => "NES",
            Platform::Snes => "SNES",
            Platform::N64 => "N64",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
            Platform::Gba => "GBA",
            Platform::Nds => "NDS",
            Platform::Genesis => "MD",
            Platform::Dreamcast => "DC",
            Platform::Unknown => "?",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Platform::Ps1 => Color32::from_rgb(150, 155, 175),
            Platform::Ps2 => Color32::from_rgb(60, 105, 225),
            Platform::Ps3 => Color32::from_rgb(210, 60, 80),
            Platform::Psp => Color32::from_rgb(40, 175, 205),
            Platform::GameCube => Color32::from_rgb(130, 90, 215),
            Platform::Nes => Color32::from_rgb(220, 70, 60),
            Platform::Snes => Color32::from_rgb(170, 130, 230),
            Platform::N64 => Color32::from_rgb(60, 170, 90),
            Platform::Gb => Color32::from_rgb(140, 160, 70),
            Platform::Gbc => Color32::from_rgb(230, 190, 40),
            Platform::Gba => Color32::from_rgb(95, 85, 200),
            Platform::Nds => Color32::from_rgb(200, 200, 210),
            Platform::Genesis => Color32::from_rgb(230, 150, 40),
            Platform::Dreamcast => Color32::from_rgb(245, 120, 40),
            Platform::Unknown => Color32::from_rgb(110, 110, 120),
        }
    }

    pub fn emulator(self) -> Option<EmulatorInfo> {
        let retroarch = |core: &'static str| EmulatorInfo {
            name: RETROARCH,
            url: "https://www.retroarch.com/?page=platforms",
            exe_hint: r"C:\RetroArch-Win64\retroarch.exe",
            default_args: core,
            note:
                "Download the core inside RetroArch: Main Menu > Online Updater > Core Downloader.",
        };
        Some(match self {
            Platform::Ps1 => EmulatorInfo {
                name: "DuckStation",
                url: "https://www.duckstation.org/",
                exe_hint: r"C:\DuckStation\duckstation-qt-x64-ReleaseLTCG.exe",
                default_args: "-batch -- {rom}",
                note: "Needs a PS1 BIOS dumped from your own console.",
            },
            Platform::Ps2 => EmulatorInfo {
                name: "PCSX2",
                url: "https://pcsx2.net/downloads",
                exe_hint: r"C:\PCSX2\pcsx2-qt.exe",
                default_args: "-batch -- {rom}",
                note: "Needs a PS2 BIOS dumped from your own console.",
            },
            Platform::Ps3 => EmulatorInfo {
                name: "RPCS3",
                url: "https://rpcs3.net/download",
                exe_hint: r"C:\RPCS3\rpcs3.exe",
                default_args: "--no-gui {rom}",
                note: "Install the official PS3 firmware in RPCS3 first. Needs a strong CPU.",
            },
            Platform::Psp => EmulatorInfo {
                name: "PPSSPP",
                url: "https://www.ppsspp.org/download/",
                exe_hint: r"C:\PPSSPP\PPSSPPWindows64.exe",
                default_args: "{rom}",
                note: "",
            },
            Platform::GameCube => EmulatorInfo {
                name: "Dolphin",
                url: "https://dolphin-emu.org/download/",
                exe_hint: r"C:\Dolphin-x64\Dolphin.exe",
                default_args: "-b -e {rom}",
                note: "",
            },
            Platform::Dreamcast => EmulatorInfo {
                name: "Flycast",
                url: "https://github.com/flyinghead/flycast/releases",
                exe_hint: r"C:\Flycast\flycast.exe",
                default_args: "{rom}",
                note: "",
            },
            Platform::Nes => retroarch(r"-L cores\fceumm_libretro.dll {rom}"),
            Platform::Snes => retroarch(r"-L cores\snes9x_libretro.dll {rom}"),
            Platform::N64 => retroarch(r"-L cores\mupen64plus_next_libretro.dll {rom}"),
            // mGBA runs Game Boy and Game Boy Color too.
            Platform::Gb => retroarch(r"-L cores\mgba_libretro.dll {rom}"),
            Platform::Gbc => retroarch(r"-L cores\mgba_libretro.dll {rom}"),
            Platform::Gba => retroarch(r"-L cores\mgba_libretro.dll {rom}"),
            Platform::Nds => retroarch(r"-L cores\melonds_libretro.dll {rom}"),
            Platform::Genesis => retroarch(r"-L cores\genesis_plus_gx_libretro.dll {rom}"),
            Platform::Unknown => return None,
        })
    }

    /// Folder names that mean this system (lowercase).
    fn folder_names(self) -> &'static [&'static str] {
        match self {
            Platform::Ps1 => &["playstation", "playstation 1", "ps1", "psx"],
            Platform::Ps2 => &["playstation 2", "ps2"],
            Platform::Ps3 => &["playstation 3", "ps3"],
            Platform::Psp => &["psp", "playstation portable"],
            Platform::GameCube => &["gamecube", "nintendo gamecube", "gc", "ngc"],
            Platform::Nes => &["nes", "famicom", "nintendo entertainment system"],
            Platform::Snes => &["snes", "super nintendo", "super famicom", "sfc"],
            Platform::N64 => &["n64", "nintendo 64"],
            Platform::Gb => &["gb", "game boy", "gameboy"],
            Platform::Gbc => &["gbc", "game boy color", "gameboy color"],
            Platform::Gba => &["gba", "game boy advance", "gameboy advance"],
            Platform::Nds => &["nds", "ds", "nintendo ds"],
            Platform::Genesis => &["sega genesis", "genesis", "mega drive", "megadrive", "md"],
            Platform::Dreamcast => &["sega dreamcast", "dreamcast", "dc"],
            Platform::Unknown => &[],
        }
    }
}

/// The system named by one of the file's parent folders, if any
/// (e.g. `Games\PlayStation 2\Okami.iso` → PS2).
pub fn folder_hint(path: &Path) -> Option<Platform> {
    path.ancestors().skip(1).take(4).find_map(|dir| {
        let name = dir.file_name()?.to_str()?.trim().to_ascii_lowercase();
        ALL.into_iter()
            .find(|p| p.folder_names().contains(&name.as_str()))
    })
}

/// Is this a PS3 game folder's `PS3_GAME/USRDIR/EBOOT.BIN`?
pub fn is_ps3_eboot(path: &Path) -> bool {
    let is = |p: Option<&Path>, name: &str| {
        p.and_then(Path::file_name)
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
    };
    let usrdir = path.parent();
    is(Some(path), "EBOOT.BIN")
        && is(usrdir, "USRDIR")
        && is(usrdir.and_then(Path::parent), "PS3_GAME")
}

/// Which system a file belongs to. `None` means "not a game file, skip it";
/// `Some(Unknown)` means "a game, but the user has to pick the system".
pub fn detect(path: &Path) -> Option<Platform> {
    if is_ps3_eboot(path) {
        return Some(Platform::Ps3);
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "cue" => Platform::Ps1,
        // A .bin next to a .cue is a data track; the .cue is the game.
        "bin" if path.with_extension("cue").exists() => return None,
        "bin" if path.file_name()?.eq_ignore_ascii_case("EBOOT.BIN") => return None,
        "bin" => Platform::Ps1,
        "pbp" | "cso" => Platform::Psp,
        "iso" => sniff_iso(path).unwrap_or(Platform::Unknown),
        "chd" => Platform::Unknown,
        "gcm" | "rvz" | "gcz" | "ciso" => Platform::GameCube,
        "nes" => Platform::Nes,
        "sfc" | "smc" => Platform::Snes,
        "n64" | "z64" | "v64" => Platform::N64,
        "gb" => Platform::Gb,
        "gbc" => Platform::Gbc,
        "gba" => Platform::Gba,
        "nds" => Platform::Nds,
        "md" | "gen" | "smd" => Platform::Genesis,
        "gdi" | "cdi" => Platform::Dreamcast,
        _ => return None,
    })
}

/// Look inside a disc image to tell PS2 / PSP / PS3 / GameCube apart.
fn sniff_iso(path: &Path) -> Option<Platform> {
    let mut file = File::open(path).ok()?;
    let mut head = [0u8; 0x20];
    file.read_exact(&mut head).ok()?;
    if head.get(0x1C..0x20) == Some(&[0xC2, 0x33, 0x9F, 0x3D][..]) {
        return Some(Platform::GameCube);
    }
    // ISO 9660 primary volume descriptor at sector 16.
    file.seek(SeekFrom::Start(0x8000)).ok()?;
    let mut pvd = [0u8; 0x48];
    file.read_exact(&mut pvd).ok()?;
    let system = String::from_utf8_lossy(pvd.get(8..40)?).to_string();
    let volume = String::from_utf8_lossy(pvd.get(40..72)?).to_string();
    if volume.contains("PS3VOLUME") {
        Some(Platform::Ps3)
    } else if system.contains("PSP GAME") {
        Some(Platform::Psp)
    } else if system.contains("PLAYSTATION") {
        Some(Platform::Ps2)
    } else {
        None
    }
}
