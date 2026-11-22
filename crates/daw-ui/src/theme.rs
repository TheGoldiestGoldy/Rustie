// Rusty / Slate Studio: original Midnight Ember theme.
// A shared, legible palette for the real native egui UI.
// Uses fills/strokes instead of pretending that egui has background blur.
const INK_0: Color32 = Color32::from_rgb(7, 8, 10);
const INK_1: Color32 = Color32::from_rgb(13, 15, 19);
const INK_2: Color32 = Color32::from_rgb(20, 22, 27);
const INK_3: Color32 = Color32::from_rgb(29, 31, 37);
const STUDIO_BORDER: Color32 = Color32::from_rgb(45, 47, 53);
const STUDIO_TEXT: Color32 = Color32::from_rgb(234, 235, 237);
const STUDIO_DIM: Color32 = Color32::from_rgb(152, 155, 162);
const ACCENT: Color32 = Color32::from_rgb(255, 132, 37);
const ACCENT_BRIGHT: Color32 = Color32::from_rgb(255, 165, 75);
const ACCENT_DARK: Color32 = Color32::from_rgb(109, 51, 26);
const ACCENT_TINT: Color32 = Color32::from_rgb(51, 30, 22);
const CLIP_PURPLE: Color32 = Color32::from_rgb(87, 59, 119);
const CLIP_PINK: Color32 = Color32::from_rgb(138, 66, 87);

fn install_midnight_ember_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = INK_1;
    visuals.window_fill = INK_2;
    visuals.extreme_bg_color = INK_0;
    visuals.faint_bg_color = INK_2;
    visuals.override_text_color = Some(STUDIO_TEXT);
    visuals.selection.bg_fill = ACCENT_DARK;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT_BRIGHT);
    visuals.hyperlink_color = ACCENT_BRIGHT;
    visuals.widgets.noninteractive.bg_fill = INK_1;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.7_f32, STUDIO_BORDER);
    visuals.widgets.inactive.bg_fill = INK_3;
    visuals.widgets.inactive.weak_bg_fill = INK_2;
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.8_f32, STUDIO_BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, STUDIO_TEXT);
    visuals.widgets.hovered.bg_fill = ACCENT_TINT;
    visuals.widgets.hovered.weak_bg_fill = ACCENT_TINT;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_DARK);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, STUDIO_TEXT);
    visuals.widgets.active.bg_fill = ACCENT_DARK;
    visuals.widgets.active.weak_bg_fill = ACCENT_DARK;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(7.0, 7.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(9);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(9);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(9);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(9);
        style.visuals.window_corner_radius = egui::CornerRadius::same(12);
    });
}
