use egui::{Color32, FontFamily, FontId, Rounding, Stroke, Style, TextStyle, Visuals};

/// Professional dark theme tuned for meteorological analysis.
pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut style = Style::default();

    // Dark charcoal background
    let bg = Color32::from_rgb(22, 22, 30);
    let panel_bg = Color32::from_rgb(28, 28, 38);
    let widget_bg = Color32::from_rgb(40, 40, 55);
    let accent = Color32::from_rgb(60, 140, 255);
    let accent_dim = Color32::from_rgb(45, 100, 180);
    let text = Color32::from_rgb(220, 220, 230);
    let text_dim = Color32::from_rgb(140, 140, 160);
    let border = Color32::from_rgb(55, 55, 70);
    let hover = Color32::from_rgb(50, 50, 70);

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(text);
    visuals.panel_fill = panel_bg;
    visuals.window_fill = panel_bg;
    visuals.extreme_bg_color = bg;
    visuals.faint_bg_color = Color32::from_rgb(30, 30, 42);

    // Widgets
    visuals.widgets.noninteractive.bg_fill = widget_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_dim);
    visuals.widgets.noninteractive.rounding = Rounding::same(4.0);

    visuals.widgets.inactive.bg_fill = widget_bg;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    visuals.widgets.inactive.rounding = Rounding::same(4.0);

    visuals.widgets.hovered.bg_fill = hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent_dim);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text);
    visuals.widgets.hovered.rounding = Rounding::same(4.0);

    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    visuals.widgets.active.rounding = Rounding::same(4.0);

    visuals.widgets.open.bg_fill = hover;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, accent_dim);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, text);
    visuals.widgets.open.rounding = Rounding::same(4.0);

    visuals.selection.bg_fill = accent.linear_multiply(0.3);
    visuals.selection.stroke = Stroke::new(1.0, accent);

    visuals.window_rounding = Rounding::same(6.0);
    visuals.window_stroke = Stroke::new(1.0, border);
    visuals.menu_rounding = Rounding::same(6.0);

    style.visuals = visuals;

    // Typography
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(18.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.0, FontFamily::Monospace),
    );

    // Spacing
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(12.0);

    ctx.set_style(style);
}

/// Accent color for highlights and active elements.
pub const ACCENT: Color32 = Color32::from_rgb(60, 140, 255);

/// Muted text color.
pub const TEXT_DIM: Color32 = Color32::from_rgb(140, 140, 160);

/// Success green.
pub const SUCCESS: Color32 = Color32::from_rgb(80, 200, 120);

/// Warning amber.
pub const WARNING: Color32 = Color32::from_rgb(240, 180, 40);

/// Error red.
pub const ERROR: Color32 = Color32::from_rgb(240, 60, 60);

/// Panel background.
pub const PANEL_BG: Color32 = Color32::from_rgb(28, 28, 38);

/// Deep background.
pub const DEEP_BG: Color32 = Color32::from_rgb(22, 22, 30);
