use crate::state::{AppState, RenderMode};
use crate::theme;

/// Render the field to an RGBA pixel buffer and upload as egui texture.
fn render_field(state: &mut AppState, ctx: &egui::Context) {
    let Some(ref values) = state.field_values else { return };
    let nx = state.field_nx;
    let ny = state.field_ny;
    if nx == 0 || ny == 0 {
        return;
    }

    let cmap_name = state.colormap_name();

    let pixels: Vec<u8> = match state.render_mode {
        RenderMode::Raster => {
            rustmet_core::render::render_raster(values, nx, ny, cmap_name, state.vmin, state.vmax)
        }
        RenderMode::FilledContour => {
            let levels = rustmet_core::render::auto_levels(state.vmin, state.vmax, state.contour_levels);
            rustmet_core::render::render_filled_contours(
                values,
                nx,
                ny,
                &levels,
                cmap_name,
                nx as u32,
                ny as u32,
            )
        }
    };

    // Convert Vec<u8> (RGBA) to egui::ColorImage
    let image = egui::ColorImage::from_rgba_unmultiplied([nx, ny], &pixels);
    let texture = ctx.load_texture("field_map", image, egui::TextureOptions::LINEAR);
    state.field_texture = Some(texture);
    state.needs_rerender = false;
}

pub fn map_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    // Re-render if needed
    if state.needs_rerender && state.field_values.is_some() {
        render_field(state, ctx);
    }

    let Some(ref texture) = state.field_texture else {
        // No data loaded — prompt to download or open
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(
                    egui::RichText::new("WxView")
                        .heading()
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Atmospheric Analysis Engine")
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(32.0);

                // Big download button
                let dl_btn = egui::Button::new(
                    egui::RichText::new("   Download Weather Data   ")
                        .strong()
                        .size(16.0)
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::ACCENT)
                .min_size(egui::vec2(280.0, 44.0));
                if ui.add(dl_btn).clicked() {
                    state.active_view = crate::state::View::Download;
                }

                ui.add_space(16.0);
                ui.label(egui::RichText::new("or").color(theme::TEXT_DIM));
                ui.add_space(8.0);

                let open_btn = egui::Button::new(
                    egui::RichText::new("Open GRIB2 File  (Ctrl+O)")
                        .color(egui::Color32::WHITE),
                )
                .min_size(egui::vec2(220.0, 32.0));
                if ui.add(open_btn).clicked() {
                    super::open_file_dialog(state);
                }

                ui.add_space(24.0);
                if state.selected_msg.is_some() && state.field_values.is_some() {
                    ui.label(
                        egui::RichText::new("Data loaded — select a message in the sidebar to render")
                            .color(theme::WARNING),
                    );
                }

                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new("Keyboard: Up/Down navigate  |  R render mode  |  +/- zoom")
                        .small()
                        .color(theme::TEXT_DIM),
                );
            });
        });
        return;
    };

    let available = ui.available_size();
    let tex_size = texture.size_vec2();

    // Compute display size maintaining aspect ratio, fitted to available space
    let fit_scale = (available.x / tex_size.x).min(available.y / tex_size.y);
    let base_size = tex_size * fit_scale;
    let display_size = base_size * state.zoom;

    // Central area with pan/zoom interaction
    let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
    let rect = response.rect;

    // Background
    painter.rect_filled(rect, 0.0, theme::DEEP_BG);

    // Calculate image position (centered + pan offset)
    let center = rect.center().to_vec2() + state.pan;
    let img_min = egui::pos2(
        center.x - display_size.x / 2.0,
        center.y - display_size.y / 2.0,
    );
    let img_rect = egui::Rect::from_min_size(img_min, display_size);

    // Draw the field texture
    painter.image(
        texture.id(),
        img_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    // Thin border around image
    painter.rect_stroke(img_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)));

    // Pan with drag
    if response.dragged() {
        state.pan += response.drag_delta();
    }

    // Zoom with scroll wheel
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
            state.zoom = (state.zoom * factor).clamp(0.05, 50.0);
        }
    }

    // Hover — compute grid coordinates and data value
    if let Some(hover_pos) = response.hover_pos() {
        if img_rect.contains(hover_pos) {
            let rel_x = (hover_pos.x - img_rect.min.x) / display_size.x;
            let rel_y = (hover_pos.y - img_rect.min.y) / display_size.y;
            let gi = (rel_x * state.field_nx as f32) as usize;
            let gj = (rel_y * state.field_ny as f32) as usize;

            if gi < state.field_nx && gj < state.field_ny {
                state.hover_grid = Some((gi, gj));
                if let Some(ref vals) = state.field_values {
                    let idx = gj * state.field_nx + gi;
                    if idx < vals.len() {
                        state.hover_value = Some(vals[idx]);
                    }
                }

                // Crosshair
                let cross_color = egui::Color32::from_white_alpha(120);
                painter.line_segment(
                    [egui::pos2(hover_pos.x, img_rect.min.y), egui::pos2(hover_pos.x, img_rect.max.y)],
                    egui::Stroke::new(0.5, cross_color),
                );
                painter.line_segment(
                    [egui::pos2(img_rect.min.x, hover_pos.y), egui::pos2(img_rect.max.x, hover_pos.y)],
                    egui::Stroke::new(0.5, cross_color),
                );

                // Value tooltip
                if let Some(val) = state.hover_value {
                    let tip = format!("({}, {})  {:.2}", gi, gj, val);
                    let font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
                    let tip_pos = hover_pos + egui::vec2(12.0, -20.0);
                    painter.text(
                        tip_pos,
                        egui::Align2::LEFT_BOTTOM,
                        &tip,
                        font,
                        egui::Color32::WHITE,
                    );
                }
            }
        }
    }

    // Colorbar at bottom of image
    if state.show_colorbar {
        draw_inline_colorbar(&painter, img_rect, state);
    }

    // Grid info overlay (top-left)
    {
        let info = format!(
            "{}x{} | {} | {:.1}..{:.1}",
            state.field_nx,
            state.field_ny,
            state.colormap_name(),
            state.vmin,
            state.vmax,
        );
        let font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
        let text_pos = rect.min + egui::vec2(8.0, 8.0);
        // Background rect for readability
        let galley = painter.layout_no_wrap(info.clone(), font.clone(), egui::Color32::WHITE);
        let bg_rect = egui::Rect::from_min_size(
            text_pos - egui::vec2(2.0, 1.0),
            galley.size() + egui::vec2(4.0, 2.0),
        );
        painter.rect_filled(bg_rect, 2.0, egui::Color32::from_black_alpha(160));
        painter.text(text_pos, egui::Align2::LEFT_TOP, &info, font, egui::Color32::from_white_alpha(200));
    }
}

