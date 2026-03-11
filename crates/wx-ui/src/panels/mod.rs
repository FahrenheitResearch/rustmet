mod sidebar;
mod map_view;
mod message_list;
mod sounding;
mod hodograph;
mod download;
mod info_panel;

pub use sidebar::sidebar;
pub use map_view::map_view;
pub use sounding::sounding_view;
pub use hodograph::hodograph_view;
pub use download::download_panel;
pub use info_panel::info_panel;

use crate::state::AppState;

/// Top menu bar.
pub fn menu_bar(ui: &mut egui::Ui, state: &mut AppState) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Open GRIB2...  (Ctrl+O)").clicked() {
                open_file_dialog(state);
                ui.close_menu();
            }
            if ui.button("Export PNG...  (Ctrl+E)").clicked() {
                export_png(state);
                ui.close_menu();
            }
            ui.separator();
            if !state.recent_files.is_empty() {
                ui.label("Recent:");
                let recent = state.recent_files.clone();
                for path in &recent {
                    let label = path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    if ui.button(&label).clicked() {
                        let p = path.clone();
                        state.open_file(p);
                        ui.close_menu();
                    }
                }
                ui.separator();
            }
            if ui.button("Quit").clicked() {
                std::process::exit(0);
            }
        });

        ui.menu_button("View", |ui| {
            for &view in crate::state::ALL_VIEWS {
                if ui.button(view.label()).clicked() {
                    state.active_view = view;
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button("Reset Zoom  (Home)").clicked() {
                state.zoom = 1.0;
                state.pan = egui::Vec2::ZERO;
                ui.close_menu();
            }
        });

        ui.menu_button("Render", |ui| {
            let mut raster = state.render_mode == crate::state::RenderMode::Raster;
            if ui.checkbox(&mut raster, "Raster").clicked() {
                state.render_mode = crate::state::RenderMode::Raster;
                state.needs_rerender = true;
                ui.close_menu();
            }
            let mut filled = state.render_mode == crate::state::RenderMode::FilledContour;
            if ui.checkbox(&mut filled, "Filled Contour").clicked() {
                state.render_mode = crate::state::RenderMode::FilledContour;
                state.needs_rerender = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.checkbox(&mut state.auto_range, "Auto Range").clicked() {
                state.needs_rerender = true;
            }
            if ui.checkbox(&mut state.show_colorbar, "Show Colorbar").clicked() {
                // no rerender needed, colorbar is drawn by egui
            }
        });

        ui.menu_button("Help", |ui| {
            ui.label("WxView v0.1.0");
            ui.label("Atmospheric Analysis Engine");
            ui.separator();
            ui.label("Shortcuts:");
            ui.label("  Ctrl+O     Open file");
            ui.label("  Ctrl+E     Export PNG");
            ui.label("  Up/Down    Navigate messages");
            ui.label("  +/-        Zoom in/out");
            ui.label("  Home       Fit to view");
            ui.label("  R          Toggle render mode");
            ui.label("  A          Toggle auto-range");
            ui.label("  1-5        Switch views");
        });

        // Right-aligned: file name
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(ref path) = state.file_path {
                let name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                ui.label(
                    egui::RichText::new(name)
                        .color(crate::theme::TEXT_DIM)
                        .small(),
                );
            }
        });
    });
}

/// Open file dialog and load GRIB2 file.
pub fn open_file_dialog(state: &mut AppState) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("GRIB2 files", &["grib2", "grb2", "grb", "grib"])
        .add_filter("All files", &["*"])
        .pick_file()
    {
        state.open_file(path);
    }
}

/// Export the current field as a PNG file.
pub fn export_png(state: &mut AppState) {
    let Some(ref values) = state.field_values else {
        state.status = "No field data to export".into();
        return;
    };
    if state.field_nx == 0 || state.field_ny == 0 {
        return;
    }

    if let Some(path) = rfd::FileDialog::new()
        .add_filter("PNG image", &["png"])
        .set_file_name("wxview_export.png")
        .save_file()
    {
        let cmap_name = state.colormap_name();
        let pixels = rustmet_core::render::render_raster(
            values,
            state.field_nx,
            state.field_ny,
            cmap_name,
            state.vmin,
            state.vmax,
        );
        match rustmet_core::render::write_png(
            &pixels,
            state.field_nx as u32,
            state.field_ny as u32,
            &path,
        ) {
            Ok(()) => {
                state.status = format!("Exported: {}", path.display());
                state.last_export = Some(path);
            }
            Err(e) => {
                state.status = format!("Export error: {e}");
            }
        }
    }
}
