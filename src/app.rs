//! The window: header, systems sidebar, game cards and the Emulators tab.

use crate::controls;
use crate::covers::{self, Cover, Covers, Job, Update};
use crate::library::{now_secs, EmuConfig, Game, Library};
use crate::organize::{self, Progress};
use crate::platform::{Platform, ALL, GAME_EXTENSIONS};
use crate::theme::{Theme, BURGUNDY, INK};
use crate::workspace::{self, FolderInfo};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

/// Box art is roughly 3:4, so covers are a bit taller than wide.
const COVER_ASPECT: f32 = 1.34;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Library,
    Emulators,
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
    /// Emulator whose controls window is open.
    controls_for: Option<&'static str>,
    /// The RetroArch setting waiting for a new key, and the last result to show.
    listening: Option<&'static str>,
    controls_note: Option<String>,
    /// What's in the GameShelf folder; refreshed when Settings opens.
    folder_info: Option<FolderInfo>,
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
            controls_for: None,
            listening: None,
            controls_note: None,
            folder_info: None,
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

    fn game_by_path(&mut self, key: &Path) -> Option<&mut Game> {
        self.lib.games.iter_mut().find(|g| g.path == key)
    }

    /// Applies what the background mover reports.
    fn poll_organize(&mut self) {
        let Some(rx) = &self.organize_rx else {
            return;
        };
        let events: Vec<Progress> = rx.try_iter().collect();
        for event in events {
            match event {
                Progress::Working { done, total, title } => {
                    self.organize_progress = (done, total, title);
                }
                Progress::Moved { old_path, new_path } => {
                    let cover = self.game_by_path(&old_path).map(|g| {
                        g.path = new_path.clone();
                        g.cover.clone()
                    });
                    self.covers.forget(&old_path);
                    if let Some(file) = cover.flatten().filter(|f| f.is_file()) {
                        self.covers.request(Job::Load {
                            key: new_path,
                            file,
                        });
                    }
                    self.organize_moved += 1;
                    self.dirty = true;
                }
                Progress::Failed { title, error } => {
                    self.organize_errors.push(format!("{title}: {error}"));
                }
                Progress::Finished => {
                    self.organize_rx = None;
                    self.folder_info = None;
                    self.status = match self.organize_errors.first() {
                        None => format!("Organized {} game(s).", self.organize_moved),
                        Some(first) => format!(
                            "Organized {} game(s); {} could not be moved ({first}).",
                            self.organize_moved,
                            self.organize_errors.len()
                        ),
                    };
                    self.queue_covers();
                }
            }
        }
    }

    /// Keyboard controls for one emulator, with the "why is it fast?" tip first.
    /// For RetroArch, click a key and press the new one to change it.
    fn controls_window(&mut self, ctx: &egui::Context) {
        let Some(name) = self.controls_for else {
            return;
        };
        let t = self.theme;
        let exe = ALL
            .into_iter()
            .find(|p| p.emulator().is_some_and(|i| i.name == name))
            .and_then(|p| self.lib.emulators.get(&p))
            .filter(|c| c.is_ready())
            .map(|c| c.exe_path());

        // Waiting for a new key: take the next key press, before any widget sees it.
        if let (Some(setting), Some(exe)) = (self.listening, exe.as_deref()) {
            let pressed = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        ..
                    } => Some(*key),
                    _ => None,
                })
            });
            if let Some(key) = pressed {
                ctx.input_mut(|i| {
                    i.events
                        .retain(|e| !matches!(e, egui::Event::Key { key: k, .. } if *k == key))
                });
                if key == egui::Key::Escape {
                    self.listening = None;
                    self.controls_note = Some("Cancelled.".into());
                } else {
                    match controls::retroarch_key_name(key) {
                        Some(raw) => self.rebind(exe, setting, &raw),
                        None => {
                            self.controls_note =
                                Some("That key can't be used here. Try another one.".into())
                        }
                    }
                }
            }
        }

        let controls = controls::for_emulator(name, exe.as_deref());
        let listening = self.listening;
        let note = self.controls_note.clone();
        let (mut clicked, mut reset) = (None, false);
        let mut open = true;
        egui::Window::new(RichText::new(format!("🎮  {name} controls")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .default_width(660.0)
            .show(ctx, |ui| {
                if let Some(tip) = &controls.tip {
                    egui::Frame::new()
                        .fill(t.gold.gamma_multiply(0.18))
                        .stroke(Stroke::new(1.0_f32, t.gold))
                        .corner_radius(8.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(format!("⚡ {tip}")).strong().color(t.text));
                        });
                    ui.add_space(8.0);
                }
                if controls.editable {
                    let hint = if listening.is_some() {
                        "Now press the new key on your keyboard (Esc cancels)."
                    } else {
                        "To change a key: click it, then press the new key on your keyboard."
                    };
                    ui.label(RichText::new(hint).strong().color(t.gold));
                    ui.add_space(6.0);
                }
                let editable = controls.editable;
                if controls.buttons.is_empty() {
                    clicked =
                        controls_column(ui, t, "HOTKEYS", &controls.hotkeys, editable, listening);
                } else {
                    ui.columns(2, |cols| {
                        if let [left, right] = cols {
                            let a = controls_column(
                                left,
                                t,
                                "GAME BUTTONS",
                                &controls.buttons,
                                editable,
                                listening,
                            );
                            let b = controls_column(
                                right,
                                t,
                                "HOTKEYS",
                                &controls.hotkeys,
                                editable,
                                listening,
                            );
                            clicked = a.or(b);
                        }
                    });
                }
                if let Some(note) = &note {
                    ui.add_space(8.0);
                    let color = if note.starts_with('✔') {
                        t.success
                    } else {
                        t.warning
                    };
                    ui.label(RichText::new(note).strong().color(color));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if editable && ui.button("↺  Reset to RetroArch defaults").clicked() {
                        reset = true;
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("Change keys: {}", controls.where_to_change))
                        .size(12.5)
                        .color(t.text_weak),
                );
                ui.label(
                    RichText::new(&controls.source)
                        .size(11.5)
                        .color(t.text_weak),
                );
            });

        if let Some(setting) = clicked {
            self.listening = Some(setting);
            self.controls_note = None;
        }
        if reset {
            if let Some(exe) = exe.as_deref() {
                self.listening = None;
                self.controls_note = Some(
                    match controls::write_retroarch(exe, &controls::retroarch_defaults()) {
                        Ok(()) => with_running_warning(
                            "✔ All keys are back to RetroArch's defaults.".into(),
                        ),
                        Err(e) => format!("Could not save: {e}"),
                    },
                );
            }
        }
        if !open {
            self.controls_for = None;
            self.listening = None;
            self.controls_note = None;
        }
    }

    /// Gives `setting` the key `raw`. If another action already used that key,
    /// the two swap so nothing ends up with two jobs.
    fn rebind(&mut self, exe: &Path, setting: &'static str, raw: &str) {
        let controls = controls::for_emulator("RetroArch", Some(exe));
        let current = controls.all().find(|b| b.setting == Some(setting));
        let action = current.map_or("Key", |b| b.action);
        let old = current.map(|b| b.raw.clone());
        let mut changes = vec![(setting, raw.to_string())];
        let mut swapped = None;
        let clash = controls.all().find(|b| {
            b.setting.is_some() && b.setting != Some(setting) && b.raw.eq_ignore_ascii_case(raw)
        });
        if let (Some(other), Some(old)) = (clash, old) {
            if let Some(other_setting) = other.setting {
                changes.push((other_setting, old.clone()));
                swapped = Some((other.action, controls::pretty_key(&old)));
            }
        }
        self.listening = None;
        self.controls_note = Some(match controls::write_retroarch(exe, &changes) {
            Ok(()) => {
                let mut msg = format!("✔ {action} → {}", controls::pretty_key(raw));
                if let Some((other, key)) = swapped {
                    msg.push_str(&format!("   ·   {other} moved to {key}"));
                }
                with_running_warning(msg)
            }
            Err(e) => format!("Could not save: {e}"),
        });
    }

    /// The plan preview (confirm / cancel) and, while moving, a progress window.
    fn organize_window(&mut self, ctx: &egui::Context) {
        let t = self.theme;
        if self.organize_rx.is_some() {
            let (done, total, title) = &self.organize_progress;
            egui::Window::new(RichText::new("🧹 Organizing your games").strong())
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .default_width(460.0)
                .show(ctx, |ui| {
                    let fraction = if *total == 0 {
                        0.0
                    } else {
                        *done as f32 / *total as f32
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .text(format!("{done} of {total}"))
                            .fill(BURGUNDY),
                    );
                    ui.label(RichText::new(format!("Moving {title}…")).color(t.text_weak));
                    ui.label(
                        RichText::new("Large games on another drive can take a while.")
                            .size(12.0)
                            .color(t.text_weak),
                    );
                });
            return;
        }
        let Some(plan) = &self.organize_plan else {
            return;
        };
        let root = self.lib.root.clone().unwrap_or_default();
        let (mut start, mut close) = (false, false);
        egui::Window::new(RichText::new("🧹 Organize my games").strong())
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .default_width(600.0)
            .show(ctx, |ui| {
                let n = plan.moves.len();
                if n == 0 {
                    ui.label(
                        RichText::new("✔ Nothing to move.")
                            .size(16.0)
                            .color(t.success),
                    );
                } else {
                    ui.label(
                        RichText::new(format!(
                            "{n} game(s) will be moved into {}\\Games\\<system>:",
                            root.display()
                        ))
                        .size(15.0)
                        .color(t.text),
                    );
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for m in &plan.moves {
                                ui.horizontal(|ui| {
                                    let (dot, _) =
                                        ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                                    ui.painter().circle_filled(
                                        dot.center(),
                                        4.0,
                                        m.platform.color(),
                                    );
                                    ui.label(RichText::new(&m.title).strong().color(t.text));
                                    ui.label(
                                        RichText::new(format!("→ Games\\{}", m.platform.name()))
                                            .color(t.text_weak),
                                    );
                                    if m.items.len() > 1 {
                                        ui.label(
                                            RichText::new(format!(
                                                "(+{} files)",
                                                m.items.len() - 1
                                            ))
                                            .size(12.0)
                                            .color(t.text_weak),
                                        );
                                    }
                                });
                            }
                        });
                }
                ui.add_space(8.0);
                let note = |ui: &mut egui::Ui, n: usize, text: &str, color: Color32| {
                    if n > 0 {
                        ui.label(
                            RichText::new(format!("• {n} {text}"))
                                .size(12.5)
                                .color(color),
                        );
                    }
                };
                note(
                    ui,
                    plan.in_place,
                    "already in the right folder",
                    t.text_weak,
                );
                note(
                    ui,
                    plan.unknown,
                    "with no system yet (pick one from the ⋯ menu first)",
                    t.warning,
                );
                note(ui, plan.missing, "whose file is missing", t.warning);
                note(
                    ui,
                    plan.conflicts.len(),
                    "skipped: a file with that name is already there",
                    t.warning,
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Files are moved, not copied. Nothing is ever overwritten.")
                        .size(12.0)
                        .color(t.text_weak),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if n > 0 {
                        let go = egui::Button::new(
                            RichText::new(format!("Move {n} game(s)"))
                                .strong()
                                .color(INK),
                        )
                        .fill(t.gold)
                        .min_size(vec2(0.0, 32.0));
                        if ui.add(go).clicked() {
                            start = true;
                        }
                    }
                    let label = if n > 0 { "Cancel" } else { "Close" };
                    if ui
                        .add(egui::Button::new(label).min_size(vec2(0.0, 32.0)))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if close {
            self.organize_plan = None;
        }
        if start {
            if let Some(plan) = self.organize_plan.take() {
                let (tx, rx) = channel();
                self.organize_progress = (0, plan.moves.len(), String::new());
                self.organize_moved = 0;
                self.organize_errors.clear();
                self.organize_rx = Some(rx);
                organize::run(plan.moves, tx, ctx.clone());
            }
        }
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

    /// Indices of the games to show, filtered and sorted.
    fn visible(&self) -> Vec<usize> {
        let query = self.search.to_lowercase();
        let mut games: Vec<(usize, &Game)> = self
            .lib
            .games
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                self.filter.is_none_or(|p| g.platform == p)
                    && (!self.favorites_only || g.favorite)
                    && (query.is_empty() || g.title.to_lowercase().contains(&query))
            })
            .collect();
        let by_name = |a: &Game, b: &Game| a.title.to_lowercase().cmp(&b.title.to_lowercase());
        games.sort_by(|(_, a), (_, b)| match self.sort {
            Sort::Name => by_name(a, b),
            Sort::RecentlyPlayed => b
                .last_played
                .cmp(&a.last_played)
                .then_with(|| by_name(a, b)),
            Sort::RecentlyAdded => b.added.cmp(&a.added).then_with(|| by_name(a, b)),
            Sort::System => a.platform.cmp(&b.platform).then_with(|| by_name(a, b)),
        });
        games.into_iter().map(|(i, _)| i).collect()
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

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        ui.add_space(12.0);
        section_label(ui, "LIBRARY", t);
        let total = self.lib.games.len();
        let favorites = self.lib.games.iter().filter(|g| g.favorite).count();
        if side_row(
            ui,
            t,
            self.filter.is_none() && !self.favorites_only,
            None,
            "All games",
            total,
        ) {
            self.filter = None;
            self.favorites_only = false;
        }
        if side_row(
            ui,
            t,
            self.favorites_only,
            Some(t.gold),
            "Favorites",
            favorites,
        ) {
            self.favorites_only = !self.favorites_only;
        }
        ui.add_space(14.0);
        section_label(ui, "SYSTEMS", t);
        for p in ALL {
            let count = self.lib.games.iter().filter(|g| g.platform == p).count();
            if count == 0 {
                continue;
            }
            if side_row(
                ui,
                t,
                self.filter == Some(p),
                Some(p.color()),
                p.name(),
                count,
            ) {
                self.filter = if self.filter == Some(p) {
                    None
                } else {
                    Some(p)
                };
            }
        }
    }

    fn library_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        if self.lib.games.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Drop game files or folders here\nor click ➕ Add games")
                        .size(20.0)
                        .color(t.text_weak),
                );
            });
            return;
        }
        let visible = self.visible();
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let title = match (self.filter, self.favorites_only) {
                (_, true) => "Favorites".to_string(),
                (Some(p), _) => p.name().to_string(),
                (None, _) => "All games".to_string(),
            };
            ui.label(RichText::new(title).size(24.0).strong().color(t.text));
            ui.label(RichText::new(format!("{} games", visible.len())).color(t.text_weak));
        });
        ui.add_space(10.0);
        if visible.is_empty() {
            ui.label(
                RichText::new("No games match.")
                    .size(16.0)
                    .color(t.text_weak),
            );
            return;
        }
        let mut action = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let (columns, cell) = grid(ui.available_width() - 6.0, GAME_CARD_MIN, GAP);
                for row in visible.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for &i in row {
                            if let Some(game) = self.lib.games.get(i) {
                                let cover = self.covers.get(&game.path);
                                if let Some(a) = game_card(ui, t, cell, game, cover, i) {
                                    action = Some(a);
                                }
                            }
                        }
                    });
                    ui.add_space(GAP);
                }
            });
        if let Some(a) = action {
            self.apply(a);
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        if self.folder_info.is_none() {
            self.folder_info = self.lib.root.as_deref().map(workspace::inspect);
        }
        ui.add_space(14.0);
        ui.label(RichText::new("Settings").size(24.0).strong().color(t.text));
        ui.label(RichText::new("Your GameShelf folder, appearance and covers.").color(t.text_weak));
        ui.add_space(14.0);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if ui.available_width() > 980.0 {
                    ui.columns(2, |cols| {
                        if let [left, right] = cols {
                            self.folder_section(left);
                            self.other_sections(right);
                        }
                    });
                } else {
                    self.folder_section(ui);
                    ui.add_space(GAP);
                    self.other_sections(ui);
                }
                ui.add_space(20.0);
            });
    }

    fn folder_section(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        let mut act = None;
        section(
            ui,
            t,
            "📁  GameShelf folder",
            "One folder for your games (sorted by system), emulators and BIOS files.",
            |ui| {
                egui::Frame::new()
                    .fill(t.input)
                    .stroke(Stroke::new(1.0_f32, t.border))
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &self.lib.root {
                            Some(root) => ui.label(
                                RichText::new(root.display().to_string())
                                    .strong()
                                    .color(t.text),
                            ),
                            None => {
                                ui.label(RichText::new("No folder chosen yet").color(t.text_weak))
                            }
                        };
                    });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    let choose =
                        egui::Button::new(RichText::new("Choose folder…").strong().color(INK))
                            .fill(t.gold)
                            .min_size(vec2(0.0, 30.0));
                    if ui.add(choose).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            act = Some(FolderAct::Choose(dir));
                        }
                    }
                    if let Some(default) = workspace::default_root() {
                        if self.lib.root.as_ref() != Some(&default) {
                            let label = "Use Documents\\GameShelf";
                            if ui
                                .add(egui::Button::new(label).min_size(vec2(0.0, 30.0)))
                                .on_hover_text(default.display().to_string())
                                .clicked()
                            {
                                act = Some(FolderAct::Choose(default));
                            }
                        }
                    }
                    if let Some(root) = &self.lib.root {
                        if ui
                            .add(egui::Button::new("📂 Open").min_size(vec2(0.0, 30.0)))
                            .clicked()
                        {
                            act = Some(FolderAct::Open(root.clone()));
                        }
                        if ui
                            .add(egui::Button::new("🔄 Scan now").min_size(vec2(0.0, 30.0)))
                            .on_hover_text("Add new games and set up new emulators")
                            .clicked()
                        {
                            act = Some(FolderAct::Scan);
                        }
                        let busy = self.organize_rx.is_some();
                        let organize = egui::Button::new(
                            RichText::new("🧹 Organize my games").strong().color(Color32::WHITE),
                        )
                        .fill(BURGUNDY)
                        .min_size(vec2(0.0, 30.0));
                        if ui
                            .add_enabled(!busy, organize)
                            .on_hover_text("Move every game into Games\\<its system>. You'll see the plan first.")
                            .clicked()
                        {
                            act = Some(FolderAct::Organize);
                        }
                    }
                });

                if let (Some(root), Some(info)) = (&self.lib.root, &self.folder_info) {
                    ui.add_space(12.0);
                    if !info.has_structure {
                        ui.label(
                            RichText::new(
                                "The Games, Emulators and BIOS folders aren't there yet.",
                            )
                            .color(t.warning),
                        );
                        if ui.button("Create folders").clicked() {
                            act = Some(FolderAct::Create);
                        }
                    } else {
                        section_label(ui, "GAMES  ·  click a system to open its folder", t);
                        let (columns, cell) = grid(ui.available_width(), 200.0, 8.0);
                        for row in info.games_per_system.chunks(columns) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                for (p, count) in row {
                                    if folder_row(ui, t, cell, *p, *count) {
                                        act =
                                            Some(FolderAct::Open(workspace::system_dir(root, *p)));
                                    }
                                }
                            });
                            ui.add_space(4.0);
                        }
                        ui.add_space(8.0);
                        section_label(ui, "EMULATORS", t);
                        if info.emulators.is_empty() {
                            ui.label(
                                RichText::new(
                                    "Unzip emulators into the Emulators folder, then click Scan now.",
                                )
                                .size(12.5)
                                .color(t.text_weak),
                            );
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                for (name, _) in &info.emulators {
                                    pill(ui, &format!("✔ {name}"), t.button, t.text);
                                }
                            });
                        }
                        if ui
                            .link(
                                RichText::new("Open Emulators folder")
                                    .size(12.5)
                                    .color(t.gold),
                            )
                            .clicked()
                        {
                            act = Some(FolderAct::Open(root.join(workspace::EMULATORS)));
                        }
                        ui.add_space(8.0);
                        section_label(ui, "BIOS", t);
                        ui.label(
                            RichText::new("Keep BIOS files dumped from your own consoles here.")
                                .size(12.5)
                                .color(t.text_weak),
                        );
                        if ui
                            .link(RichText::new("Open BIOS folder").size(12.5).color(t.gold))
                            .clicked()
                        {
                            act = Some(FolderAct::Open(root.join(workspace::BIOS)));
                        }
                    }
                }
                ui.add_space(10.0);
                if ui
                    .checkbox(
                        &mut self.lib.scan_on_start,
                        "Scan this folder every time GameShelf starts",
                    )
                    .changed()
                {
                    self.dirty = true;
                }
            },
        );

        match act {
            Some(FolderAct::Choose(dir)) => {
                self.lib.root = Some(dir.clone());
                self.dirty = true;
                match workspace::create(&dir) {
                    Ok(()) => self.sync_folder(),
                    Err(e) => self.status = format!("Could not create the folders: {e}"),
                }
                self.folder_info = None;
            }
            Some(FolderAct::Create) => {
                if let Some(root) = self.lib.root.clone() {
                    match workspace::create(&root) {
                        Ok(()) => self.status = "Folders created.".to_string(),
                        Err(e) => self.status = format!("Could not create the folders: {e}"),
                    }
                    self.folder_info = None;
                }
            }
            Some(FolderAct::Scan) => self.sync_folder(),
            Some(FolderAct::Organize) => {
                if let Some(root) = self.lib.root.clone() {
                    match workspace::create(&root) {
                        Ok(()) => self.organize_plan = Some(organize::plan(&self.lib, &root)),
                        Err(e) => self.status = format!("Could not create the folders: {e}"),
                    }
                }
            }
            Some(FolderAct::Open(path)) => {
                if let Err(e) = workspace::open_folder(&path) {
                    self.status = e;
                }
            }
            None => {}
        }
    }

    fn other_sections(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        section(
            ui,
            t,
            "🎨  Appearance",
            "Burgundy, gold and beige, in dark or light.",
            |ui| {
                ui.horizontal(|ui| {
                    for (dark, label) in [(true, "🌙  Dark"), (false, "☀  Light")] {
                        if chip(ui, t, label, self.lib.dark_mode == dark)
                            && self.lib.dark_mode != dark
                        {
                            self.lib.dark_mode = dark;
                            self.theme = Theme::for_mode(dark);
                            self.theme.apply(ui.ctx());
                            self.dirty = true;
                        }
                    }
                });
            },
        );
        ui.add_space(GAP);

        let mut retry = false;
        section(
            ui,
            t,
            "🖼  Covers",
            "Box art comes from the free libretro-thumbnails collection.",
            |ui| {
                if ui
                    .checkbox(&mut self.lib.auto_covers, "Download covers automatically")
                    .changed()
                {
                    self.dirty = true;
                }
                let missing = self.lib.games.iter().filter(|g| g.cover.is_none()).count();
                ui.label(
                    RichText::new(format!("{missing} game(s) without a cover"))
                        .size(12.5)
                        .color(t.text_weak),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("🔍 Look for missing covers").clicked() {
                        retry = true;
                    }
                    if let Some(dir) = Library::covers_dir() {
                        if ui.button("📂 Open covers folder").clicked() {
                            let _ = std::fs::create_dir_all(&dir);
                            if let Err(e) = workspace::open_folder(&dir) {
                                self.status = e;
                            }
                        }
                    }
                });
            },
        );
        if retry {
            let keys: Vec<PathBuf> = self
                .lib
                .games
                .iter_mut()
                .filter(|g| g.cover.is_none())
                .map(|g| {
                    g.cover_missing = false;
                    g.path.clone()
                })
                .collect();
            for key in &keys {
                self.covers.forget(key);
                self.fetch_cover_for(key);
            }
            self.status = format!("Looking for {} cover(s)…", keys.len());
            self.dirty = true;
        }
        ui.add_space(GAP);

        section(
            ui,
            t,
            "💾  App data",
            "Your library list, emulator settings and covers are saved here.",
            |ui| {
                if let Some(dir) =
                    Library::covers_dir().and_then(|c| c.parent().map(Path::to_path_buf))
                {
                    ui.label(
                        RichText::new(dir.display().to_string())
                            .size(12.5)
                            .color(t.text),
                    );
                    if ui.button("📂 Open data folder").clicked() {
                        if let Err(e) = workspace::open_folder(&dir) {
                            self.status = e;
                        }
                    }
                }
            },
        );
    }

    fn emulators_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        ui.add_space(14.0);
        ui.label(RichText::new("Emulators").size(24.0).strong().color(t.text));
        ui.label(
            RichText::new(
                "GameShelf does not include emulators. Download the recommended one for each system, \
                 then point GameShelf to its .exe file.",
            )
            .color(t.text_weak),
        );
        ui.add_space(10.0);

        // One card per emulator; RetroArch covers several systems.
        let mut groups: Vec<(&'static str, Vec<Platform>)> = Vec::new();
        for p in ALL {
            let Some(info) = p.emulator() else { continue };
            match groups.iter_mut().find(|(name, _)| *name == info.name) {
                Some((_, systems)) => systems.push(p),
                None => groups.push((info.name, vec![p])),
            }
        }
        let is_ready =
            |systems: &[Platform]| systems.first().is_some_and(|p| self.lib.emulator_ready(*p));
        let ready = groups.iter().filter(|(_, s)| is_ready(s)).count();
        let total = groups.len();

        ui.horizontal(|ui| {
            pill(
                ui,
                &format!("{ready} of {total} ready"),
                BURGUNDY,
                Color32::WHITE,
            );
            ui.add_space(12.0);
            for (filter, label) in [
                (EmuFilter::All, "All"),
                (EmuFilter::Ready, "Ready"),
                (EmuFilter::NeedsSetup, "Needs setup"),
            ] {
                if chip(ui, t, label, self.emu_filter == filter) {
                    self.emu_filter = filter;
                }
            }
        });
        ui.add_space(14.0);

        let shown: Vec<(&'static str, Vec<Platform>)> = groups
            .into_iter()
            .filter(|(_, s)| match self.emu_filter {
                EmuFilter::All => true,
                EmuFilter::Ready => is_ready(s),
                EmuFilter::NeedsSetup => !is_ready(s),
            })
            .collect();
        if shown.is_empty() {
            ui.label(RichText::new("Nothing here.").color(t.text_weak));
            return;
        }

        let mut changed = false;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let (columns, cell) = grid(ui.available_width() - 6.0, TILE_MIN, GAP);
                for row in shown.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for (name, systems) in row {
                            match emulator_tile(ui, t, cell, name, systems, &mut self.lib) {
                                TileAction::Changed => changed = true,
                                TileAction::OpenSettings => self.editing = Some(name),
                                TileAction::ShowControls => self.controls_for = Some(name),
                                TileAction::None => {}
                            }
                        }
                    });
                    ui.add_space(GAP);
                }
            });
        if let Some(name) = self.editing {
            let systems: Vec<Platform> = ALL
                .into_iter()
                .filter(|p| p.emulator().is_some_and(|i| i.name == name))
                .collect();
            let mut open = true;
            egui::Window::new(RichText::new(format!("{name} settings")).strong())
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(520.0)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    changed |= emulator_settings(ui, t, &systems, &mut self.lib)
                });
            if !open {
                self.editing = None;
            }
        }
        if changed {
            self.dirty = true;
        }
    }
}

