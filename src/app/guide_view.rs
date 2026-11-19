//! The Guide tab: a "Getting started" checklist that follows what you've actually
//! done, then a short explanation of every part of GameShelf and common fixes.
//! It opens by itself the first time GameShelf starts.

use super::settings_view::section;
use super::*;

/// One "Getting started" step.
struct Step {
    title: &'static str,
    how: &'static str,
    done: bool,
    /// Button label and the tab it opens.
    go: Option<(&'static str, Tab)>,
}

impl GameShelfApp {
    fn guide_steps(&self) -> Vec<Step> {
        let any_emulator = ALL.into_iter().any(|p| self.lib.emulator_ready(p));
        let played = self.lib.games.iter().any(|g| g.last_played.is_some());
        let backed_up = self.lib.root.as_ref().is_some_and(|r| {
            std::fs::read_dir(r.join("Backups")).is_ok_and(|mut d| {
                d.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == "zip")))
            })
        });
        vec![
            Step {
                title: "Choose your GameShelf folder",
                how: "One folder holds everything: your games (sorted by system), emulators, BIOS files and saves. \
                      In Settings, click \"Use Documents\\GameShelf\" or \"Choose folder…\".",
                done: self.lib.root.is_some(),
                go: Some(("Open Settings", Tab::Settings)),
            },
            Step {
                title: "Set up an emulator",
                how: "An emulator is the program that plays the games. In Emulators, press ⬇ Install on the system you want. \
                      RetroArch plays Nintendo (NES to Game Boy Advance) and Sega Genesis; DuckStation plays PlayStation; \
                      PPSSPP plays PSP.",
                done: any_emulator,
                go: Some(("Open Emulators", Tab::Emulators)),
            },
            Step {
                title: "Add some games",
                how: "Get free games in one click from Free games. For games you own: press ➕ Add games, drag files onto \
                      the window, or put them in GameShelf\\Games\\<system> and press Scan now in Settings.",
                done: !self.lib.games.is_empty(),
                go: Some(("Open Free games", Tab::FreeGames)),
            },
            Step {
                title: "Play",
                how: "In Library, press ▶ Play on a game, or double-click its cover. Next time it waits for you in \
                      \"Continue playing\" at the top.",
                done: played,
                go: Some(("Open Library", Tab::Library)),
            },
            Step {
                title: "Keep your progress safe",
                how: "In Settings › All your progress, press 📦 Back up all my saves. It puts every save of every \
                      emulator into one .zip you can copy to a USB stick or the cloud.",
                done: backed_up,
                go: Some(("Open Settings", Tab::Settings)),
            },
        ]
    }

    pub(super) fn guide_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.theme;
        let steps = self.guide_steps();
        let done = steps.iter().filter(|s| s.done).count();
        let mut go_to = None;

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(14.0);
            ui.label(RichText::new("Welcome to GameShelf").size(26.0).strong().color(t.text));
            ui.label(
                RichText::new(
                    "Your classic games in one place: GameShelf keeps the library, and starts each game in the right emulator.",
                )
                .color(t.text_weak),
            );
            ui.add_space(14.0);

            // Getting started: follows what you've actually done.
            section(
                ui,
                t,
                "🚀  Getting started",
                "Five steps. Each one turns green when it's done.",
                |ui| {
                    ui.horizontal(|ui| {
                        let bar = egui::ProgressBar::new(done as f32 / steps.len().max(1) as f32)
                            .fill(if done == steps.len() { t.success } else { BURGUNDY })
                            .desired_width(ui.available_width() - 110.0)
                            .desired_height(12.0);
                        ui.add(bar);
                        ui.label(
                            RichText::new(format!("{done} of {} done", steps.len()))
                                .strong()
                                .color(t.text),
                        );
                    });
                    ui.add_space(10.0);
                    for (i, step) in steps.iter().enumerate() {
                        if let Some(tab) = step_row(ui, t, i + 1, step) {
                            go_to = Some(tab);
                        }
                        ui.add_space(8.0);
                    }
                    if done == steps.len() {
                        ui.label(
                            RichText::new("🎉 All set. Have fun!")
                                .size(16.0)
                                .strong()
                                .color(t.success),
                        );
                    }
                },
            );
            ui.add_space(GAP);

            // How each part works, two columns when there's room.
            let topics = topics();
            if ui.available_width() > 900.0 {
                for pair in topics.chunks(2) {
                    ui.columns(2, |cols| {
                        for (col, topic) in cols.iter_mut().zip(pair) {
                            topic_card(col, t, topic);
                        }
                    });
                    ui.add_space(GAP);
                }
            } else {
                for topic in &topics {
                    topic_card(ui, t, topic);
                    ui.add_space(GAP);
                }
            }

            // Problems and fixes.
            section(
                ui,
                t,
                "🛠  Something's not right?",
                "The usual problems, and what fixes them.",
                |ui| {
                    egui::Grid::new("fixes")
                        .num_columns(2)
                        .spacing(vec2(16.0, 10.0))
                        .striped(false)
                        .show(ui, |ui| {
                            for (problem, fix) in FIXES {
                                ui.label(RichText::new(*problem).strong().color(t.text));
                                ui.add(egui::Label::new(RichText::new(*fix).color(t.text_weak)).wrap());
                                ui.end_row();
                            }
                        });
                },
            );
            ui.add_space(30.0);
        });

        if let Some(tab) = go_to {
            self.tab = tab;
            self.folder_info = None;
        }
    }
}

