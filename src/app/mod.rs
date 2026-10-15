//! The window: app state, the header, and what every screen shares.
//! Each tab lives in its own file next to this one.

use crate::controls;
use crate::covers::{self, Cover, Covers, Job, Update};
use crate::install;
use crate::library::{now_secs, EmuConfig, Game, Library};
use crate::organize::{self, Progress};
use crate::platform::{Platform, ALL, GAME_EXTENSIONS, RETROARCH};
use crate::saves;
use crate::theme::{Theme, BURGUNDY, INK};
use crate::workspace::{self, FolderInfo};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

mod emulators_view;
mod freegames_view;
mod library_view;
mod settings_view;
mod widgets;
mod windows;

use emulators_view::*;
use freegames_view::FreeState;
use widgets::*;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Library,
    Emulators,
    FreeGames,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
enum Sort {
    Name,
    RecentlyPlayed,
    RecentlyAdded,
    System,
}

impl Sort {
    const ALL: [Sort; 4] = [
        Sort::Name,
        Sort::RecentlyPlayed,
        Sort::RecentlyAdded,
        Sort::System,
    ];

    fn label(self) -> &'static str {
        match self {
            Sort::Name => "Name",
            Sort::RecentlyPlayed => "Recently played",
            Sort::RecentlyAdded => "Recently added",
            Sort::System => "System",
        }
    }
}

enum Action {
    Play(usize),
    ToggleFavorite(usize),
    Remove(usize),
    SetPlatform(usize, Platform),
    PickCover(usize),
    FindCover(usize),
    ShowControls(usize),
}

pub struct GameShelfApp {
    lib: Library,
    covers: Covers,
    theme: Theme,
    tab: Tab,
    sort: Sort,
    search: String,
    filter: Option<Platform>,
    favorites_only: bool,
    emu_filter: EmuFilter,
    /// Emulator whose settings window is open.
    editing: Option<&'static str>,
    /// The Free games tab: catalogs, search, downloads.
    free: FreeState,
    /// Emulator whose controls window is open.
    controls_for: Option<&'static str>,
    /// The RetroArch setting waiting for a new key, and the last result to show.
    listening: Option<&'static str>,
    controls_note: Option<String>,
    /// What's in the GameShelf folder; refreshed when Settings opens.
    folder_info: Option<FolderInfo>,
    /// Standalone emulators' save folders; refreshed with `folder_info`.
    emu_saves: Vec<saves::EmuSaves>,
    /// A running "Back up all my saves": the zip and file count, or an error.
    backup_rx: Option<Receiver<Result<(PathBuf, usize), String>>>,
    /// "Restore a backup": the plan waiting for confirmation, then the running restore
    /// (files restored, and the safety backup made first).
    restore_plan: Option<saves::RestorePlan>,
    restore_rx: Option<Receiver<Result<(usize, PathBuf), String>>>,
    /// An emulator being installed.
    installing: Option<Installing>,
    /// "Organize my games": the plan waiting for confirmation…
    organize_plan: Option<organize::Plan>,
    /// …then the running move: progress messages, (done, total, current title), errors.
    organize_rx: Option<Receiver<Progress>>,
    organize_progress: (usize, usize, String),
    organize_moved: usize,
    organize_errors: Vec<String>,
    status: String,
    dirty: bool,
}

/// An install in progress: messages from the installer thread, and what to show.
struct Installing {
    emulator: &'static str,
    rx: Receiver<install::Msg>,
    status: String,
    done: u64,
    total: Option<u64>,
}