/// Narrowest an emulator card may get before the grid drops a column.
const TILE_MIN: f32 = 300.0;
const TILE_HEIGHT: f32 = 260.0;
const BUTTON_ROW: f32 = 32.0;
const TILE_BOTTOM_MARGIN: f32 = 16.0;
/// Narrowest a game card may get before the grid drops a column.
const GAME_CARD_MIN: f32 = 170.0;
const GAP: f32 = 18.0;

/// How many columns fit in `available`, and how wide each cell must be so the
/// row fills the whole width. Cells grow as the window grows.
fn grid(available: f32, min: f32, gap: f32) -> (usize, f32) {
    let available = available.max(min);
    let columns = ((available + gap) / (min + gap)).floor().max(1.0) as usize;
    let cell = (available - gap * (columns as f32 - 1.0)) / columns as f32;
    (columns, cell)
}

enum FolderAct {
    Choose(PathBuf),
    Create,
    Scan,
    Organize,
    Open(PathBuf),
}

/// A settings card with a title and a one-line description.
fn section(
    ui: &mut egui::Ui,
    t: Theme,
    title: &str,
    subtitle: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0_f32, t.border))
        .corner_radius(14.0)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(17.0).strong().color(t.text));
            ui.label(RichText::new(subtitle).size(12.5).color(t.text_weak));
            ui.add_space(10.0);
            add_contents(ui);
        });
}