/// A numbered step with its status and a button. Returns the tab to open, if clicked.
fn step_row(ui: &mut egui::Ui, t: Theme, number: usize, step: &Step) -> Option<Tab> {
    let mut go = None;
    egui::Frame::new()
        .fill(if step.done {
            t.success.gamma_multiply(0.12)
        } else {
            t.side
        })
        .corner_radius(10.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Number badge: gold while to do, green tick when done.
                let (badge, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                let (fill, text, color) = if step.done {
                    (t.success, "✔".to_string(), Color32::WHITE)
                } else {
                    (t.gold, number.to_string(), INK)
                };
                ui.painter().circle_filled(badge.center(), 15.0, fill);
                ui.painter().text(
                    badge.center(),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(15.0),
                    color,
                );
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 150.0).max(200.0));
                    ui.label(RichText::new(step.title).size(15.5).strong().color(t.text));
                    ui.add(
                        egui::Label::new(RichText::new(step.how).size(12.5).color(t.text_weak))
                            .wrap(),
                    );
                });
                if let Some((label, tab)) = step.go {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (fill, color) = if step.done {
                            (t.button, t.text)
                        } else {
                            (t.gold, INK)
                        };
                        let button = egui::Button::new(RichText::new(label).strong().color(color))
                            .fill(fill)
                            .min_size(vec2(130.0, 30.0));
                        if ui.add(button).clicked() {
                            go = Some(tab);
                        }
                    });
                }
            });
        });
    go
}

struct Topic {
    title: &'static str,
    intro: &'static str,
    points: &'static [&'static str],
}

fn topics() -> Vec<Topic> {
    vec![
        Topic {
            title: "📚  Library",
            intro: "All your games, as cards with their covers.",
            points: &[
                "▶ Play starts a game; double-clicking the cover does too.",
                "Continue playing (at the top) shows what you played last.",
                "⭐ marks a favorite; the left side filters by system or favorites.",
                "Search and Sort are in the top bar.",
                "⋯ on a card: its controls, cover, system, or remove it from the list (the file stays).",
            ],
        },
        Topic {
            title: "🎮  Emulators",
            intro: "Each system needs its emulator, set up once.",
            points: &[
                "⬇ Install downloads the official emulator and sets it up for you.",
                "Choose .exe: point to an emulator you already have.",
                "RetroArch needs a \"core\" per system: in RetroArch, Online Updater › Core Downloader. ⚠ on a card means one is missing.",
                "PlayStation needs a BIOS from your own console (put it in GameShelf\\BIOS).",
                "⚙ holds advanced options; 🎮 shows (and changes) the keys.",
            ],
        },
        Topic {
            title: "🎁  Free games",
            intro: "Homebrew games their makers give away, one click each.",
            points: &[
                "Pick a system, search by name or tag, then ⬇ Get.",
                "The game lands in its system folder and in your Library, with its picture.",
                "★ games come from your own list on GitHub (Settings › Free games › Edit my list).",
            ],
        },
        Topic {
            title: "📁  Your GameShelf folder",
            intro: "Games\\<system>, Emulators, BIOS, Saves, States, Backups.",
            points: &[
                "Drop a game into Games\\<system>: the folder name tells GameShelf the system.",
                "Unzip an emulator into Emulators: GameShelf finds it on Scan now.",
                "🧹 Organize my games moves every game into its system folder (you see the plan first).",
            ],
        },
        Topic {
            title: "💾  Saves",
            intro: "Two kinds of progress, both kept for you.",
            points: &[
                "In-game saves: when a game saves itself, like any console.",
                "Save states: a snapshot anytime. In RetroArch F2 saves, F4 loads.",
                "Resume where I left off (Settings): close a game, it continues next time.",
                "📦 Back up all my saves / ♻ Restore a backup, in Settings › All your progress.",
            ],
        },
        Topic {
            title: "⌨  Playing with the keyboard (RetroArch)",
            intro: "Defaults; change any key in a game's 🎮 Controls.",
            points: &[
                "Arrows move · X = A · Z = B · Enter = Start · Right Shift = Select.",
                "F1 opens RetroArch's menu · Esc twice quits the game.",
                "⚠ Space is fast-forward: if the game suddenly runs fast, press Space again.",
                "A gamepad works too: plug it in before starting the game.",
            ],
        },
    ]
}

fn topic_card(ui: &mut egui::Ui, t: Theme, topic: &Topic) {
    section(ui, t, topic.title, topic.intro, |ui| {
        for point in topic.points {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new("•").strong().color(t.gold));
                ui.add(egui::Label::new(RichText::new(*point).color(t.text)).wrap());
            });
        }
    });
}

const FIXES: &[(&str, &str)] = &[
    (
        "A game won't start",
        "Open Emulators: its system's card should say ✔ Ready. For RetroArch, check there's no ⚠ (missing core).",
    ),
    (
        "The game is suddenly fast",
        "That's fast-forward. In RetroArch press Space again (or change that key in 🎮 Controls).",
    ),
    (
        "PlayStation: black screen or a BIOS message",
        "DuckStation and PCSX2 need a BIOS from your own console: put it in GameShelf\\BIOS and select it in the emulator.",
    ),
    (
        "A game shows \"?\" as its system",
        "Click ⋯ on its card and pick the system. Putting it in Games\\<system> also tells GameShelf.",
    ),
    (
        "No cover",
        "⋯ › Find cover online, or ⋯ › Choose cover image… to pick any picture.",
    ),
    (
        "A key change didn't stick",
        "Close the emulator first: emulators save their own settings when they close.",
    ),
    (
        "Where are my saves?",
        "Settings › Saves & cheats and All your progress list every save folder; click one to open it.",
    ),
];
