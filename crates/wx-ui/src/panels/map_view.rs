use crate::state::{AppState, RenderMode};
use crate::theme;
use std::sync::OnceLock;

// ── Geodata singleton ─────────────────────────────────

static GEODATA: OnceLock<Option<rustmaps::geo::GeoData>> = OnceLock::new();

fn dirs_fallback() -> Option<String> {
    // Try common locations for rustmaps Natural Earth data
    let candidates = [
        // Sibling rustmaps project
        "../rustmaps/data",
        "../../rustmaps/data",
        // Home directory
        #[cfg(target_os = "windows")]
        "C:/Users/drew/rustmaps/data",
    ];
    for c in &candidates {
        let p = std::path::Path::new(c);
        if p.exists() && p.is_dir() {
            return Some(p.display().to_string());
        }
    }
    // Try relative to exe parent's parent (workspace root)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(workspace) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
            let candidate = workspace.join("data");
            if candidate.exists() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

fn get_geodata() -> Option<&'static rustmaps::geo::GeoData> {
    GEODATA
        .get_or_init(|| {
            // Search for geodata directory
            let search_paths = [
                std::env::var("WRF_GEODATA").ok(),
                std::env::var("HRRR_GEODATA").ok(),
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("geodata").display().to_string())),
                Some("./geodata".to_string()),
                Some("geodata".to_string()),
                // Common locations for rustmaps data
                dirs_fallback(),
            ];

            for path_opt in &search_paths {
                if let Some(ref path) = path_opt {
                    let p = std::path::Path::new(path);
                    if p.exists() && p.is_dir() {
                        if let Ok(geo) = rustmaps::geo::GeoData::load(p) {
                            return Some(geo);
                        }
                    }
                }
            }
            None
        })
        .as_ref()
}

// ── Projection builder ────────────────────────────────

fn build_projection(
    grid: &rustmet_core::grib2::GridDefinition,
) -> Option<Box<dyn wx_field::projection::Projection>> {
    use wx_field::projection::*;

    match grid.template {
        30 => {
            let lon1 = if grid.lon1 > 180.0 { grid.lon1 - 360.0 } else { grid.lon1 };
            let lov = if grid.lov > 180.0 { grid.lov - 360.0 } else { grid.lov };
            Some(Box::new(LambertProjection::new(
                grid.latin1, grid.latin2, lov,
                grid.lat1, lon1,
                grid.dx, grid.dy,
                grid.nx, grid.ny,
            )))
        }
        0 => {
            let lon1 = if grid.lon1 > 180.0 { grid.lon1 - 360.0 } else { grid.lon1 };
            let lon2 = if grid.lon2 > 180.0 { grid.lon2 - 360.0 } else { grid.lon2 };
            Some(Box::new(LatLonProjection::new(
                grid.lat1, lon1, grid.lat2, lon2, grid.nx, grid.ny,
            )))
        }
        _ => None,
    }
}

// ── Render field ──────────────────────────────────────

fn render_field(state: &mut AppState, ctx: &egui::Context) {
    let Some(ref values) = state.field_values else { return };
    let nx = state.field_nx;
    let ny = state.field_ny;
    if nx == 0 || ny == 0 { return; }

    let cmap_name = state.colormap_name();

    let pixels: Vec<u8> = match state.render_mode {
        RenderMode::Raster => {
            rustmet_core::render::render_raster(values, nx, ny, cmap_name, state.vmin, state.vmax)
        }
        RenderMode::FilledContour => {
            let levels = rustmet_core::render::auto_levels(state.vmin, state.vmax, state.contour_levels);
            rustmet_core::render::render_filled_contours(
                values, nx, ny, &levels, cmap_name, nx as u32, ny as u32,
            )
        }
    };

    let image = egui::ColorImage::from_rgba_unmultiplied([nx, ny], &pixels);
    let texture = ctx.load_texture("field_map", image, egui::TextureOptions::LINEAR);
    state.field_texture = Some(texture);
    state.needs_rerender = false;
}

// ── Main map view ─────────────────────────────────────

