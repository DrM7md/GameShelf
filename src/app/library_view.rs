//! The Library tab: systems sidebar, game cards and covers.

use super::*;

impl GameShelfApp {
    /// Indices of the games to show, filtered and sorted.
    pub(super) fn visible(&self) -> Vec<usize> {
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

    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn library_tab(&mut self, ui: &mut egui::Ui) {
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
        // "Continue playing" only on the plain All games view.
        let browsing_all =
            self.filter.is_none() && !self.favorites_only && self.search.trim().is_empty();
        let mut recent: Vec<(usize, u64)> = self
            .lib
            .games
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.last_played.map(|at| (i, at)))
            .collect();
        recent.sort_by_key(|&(_, at)| std::cmp::Reverse(at));

        let mut action = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_space(14.0);
                if browsing_all && !recent.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Continue playing")
                                .size(20.0)
                                .strong()
                                .color(t.text),
                        );
                        ui.label(RichText::new("pick up where you left off").color(t.text_weak));
                    });
                    ui.add_space(8.0);
                    let (columns, cell) = grid(ui.available_width() - 6.0, CONTINUE_MIN, GAP);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for &(i, _) in recent.iter().take(columns) {
                            if let Some(game) = self.lib.games.get(i) {
                                let cover = self.covers.get(&game.path);
                                if let Some(a) = continue_card(ui, t, cell, game, cover, i) {
                                    action = Some(a);
                                }
                            }
                        }
                    });
                    ui.add_space(24.0);
                }

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
}

/// Box art is roughly 3:4, so covers are a bit taller than wide.
pub(super) const COVER_ASPECT: f32 = 1.34;

/// Narrowest a game card may get before the grid drops a column.
pub(super) const GAME_CARD_MIN: f32 = 170.0;

/// "Continue playing" cards: one row, as many as fit.
const CONTINUE_MIN: f32 = 290.0;
const CONTINUE_HEIGHT: f32 = 116.0;

/// A wide card in the "Continue playing" row: cover, title, when, and a big button.
fn continue_card(
    ui: &mut egui::Ui,
    t: Theme,
    cell: f32,
    game: &Game,
    cover: Option<&Cover>,
    index: usize,
) -> Option<Action> {
    let mut action = None;
    let width = cell - 2.0;
    let frame = egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0_f32, t.border))
        .corner_radius(12.0)
        .show(ui, |ui| {
            let size = vec2(width, CONTINUE_HEIGHT);
            ui.allocate_ui_with_layout(
                size,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(size);
                    ui.add_space(10.0);
                    let thumb_height = CONTINUE_HEIGHT - 20.0;
                    let thumb = vec2(thumb_height / COVER_ASPECT, thumb_height);
                    let (area, response) = ui.allocate_exact_size(thumb, Sense::click());
                    draw_cover(ui, t, game, cover, area);
                    if response.double_clicked() {
                        action = Some(Action::Play(index));
                    }
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width((width - thumb.x - 44.0).max(80.0));
                        ui.add(
                            egui::Label::new(
                                RichText::new(&game.title).strong().size(16.0).color(t.text),
                            )
                            .truncate(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{}  ·  {}",
                                game.platform.name(),
                                played_text(game)
                            ))
                            .size(12.0)
                            .color(t.text_weak),
                        );
                        ui.add_space(8.0);
                        let play = egui::Button::new(
                            RichText::new("▶  Continue").strong().color(Color32::WHITE),
                        )
                        .fill(BURGUNDY)
                        .min_size(vec2(124.0, 30.0));
                        if ui.add(play).clicked() {
                            action = Some(Action::Play(index));
                        }
                    });
                },
            );
        });
    let rect = frame.response.rect;
    if ui.rect_contains_pointer(rect) {
        ui.painter()
            .rect_stroke(rect, 12.0, Stroke::new(1.5_f32, t.gold), StrokeKind::Inside);
    }
    action
}

/// A full-width sidebar row with an optional coloured dot and a count.
pub(super) fn side_row(
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

/// One game card. Returns what the user clicked, if anything.
pub(super) fn game_card(
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

pub(super) fn draw_cover(ui: &egui::Ui, t: Theme, game: &Game, cover: Option<&Cover>, area: Rect) {
    draw_art(
        ui,
        t,
        &game.title,
        game.platform,
        game.favorite,
        cover,
        area,
    );
}

/// Box art (or a placeholder with the initials), the system badge and a favorite star.
pub(super) fn draw_art(
    ui: &egui::Ui,
    t: Theme,
    title: &str,
    platform: Platform,
    favorite: bool,
    cover: Option<&Cover>,
    area: Rect,
) {
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
                initials(title),
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
        platform.short().to_string(),
        FontId::proportional(11.5),
        Color32::WHITE,
    );
    let badge = Rect::from_min_size(
        area.left_top() + vec2(8.0, 8.0),
        galley.size() + vec2(12.0, 6.0),
    );
    painter.rect_filled(badge, 6.0, platform.color().gamma_multiply(0.9));
    painter.galley(badge.min + vec2(6.0, 3.0), galley, Color32::WHITE);
    if favorite {
        painter.text(
            area.right_top() + vec2(-10.0, 8.0),
            Align2::RIGHT_TOP,
            "⭐",
            FontId::proportional(15.0),
            t.gold,
        );
    }
}

pub(super) fn initials(title: &str) -> String {
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

pub(super) fn played_text(game: &Game) -> String {
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
