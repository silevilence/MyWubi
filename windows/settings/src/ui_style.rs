//! Shared, motion-free desktop surfaces for settings and skin authoring.
use eframe::egui::{self, Color32, FontId, RichText, Stroke, TextStyle};

pub const ACCENT: Color32 = Color32::from_rgb(28, 91, 153);
pub const MUTED: Color32 = Color32::from_rgb(87, 98, 112);
pub const SUCCESS: Color32 = Color32::from_rgb(31, 108, 65);
pub const WARNING: Color32 = Color32::from_rgb(133, 83, 12);
pub const ERROR: Color32 = Color32::from_rgb(166, 40, 40);
pub const BACKGROUND: Color32 = Color32::from_rgb(245, 246, 248);

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.error_fg_color = ERROR;
    style.visuals.warn_fg_color = WARNING;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.override_text_color = Some(Color32::from_rgb(31, 41, 55));
    style.visuals.selection.bg_fill = Color32::from_rgb(220, 234, 248);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(196, 204, 214);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke =
        Stroke::new(1.0_f32, Color32::from_rgb(130, 140, 151));
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 241, 248);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(235, 241, 248);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(220, 234, 248);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(220, 234, 248);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.rounding = egui::Rounding::same(5.0);
    }
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size = egui::vec2(36.0, 32.0);
    style.spacing.combo_width = 190.0;
    style.spacing.text_edit_width = 220.0;
    style.animation_time = 0.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(24.0));
    ctx.set_style(style);
}

pub fn heading(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.heading(title);
    ui.add(egui::Label::new(RichText::new(description).color(MUTED)).wrap(true));
    ui.add_space(12.0);
}

pub fn card<R>(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let result = egui::Frame::none()
        .fill(Color32::WHITE)
        .rounding(8.0)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            if !title.is_empty() {
                ui.label(RichText::new(title).size(17.0).strong());
                ui.add_space(4.0);
            }
            body(ui)
        })
        .inner;
    ui.add_space(6.0);
    result
}

pub fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(ACCENT),
    )
}

/// Persist incomplete input across frames; the caller decides when to commit it.
pub fn text_buffer(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    current: &str,
) -> (String, egui::Response) {
    let id = ui.make_persistent_id(id);
    let cached = ui
        .ctx()
        .data_mut(|data| data.get_temp::<(String, String)>(id));
    let mut text = cached
        .filter(|(source, _)| source == current)
        .map(|(_, text)| text)
        .unwrap_or_else(|| current.to_owned());
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(ui.available_width().clamp(80.0, 220.0)),
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, (current.to_owned(), text.clone())));
    (text, response)
}
