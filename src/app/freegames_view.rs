//! The Free games tab: "My list" (GameShelf's own list on GitHub) and Homebrew Hub,
//! one click to add a game to the library.

use super::library_view::{draw_art, COVER_ASPECT, GAME_CARD_MIN};
use super::*;
use crate::freegames::{self, FreeGame, GetResult, HubMsg, ListMsg, Source};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

/// How many results show at first; "Show more" adds this many again.
const PAGE: usize = 60;

#[derive(Default)]
struct HubCatalog {
    games: Vec<FreeGame>,
    total: usize,
    loading: bool,
    error: Option<String>,
}

#[derive(Default)]
struct MyList {
    games: Vec<FreeGame>,
    loading: bool,
    note: Option<String>,
}

pub(super) struct FreeState {
    system: Option<Platform>,
    search: String,
    shown: usize,
    /// `None` until first needed, or after the settings change (then it reloads).
    list: Option<MyList>,
    hub: HashMap<Platform, HubCatalog>,
    list_tx: Sender<ListMsg>,
    list_rx: Receiver<ListMsg>,
    hub_tx: Sender<HubMsg>,
    hub_rx: Receiver<HubMsg>,
    get_tx: Sender<GetResult>,
    get_rx: Receiver<GetResult>,
    /// Games being downloaded (by key).
    getting: HashSet<String>,
}

impl FreeState {
    pub(super) fn new() -> Self {
        let (list_tx, list_rx) = channel();
        let (hub_tx, hub_rx) = channel();
        let (get_tx, get_rx) = channel();
        Self {
            system: None,
            search: String::new(),
            shown: PAGE,
            list: None,
            hub: HashMap::new(),
            list_tx,
            list_rx,
            hub_tx,
            hub_rx,
            get_tx,
            get_rx,
            getting: HashSet::new(),
        }
    }

    /// Reload both sources next time the tab is shown (after a settings change).
    pub(super) fn reset(&mut self) {
        self.list = None;
        self.hub.clear();
        self.system = None;
    }
}

/// Cache key for a free game's picture (not a file path; just unique).
fn art_key(key: &str) -> PathBuf {
    PathBuf::from(format!("free-games:{key}"))
}

/// Where a free game's picture is kept once downloaded.
fn art_file(key: &str) -> Option<PathBuf> {
    let safe: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Library::covers_dir().map(|d| d.join("free-games").join(format!("{safe}.img")))
}

enum CardState {
    Available,
    Getting,
    InLibrary,
}

impl GameShelfApp {
    /// The list, Homebrew Hub pages, and finished downloads.
    pub(super) fn poll_free_games(&mut self) {
        while let Ok(ListMsg::Loaded { games, note, .. }) = self.free.list_rx.try_recv() {
            self.free.list = Some(MyList {
                games,
                loading: false,
                note,
            });
        }
        while let Ok(msg) = self.free.hub_rx.try_recv() {
            match msg {
                HubMsg::Page {
                    platform,
                    games,
                    total,
                } => {
                    let c = self.free.hub.entry(platform).or_default();
                    c.games.extend(games);
                    c.total = total;
                }
                HubMsg::Done { platform } => {
                    self.free.hub.entry(platform).or_default().loading = false;
                }
                HubMsg::Failed { platform, error } => {
                    let c = self.free.hub.entry(platform).or_default();
                    c.loading = false;
                    c.error = Some(error);
                }
            }
        }
        while let Ok(done) = self.free.get_rx.try_recv() {
            self.free.getting.remove(&done.key);
            match done.result {
                Ok(path) => {
                    let added = self.lib.add_path(&path);
                    let art = art_file(&done.key).filter(|f| f.is_file());
                    let new_games: Vec<&mut Game> = self
                        .lib
                        .games
                        .iter_mut()
                        .filter(|g| g.path.starts_with(&path))
                        .collect();
                    let single = new_games.len() == 1;
                    for g in new_games {
                        // One game: use the list's title (better than "EBOOT" or a file name).
                        if single {
                            g.title = done.title.clone();
                        }
                        if g.cover.is_none() {
                            if let Some(art) = &art {
                                g.cover = Some(art.clone());
                                g.cover_missing = false;
                            }
                        }
                    }
                    self.status = if added > 0 {
                        format!("✔ {} is in your library.", done.title)
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
                Err(e) => self.status = format!("Could not get {}: {e}", done.title),
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
        self.free.getting.insert(game.key());
        self.status = format!("Downloading {}…", game.title);
        freegames::get(game, dir, self.free.get_tx.clone(), ctx.clone());
    }

    /// Systems with games: those in My list, plus Homebrew Hub's when it's on.
    fn free_systems(&self) -> Vec<Platform> {
        ALL.into_iter()
            .filter(|p| {
                let in_list = self
                    .free
                    .list
                    .as_ref()
                    .is_some_and(|l| l.games.iter().any(|g| g.platform == *p));
                in_list || (self.lib.homebrew_hub && freegames::HUB_SYSTEMS.contains(p))
            })
            .collect()
    }

    pub(super) fn free_games_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        if self.free.list.is_none() {
            self.free.list = Some(MyList {
                loading: true,
                ..Default::default()
            });
            freegames::load_list(
                self.lib.free_list_url.clone(),
                self.free.list_tx.clone(),
                ui.ctx().clone(),
            );
        }
        let systems = self.free_systems();
        let system = match self.free.system.filter(|s| systems.contains(s)) {
            Some(s) => s,
            None => {
                let first = systems.first().copied().unwrap_or(Platform::Gba);
                self.free.system = Some(first);
                first
            }
        };
        let hub_on = self.lib.homebrew_hub && freegames::HUB_SYSTEMS.contains(&system);
        if hub_on && !self.free.hub.contains_key(&system) {
            self.free.hub.insert(
                system,
                HubCatalog {
                    loading: true,
                    ..Default::default()
                },
            );
            freegames::load_hub(system, self.free.hub_tx.clone(), ui.ctx().clone());
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Free games")
                    .size(24.0)
                    .strong()
                    .color(t.text),
            );
            let sources = if self.lib.homebrew_hub {
                "homebrew their makers share for free, from your list ★ and Homebrew Hub"
            } else {
                "homebrew their makers share for free, from your list ★"
            };
            ui.label(RichText::new(sources).color(t.text_weak));
        });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            for p in &systems {
                let mine = self
                    .free
                    .list
                    .as_ref()
                    .map_or(0, |l| l.games.iter().filter(|g| g.platform == *p).count());
                let hub = self.free.hub.get(p).map_or(0, |c| c.total);
                let count = mine + hub;
                let label = if count > 0 {
                    format!("{}  {count}", p.name())
                } else {
                    p.name().to_string()
                };
                if chip(ui, t, &label, system == *p) && system != *p {
                    self.free.system = Some(*p);
                    self.free.shown = PAGE;
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.free.search)
                        .hint_text("🔍  Title, maker or tag (e.g. puzzle)")
                        .desired_width(300.0)
                        .margin(vec2(8.0, 5.0)),
                )
                .changed()
            {
                self.free.shown = PAGE;
            }
            if let Some(list) = &self.free.list {
                if list.loading {
                    ui.spinner();
                    ui.label(RichText::new("Reading your list…").color(t.text_weak));
                }
                if let Some(note) = &list.note {
                    ui.label(RichText::new(note).size(12.0).color(t.warning));
                }
            }
        });
        ui.add_space(6.0);