impl GameShelfApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (lib, warning) = Library::load();
        let theme = Theme::for_mode(lib.dark_mode);
        theme.apply(&cc.egui_ctx);
        let status =
            warning.unwrap_or_else(|| format!("{} games in your library", lib.games.len()));
        let mut app = Self {
            lib,
            covers: Covers::new(&cc.egui_ctx),
            theme,
            tab: Tab::Library,
            sort: Sort::Name,
            search: String::new(),
            filter: None,
            favorites_only: false,
            emu_filter: EmuFilter::All,
            editing: None,
            free: FreeState::new(),
            controls_for: None,
            listening: None,
            controls_note: None,
            folder_info: None,
            emu_saves: Vec::new(),
            backup_rx: None,
            restore_plan: None,
            restore_rx: None,
            installing: None,
            organize_plan: None,
            organize_rx: None,
            organize_progress: (0, 0, String::new()),
            organize_moved: 0,
            organize_errors: Vec::new(),
            status,
            dirty: false,
        };
        if app.lib.scan_on_start && app.lib.root.is_some() {
            app.sync_folder();
        }
        app.queue_covers();
        app
    }

    /// Asks for covers of every game that doesn't have one in memory yet.
    fn queue_covers(&mut self) {
        let dir = Library::covers_dir();
        for game in &self.lib.games {
            if self.covers.get(&game.path).is_some() {
                continue;
            }
            let job = match (&game.cover, &dir) {
                (Some(file), _) if file.is_file() => Job::Load {
                    key: game.path.clone(),
                    file: file.clone(),
                },
                (_, Some(dir))
                    if self.lib.auto_covers
                        && !game.cover_missing
                        && game.platform != Platform::Unknown =>
                {
                    Job::Fetch {
                        key: game.path.clone(),
                        platform: game.platform,
                        title: game.title.clone(),
                        save_to: covers::cover_file(dir, &game.path, ".png"),
                    }
                }
                _ => continue,
            };
            self.covers.request(job);
        }
    }

    /// Looks one game's cover up online, even when automatic covers are off.
    fn fetch_cover_for(&mut self, key: &Path) {
        let Some(dir) = Library::covers_dir() else {
            return;
        };
        let Some(game) = self.lib.games.iter().find(|g| g.path == key) else {
            return;
        };
        if game.platform == Platform::Unknown {
            return;
        }
        let job = Job::Fetch {
            key: game.path.clone(),
            platform: game.platform,
            title: game.title.clone(),
            save_to: covers::cover_file(&dir, &game.path, ".png"),
        };
        self.covers.request(job);
    }

    /// Picks up new games and emulators from the GameShelf folder.
    fn sync_folder(&mut self) {
        let (games, emulators) = self.lib.sync_root();
        self.status = match (games, emulators.is_empty()) {
            (0, true) => "GameShelf folder is up to date.".to_string(),
            (n, true) => format!("Found {n} new game(s) in your GameShelf folder."),
            (n, false) => format!("Found {n} new game(s); set up {}.", emulators.join(", ")),
        };
        if games > 0 || !emulators.is_empty() {
            self.dirty = true;
            self.queue_covers();
        }
        self.folder_info = None;
    }

    fn poll_backup(&mut self) {
        if let Some(rx) = &self.backup_rx {
            if let Ok(result) = rx.try_recv() {
                self.backup_rx = None;
                self.status = match result {
                    Ok((zip, files)) => {
                        format!("Backed up {files} save file(s) to {}", zip.display())
                    }
                    Err(e) => format!("Backup failed: {e}"),
                };
            }
        }
        if let Some(rx) = &self.restore_rx {
            if let Ok(result) = rx.try_recv() {
                self.restore_rx = None;
                self.restore_plan = None;
                self.status = match result {
                    Ok((files, safety)) => format!(
                        "✔ Restored {files} save file(s). Your saves from just before are kept in {}",
                        safety.display()
                    ),
                    Err(e) => format!("Restore failed: {e}"),
                };
            }
        }
    }

    /// Every save folder GameShelf knows about, named as in backups.
    fn save_sources(&self) -> Vec<(String, PathBuf)> {
        let retroarch = self
            .retroarch_exe()
            .as_deref()
            .and_then(saves::retroarch_dirs);
        saves::backup_sources(retroarch.as_ref(), &self.emu_saves)
    }

    /// Programs of every emulator that's set up (to check none is running).
    fn emulator_exes(&self) -> Vec<PathBuf> {
        let mut exes: Vec<PathBuf> = Vec::new();
        for cfg in self.lib.emulators.values().filter(|c| c.is_ready()) {
            let exe = cfg.exe_path();
            if !exes.contains(&exe) {
                exes.push(exe);
            }
        }
        exes
    }

    /// The progress shown on an emulator's card while it installs.
    fn install_view(&self, name: &str) -> Option<InstallView> {
        let inst = self.installing.as_ref().filter(|i| i.emulator == name)?;
        Some(InstallView {
            status: inst.status.clone(),
            fraction: inst
                .total
                .filter(|t| *t > 0)
                .map(|t| (inst.done as f32 / t as f32).clamp(0.0, 1.0)),
        })
    }

    /// Installs an emulator into `GameShelf\Emulators` (one at a time).
    fn start_install(&mut self, name: &'static str, ctx: &egui::Context) {
        if self.installing.is_some() {
            self.status = "Wait for the current install to finish.".into();
            return;
        }
        // No GameShelf folder yet: use Documents\GameShelf.
        let root = match self.lib.root.clone().or_else(workspace::default_root) {
            Some(root) => root,
            None => {
                self.status = "Choose a GameShelf folder in Settings first.".into();
                return;
            }
        };
        if let Err(e) = workspace::create(&root) {
            self.status = format!("Could not create the GameShelf folder: {e}");
            return;
        }
        if self.lib.root.is_none() {
            self.lib.root = Some(root.clone());
            self.dirty = true;
        }
        let (tx, rx) = channel();
        install::install(name, root, tx, ctx.clone());
        self.installing = Some(Installing {
            emulator: name,
            rx,
            status: "Starting…".into(),
            done: 0,
            total: None,
        });
    }

    fn poll_install(&mut self) {
        let Some(inst) = &mut self.installing else {
            return;
        };
        let mut finished = None;
        for msg in inst.rx.try_iter() {
            match msg {
                install::Msg::Status(s) => inst.status = s,
                install::Msg::Progress { done, total } => {
                    inst.done = done;
                    inst.total = total;
                    let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
                    inst.status = match total {
                        Some(t) => format!("Downloading… {:.0} of {:.0} MB", mb(done), mb(t)),
                        None => format!("Downloading… {:.0} MB", mb(done)),
                    };
                }
                install::Msg::Done(result) => finished = Some(result),
            }
        }
        let Some(result) = finished else {
            return;
        };
        let name = inst.emulator;
        self.installing = None;
        match result {
            Ok(exe) => {
                for p in ALL {
                    if p.emulator().is_some_and(|i| i.name == name) {
                        self.lib.emulators.entry(p).or_default().exe = exe.display().to_string();
                    }
                }
                self.dirty = true;
                self.folder_info = None;
                self.status = if name == RETROARCH {
                    "✔ RetroArch is installed. Get cores in RetroArch: Online Updater › Core Downloader.".into()
                } else {
                    format!("✔ {name} is installed and ready.")
                };
            }
            Err(e) => self.status = format!("Could not install {name}: {e}"),
        }
    }

    fn game_by_path(&mut self, key: &Path) -> Option<&mut Game> {
        self.lib.games.iter_mut().find(|g| g.path == key)
    }

    fn handle_cover_updates(&mut self, ctx: &egui::Context) {
        for update in self.covers.poll(ctx) {
            match update {
                Update::Found { key, file } => {
                    if let Some(g) = self.game_by_path(&key) {
                        g.cover = Some(file);
                        g.cover_missing = false;
                    }
                }
                Update::NotFound { key } => {
                    if let Some(g) = self.game_by_path(&key) {
                        g.cover_missing = true;
                    }
                }
            }
            self.dirty = true;
        }
    }

    fn add_paths(&mut self, paths: &[PathBuf]) {
        let added: usize = paths.iter().map(|p| self.lib.add_path(p)).sum();
        self.status = if added == 0 {
            "No new games found there.".to_string()
        } else {
            format!("Added {added} game(s). Looking for covers…")
        };
        if added > 0 {
            self.dirty = true;
            self.queue_covers();
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Play(i) => {
                let platform = self.lib.games.get(i).map(|g| g.platform);
                if let Some(p) = platform.filter(|p| *p != Platform::Unknown) {
                    if !self.lib.emulator_ready(p) {
                        self.tab = Tab::Emulators;
                    }
                }
                match self.lib.launch(i) {
                    Ok(msg) => {
                        self.status = msg;
                        self.dirty = true;
                    }
                    Err(e) => self.status = e,
                }
            }
            Action::ToggleFavorite(i) => {
                if let Some(g) = self.lib.games.get_mut(i) {
                    g.favorite = !g.favorite;
                    self.dirty = true;
                }
            }
            Action::Remove(i) => {
                if i < self.lib.games.len() {
                    let g = self.lib.games.remove(i);
                    self.covers.forget(&g.path);
                    self.status = format!("Removed {} (the game file was not deleted).", g.title);
                    self.dirty = true;
                }
            }
            Action::SetPlatform(i, p) => {
                if let Some(g) = self.lib.games.get_mut(i) {
                    g.platform = p;
                    // A different system may have a cover we couldn't find before.
                    if g.cover.is_none() {
                        g.cover_missing = false;
                        let key = g.path.clone();
                        self.covers.forget(&key);
                        self.fetch_cover_for(&key);
                    }
                    self.dirty = true;
                }
            }
            Action::FindCover(i) => {
                if let Some(g) = self.lib.games.get_mut(i) {
                    g.cover = None;
                    g.cover_missing = false;
                    let key = g.path.clone();
                    self.covers.forget(&key);
                    self.fetch_cover_for(&key);
                    self.status = "Looking for a cover online…".to_string();
                    self.dirty = true;
                }
            }
            Action::PickCover(i) => self.pick_cover(i),
            Action::ShowControls(i) => {
                let emulator = self
                    .lib
                    .games
                    .get(i)
                    .and_then(|g| g.platform.emulator())
                    .map(|info| info.name);
                match emulator {
                    Some(name) => self.controls_for = Some(name),
                    None => self.status = "Pick a system for this game first.".to_string(),
                }
            }
        }
    }

    fn pick_cover(&mut self, i: usize) {
        let Some(picked) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            return;
        };
        let (Some(dir), Some(game)) = (Library::covers_dir(), self.lib.games.get_mut(i)) else {
            return;
        };
        let ext = picked
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        let dest = covers::cover_file(&dir, &game.path, &format!("-custom.{ext}"));
        let copied = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(&picked, &dest));
        match copied {
            Ok(_) => {
                game.cover = Some(dest.clone());
                game.cover_missing = false;
                let key = game.path.clone();
                self.covers.request(Job::Load { key, file: dest });
                self.status = "Cover updated.".to_string();
                self.dirty = true;
            }
            Err(e) => self.status = format!("Could not copy the image: {e}"),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        ui.horizontal(|ui| {
            ui.label(RichText::new("🎮").size(24.0).color(t.gold));
            ui.label(
                RichText::new("GameShelf")
                    .size(22.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.add_space(24.0);
            if header_tab(ui, "Library", self.tab == Tab::Library, t.gold) {
                self.tab = Tab::Library;
            }
            if header_tab(ui, "Emulators", self.tab == Tab::Emulators, t.gold) {
                self.tab = Tab::Emulators;
            }
            if header_tab(ui, "Free games", self.tab == Tab::FreeGames, t.gold) {
                self.tab = Tab::FreeGames;
            }
            ui.add_space(24.0);
            let add =
                egui::Button::new(RichText::new("➕  Add games").strong().color(INK)).fill(t.gold);
            if ui.add(add).clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Games", GAME_EXTENSIONS)
                    .add_filter("All files", &["*"])
                    .pick_files()
                {
                    self.add_paths(&files);
                }
            }
            let folder = egui::Button::new(RichText::new("📁  Add folder").color(Color32::WHITE))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(140)));
            if ui.add(folder).clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.add_paths(&[dir]);
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let in_settings = self.tab == Tab::Settings;
                let gear = egui::Button::new(RichText::new("⚙").size(18.0).color(if in_settings {
                    INK
                } else {
                    Color32::WHITE
                }))
                .fill(if in_settings {
                    t.gold
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(140)))
                .min_size(vec2(36.0, 30.0));
                if ui.add(gear).on_hover_text("Settings").clicked() {
                    self.tab = if in_settings {
                        Tab::Library
                    } else {
                        Tab::Settings
                    };
                    self.folder_info = None;
                }
                ui.add_space(4.0);
                let mode = if self.lib.dark_mode {
                    "☀  Light"
                } else {
                    "🌙  Dark"
                };
                let toggle = egui::Button::new(RichText::new(mode).color(Color32::WHITE))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(140)));
                if ui
                    .add(toggle)
                    .on_hover_text("Switch light / dark")
                    .clicked()
                {
                    self.lib.dark_mode = !self.lib.dark_mode;
                    self.theme = Theme::for_mode(self.lib.dark_mode);
                    self.theme.apply(ui.ctx());
                    self.dirty = true;
                }
                if self.tab == Tab::Library {
                    ui.add_space(8.0);
                    egui::ComboBox::from_id_salt("sort")
                        .selected_text(self.sort.label())
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for s in Sort::ALL {
                                ui.selectable_value(&mut self.sort, s, s.label());
                            }
                        });
                    ui.label(RichText::new("Sort").color(Color32::from_white_alpha(200)));
                    ui.add_space(8.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("🔍  Search your games")
                            .desired_width(240.0)
                            .margin(vec2(8.0, 5.0)),
                    );
                }
            });
        });
    }

    /// RetroArch's program, if it's set up.
    fn retroarch_exe(&self) -> Option<PathBuf> {
        ALL.into_iter()
            .filter(|p| p.emulator().is_some_and(|i| i.name == RETROARCH))
            .find_map(|p| {
                self.lib
                    .emulators
                    .get(&p)
                    .filter(|c| c.is_ready())
                    .map(|c| c.exe_path())
            })
    }
}

