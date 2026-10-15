//! Colours: burgundy, white, soft gold, beige and grey, in a dark and a light mode.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Visuals};

pub const BURGUNDY: Color32 = Color32::from_rgb(122, 30, 44);
pub const BURGUNDY_HOVER: Color32 = Color32::from_rgb(150, 42, 60);
/// Dark text for use on gold.
pub const INK: Color32 = Color32::from_rgb(38, 26, 20);

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color32,
    pub side: Color32,
    pub card: Color32,
    pub cover_bg: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub gold: Color32,
    pub header: Color32,
    pub input: Color32,
    pub button: Color32,
    pub success: Color32,
    pub warning: Color32,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(28, 25, 26),
            side: Color32::from_rgb(36, 32, 33),
            card: Color32::from_rgb(44, 39, 41),
            cover_bg: Color32::from_rgb(62, 36, 42),
            border: Color32::from_rgb(68, 60, 62),
            text: Color32::WHITE,
            text_weak: Color32::from_rgb(186, 178, 170),
            gold: Color32::from_rgb(214, 184, 110),
            header: Color32::from_rgb(86, 20, 32),
            input: Color32::from_rgb(22, 19, 20),
            button: Color32::from_rgb(60, 53, 55),
            success: Color32::from_rgb(130, 205, 150),
            warning: Color32::from_rgb(224, 190, 110),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(243, 236, 223),
            side: Color32::from_rgb(234, 223, 204),
            card: Color32::WHITE,
            cover_bg: Color32::from_rgb(236, 222, 212),
            border: Color32::from_rgb(214, 202, 184),
            text: Color32::from_rgb(32, 28, 29),
            text_weak: Color32::from_rgb(108, 102, 97),
            gold: Color32::from_rgb(166, 128, 44),
            header: Color32::from_rgb(110, 24, 38),
            input: Color32::WHITE,
            button: Color32::from_rgb(228, 217, 199),
            success: Color32::from_rgb(36, 135, 70),
            warning: Color32::from_rgb(150, 104, 18),
        }
    }

    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        v.panel_fill = self.bg;
        v.window_fill = self.card;
        v.window_stroke = Stroke::new(1.0_f32, self.border);
        v.extreme_bg_color = self.input;
        v.faint_bg_color = self.side;
        v.hyperlink_color = self.gold;
        v.selection.bg_fill = BURGUNDY;
        v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);

        let radius = CornerRadius::same(6);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = self.bg;
        w.noninteractive.weak_bg_fill = self.bg;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text);
        w.inactive.bg_fill = self.button;
        w.inactive.weak_bg_fill = self.button;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0_f32, self.text);
        for state in [&mut w.hovered, &mut w.active, &mut w.open] {
            state.bg_fill = BURGUNDY_HOVER;
            state.weak_bg_fill = BURGUNDY_HOVER;
            state.bg_stroke = Stroke::new(1.0_f32, self.gold);
            state.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
        }
        w.active.bg_fill = BURGUNDY;
        w.active.weak_bg_fill = BURGUNDY;
        for state in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            state.corner_radius = radius;
        }
        // egui keeps one style per system theme and follows Windows by default;
        // pin our mode and set both styles so nothing falls back to egui's defaults.
        ctx.set_theme(if self.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
        for system_theme in [egui::Theme::Dark, egui::Theme::Light] {
            ctx.set_visuals_of(system_theme, v.clone());
            ctx.style_mut_of(system_theme, |s| {
                s.spacing.button_padding = egui::vec2(10.0, 5.0);
                s.spacing.item_spacing = egui::vec2(8.0, 6.0);
                s.text_styles
                    .insert(TextStyle::Body, FontId::proportional(14.5));
                s.text_styles
                    .insert(TextStyle::Button, FontId::proportional(14.5));
                s.text_styles
                    .insert(TextStyle::Heading, FontId::proportional(22.0));
                s.text_styles
                    .insert(TextStyle::Small, FontId::proportional(11.5));
            });
        }
    }
}