pub fn map_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    if state.needs_rerender && state.field_values.is_some() {
        render_field(state, ctx);
    }

    let Some(ref texture) = state.field_texture else {
        // No data — welcome screen
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(egui::RichText::new("WxView").heading().strong().color(theme::ACCENT));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Atmospheric Analysis Engine").color(theme::TEXT_DIM));
                ui.add_space(32.0);

                let dl_btn = egui::Button::new(
                    egui::RichText::new("   Download Weather Data   ").strong().size(16.0).color(egui::Color32::WHITE),
                ).fill(theme::ACCENT).min_size(egui::vec2(280.0, 44.0));
                if ui.add(dl_btn).clicked() {
                    state.active_view = crate::state::View::Download;
                }

                ui.add_space(16.0);
                ui.label(egui::RichText::new("or").color(theme::TEXT_DIM));
                ui.add_space(8.0);

                let open_btn = egui::Button::new(
                    egui::RichText::new("Open GRIB2 File  (Ctrl+O)").color(egui::Color32::WHITE),
                ).min_size(egui::vec2(220.0, 32.0));
                if ui.add(open_btn).clicked() {
                    super::open_file_dialog(state);
                }

                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new("Up/Down navigate  |  R render mode  |  +/- zoom")
                        .small().color(theme::TEXT_DIM),
                );
            });
        });
        return;
    };

    let available = ui.available_size();
    let tex_size = texture.size_vec2();
    let fit_scale = (available.x / tex_size.x).min(available.y / tex_size.y);
    let base_size = tex_size * fit_scale;
    let display_size = base_size * state.zoom;

    let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, theme::DEEP_BG);

    let center = rect.center().to_vec2() + state.pan;
    let img_min = egui::pos2(center.x - display_size.x / 2.0, center.y - display_size.y / 2.0);
    let img_rect = egui::Rect::from_min_size(img_min, display_size);

    // Clip to visible area
    let clip = painter.clip_rect();

    // Draw field texture
    painter.image(
        texture.id(), img_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    // ── Draw basemap borders ──────────────────────────
    if let Some(ref grib) = state.grib {
        if let Some(msg) = state.selected_msg.and_then(|i| grib.messages.get(i)) {
            if let Some(proj) = build_projection(&msg.grid) {
                draw_borders(&painter, img_rect, state.field_nx, state.field_ny, proj.as_ref(), &clip);
            }
        }
    }

    // Border around image
    painter.rect_stroke(img_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)));

    // Pan
    if response.dragged() {
        state.pan += response.drag_delta();
    }

    // Zoom with scroll
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
            state.zoom = (state.zoom * factor).clamp(0.05, 50.0);
        }
    }

    // Hover
    if let Some(hover_pos) = response.hover_pos() {
        if img_rect.contains(hover_pos) {
            let rel_x = (hover_pos.x - img_rect.min.x) / display_size.x;
            let rel_y = (hover_pos.y - img_rect.min.y) / display_size.y;
            let gi = (rel_x * state.field_nx as f32) as usize;
            let gj = (rel_y * state.field_ny as f32) as usize;

            if gi < state.field_nx && gj < state.field_ny {
                state.hover_grid = Some((gi, gj));

                // Get lat/lon if projection available
                let latlon_str = if let Some(ref grib) = state.grib {
                    if let Some(msg) = state.selected_msg.and_then(|i| grib.messages.get(i)) {
                        if let Some(proj) = build_projection(&msg.grid) {
                            use wx_field::projection::Projection;
                            let (lat, lon) = proj.grid_to_latlon(gi as f64, gj as f64);
                            Some(format!("{:.2}N {:.2}W", lat, -lon))
                        } else { None }
                    } else { None }
                } else { None };

                if let Some(ref vals) = state.field_values {
                    let idx = gj * state.field_nx + gi;
                    if idx < vals.len() {
                        state.hover_value = Some(vals[idx]);
                    }
                }

                // Crosshair
                let cc = egui::Color32::from_white_alpha(100);
                painter.line_segment(
                    [egui::pos2(hover_pos.x, img_rect.min.y), egui::pos2(hover_pos.x, img_rect.max.y)],
                    egui::Stroke::new(0.5, cc),
                );
                painter.line_segment(
                    [egui::pos2(img_rect.min.x, hover_pos.y), egui::pos2(img_rect.max.x, hover_pos.y)],
                    egui::Stroke::new(0.5, cc),
                );

                // Tooltip
                if let Some(val) = state.hover_value {
                    let tip = if let Some(ref ll) = latlon_str {
                        format!("{}  val={:.2}", ll, val)
                    } else {
                        format!("({},{})  val={:.2}", gi, gj, val)
                    };
                    let font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
                    let tip_pos = hover_pos + egui::vec2(14.0, -18.0);
                    // Background
                    let galley = painter.layout_no_wrap(tip.clone(), font.clone(), egui::Color32::WHITE);
                    let bg = egui::Rect::from_min_size(tip_pos - egui::vec2(2.0, galley.size().y + 1.0), galley.size() + egui::vec2(4.0, 2.0));
                    painter.rect_filled(bg, 2.0, egui::Color32::from_black_alpha(180));
                    painter.text(tip_pos, egui::Align2::LEFT_BOTTOM, &tip, font, egui::Color32::WHITE);
                }
            }
        }
    }

    // Colorbar
    if state.show_colorbar {
        draw_inline_colorbar(&painter, img_rect, state);
    }

    // Info overlay
    {
        let info = format!(
            "{}x{} | {} | {:.1}..{:.1}",
            state.field_nx, state.field_ny, state.colormap_name(), state.vmin, state.vmax,
        );
        let font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
        let text_pos = rect.min + egui::vec2(8.0, 8.0);
        let galley = painter.layout_no_wrap(info.clone(), font.clone(), egui::Color32::WHITE);
        let bg_rect = egui::Rect::from_min_size(text_pos - egui::vec2(2.0, 1.0), galley.size() + egui::vec2(4.0, 2.0));
        painter.rect_filled(bg_rect, 2.0, egui::Color32::from_black_alpha(160));
        painter.text(text_pos, egui::Align2::LEFT_TOP, &info, font, egui::Color32::from_white_alpha(200));
    }
}