/// One system folder in the settings overview. Returns true when clicked.
fn folder_row(ui: &mut egui::Ui, t: Theme, width: f32, p: Platform, count: usize) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let painter = ui.painter();
    let fill = if response.hovered() { t.button } else { t.side };
    painter.rect_filled(rect, 8.0, fill);
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(4.0, rect.height())),
        egui::CornerRadius {
            nw: 8,
            sw: 8,
            ne: 0,
            se: 0,
        },
        p.color(),
    );
    painter.text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        p.name(),
        FontId::proportional(13.5),
        t.text,
    );
    let (text, color) = if count == 0 {
        ("empty".to_string(), t.text_weak.gamma_multiply(0.7))
    } else {
        (count.to_string(), t.gold)
    };
    painter.text(
        pos2(rect.right() - 12.0, rect.center().y),
        Align2::RIGHT_CENTER,
        text,
        FontId::proportional(12.5),
        color,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Open Games\\{}", p.name()))
        .clicked()
}

enum TileAction {
    None,
    Changed,
    OpenSettings,
    ShowControls,
}

#[derive(PartialEq, Clone, Copy)]
enum EmuFilter {
    All,
    Ready,
    NeedsSetup,
}

/// A fixed-size card for one emulator and the systems it runs.
fn emulator_tile(
    ui: &mut egui::Ui,
    t: Theme,
    cell: f32,
    name: &str,
    systems: &[Platform],
    lib: &mut Library,
) -> TileAction {
    let (Some(&first), Some(info)) = (systems.first(), systems.first().and_then(|p| p.emulator()))
    else {
        return TileAction::None;
    };
    let ready = lib.emulator_ready(first);
    let exe = lib
        .emulators
        .get(&first)
        .map(|c| c.exe.trim().to_string())
        .unwrap_or_default();
    let mut action = TileAction::None;
    // The 1px border sits outside the content.
    let tile_width = cell - 2.0;

    let frame = egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0_f32, t.border))
        .corner_radius(14.0)
        .show(ui, |ui| {
            // The grid lays cards out left-to-right; the card itself stacks top-down.
            let size = vec2(tile_width, TILE_HEIGHT);
            ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_min_size(size);
                ui.set_max_width(tile_width);
                let card_top = ui.max_rect().top();
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);

                // Banner: emulator name on burgundy, underlined in its system colour.
                let (banner, _) = ui.allocate_exact_size(vec2(tile_width, 64.0), Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(
                    banner,
                    egui::CornerRadius {
                        nw: 14,
                        ne: 14,
                        sw: 0,
                        se: 0,
                    },
                    BURGUNDY,
                );
                painter.rect_filled(
                    Rect::from_min_size(
                        banner.left_bottom() - vec2(0.0, 4.0),
                        vec2(tile_width, 4.0),
                    ),
                    0.0,
                    first.color(),
                );
                painter.text(
                    banner.left_center() + vec2(16.0, -2.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(20.0),
                    Color32::WHITE,
                );
                let (status, fill) = if ready {
                    ("✔ Ready", t.success)
                } else {
                    ("Needs setup", t.gold)
                };
                let galley =
                    painter.layout_no_wrap(status.to_string(), FontId::proportional(12.0), INK);
                let pill_rect = Rect::from_min_size(
                    pos2(
                        banner.right() - galley.size().x - 32.0,
                        banner.center().y - galley.size().y / 2.0 - 6.0,
                    ),
                    galley.size() + vec2(18.0, 8.0),
                );
                painter.rect_filled(pill_rect, 10.0, fill);
                painter.galley(pill_rect.min + vec2(9.0, 4.0), galley, INK);

                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(14, 6))
                    .show(ui, |ui| {
                        ui.set_width(tile_width - 28.0);
                        ui.set_max_width(tile_width - 28.0);
                        let mut missing_cores = 0;
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
                            for p in systems {
                                let core = lib.emulators.get(p).and_then(core_installed);
                                if core == Some(false) {
                                    missing_cores += 1;
                                }
                                system_chip(ui, t, *p, core);
                            }
                        });
                        ui.add_space(2.0);
                        if missing_cores > 0 {
                            let plural = if missing_cores == 1 { "core" } else { "cores" };
                            ui.label(
                                RichText::new(format!(
                                    "⚠ {missing_cores} {plural} missing: RetroArch › Online Updater › Core Downloader"
                                ))
                                .size(12.0)
                                .color(t.warning),
                            );
                        }
                        if ready {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("📂  {exe}"))
                                        .size(12.0)
                                        .color(t.text_weak),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&exe);
                        } else if info.note.is_empty() {
                            ui.label(
                                RichText::new("Download it, then choose its .exe")
                                    .size(12.0)
                                    .color(t.text_weak),
                            );
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(info.note).size(12.0).color(t.warning),
                                )
                                .wrap(),
                            );
                        }
                    });

                // Buttons sit near the bottom, with a margin under them.
                let free = card_top + TILE_HEIGHT
                    - BUTTON_ROW
                    - TILE_BOTTOM_MARGIN
                    - ui.cursor().top();
                ui.add_space(free.max(8.0));
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        let choose_label = if ready { "Change .exe" } else { "Choose .exe" };
                        let (fill, color) = if ready {
                            (t.button, t.text)
                        } else {
                            (t.gold, INK)
                        };
                        let choose =
                            egui::Button::new(RichText::new(choose_label).strong().color(color))
                                .fill(fill)
                                .min_size(vec2(112.0, 32.0));
                        if ui.add(choose).clicked() {
                            if let Some(f) = rfd::FileDialog::new()
                                .add_filter("Program", &["exe"])
                                .pick_file()
                            {
                                let path = f.display().to_string();
                                for p in systems {
                                    lib.emulators.entry(*p).or_default().exe = path.clone();
                                }
                                action = TileAction::Changed;
                            }
                        }
                        let download = egui::Button::new(RichText::new("⬇ Download").color(t.text))
                            .fill(t.button)
                            .min_size(vec2(0.0, 32.0));
                        if ui.add(download).on_hover_text(info.url).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(info.url));
                        }
                        let gear = egui::Button::new(RichText::new("⚙").size(16.0).color(t.text))
                            .fill(t.button)
                            .min_size(vec2(36.0, 32.0));
                        if ui.add(gear).on_hover_text("Settings").clicked() {
                            action = TileAction::OpenSettings;
                        }
                        let pad = egui::Button::new(RichText::new("🎮").size(16.0).color(t.text))
                            .fill(t.button)
                            .min_size(vec2(36.0, 32.0));
                        if ui.add(pad).on_hover_text("Controls").clicked() {
                            action = TileAction::ShowControls;
                        }
                    });
                // A real allocation (not just a gap) so the card always ends below it.
                ui.allocate_space(vec2(1.0, TILE_BOTTOM_MARGIN));
            });
        });
    let rect = frame.response.rect;
    if ui.rect_contains_pointer(rect) {
        ui.painter()
            .rect_stroke(rect, 14.0, Stroke::new(1.5_f32, t.gold), StrokeKind::Inside);
    }
    action
}

