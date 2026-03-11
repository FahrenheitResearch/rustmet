use crate::state::AppState;
use crate::theme;

pub fn status_bar(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        // Status text
        let status_color = if state.status.contains("Error") || state.status.contains("error") {
            theme::ERROR
        } else if state.status.contains("Exported") || state.status.contains("Done") {
            theme::SUCCESS
        } else {
            theme::TEXT_DIM
        };
        ui.label(egui::RichText::new(&state.status).small().color(status_color));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Hover info
            if let Some((gi, gj)) = state.hover_grid {
                if let Some(val) = state.hover_value {
                    ui.label(
                        egui::RichText::new(format!("({}, {})  val={:.4}", gi, gj, val))
                            .small()
                            .color(theme::ACCENT)
                            .family(egui::FontFamily::Monospace),
                    );
                }
            }

            // Zoom
            ui.label(
                egui::RichText::new(format!("{:.0}%", state.zoom * 100.0))
                    .small()
                    .color(theme::TEXT_DIM),
            );

            // View mode
            ui.label(
                egui::RichText::new(format!(
                    "{} | {}",
                    state.active_view.label(),
                    if state.render_mode == crate::state::RenderMode::Raster {
                        "Raster"
                    } else {
                        "Contour"
                    }
                ))
                .small()
                .color(theme::TEXT_DIM),
            );
        });
    });
}
