use rustmet_core::render;

/// Draw a standalone horizontal colorbar widget.
pub fn draw_colorbar(
    ui: &mut egui::Ui,
    colormap_name: &str,
    vmin: f64,
    vmax: f64,
    width: f32,
    height: f32,
) {
    let (rect, _response) = ui.allocate_exact_size(
        egui::vec2(width, height + 16.0),
        egui::Sense::hover(),
    );

    let painter = ui.painter_at(rect);

    let bar_rect = egui::Rect::from_min_size(rect.min, egui::vec2(width, height));

    let cmap = render::get_colormap(colormap_name).unwrap_or(render::TEMPERATURE);

    // Draw gradient
    let n_strips = (width as usize).min(256).max(32);
    let strip_w = width / n_strips as f32;

    for i in 0..n_strips {
        let t = i as f64 / (n_strips - 1) as f64;
        let (r, g, b) = render::interpolate_color(cmap, t);
        let color = egui::Color32::from_rgb(r, g, b);
        let x = bar_rect.min.x + i as f32 * strip_w;
        let strip = egui::Rect::from_min_size(
            egui::pos2(x, bar_rect.min.y),
            egui::vec2(strip_w + 0.5, height),
        );
        painter.rect_filled(strip, 0.0, color);
    }

    painter.rect_stroke(
        bar_rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)),
    );

    // Labels
    let font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
    let label_y = bar_rect.max.y + 2.0;

    let n_labels = 5;
    for i in 0..n_labels {
        let t = i as f64 / (n_labels - 1) as f64;
        let val = vmin + t * (vmax - vmin);
        let x = bar_rect.min.x + t as f32 * width;
        let align = if i == 0 {
            egui::Align2::LEFT_TOP
        } else if i == n_labels - 1 {
            egui::Align2::RIGHT_TOP
        } else {
            egui::Align2::CENTER_TOP
        };
        painter.text(
            egui::pos2(x, label_y),
            align,
            format!("{:.1}", val),
            font.clone(),
            egui::Color32::from_white_alpha(180),
        );
    }
}