/// The settings window body: program path and per-system arguments.
fn emulator_settings(ui: &mut egui::Ui, t: Theme, systems: &[Platform], lib: &mut Library) -> bool {
    let Some(&first) = systems.first() else {
        return false;
    };
    let Some(info) = first.emulator() else {
        return false;
    };
    let mut changed = false;
    let mut exe = lib
        .emulators
        .get(&first)
        .map(|c| c.exe.clone())
        .unwrap_or_default();

    ui.label(RichText::new("Program").strong().color(t.text));
    ui.horizontal(|ui| {
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut exe)
                    .hint_text(info.exe_hint)
                    .desired_width(400.0),
            )
            .changed();
        if ui.button("Browse…").clicked() {
            if let Some(f) = rfd::FileDialog::new()
                .add_filter("Program", &["exe"])
                .pick_file()
            {
                exe = f.display().to_string();
                changed = true;
            }
        }
    });
    if changed {
        for p in systems {
            lib.emulators.entry(*p).or_default().exe = exe.clone();
        }
    }
    ui.add_space(10.0);
    ui.label(RichText::new("Arguments").strong().color(t.text));
    ui.label(
        RichText::new("{rom} is replaced by the game file.")
            .size(12.0)
            .color(t.text_weak),
    );
    egui::Grid::new("args_grid")
        .num_columns(3)
        .spacing(vec2(8.0, 6.0))
        .show(ui, |ui| {
            for p in systems {
                let default = p.emulator().map_or("{rom}", |i| i.default_args);
                let cfg = lib.emulators.entry(*p).or_default();
                ui.label(RichText::new(p.name()).color(t.text_weak));
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut cfg.args).desired_width(300.0))
                    .changed();
                if ui.button("Reset").clicked() {
                    cfg.args = default.to_string();
                    changed = true;
                }
                ui.end_row();
            }
        });
    changed
}

