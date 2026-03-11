use crate::state::{self, AppState, COLORMAP_NAMES, RenderMode};
use crate::theme;

pub fn sidebar(ui: &mut egui::Ui, state: &mut AppState, _ctx: &egui::Context) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;

        // ── Navigation ────────────────────
        ui.add_space(4.0);
        ui.label(egui::RichText::new("NAVIGATION").small().color(theme::TEXT_DIM));
        ui.add_space(2.0);

        for &view in state::ALL_VIEWS {
            let selected = state.active_view == view;
            let text = format!("  {}", view.label());
            let btn = egui::Button::new(
                egui::RichText::new(&text).color(if selected {
                    egui::Color32::WHITE
                } else {
                    theme::TEXT_DIM
                }),
            )
            .fill(if selected {
                theme::ACCENT.linear_multiply(0.25)
            } else {
                egui::Color32::TRANSPARENT
            })
            .min_size(egui::vec2(ui.available_width(), 28.0));

            if ui.add(btn).clicked() {
                state.active_view = view;
            }
        }

        ui.add_space(12.0);
        ui.separator();

        // ── File ──────────────────────────
        ui.add_space(8.0);
        ui.label(egui::RichText::new("FILE").small().color(theme::TEXT_DIM));
        ui.add_space(2.0);

        if ui
            .button(egui::RichText::new("Open GRIB2...").color(egui::Color32::WHITE))
            .clicked()
        {
            super::open_file_dialog(state);
        }

        if let Some(ref path) = state.file_path {
            let name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            ui.label(egui::RichText::new(&name).small().color(theme::ACCENT));
            ui.label(
                egui::RichText::new(format!("{} messages", state.messages.len()))
                    .small()
                    .color(theme::TEXT_DIM),
            );
        }

        ui.add_space(12.0);
        ui.separator();

        // ── Messages ──────────────────────
        ui.add_space(8.0);
        ui.label(egui::RichText::new("MESSAGES").small().color(theme::TEXT_DIM));
        ui.add_space(2.0);

        super::message_list::message_list(ui, state);

        ui.add_space(12.0);
        ui.separator();

        // ── Render Controls ───────────────
        ui.add_space(8.0);
        ui.label(egui::RichText::new("RENDER").small().color(theme::TEXT_DIM));
        ui.add_space(2.0);

        // Render mode
        ui.horizontal(|ui| {
            ui.label("Mode:");
            if ui
                .selectable_label(state.render_mode == RenderMode::Raster, "Raster")
                .clicked()
            {
                state.render_mode = RenderMode::Raster;
                state.needs_rerender = true;
            }
            if ui
                .selectable_label(state.render_mode == RenderMode::FilledContour, "Contour")
                .clicked()
            {
                state.render_mode = RenderMode::FilledContour;
                state.needs_rerender = true;
            }
        });

        // Contour levels (only shown for filled contour mode)
        if state.render_mode == RenderMode::FilledContour {
            ui.horizontal(|ui| {
                ui.label("Levels:");
                let mut levels = state.contour_levels as f32;
                if ui
                    .add(egui::Slider::new(&mut levels, 5.0..=100.0).step_by(1.0))
                    .changed()
                {
                    state.contour_levels = levels as usize;
                    state.needs_rerender = true;
                }
            });
        }

        // Colormap
        ui.horizontal(|ui| {
            ui.label("Colormap:");
        });
        let current_cmap = state.colormap_name().to_string();
        egui::ComboBox::from_id_salt("colormap_combo")
            .selected_text(&current_cmap)
            .width(ui.available_width() - 16.0)
            .show_ui(ui, |ui| {
                for (i, &name) in COLORMAP_NAMES.iter().enumerate() {
                    if ui.selectable_label(state.colormap_idx == i, name).clicked() {
                        state.colormap_idx = i;
                        state.needs_rerender = true;
                    }
                }
            });

        // Auto colormap button
        if ui
            .add_enabled(state.selected_msg.is_some(), egui::Button::new("Auto-detect colormap"))
            .clicked()
        {
            state.auto_colormap();
            state.needs_rerender = true;
        }

        // Range
        ui.add_space(4.0);
        ui.checkbox(&mut state.auto_range, "Auto range");

        if !state.auto_range {
            ui.horizontal(|ui| {
                ui.label("Min:");
                let mut vmin = state.vmin as f32;
                if ui
                    .add(egui::DragValue::new(&mut vmin).speed(0.1))
                    .changed()
                {
                    state.vmin = vmin as f64;
                    state.needs_rerender = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Max:");
                let mut vmax = state.vmax as f32;
                if ui
                    .add(egui::DragValue::new(&mut vmax).speed(0.1))
                    .changed()
                {
                    state.vmax = vmax as f64;
                    state.needs_rerender = true;
                }
            });
        }

        // Zoom
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(egui::RichText::new("VIEW").small().color(theme::TEXT_DIM));
        ui.horizontal(|ui| {
            ui.label("Zoom:");
            ui.label(format!("{:.0}%", state.zoom * 100.0));
        });
        ui.horizontal(|ui| {
            if ui.button("-").clicked() {
                state.zoom = (state.zoom / 1.25).max(0.1);
            }
            if ui.button("Fit").clicked() {
                state.zoom = 1.0;
                state.pan = egui::Vec2::ZERO;
            }
            if ui.button("+").clicked() {
                state.zoom = (state.zoom * 1.25).min(20.0);
            }
        });
    });
}