// ── Border drawing ────────────────────────────────────

fn draw_borders(
    painter: &egui::Painter,
    img_rect: egui::Rect,
    nx: usize,
    ny: usize,
    proj: &dyn wx_field::projection::Projection,
    clip: &egui::Rect,
) {
    use wx_field::projection::Projection;

    let Some(geo) = get_geodata() else { return };

    let scale_x = img_rect.width() / nx as f32;
    let scale_y = img_rect.height() / ny as f32;

    // Convert lat/lon to pixel position on the displayed image
    let to_px = |lat: f64, lon: f64| -> Option<egui::Pos2> {
        let (gi, gj) = proj.latlon_to_grid(lat, lon);
        // Check if point is within grid bounds (with margin)
        if gi < -0.5 || gi > nx as f64 + 0.5 || gj < -0.5 || gj > ny as f64 + 0.5 {
            return None;
        }
        let px = img_rect.min.x + gi as f32 * scale_x;
        let py = img_rect.min.y + gj as f32 * scale_y;
        Some(egui::pos2(px, py))
    };

    // Draw polyline segments — geodata stores (lon, lat) pairs
    let draw_polyline = |points: &[(f64, f64)], color: egui::Color32, width: f32| {
        for pair in points.windows(2) {
            let (lon1, lat1) = pair[0];
            let (lon2, lat2) = pair[1];

            // Skip very long segments (crossing dateline or outside domain)
            if (lon2 - lon1).abs() > 10.0 || (lat2 - lat1).abs() > 10.0 {
                continue;
            }

            if let (Some(p1), Some(p2)) = (to_px(lat1, lon1), to_px(lat2, lon2)) {
                // Only draw if at least one endpoint is in clip rect (rough culling)
                if clip.contains(p1) || clip.contains(p2) {
                    painter.line_segment([p1, p2], egui::Stroke::new(width, color));
                }
            }
        }
    };

    let border_color = egui::Color32::from_rgba_premultiplied(180, 180, 180, 200);
    let state_color = egui::Color32::from_rgba_premultiplied(120, 120, 120, 160);
    let coast_color = egui::Color32::from_rgba_premultiplied(200, 200, 200, 220);

    // Country borders
    for polyline in &geo.country_borders {
        draw_polyline(polyline, border_color, 1.2);
    }

    // State borders
    for polyline in &geo.state_borders {
        draw_polyline(polyline, state_color, 0.7);
    }

    // Coastlines
    let coastlines = geo.coastlines_for_zoom(5);
    for polyline in coastlines {
        draw_polyline(polyline, coast_color, 0.8);
    }
}

// ── Colorbar ──────────────────────────────────────────

fn draw_inline_colorbar(painter: &egui::Painter, img_rect: egui::Rect, state: &AppState) {
    let bar_height = 14.0;
    let bar_y = img_rect.max.y + 4.0;
    let bar_rect = egui::Rect::from_min_size(
        egui::pos2(img_rect.min.x, bar_y),
        egui::vec2(img_rect.width(), bar_height),
    );

    let cmap = rustmet_core::render::get_colormap(state.colormap_name())
        .unwrap_or(rustmet_core::render::TEMPERATURE);

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

    painter.rect_stroke(bar_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(60)));

    let font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
    let label_y = bar_rect.max.y + 2.0;
    painter.text(egui::pos2(bar_rect.min.x, label_y), egui::Align2::LEFT_TOP, format!("{:.1}", state.vmin), font.clone(), egui::Color32::from_white_alpha(180));
    painter.text(egui::pos2(bar_rect.center().x, label_y), egui::Align2::CENTER_TOP, format!("{:.1}", (state.vmin + state.vmax) / 2.0), font.clone(), egui::Color32::from_white_alpha(140));
    painter.text(egui::pos2(bar_rect.max.x, label_y), egui::Align2::RIGHT_TOP, format!("{:.1}", state.vmax), font, egui::Color32::from_white_alpha(180));
}