/// For RetroArch: is the core named after `-L` in the arguments installed?
fn core_installed(cfg: &EmuConfig) -> Option<bool> {
    if !cfg.is_ready() {
        return None;
    }
    let mut tokens = cfg.args.split_whitespace();
    tokens.find(|t| *t == "-L")?;
    let core = Path::new(tokens.next()?);
    let full = if core.is_absolute() {
        core.to_path_buf()
    } else {
        cfg.exe_path().parent()?.join(core)
    };
    Some(full.is_file())
}

/// A system chip: dot, name, and for RetroArch a core mark (✔ installed, ⚠ missing).
/// Measured before it's placed, so wrapped rows know when to break the line.
fn system_chip(ui: &mut egui::Ui, t: Theme, p: Platform, core: Option<bool>) {
    let name =
        ui.painter()
            .layout_no_wrap(p.name().to_string(), FontId::proportional(12.5), t.text);
    let mark = match core {
        Some(true) => Some(("✔", t.success)),
        Some(false) => Some(("⚠", t.warning)),
        None => None,
    };
    let mark = mark.map(|(m, c)| {
        ui.painter()
            .layout_no_wrap(m.to_string(), FontId::proportional(12.5), c)
    });
    let (pad, dot, gap) = (9.0, 8.0, 5.0);
    let mark_width = mark.as_ref().map_or(0.0, |g| gap + g.size().x);
    let size = vec2(pad + dot + gap + name.size().x + mark_width + pad, 24.0);

    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, t.button);
    let mut x = rect.left() + pad;
    painter.circle_filled(pos2(x + dot / 2.0, rect.center().y), 3.5, p.color());
    x += dot + gap;
    painter.galley(
        pos2(x, rect.center().y - name.size().y / 2.0),
        name.clone(),
        t.text,
    );
    x += name.size().x + gap;
    if let Some(mark) = mark {
        painter.galley(pos2(x, rect.center().y - mark.size().y / 2.0), mark, t.text);
    }
    match core {
        Some(true) => {
            response.on_hover_text(format!("{} core installed", p.name()));
        }
        Some(false) => {
            response.on_hover_text(format!(
                "{} core missing. Get it in RetroArch: Main Menu › Online Updater › Core Downloader",
                p.name()
            ));
        }
        None => {}
    }
}

