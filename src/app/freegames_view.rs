//! The Free games tab: homebrew from Homebrew Hub, one click to add to the library.

use super::library_view::{draw_art, COVER_ASPECT, GAME_CARD_MIN};
use super::*;
use crate::freegames::{self, CatalogMsg, FreeGame, GetResult};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

/// How many results show at first; "Show more" adds this many again.
const PAGE: usize = 60;

#[derive(Default)]
pub(super) struct Catalog {
    games: Vec<FreeGame>,
    total: usize,
    loading: bool,
    error: Option<String>,
}

pub(super) struct FreeState {
    system: Platform,
    search: String,
    shown: usize,
    catalogs: HashMap<Platform, Catalog>,
    catalog_tx: Sender<CatalogMsg>,
    catalog_rx: Receiver<CatalogMsg>,
    get_tx: Sender<GetResult>,
    get_rx: Receiver<GetResult>,
    /// Games being downloaded (by slug).
    getting: HashSet<String>,
}

impl FreeState {
    pub(super) fn new() -> Self {
        let (catalog_tx, catalog_rx) = channel();
        let (get_tx, get_rx) = channel();
        Self {
            system: Platform::Gba,
            search: String::new(),
            shown: PAGE,
            catalogs: HashMap::new(),
            catalog_tx,
            catalog_rx,
            get_tx,
            get_rx,
            getting: HashSet::new(),
        }
    }
}

/// Cache key for a free game's picture (not a file path; just unique).
fn art_key(slug: &str) -> PathBuf {
    PathBuf::from(format!("homebrew-hub:{slug}"))
}

/// Where a free game's picture is kept once downloaded.
fn art_file(slug: &str) -> Option<PathBuf> {
    let safe: String = slug
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Library::covers_dir().map(|d| d.join("homebrew-hub").join(format!("{safe}.img")))
}

enum CardState {
    Available,
    Getting,
    InLibrary,
}

impl GameShelfApp {
    /// New pages of the catalog, and finished downloads.
    pub(super) fn poll_free_games(&mut self) {
        while let Ok(msg) = self.free.catalog_rx.try_recv() {
            match msg {
                CatalogMsg::Page {
                    platform,
                    games,
                    total,
                } => {
                    let c = self.free.catalogs.entry(platform).or_default();
                    c.games.extend(games);
                    c.total = total;
                }
                CatalogMsg::Done { platform } => {
                    self.free.catalogs.entry(platform).or_default().loading = false;
                }
                CatalogMsg::Failed { platform, error } => {
                    let c = self.free.catalogs.entry(platform).or_default();
                    c.loading = false;
                    c.error = Some(error);
                }
            }
        }
        while let Ok(done) = self.free.get_rx.try_recv() {
            self.free.getting.remove(&done.slug);
            match done.result {
                Ok(path) => {
                    let added = self.lib.add_path(&path);
                    // Give the new game the catalog picture as its cover.
                    let art = art_file(&done.slug).filter(|f| f.is_file());
                    let mut title = String::new();
                    for g in self
                        .lib
                        .games
                        .iter_mut()
                        .filter(|g| g.path.starts_with(&path))
                    {
                        title = g.title.clone();
                        if g.cover.is_none() {
                            if let Some(art) = &art {
                                g.cover = Some(art.clone());
                                g.cover_missing = false;
                            }
                        }
                    }
                    self.status = if added > 0 {
                        format!("✔ {title} is in your library.")
                    } else {
                        format!(
                            "Downloaded to {}, but no game file was found in it.",
                            path.display()
                        )
                    };
                    self.dirty = true;
                    self.folder_info = None;
                    self.queue_covers();
                }
                Err(e) => self.status = format!("Could not get that game: {e}"),
            }
        }
    }

