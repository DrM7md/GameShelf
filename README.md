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
- **Controls** guide per emulator, with keys you can change by pressing them.
- Burgundy, gold and beige theme, in dark and light.

## Build

```sh
cargo build --release
```

The app is `target\release\gameshelf.exe`. `gameshelf --organize` runs "Organize my games"
without opening the window.

## Games

GameShelf ships no games or BIOS files. Use games you own, or free homebrew.