/// Adds a warning when RetroArch is open: it saves its settings on exit and would undo ours.
fn with_running_warning(mut msg: String) -> String {
    if controls::retroarch_running() {
        msg.push_str("   ·   Close RetroArch now, or it will undo this when it closes.");
    }
    msg
}

/// One column of the controls window. Returns the setting whose key was clicked.
fn controls_column(
    ui: &mut egui::Ui,
    t: Theme,
    title: &str,
    list: &[controls::Binding],
    editable: bool,
    listening: Option<&'static str>,
) -> Option<&'static str> {
    section_label(ui, title, t);
    let mut clicked = None;
    egui::Grid::new(title)
        .num_columns(2)
        .spacing(vec2(14.0, 7.0))
        .show(ui, |ui| {
            for b in list {
                let color = if b.careful { t.warning } else { t.text };
                ui.label(RichText::new(b.action).color(color));
                let waiting = b.setting.is_some() && b.setting == listening;
                if keycap(
                    ui,
                    t,
                    &b.key,
                    b.careful,
                    editable && b.setting.is_some(),
                    waiting,
                ) {
                    clicked = b.setting;
                }
                ui.end_row();
            }
        });
    clicked
}

/// A key drawn like a keyboard keycap. Editable keys can be clicked; `waiting`
/// shows the "press a key" state. Returns true when clicked.
fn keycap(
    ui: &mut egui::Ui,
    t: Theme,
    key: &str,
    careful: bool,
    editable: bool,
    waiting: bool,
) -> bool {
    let label = if waiting { "Press a key…" } else { key };
    let text_color = if waiting || careful { INK } else { t.text };
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), FontId::monospace(13.0), text_color);
    let size = vec2(galley.size().x.max(14.0) + 18.0, 26.0);
    let sense = if editable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = editable && response.hovered();
    let fill = if waiting || careful { t.gold } else { t.button };
    let stroke = if waiting || hovered {
        Stroke::new(2.0_f32, if waiting { BURGUNDY } else { t.gold })
    } else {
        Stroke::new(1.0_f32, t.border)
    };
    let painter = ui.painter();
    // A darker lip under the key gives it depth.
    painter.rect_filled(rect.translate(vec2(0.0, 2.0)), 6.0, t.border);
    painter.rect_filled(rect, 6.0, fill);
    painter.rect_stroke(rect, 6.0, stroke, StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, text_color);

    let clicked = editable && response.clicked();
    if clicked {
        // Otherwise Space/Enter would "click" this key again instead of being picked.
        response.surrender_focus();
    }
    if editable {
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click, then press the new key");
    }
    clicked
}

