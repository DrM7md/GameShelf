//! The Emulators tab: one card per emulator, and its settings window.

use super::*;

impl GameShelfApp {
    pub(super) fn emulators_tab(&mut self, ui: &mut egui::Ui) {
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
        let mut install_request: Option<&'static str> = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let (columns, cell) = grid(ui.available_width() - 6.0, TILE_MIN, GAP);
                for row in shown.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for (name, systems) in row {
                            let progress = self.install_view(name);
                            let tile = emulator_tile(
                                ui,
                                t,
                                cell,
                                name,
                                systems,
                                &mut self.lib,
                                progress.as_ref(),
                            );
                            match tile {
                                TileAction::Changed => changed = true,
                                TileAction::OpenSettings => self.editing = Some(name),
                                TileAction::ShowControls => self.controls_for = Some(name),
                                TileAction::Install => install_request = Some(*name),
                                TileAction::None => {}
                            }
                        }
                    });
                    ui.add_space(GAP);
                }
            });
        if let Some(name) = install_request {
            self.start_install(name, ui.ctx());
        }
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
pub(super) const TILE_MIN: f32 = 300.0;

pub(super) const TILE_HEIGHT: f32 = 296.0;

pub(super) const BUTTON_HEIGHT: f32 = 32.0;

pub(super) const BUTTON_GAP: f32 = 8.0;

/// Both button rows: tools, then the main buttons.
pub(super) const BUTTON_ROWS: f32 = BUTTON_HEIGHT * 2.0 + BUTTON_GAP;

pub(super) const TILE_BOTTOM_MARGIN: f32 = 16.0;

pub(super) enum TileAction {
    None,
    Changed,
    OpenSettings,
    ShowControls,
    Install,
}

/// What an emulator card shows while that emulator is being installed.
pub(super) struct InstallView {
    pub(super) status: String,
    /// Download progress (0–1), when the size is known.
    pub(super) fraction: Option<f32>,
}

#[derive(PartialEq, Clone, Copy)]
pub(super) enum EmuFilter {
    All,
    Ready,
    NeedsSetup,
}

/// A fixed-size card for one emulator and the systems it runs.
pub(super) fn emulator_tile(
    ui: &mut egui::Ui,
    t: Theme,
    cell: f32,
    name: &str,
    systems: &[Platform],
    lib: &mut Library,
    installing: Option<&InstallView>,
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
                        if let Some(iv) = installing {
                            let bar = egui::ProgressBar::new(iv.fraction.unwrap_or(0.0))
                                .fill(BURGUNDY)
                                .desired_height(10.0);
                            ui.add(if iv.fraction.is_some() { bar } else { bar.animate(true) });
                            ui.label(RichText::new(&iv.status).size(12.0).color(t.text));
                        } else if ready {
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

                // Buttons sit near the bottom, with a margin under them: a row of small
                // tool icons, then the two main buttons sharing the card's width.
                let free = card_top + TILE_HEIGHT
                    - BUTTON_ROWS
                    - TILE_BOTTOM_MARGIN
                    - ui.cursor().top();
                ui.add_space(free.max(8.0));
                let inner = tile_width - 28.0;
                // Not set up yet and GameShelf can fetch it: Install is the main button.
                let offer_install = !ready && install::can_install(name);
                let busy = installing.is_some();

                // Tools, right-aligned: (🌐) ⚙ 🎮
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.allocate_ui_with_layout(
                        vec2(inner, BUTTON_HEIGHT),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if busy {
                                ui.disable();
                            }
                            let icon = |glyph: &str| {
                                egui::Button::new(RichText::new(glyph).size(16.0).color(t.text))
                                    .fill(t.button)
                                    .min_size(vec2(36.0, BUTTON_HEIGHT))
                            };
                            if ui.add(icon("🎮")).on_hover_text("Controls").clicked() {
                                action = TileAction::ShowControls;
                            }
                            if ui.add(icon("⚙")).on_hover_text("Settings").clicked() {
                                action = TileAction::OpenSettings;
                            }
                            // With Install as a main button, the website moves up here.
                            if offer_install
                                && ui
                                    .add(icon("🌐"))
                                    .on_hover_text(format!("Open the download page: {}", info.url))
                                    .clicked()
                            {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(info.url));
                            }
                        },
                    );
                });
                ui.add_space(BUTTON_GAP);

                // Main buttons: [Install | Choose .exe], [Choose .exe | Download] or
                // [Change .exe | Download], each half of the card.
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    if busy {
                        ui.disable();
                    }
                    let half = (inner - ui.spacing().item_spacing.x) / 2.0;
                    let button = |label: &str, primary: bool| {
                        let (fill, color) = if primary { (t.gold, INK) } else { (t.button, t.text) };
                        egui::Button::new(RichText::new(label).strong().color(color))
                            .fill(fill)
                            .min_size(vec2(half, BUTTON_HEIGHT))
                    };
                    let mut choose_exe = |ui: &mut egui::Ui, primary: bool| -> bool {
                        let label = if ready { "Change .exe" } else { "Choose .exe" };
                        if !ui.add(button(label, primary)).clicked() {
                            return false;
                        }
                        let Some(f) = rfd::FileDialog::new().add_filter("Program", &["exe"]).pick_file()
                        else {
                            return false;
                        };
                        let path = f.display().to_string();
                        for p in systems {
                            lib.emulators.entry(*p).or_default().exe = path.clone();
                        }
                        true
                    };
                    if offer_install {
                        if ui
                            .add(button("⬇ Install", true))
                            .on_hover_text(
                                "Download the latest official build and set it up in your GameShelf folder",
                            )
                            .clicked()
                        {
                            action = TileAction::Install;
                        }
                        if choose_exe(ui, false) {
                            action = TileAction::Changed;
                        }
                    } else {
                        if choose_exe(ui, !ready) {
                            action = TileAction::Changed;
                        }
                        if ui
                            .add(button("⬇ Download", false))
                            .on_hover_text(format!("Open the download page: {}", info.url))
                            .clicked()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(info.url));
                        }
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
pub(super) fn emulator_settings(
    ui: &mut egui::Ui,
    t: Theme,
    systems: &[Platform],
    lib: &mut Library,
) -> bool {
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
pub(super) fn core_installed(cfg: &EmuConfig) -> Option<bool> {
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
pub(super) fn system_chip(ui: &mut egui::Ui, t: Theme, p: Platform, core: Option<bool>) {
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