    fn get_free_game(&mut self, game: FreeGame, ctx: &egui::Context) {
        let Some(root) = self.lib.root.clone().or_else(workspace::default_root) else {
            self.status = "Choose a GameShelf folder in Settings first.".into();
            return;
        };
        if let Err(e) = workspace::create(&root) {
            self.status = format!("Could not create the GameShelf folder: {e}");
            return;
        }
        if self.lib.root.is_none() {
            self.lib.root = Some(root.clone());
            self.dirty = true;
        }
        let dir = workspace::system_dir(&root, game.platform);
        self.free.getting.insert(game.slug.clone());
        self.status = format!("Downloading {}…", game.title);
        freegames::get(game, dir, self.free.get_tx.clone(), ctx.clone());
    }

    pub(super) fn free_games_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        let system = self.free.system;
        if !self.free.catalogs.contains_key(&system) {
            self.free.catalogs.insert(
                system,
                Catalog {
                    loading: true,
                    ..Default::default()
                },
            );
            freegames::load(system, self.free.catalog_tx.clone(), ui.ctx().clone());
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Free games")
                    .size(24.0)
                    .strong()
                    .color(t.text),
            );
            ui.label(
                RichText::new("homebrew their makers share for free, from").color(t.text_weak),
            );
            ui.hyperlink_to(
                RichText::new("Homebrew Hub").color(t.gold),
                "https://hh.gbdev.io",
            );
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            for p in freegames::SYSTEMS {
                let label = match self
                    .free
                    .catalogs
                    .get(&p)
                    .map(|c| c.total)
                    .filter(|n| *n > 0)
                {
                    Some(n) => format!("{}  {n}", p.name()),
                    None => p.name().to_string(),
                };
                if chip(ui, t, &label, self.free.system == p) && self.free.system != p {
                    self.free.system = p;
                    self.free.shown = PAGE;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.free.search)
                            .hint_text("🔍  Title, maker or tag (e.g. puzzle)")
                            .desired_width(260.0)
                            .margin(vec2(8.0, 5.0)),
                    )
                    .changed()
                {
                    self.free.shown = PAGE;
                }
            });
        });
        ui.add_space(8.0);

        let mut retry = false;
        if let Some(c) = self.free.catalogs.get(&system) {
            if c.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new(format!(
                            "Loading {} of {}…",
                            c.games.len(),
                            c.total.max(c.games.len())
                        ))
                        .color(t.text_weak),
                    );
                });
            }
            if let Some(e) = &c.error {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Couldn't load the list: {e}")).color(t.warning),
                    );
                    retry = ui.button("Try again").clicked();
                });
            }
        }
        if retry {
            self.free.catalogs.remove(&system);
            return;
        }

        // Which results to show, and whether each is already in the library.
        let root = self.lib.root.clone();
        let search = self.free.search.clone();
        let matching: Vec<FreeGame> = self
            .free
            .catalogs
            .get(&system)
            .map(|c| {
                c.games
                    .iter()
                    .filter(|g| g.matches(&search))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let total_matching = matching.len();
        let visible: Vec<(FreeGame, CardState)> = matching
            .into_iter()
            .take(self.free.shown)
            .map(|g| {
                let in_library = root.as_ref().is_some_and(|r| {
                    freegames::destination(&g, &workspace::system_dir(r, g.platform)).is_some_and(
                        |d| self.lib.games.iter().any(|lg| lg.path.starts_with(&d)) || d.exists(),
                    )
                });
                let state = if self.free.getting.contains(&g.slug) {
                    CardState::Getting
                } else if in_library {
                    CardState::InLibrary
                } else {
                    CardState::Available
                };
                (g, state)
            })
            .collect();
        if visible.is_empty() && !self.free.catalogs.get(&system).is_some_and(|c| c.loading) {
            ui.label(
                RichText::new("No games match.")
                    .size(16.0)
                    .color(t.text_weak),
            );
            return;
        }

        let mut get = None;
        let mut show_more = false;
        let mut want_art: Vec<(String, String)> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let (columns, cell) = grid(ui.available_width() - 6.0, GAME_CARD_MIN, GAP);
                for row in visible.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for (game, state) in row {
                            let key = art_key(&game.slug);
                            let art = self.covers.get(&key);
                            let (clicked, rect) = free_card(ui, t, cell, game, art, state);
                            if clicked {
                                get = Some(game.clone());
                            }
                            // Fetch pictures only for cards on screen.
                            if art.is_none() && ui.is_rect_visible(rect) {
                                if let Some(url) = game.cover_url() {
                                    want_art.push((game.slug.clone(), url));
                                }
                            }
                        }
                    });
                    ui.add_space(GAP);
                }
                if total_matching > visible.len() {
                    ui.vertical_centered(|ui| {
                        let more = egui::Button::new(
                            RichText::new(format!(
                                "Show more  ({} left)",
                                total_matching - visible.len()
                            ))
                            .color(t.text),
                        )
                        .min_size(vec2(200.0, 32.0));
                        show_more = ui.add(more).clicked();
                    });
                    ui.add_space(20.0);
                }
            });

        for (slug, url) in want_art {
            if let Some(save_to) = art_file(&slug) {
                self.covers.request(Job::Url {
                    key: art_key(&slug),
                    url,
                    save_to,
                });
            }
        }
        if show_more {
            self.free.shown += PAGE;
        }
        if let Some(game) = get {
            self.get_free_game(game, ui.ctx());
        }
    }
}