/// A small toggle chip. Returns true when clicked.
fn chip(ui: &mut egui::Ui, t: Theme, label: &str, selected: bool) -> bool {
    let (fill, color) = if selected {
        (BURGUNDY, Color32::WHITE)
    } else {
        (t.button, t.text)
    };
    let button = egui::Button::new(RichText::new(label).size(13.0).color(color))
        .fill(fill)
        .corner_radius(14.0)
        .min_size(vec2(0.0, 26.0));
    ui.add(button)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn header_tab(ui: &mut egui::Ui, label: &str, selected: bool, gold: Color32) -> bool {
    let color = if selected {
        gold
    } else {
        Color32::from_white_alpha(190)
    };
    let response = ui
        .add(egui::Button::new(RichText::new(label).size(15.5).strong().color(color)).frame(false));
    if selected {
        let r = response.rect;
        ui.painter().line_segment(
            [
                pos2(r.left(), r.bottom() + 6.0),
                pos2(r.right(), r.bottom() + 6.0),
            ],
            Stroke::new(2.5_f32, gold),
        );
    }
    response.clicked()
}

fn section_label(ui: &mut egui::Ui, text: &str, t: Theme) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(11.0).strong().color(t.gold));
    ui.add_space(2.0);
}

/// A full-width sidebar row with an optional coloured dot and a count.
fn side_row(
    ui: &mut egui::Ui,
    t: Theme,
    selected: bool,
    dot: Option<Color32>,
    label: &str,
    count: usize,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 7.0, BURGUNDY);
    } else if response.hovered() {
        painter.rect_filled(rect, 7.0, t.button);
    }
    let text_color = if selected { Color32::WHITE } else { t.text };
    let mut x = rect.left() + 12.0;
    if let Some(color) = dot {
        painter.circle_filled(pos2(x + 4.0, rect.center().y), 4.5, color);
        x += 18.0;
    }
    painter.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        text_color,
    );
    let count_color = if selected {
        Color32::from_white_alpha(210)
    } else {
        t.text_weak
    };
    painter.text(
        pos2(rect.right() - 12.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(12.5),
        count_color,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn pill(ui: &mut egui::Ui, text: &str, fill: Color32, color: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).strong().color(color))
        });
}

