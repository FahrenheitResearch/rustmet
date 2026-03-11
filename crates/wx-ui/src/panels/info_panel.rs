use crate::state::AppState;
use crate::theme;

pub fn info_panel(ui: &mut egui::Ui, state: &AppState) {
    ui.heading("File Information");
    ui.add_space(8.0);

    let Some(ref path) = state.file_path else {
        ui.label(egui::RichText::new("No file loaded").color(theme::TEXT_DIM));
        return;
    };

    // File info
    ui.group(|ui| {
        ui.label(egui::RichText::new("File").small().color(theme::TEXT_DIM));
        ui.label(egui::RichText::new(path.display().to_string()).strong());

        if let Ok(meta) = std::fs::metadata(path) {
            let size = meta.len();
            let size_str = if size > 1_000_000_000 {
                format!("{:.2} GB", size as f64 / 1e9)
            } else if size > 1_000_000 {
                format!("{:.2} MB", size as f64 / 1e6)
            } else {
                format!("{:.1} KB", size as f64 / 1e3)
            };
            ui.label(format!("Size: {size_str}"));
        }
        ui.label(format!("Messages: {}", state.messages.len()));
    });

    ui.add_space(8.0);

    // Grid info from first message
    if let Some(ref grib) = state.grib {
        if let Some(msg) = grib.messages.first() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Grid Definition").small().color(theme::TEXT_DIM));
                ui.label(format!("Template: {}", msg.grid.template));
                ui.label(format!("Dimensions: {} x {}", msg.grid.nx, msg.grid.ny));
                ui.label(format!("Total points: {}", msg.grid.nx as u64 * msg.grid.ny as u64));
                ui.label(format!(
                    "Lat range: {:.4} to {:.4}",
                    msg.grid.lat1, msg.grid.lat2
                ));
                ui.label(format!(
                    "Lon range: {:.4} to {:.4}",
                    msg.grid.lon1, msg.grid.lon2
                ));
                ui.label(format!("dx: {:.6}  dy: {:.6}", msg.grid.dx, msg.grid.dy));
                ui.label(format!("Scan mode: {}", msg.grid.scan_mode));
            });
        }
    }

    ui.add_space(8.0);

    // Message table
    ui.label(egui::RichText::new("Message Details").small().color(theme::TEXT_DIM));
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("msg_table")
            .striped(true)
            .min_col_width(40.0)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                // Header
                ui.label(egui::RichText::new("#").strong().small());
                ui.label(egui::RichText::new("Parameter").strong().small());
                ui.label(egui::RichText::new("Units").strong().small());
                ui.label(egui::RichText::new("Level").strong().small());
                ui.label(egui::RichText::new("F-Hr").strong().small());
                ui.label(egui::RichText::new("Grid").strong().small());
                ui.end_row();

                for msg in &state.messages {
                    ui.label(
                        egui::RichText::new(format!("{}", msg.index))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.label(egui::RichText::new(&msg.name).small());
                    ui.label(egui::RichText::new(&msg.units).small().color(theme::TEXT_DIM));
                    ui.label(egui::RichText::new(&msg.level).small());
                    ui.label(
                        egui::RichText::new(format!("{:03}", msg.forecast_hr))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new(format!("{}x{}", msg.nx, msg.ny))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.end_row();
                }
            });
    });
}