impl eframe::App for GameShelfApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_paths(&dropped);
        }
        self.handle_cover_updates(ctx);
        self.poll_organize();
        self.poll_backup();
        self.poll_install();
        self.poll_free_games();
        self.restore_window(ctx);
        self.organize_window(ctx);
        self.controls_window(ctx);

        let t = self.theme;
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(t.header)
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ctx, |ui| self.header(ui));
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(t.side)
                    .inner_margin(egui::Margin::symmetric(14, 5)),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new(&self.status).size(12.0).color(t.text_weak))
            });
        if self.tab == Tab::Library {
            egui::SidePanel::left("systems")
                .resizable(false)
                .exact_width(210.0)
                .frame(
                    egui::Frame::new()
                        .fill(t.side)
                        .inner_margin(egui::Margin::symmetric(10, 4)),
                )
                .show(ctx, |ui| self.sidebar(ui));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(t.bg)
                    .inner_margin(egui::Margin::symmetric(24, 4)),
            )
            .show(ctx, |ui| match self.tab {
                Tab::Library => self.library_tab(ui),
                Tab::Emulators => self.emulators_tab(ui),
                Tab::FreeGames => self.free_games_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            });

        if self.dirty {
            self.dirty = false;
            if let Err(e) = self.lib.save() {
                self.status = format!("Could not save the library: {e}");
            }
        }
    }
}