/// One game card. Returns what the user clicked, if anything.
fn game_card(
    ui: &mut egui::Ui,
    t: Theme,
    cell: f32,
    game: &Game,
    cover: Option<&Cover>,
    index: usize,
) -> Option<Action> {
    let mut action = None;
    // Inner margin (8 each side) and the 1px border sit outside the content.
    let width = cell - 18.0;
    let frame = egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0_f32, t.border))
        .corner_radius(12.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                let (area, response) =
                    ui.allocate_exact_size(vec2(width, width * COVER_ASPECT), Sense::click());
                draw_cover(ui, t, game, cover, area);
                if response.double_clicked() {
                    action = Some(Action::Play(index));
                }
                response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!("{}\nDouble-click to play", game.path.display()));

                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(RichText::new(&game.title).strong().size(14.5).color(t.text))
                        .truncate(),
                );
                ui.label(
                    RichText::new(played_text(game))
                        .size(11.5)
                        .color(t.text_weak),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let play =
                        egui::Button::new(RichText::new("▶  Play").strong().color(Color32::WHITE))
                            .fill(BURGUNDY)
                            .min_size(vec2(84.0, 28.0));
                    if ui.add(play).clicked() {
                        action = Some(Action::Play(index));
                    }
                    let star_color = if game.favorite {
                        t.gold
                    } else {
                        t.text_weak.gamma_multiply(0.6)
                    };
                    let star = egui::Button::new(RichText::new("⭐").color(star_color))
                        .min_size(vec2(30.0, 28.0));
                    if ui.add(star).on_hover_text("Favorite").clicked() {
                        action = Some(Action::ToggleFavorite(index));
                    }
                    ui.menu_button(RichText::new("⋯").strong(), |ui| {
                        if ui.button("🎮  Controls").clicked() {
                            action = Some(Action::ShowControls(index));
                            ui.close_menu();
                        }
                        if ui.button("🖼  Choose cover image…").clicked() {
                            action = Some(Action::PickCover(index));
                            ui.close_menu();
                        }
                        if ui.button("🔍  Find cover online").clicked() {
                            action = Some(Action::FindCover(index));
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.label(RichText::new("System").small().color(t.gold));
                        for p in ALL {
                            if ui.selectable_label(game.platform == p, p.name()).clicked() {
                                action = Some(Action::SetPlatform(index, p));
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("Remove from library").clicked() {
                            action = Some(Action::Remove(index));
                            ui.close_menu();
                        }
                    });
                });
            });
        });
    let rect = frame.response.rect;
    if ui.rect_contains_pointer(rect) {
        ui.painter()
            .rect_stroke(rect, 12.0, Stroke::new(1.5_f32, t.gold), StrokeKind::Inside);
    }
    action
}

fn draw_cover(ui: &egui::Ui, t: Theme, game: &Game, cover: Option<&Cover>, area: Rect) {
    let painter = ui.painter();
    painter.rect_filled(area, 8.0, t.cover_bg);
    match cover {
        Some(Cover::Ready(texture)) => {
            let size = texture.size_vec2();
            let scale = (area.width() / size.x).min(area.height() / size.y);
            let rect = Rect::from_center_size(area.center(), size * scale);
            painter.image(
                texture.id(),
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        other => {
            // Placeholder: burgundy panel with gold initials.
            painter.rect_filled(
                area.shrink(10.0),
                6.0,
                BURGUNDY.gamma_multiply(if t.dark { 0.55 } else { 0.85 }),
            );
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                initials(&game.title),
                FontId::proportional(44.0),
                t.gold,
            );
            if matches!(other, Some(Cover::Pending)) {
                painter.text(
                    area.center_bottom() - vec2(0.0, 20.0),
                    Align2::CENTER_CENTER,
                    "loading cover…",
                    FontId::proportional(11.0),
                    Color32::from_white_alpha(170),
                );
            }
        }
    }
    // System badge.
    let galley = painter.layout_no_wrap(
        game.platform.short().to_string(),
        FontId::proportional(11.5),
        Color32::WHITE,
    );
    let badge = Rect::from_min_size(
        area.left_top() + vec2(8.0, 8.0),
        galley.size() + vec2(12.0, 6.0),
    );
    painter.rect_filled(badge, 6.0, game.platform.color().gamma_multiply(0.9));
    painter.galley(badge.min + vec2(6.0, 3.0), galley, Color32::WHITE);
    if game.favorite {
        painter.text(
            area.right_top() + vec2(-10.0, 8.0),
            Align2::RIGHT_TOP,
            "⭐",
            FontId::proportional(15.0),
            t.gold,
        );
    }
}

fn initials(title: &str) -> String {
    let letters: String = title
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .collect();
    if letters.is_empty() {
        "?".to_string()
    } else {
        letters.to_uppercase()
    }
}

fn played_text(game: &Game) -> String {
    let Some(at) = game.last_played else {
        return "Never played".to_string();
    };
    let ago = now_secs().saturating_sub(at);
    let when = match ago {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", ago / 60),
        3600..=86_399 => format!("{} h ago", ago / 3600),
        _ => format!("{} days ago", ago / 86_400),
    };
    format!("Played {when} · {}×", game.play_count)
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
