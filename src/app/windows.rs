//! Pop-up windows: controls (with key changing) and "Organize my games".

use super::*;

impl GameShelfApp {
    /// Applies what the background mover reports.
    pub(super) fn poll_organize(&mut self) {
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
    pub(super) fn controls_window(&mut self, ctx: &egui::Context) {
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
                    self.listening = None;
                    self.controls_note = Some(match controls::set_key(name, exe, setting, key) {
                        Ok(msg) => with_running_warning(msg, exe, name),
                        Err(e) => e,
                    });
                }
            }
        }

        let controls = controls::for_emulator(name, exe.as_deref());
        let can_undo = exe
            .as_deref()
            .is_some_and(|e| controls::has_backup(name, e));
        let mut undo = false;
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
                    if editable && name == "RetroArch" {
                        if ui.button("↺  Reset to RetroArch defaults").clicked() {
                            reset = true;
                        }
                    }
                    if can_undo
                        && ui
                            .button("↶  Undo all my key changes")
                            .on_hover_text(
                                "Puts the settings file back as it was before GameShelf changed it",
                            )
                            .clicked()
                    {
                        undo = true;
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
                            exe,
                            name,
                        ),
                        Err(e) => format!("Could not save: {e}"),
                    },
                );
            }
        }
        if undo {
            if let Some(exe) = exe.as_deref() {
                self.listening = None;
                self.controls_note = Some(match controls::restore_backup(name, exe) {
                    Ok(()) => with_running_warning(
                        "✔ Your keys are back to how they were before GameShelf.".into(),
                        exe,
                        name,
                    ),
                    Err(e) => format!("Could not restore: {e}"),
                });
            }
        }
        if !open {
            self.controls_for = None;
            self.listening = None;
            self.controls_note = None;
        }
    }

    /// "Restore a backup": what goes back where, then confirm. Today's saves are
    /// backed up first, so a restore can itself be undone.
    pub(super) fn restore_window(&mut self, ctx: &egui::Context) {
        let t = self.theme;
        let Some(plan) = &self.restore_plan else {
            return;
        };
        let (mut start, mut close) = (false, false);
        egui::Window::new(RichText::new("♻  Restore a backup").strong())
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .default_width(620.0)
            .show(ctx, |ui| {
                let file = plan.zip.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                ui.label(RichText::new(file).strong().color(t.text));
                ui.add_space(8.0);
                let total: usize = plan.items.iter().map(|it| it.files).sum();
                if plan.items.is_empty() {
                    ui.label(
                        RichText::new("None of these saves can go back yet: their emulators aren't set up.")
                            .color(t.warning),
                    );
                } else {
                    section_label(ui, "WILL BE RESTORED", t);
                    egui::Grid::new("restore_items").num_columns(3).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
                        for it in &plan.items {
                            ui.label(RichText::new(&it.name).strong().color(t.text));
                            ui.label(RichText::new(format!("→ {}", it.target.display())).size(12.0).color(t.text_weak));
                            ui.label(RichText::new(format!("{} file(s)", it.files)).size(12.0).color(t.gold));
                            ui.end_row();
                        }
                    });
                }
                if !plan.skipped.is_empty() {
                    ui.add_space(8.0);
                    section_label(ui, "SKIPPED (EMULATOR NOT SET UP)", t);
                    for (name, files) in &plan.skipped {
                        ui.label(RichText::new(format!("{name}  ·  {files} file(s)")).size(12.5).color(t.warning));
                    }
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "Saves in the backup replace today's copies of the same files; other files stay. \
                         Your current saves are backed up first, so you can undo this.",
                    )
                    .size(12.0)
                    .color(t.text_weak),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {

                    if total > 0 {
                        let go = egui::Button::new(RichText::new(format!("Restore {total} file(s)")).strong().color(INK))
                            .fill(t.gold)
                            .min_size(vec2(0.0, 32.0));
                        if ui.add(go).clicked() {
                            start = true;
                        }
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(0.0, 32.0))).clicked() {
                        close = true;
                    }
                });
            });

        if close {
            self.restore_plan = None;
        }
        if start {
            // Emulators rewrite their saves while running; restoring under them would be lost.
            if let Some(open) = self
                .emulator_exes()
                .iter()
                .find(|exe| controls::is_running(exe))
            {
                let name = open
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.status = format!("Close {name} first, then restore.");
                return;
            }
            let Some(plan) = self.restore_plan.take() else {
                return;
            };
            let sources = self.save_sources();
            let out = self.backups_dir();
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = saves::make_backup(&sources, &out)
                    .map_err(|e| {
                        format!(
                            "Could not back up today's saves first, so nothing was restored: {e}"
                        )
                    })
                    .and_then(|(safety, _)| saves::restore(&plan).map(|n| (n, safety)));
                let _ = tx.send(result);
                ctx.request_repaint();
            });
            self.restore_rx = Some(rx);
            self.status = "Restoring your saves…".into();
        }
    }

    /// The plan preview (confirm / cancel) and, while moving, a progress window.
    pub(super) fn organize_window(&mut self, ctx: &egui::Context) {
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
}

/// Adds a warning when the emulator is open: emulators save their settings on exit,
/// which would undo ours.
pub(super) fn with_running_warning(mut msg: String, exe: &Path, name: &str) -> String {
    if controls::is_running(exe) {
        msg.push_str(&format!(
            "   ·   Close {name} now, or it will undo this when it closes."
        ));
    }
    msg
}

/// One column of the controls window. Returns the setting whose key was clicked.
pub(super) fn controls_column(
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
pub(super) fn keycap(
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