/// Draw a colorbar below the rendered field image.
fn draw_inline_colorbar(painter: &egui::Painter, img_rect: egui::Rect, state: &AppState) {
    let bar_height = 14.0;
    let bar_y = img_rect.max.y + 4.0;
    let bar_rect = egui::Rect::from_min_size(
        egui::pos2(img_rect.min.x, bar_y),
        egui::vec2(img_rect.width(), bar_height),
    );

    // Get colormap
    let cmap = rustmet_core::render::get_colormap(state.colormap_name())
        .unwrap_or(rustmet_core::render::TEMPERATURE);

    // Draw color gradient as thin vertical strips
    let n_strips = (bar_rect.width() as usize).min(512).max(64);
    let strip_w = bar_rect.width() / n_strips as f32;

    for i in 0..n_strips {
        let t = i as f64 / (n_strips - 1) as f64;
        let (r, g, b) = rustmet_core::render::interpolate_color(cmap, t);
        let color = egui::Color32::from_rgb(r, g, b);
        let x = bar_rect.min.x + i as f32 * strip_w;
        let strip = egui::Rect::from_min_size(egui::pos2(x, bar_rect.min.y), egui::vec2(strip_w + 0.5, bar_height));
        painter.rect_filled(strip, 0.0, color);
    }

    // Border
    painter.rect_stroke(bar_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(60)));

    // Labels
    let font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
    let label_y = bar_rect.max.y + 2.0;

    painter.text(
        egui::pos2(bar_rect.min.x, label_y),
        egui::Align2::LEFT_TOP,
        format!("{:.1}", state.vmin),
        font.clone(),
        egui::Color32::from_white_alpha(180),
    );
    painter.text(
        egui::pos2(bar_rect.center().x, label_y),
        egui::Align2::CENTER_TOP,
        format!("{:.1}", (state.vmin + state.vmax) / 2.0),
        font.clone(),
        egui::Color32::from_white_alpha(140),
    );
    painter.text(
        egui::pos2(bar_rect.max.x, label_y),
        egui::Align2::RIGHT_TOP,
        format!("{:.1}", state.vmax),
        font,
        egui::Color32::from_white_alpha(180),
    );
}