        let mut retry = false;
        if hub_on {
            if let Some(c) = self.free.hub.get(&system) {
                if c.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(format!(
                                "Loading Homebrew Hub: {} of {}…",
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
                            RichText::new(format!("Couldn't load Homebrew Hub: {e}"))
                                .color(t.warning),
                        );
                        retry = ui.button("Try again").clicked();
                    });
                }
            }
        }
        if retry {
            self.free.hub.remove(&system);
            return;
        }

        // My list first, then Homebrew Hub.
        let search = self.free.search.clone();
        let mine = self.free.list.iter().flat_map(|l| l.games.iter());
        let hub = self
            .free
            .hub
            .get(&system)
            .into_iter()
            .flat_map(|c| c.games.iter());
        let matching: Vec<FreeGame> = mine
            .chain(hub)
            .filter(|g| g.platform == system && g.matches(&search))
            .cloned()
            .collect();
        let total_matching = matching.len();
        let root = self.lib.root.clone();
        let visible: Vec<(FreeGame, CardState)> = matching
            .into_iter()
            .take(self.free.shown)
            .map(|g| {
                let in_library = root.as_ref().is_some_and(|r| {
                    freegames::destination(&g, &workspace::system_dir(r, g.platform)).is_some_and(
                        |d| d.exists() || self.lib.games.iter().any(|lg| lg.path.starts_with(&d)),
                    )
                });
                let state = if self.free.getting.contains(&g.key()) {
                    CardState::Getting
                } else if in_library {
                    CardState::InLibrary
                } else {
                    CardState::Available
                };
                (g, state)
            })
            .collect();
        let loading = self.free.list.as_ref().is_some_and(|l| l.loading)
            || self.free.hub.get(&system).is_some_and(|c| c.loading);
        if visible.is_empty() && !loading {
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
                            let key = game.key();
                            let art = self.covers.get(&art_key(&key));
                            let (clicked, rect) = free_card(ui, t, cell, game, art, state);
                            if clicked {
                                get = Some(game.clone());
                            }
                            // Fetch pictures only for cards on screen.
                            if art.is_none() && ui.is_rect_visible(rect) {
                                if let Some(url) = &game.cover_url {
                                    want_art.push((key, url.clone()));
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

        for (key, url) in want_art {
            if let Some(save_to) = art_file(&key) {
                self.covers.request(Job::Url {
                    key: art_key(&key),
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
                // Games from your own list get a gold star on the picture.
                if game.source == Source::MyList {
                    ui.painter().text(
                        area.right_top() + vec2(-8.0, 8.0),
                        Align2::RIGHT_TOP,
                        "★",
                        FontId::proportional(15.0),
                        t.gold,
                    );
                }
                let mut info = Vec::new();
                if let Some(d) = &game.description {
                    info.push(d.clone());
                }
                if !game.tags.is_empty() {
                    info.push(game.tags.join(", "));
                }
                if let Some(license) = &game.license {
                    info.push(format!("License: {license}"));
                }
                info.push(match game.source {
                    Source::MyList => "From your list".to_string(),
                    Source::HomebrewHub => "From Homebrew Hub".to_string(),
                });
                response.on_hover_text(info.join("\n"));
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(RichText::new(&game.title).strong().size(14.5).color(t.text))
                        .truncate(),
                );
                let by = game.developer.as_deref().unwrap_or("Unknown maker");
                ui.add(
                    egui::Label::new(RichText::new(by).size(11.5).color(t.text_weak)).truncate(),
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
