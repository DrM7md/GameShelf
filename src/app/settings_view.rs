//! The Settings tab: GameShelf folder, saves, appearance, covers and app data.

use super::*;

impl GameShelfApp {
    pub(super) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        if self.folder_info.is_none() {
            self.folder_info = self.lib.root.as_deref().map(workspace::inspect);
            self.emu_saves = self.find_emulator_saves();
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
                            left.add_space(GAP);
                            self.progress_section(left);
                            self.other_sections(right);
                        }
                    });
                } else {
                    self.folder_section(ui);
                    ui.add_space(GAP);
                    self.progress_section(ui);
                    ui.add_space(GAP);
                    self.other_sections(ui);
                }
                ui.add_space(20.0);
            });
    }

    pub(super) fn folder_section(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn saves_section(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        let exe = self.retroarch_exe();
        let dirs = exe.as_deref().and_then(saves::retroarch_dirs);
        let root = self.lib.root.clone();
        let mut act = None;
        section(
            ui,
            t,
            "💾  Saves & cheats",
            "Your progress: in-game saves, save states (F2 save · F4 load) and cheats.",
            |ui| {
                let Some(d) = &dirs else {
                    ui.label(
                        RichText::new("Set up RetroArch in the Emulators tab first.")
                            .color(t.text_weak),
                    );
                    return;
                };
                egui::Grid::new("save_dirs")
                    .num_columns(2)
                    .spacing(vec2(12.0, 6.0))
                    .show(ui, |ui| {
                        for (label, path) in [
                            ("In-game saves", &d.saves),
                            ("Save states", &d.states),
                            ("Cheats", &d.cheats),
                        ] {
                            ui.label(RichText::new(label).color(t.text_weak));
                            if ui
                                .link(
                                    RichText::new(path.display().to_string())
                                        .size(12.5)
                                        .color(t.gold),
                                )
                                .on_hover_text("Open this folder")
                                .clicked()
                            {
                                act = Some(SaveAct::Open(path.clone()));
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
                match &root {
                    Some(root) if d.inside(root) => {
                        ui.label(
                            RichText::new("✔ Kept in your GameShelf folder, next to your games.")
                                .strong()
                                .color(t.success),
                        );
                    }
                    Some(_) => {
                        let keep = egui::Button::new(
                            RichText::new("Keep them in my GameShelf folder")
                                .strong()
                                .color(INK),
                        )
                        .fill(t.gold)
                        .min_size(vec2(0.0, 30.0));
                        if ui.add(keep).clicked() {
                            act = Some(SaveAct::UseFolder);
                        }
                        ui.label(
                            RichText::new("Existing saves are copied to Saves, States and Cheats; the originals stay.")
                                .size(12.0)
                                .color(t.text_weak),
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new(
                                "Choose a GameShelf folder to keep saves with your games.",
                            )
                            .size(12.5)
                            .color(t.text_weak),
                        );
                    }
                }
                ui.add_space(6.0);
                let mut resume = d.resume;
                if ui
                    .checkbox(&mut resume, "Resume where I left off")
                    .on_hover_text("RetroArch saves a state when a game closes and continues from it next time")
                    .changed()
                {
                    act = Some(SaveAct::Resume(resume));
                }
            },
        );

        let (Some(act), Some(exe)) = (act, exe) else {
            return;
        };
        if let SaveAct::Open(path) = &act {
            let _ = std::fs::create_dir_all(path);
            if let Err(e) = workspace::open_folder(path) {
                self.status = e;
            }
            return;
        }
        if controls::is_running(&exe) {
            self.status = "Close RetroArch first: it rewrites its settings when it closes.".into();
            return;
        }
        self.status = match act {
            SaveAct::UseFolder => match root.map(|r| saves::use_gameshelf_folder(&exe, &r)) {
                Some(Ok(n)) => format!("Saves, states and cheats now go to your GameShelf folder ({n} file(s) copied)."),
                Some(Err(e)) => format!("Could not change the save folders: {e}"),
                None => return,
            },
            SaveAct::Resume(on) => match saves::set_resume(&exe, on) {
                Ok(()) if on => "Games will continue where you left off.".into(),
                Ok(()) => "Resume turned off.".into(),
                Err(e) => format!("Could not save: {e}"),
            },
            SaveAct::Open(_) => return,
        };
    }

    /// Save folders of every standalone emulator that's set up.
    fn find_emulator_saves(&self) -> Vec<saves::EmuSaves> {
        saves::STANDALONE
            .into_iter()
            .filter_map(|name| {
                let exe = ALL
                    .into_iter()
                    .filter(|p| p.emulator().is_some_and(|i| i.name == name))
                    .find_map(|p| self.lib.emulators.get(&p).filter(|c| c.is_ready()))
                    .map(|c| c.exe_path())?;
                Some(saves::standalone_saves(name, &exe))
            })
            .collect()
    }

    /// Every emulator's saves in one place: linked into the GameShelf folder, and
    /// backed up to a single zip.
    pub(super) fn progress_section(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        let root = self.lib.root.clone();
        let backing_up = self.backup_rx.is_some();
        let mut act = None;
        section(
            ui,
            t,
            "🗂  All your progress",
            "Memory cards and saves of every emulator, in your GameShelf folder and backed up in one click.",
            |ui| {
                if self.emu_saves.is_empty() {
                    ui.label(
                        RichText::new(
                            "Set up DuckStation, PCSX2, PPSSPP, Dolphin, RPCS3 or Flycast in the Emulators tab to see their saves here.",
                        )
                        .size(12.5)
                        .color(t.text_weak),
                    );
                }
                let mut unlinked = 0;
                for emu in &self.emu_saves {
                    ui.label(RichText::new(emu.emulator).strong().color(t.text));
                    if emu.folders.is_empty() {
                        ui.label(
                            RichText::new("No saves yet. Play a game in it first.")
                                .size(12.0)
                                .color(t.text_weak),
                        );
                    }
                    for f in &emu.folders {
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.label(RichText::new(f.label).size(12.5).color(t.text_weak));
                            if ui
                                .link(
                                    RichText::new(f.path.display().to_string())
                                        .size(12.0)
                                        .color(t.gold),
                                )
                                .on_hover_text("Open this folder")
                                .clicked()
                            {
                                act = Some(ProgressAct::Open(f.path.clone()));
                            }
                            if let Some(root) = &root {
                                let link = saves::link_path(root, emu.emulator, f.kind);
                                if saves::is_linked(&link, &f.path) {
                                    ui.label(RichText::new("✔ linked").size(12.0).color(t.success));
                                } else {
                                    unlinked += 1;
                                }
                            }
                        });
                    }
                    ui.add_space(4.0);
                }
                if root.is_some() && unlinked > 0 {
                    ui.add_space(4.0);
                    let link = egui::Button::new(
                        RichText::new("🔗 Link them into my GameShelf folder").strong().color(INK),
                    )
                    .fill(t.gold)
                    .min_size(vec2(0.0, 30.0));
                    if ui
                        .add(link)
                        .on_hover_text("Shows them in Saves\\<emulator> and States\\<emulator>. Nothing is moved or changed.")
                        .clicked()
                    {
                        act = Some(ProgressAct::LinkAll);
                    }
                }
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let backup = egui::Button::new(
                        RichText::new("📦 Back up all my saves").strong().color(Color32::WHITE),
                    )
                    .fill(BURGUNDY)
                    .min_size(vec2(0.0, 30.0));
                    if ui
                        .add_enabled(!backing_up, backup)
                        .on_hover_text("RetroArch and every emulator above, into one .zip")
                        .clicked()
                    {
                        act = Some(ProgressAct::Backup);
                    }
                    let restore = egui::Button::new(RichText::new("♻ Restore a backup…").color(t.text))
                        .min_size(vec2(0.0, 30.0));
                    if ui
                        .add_enabled(!backing_up && self.restore_rx.is_none(), restore)
                        .on_hover_text("Put the saves from a GameShelf backup .zip back. You'll see what goes where first.")
                        .clicked()
                    {
                        act = Some(ProgressAct::Restore);
                    }
                    if backing_up {
                        ui.spinner();
                        ui.label(RichText::new("Backing up…").color(t.text_weak));
                    } else if ui
                        .link(RichText::new("Open backups folder").size(12.5).color(t.gold))
                        .clicked()
                    {
                        act = Some(ProgressAct::Open(self.backups_dir()));
                    }
                });
                ui.label(
                    RichText::new("Keep the .zip somewhere safe (a USB stick, OneDrive, Google Drive).")
                        .size(11.5)
                        .color(t.text_weak),
                );
            },
        );

        match act {
            Some(ProgressAct::Open(path)) => {
                let _ = std::fs::create_dir_all(&path);
                if let Err(e) = workspace::open_folder(&path) {
                    self.status = e;
                }
            }
            Some(ProgressAct::Restore) => {
                let picked = rfd::FileDialog::new()
                    .set_directory(self.backups_dir())
                    .add_filter("GameShelf backup", &["zip"])
                    .pick_file();
                if let Some(zip) = picked {
                    match saves::plan_restore(&zip, &self.save_sources()) {
                        Ok(plan) => self.restore_plan = Some(plan),
                        Err(e) => self.status = e,
                    }
                }
            }
            Some(ProgressAct::LinkAll) => {
                if let Some(root) = &root {
                    self.status = match saves::link_all(root, &self.emu_saves) {
                        Ok(n) => format!("Linked {n} save folder(s) into your GameShelf folder."),
                        Err(e) => format!("Could not link: {e}"),
                    };
                    self.folder_info = None;
                }
            }
            Some(ProgressAct::Backup) => {
                let retroarch = self
                    .retroarch_exe()
                    .as_deref()
                    .and_then(saves::retroarch_dirs);
                let sources = saves::backup_sources(retroarch.as_ref(), &self.emu_saves);
                if sources.is_empty() {
                    self.status = "There are no saves to back up yet.".into();
                } else {
                    let out = self.backups_dir();
                    let (tx, rx) = channel();
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(saves::make_backup(&sources, &out));
                        ctx.request_repaint();
                    });
                    self.backup_rx = Some(rx);
                }
            }
            None => {}
        }
    }

    /// `GameShelf\Backups`, or `Documents\GameShelf Backups` without a GameShelf folder.
    pub(super) fn backups_dir(&self) -> PathBuf {
        match &self.lib.root {
            Some(root) => root.join("Backups"),
            None => workspace::default_root()
                .and_then(|r| r.parent().map(|d| d.join("GameShelf Backups")))
                .unwrap_or_else(|| PathBuf::from("GameShelf Backups")),
        }
    }

    fn free_games_section(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        let mut reload = false;
        section(
            ui,
            t,
            "🎁  Free games",
            "Where the Free games tab gets its games.",
            |ui| {
                if ui
                    .checkbox(
                        &mut self.lib.homebrew_hub,
                        "Homebrew Hub (Game Boy, Game Boy Color, Game Boy Advance, NES)",
                    )
                    .changed()
                {
                    reload = true;
                }
                ui.add_space(6.0);
                ui.label(RichText::new("My list ★ (a free-games.json on GitHub)").color(t.text));
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.lib.free_list_url)
                        .desired_width((ui.available_width() - 80.0).max(200.0));
                    if ui.add(edit).lost_focus() {
                        reload = true;
                    }
                    if ui.button("Reset").clicked() {
                        self.lib.free_list_url = crate::freegames::DEFAULT_LIST_URL.to_string();
                        reload = true;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("🔄 Reload the lists").clicked() {
                        reload = true;
                    }
                    if let Some(page) = github_page(&self.lib.free_list_url) {
                        if ui
                            .link(
                                RichText::new("Edit my list on GitHub")
                                    .size(12.5)
                                    .color(t.gold),
                            )
                            .on_hover_text(&page)
                            .clicked()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(page));
                        }
                    }
                });
            },
        );
        if reload {
            self.free.reset();
            self.dirty = true;
        }
    }

    pub(super) fn other_sections(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        self.saves_section(ui);
        ui.add_space(GAP);
        self.free_games_section(ui);
        ui.add_space(GAP);
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
}

pub(super) enum SaveAct {
    UseFolder,
    Resume(bool),
    Open(PathBuf),
}

enum ProgressAct {
    LinkAll,
    Backup,
    Restore,
    Open(PathBuf),
}

pub(super) enum FolderAct {
    Choose(PathBuf),
    Create,
    Scan,
    Organize,
    Open(PathBuf),
}

/// A settings card with a title and a one-line description.
pub(super) fn section(
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
pub(super) fn folder_row(
    ui: &mut egui::Ui,
    t: Theme,
    width: f32,
    p: Platform,
    count: usize,
) -> bool {
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

/// `raw.githubusercontent.com/<owner>/<repo>/<branch>/<path>` → its page on github.com.
fn github_page(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("https://raw.githubusercontent.com/")?;
    let mut parts = rest.splitn(4, '/');
    let (owner, repo, branch, path) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    Some(format!(
        "https://github.com/{owner}/{repo}/blob/{branch}/{path}"
    ))
}
