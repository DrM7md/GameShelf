# GameShelf

A good-looking game library for Windows that launches your emulators. Written in Rust with
[egui](https://github.com/emilk/egui).

GameShelf doesn't emulate anything itself: it organizes your games and starts them in the
right emulator (RetroArch, DuckStation, PCSX2, RPCS3, PPSSPP, Dolphin, Flycast).

## Features

- **Library** of game cards with box art (from [libretro-thumbnails](https://github.com/libretro-thumbnails/libretro-thumbnails)),
  favorites, search, sorting and a per-system sidebar.
- **Add games** by file, folder, or drag and drop. The system is detected from the file
  (extension, disc header) or from the folder it's in (`Games\PlayStation 2\…`).
- **Emulators** screen: the recommended emulator per system, download links, and status
  (RetroArch cores included).
- **GameShelf folder** (`Games\<System>`, `Emulators`, `BIOS`): new games and emulators
  dropped in are picked up automatically.
- **Organize my games**: moves every game into its system folder, after showing the plan.
  Never overwrites; multi-file games (`.cue`/`.gdi`/`.m3u`, PS3 folders) move together.
- **Controls** guide per emulator, with keys you can change by pressing them: RetroArch,
  DuckStation, PCSX2, PPSSPP, Dolphin, RPCS3 and Flycast. Only existing keyboard bindings
  are edited, gamepad bindings are kept, and the original file is backed up first.
- **Continue playing**: the games you played last, one click to pick up again.
- **Saves & cheats**: see where progress is kept, move RetroArch's saves, states and
  cheats into the GameShelf folder (`Saves`, `States`, `Cheats`), and "resume where I
  left off".
- **All your progress**: the memory cards and saves of every standalone emulator are
  linked into `Saves\<emulator>` and `States\<emulator>` (Windows junctions: nothing is
  moved or changed), and "Back up all my saves" zips everything into `GameShelf\Backups`.

## Code layout

| File | What's in it |
|---|---|
| `src/app/mod.rs` | App state, the header, actions shared by every screen |
| `src/app/library_view.rs` | Library: sidebar, Continue playing, game cards |
| `src/app/emulators_view.rs` | Emulators: cards and settings window |
| `src/app/settings_view.rs` | Settings: folder, saves, backup, appearance, covers |
| `src/app/windows.rs` | Controls and Organize windows |
| `src/app/widgets.rs` | Small shared drawing helpers |
| `src/library.rs`, `platform.rs` | Games, systems, launching |
| `src/covers.rs` | Box art download and cache |
| `src/workspace.rs`, `organize.rs` | GameShelf folder and Organize my games |
| `src/controls.rs`, `emukeys.rs` | Controls guide and key changing per emulator |
| `src/saves.rs` | Save folders, links and backups |

Ideas for later are in [BACKLOG.md](BACKLOG.md).
- Burgundy, gold and beige theme, in dark and light.

## Build

```sh
cargo build --release
```

The app is `target\release\gameshelf.exe`. `gameshelf --organize` runs "Organize my games"
without opening the window.

## Games

GameShelf ships no games or BIOS files. Use games you own, or free homebrew.