/// One free game. Returns whether "Get" was clicked, and the card's rectangle.
fn free_card(
    ui: &mut egui::Ui,
    t: Theme,
    cell: f32,
    game: &FreeGame,
    art: Option<&Cover>,
    state: &CardState,
) -> (bool, Rect) {
    let mut clicked = false;
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
                    ui.allocate_exact_size(vec2(width, width * COVER_ASPECT), Sense::hover());
                draw_art(ui, t, &game.title, game.platform, false, art, area);
                let mut info = String::new();
                if !game.tags.is_empty() {
                    info.push_str(&game.tags.join(", "));
                }
                if let Some(site) = &game.website {
                    info.push_str(&format!("\n{site}"));
                }
                if !info.is_empty() {
                    response.on_hover_text(info);
                }
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(RichText::new(&game.title).strong().size(14.5).color(t.text))
                        .truncate(),
                );
                let by = game.developer.as_deref().unwrap_or("Unknown maker");
                ui.add(
                    egui::Label::new(RichText::new(by).size(11.5).color(t.text_weak)).truncate(),
                );
                let license = game.license.as_deref().unwrap_or("License: see its page");
                ui.add(
                    egui::Label::new(RichText::new(license).size(10.5).color(t.text_weak))
                        .truncate(),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    match state {
                        CardState::Available => {
                            let get = egui::Button::new(RichText::new("⬇ Get").strong().color(INK))
                                .fill(t.gold)
                                .min_size(vec2(84.0, 28.0));
                            if ui
                                .add(get)
                                .on_hover_text("Download it into your library")
                                .clicked()
                            {
                                clicked = true;
                            }
                        }
                        CardState::Getting => {
                            ui.spinner();
                            ui.label(RichText::new("Downloading…").size(12.0).color(t.text_weak));
                        }
                        CardState::InLibrary => {
                            ui.label(
                                RichText::new("✔ In your library")
                                    .size(12.5)
                                    .strong()
                                    .color(t.success),
                            );
                        }
                    }
                    if let Some(site) = &game.website {
                        let web = egui::Button::new(RichText::new("🌐").color(t.text))
                            .min_size(vec2(30.0, 28.0));
                        if ui.add(web).on_hover_text(site.as_str()).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(site));
                        }
                    }
                });
            });
        });
    let rect = frame.response.rect;
    if ui.rect_contains_pointer(rect) {
        ui.painter()
            .rect_stroke(rect, 12.0, Stroke::new(1.5_f32, t.gold), StrokeKind::Inside);
    }
    (clicked, rect)
}
