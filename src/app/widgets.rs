//! Small drawing helpers shared by every screen.

use super::*;

pub(super) const GAP: f32 = 18.0;

/// How many columns fit in `available`, and how wide each cell must be so the
/// row fills the whole width. Cells grow as the window grows.
pub(super) fn grid(available: f32, min: f32, gap: f32) -> (usize, f32) {
    let available = available.max(min);
    let columns = ((available + gap) / (min + gap)).floor().max(1.0) as usize;
    let cell = (available - gap * (columns as f32 - 1.0)) / columns as f32;
    (columns, cell)
}

/// A small toggle chip. Returns true when clicked.
pub(super) fn chip(ui: &mut egui::Ui, t: Theme, label: &str, selected: bool) -> bool {
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

pub(super) fn header_tab(ui: &mut egui::Ui, label: &str, selected: bool, gold: Color32) -> bool {
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

pub(super) fn section_label(ui: &mut egui::Ui, text: &str, t: Theme) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(11.0).strong().color(t.gold));
    ui.add_space(2.0);
}

pub(super) fn pill(ui: &mut egui::Ui, text: &str, fill: Color32, color: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).strong().color(color))
        });
}
