use crate::state::{AppState, DownloadEvent};
use crate::theme;
use std::sync::mpsc;

const MODELS: &[&str] = &["hrrr", "gfs", "nam", "rap"];

pub fn download_panel(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Download Model Data");
    ui.add_space(8.0);

    ui.group(|ui| {
        ui.set_width(ui.available_width().min(500.0));

        // Model selector
        ui.horizontal(|ui| {
            ui.label("Model:");
            for (i, &model) in MODELS.iter().enumerate() {
                if ui
                    .selectable_label(state.dl_model == i, model.to_uppercase())
                    .clicked()
                {
                    state.dl_model = i;
                }
            }
        });

        ui.add_space(4.0);

        // Run time
        ui.horizontal(|ui| {
            ui.label("Run:     ");
            ui.text_edit_singleline(&mut state.dl_run);
        });
        ui.label(
            egui::RichText::new("  Format: YYYY-MM-DD/HHz or \"latest\"")
                .small()
                .color(theme::TEXT_DIM),
        );

        ui.add_space(4.0);

        // Forecast hours
        ui.horizontal(|ui| {
            ui.label("F-hours: ");
            ui.text_edit_singleline(&mut state.dl_fhours);
        });
        ui.label(
            egui::RichText::new("  Range: \"0-18\" or list: \"0,6,12,18\"")
                .small()
                .color(theme::TEXT_DIM),
        );

        ui.add_space(4.0);

        // Variable filter
        ui.horizontal(|ui| {
            ui.label("Vars:    ");
            ui.text_edit_singleline(&mut state.dl_vars);
        });
        ui.label(
            egui::RichText::new("  e.g. \"TMP:2 m,CAPE:surface,REFC\" (empty = all)")
                .small()
                .color(theme::TEXT_DIM),
        );

        ui.add_space(4.0);

        // Output directory
        ui.horizontal(|ui| {
            ui.label("Output:  ");
            ui.text_edit_singleline(&mut state.dl_output);
            if ui.button("Browse...").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    state.dl_output = dir.display().to_string();
                }
            }
        });

        ui.add_space(12.0);

        // Download button
        ui.horizontal(|ui| {
            let can_download = !state.dl_active && !state.dl_run.is_empty();
            if ui
                .add_enabled(can_download, egui::Button::new(
                    egui::RichText::new("Download").strong().color(egui::Color32::WHITE),
                ).min_size(egui::vec2(120.0, 32.0)))
                .clicked()
            {
                start_download(state);
            }

            if state.dl_active {
                ui.spinner();
                ui.label(format!("{:.0}%", state.dl_progress * 100.0));
            }
        });
    });

    ui.add_space(16.0);

    // Progress / status log
    if !state.dl_status.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new("Log:").small().color(theme::TEXT_DIM));

        if state.dl_active {
            let bar = egui::ProgressBar::new(state.dl_progress)
                .show_percentage()
                .animate(true);
            ui.add(bar);
        }

        egui::ScrollArea::vertical()
            .max_height(300.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &state.dl_status {
                    let color = if line.starts_with("ERROR") {
                        theme::ERROR
                    } else if line.starts_with("Done") {
                        theme::SUCCESS
                    } else {
                        theme::TEXT_DIM
                    };
                    ui.label(egui::RichText::new(line).small().color(color));
                }
            });
    }
}

fn start_download(state: &mut AppState) {
    let model = MODELS[state.dl_model].to_string();
    let run = state.dl_run.clone();
    let fhours = state.dl_fhours.clone();
    let vars = state.dl_vars.clone();
    let output = state.dl_output.clone();

    let (tx, rx) = mpsc::channel::<DownloadEvent>();
    state.dl_rx = Some(rx);
    state.dl_active = true;
    state.dl_progress = 0.0;
    state.dl_status.clear();
    state.dl_status.push(format!("Starting {} download: run={}, fhours={}", model.to_uppercase(), run, fhours));

    std::thread::spawn(move || {
        // Parse forecast hours
        let hours: Vec<u32> = if fhours.contains('-') {
            let parts: Vec<&str> = fhours.split('-').collect();
            if parts.len() == 2 {
                let start: u32 = parts[0].trim().parse().unwrap_or(0);
                let end: u32 = parts[1].trim().parse().unwrap_or(0);
                (start..=end).collect()
            } else {
                vec![0]
            }
        } else {
            fhours.split(',').filter_map(|s| s.trim().parse().ok()).collect()
        };

        if hours.is_empty() {
            let _ = tx.send(DownloadEvent::Error("Invalid forecast hours".into()));
            return;
        }

        let total = hours.len();
        let out_dir = std::path::PathBuf::from(&output);
        let _ = std::fs::create_dir_all(&out_dir);

        for (i, &fhr) in hours.iter().enumerate() {
            let pct = i as f32 / total as f32;
            let _ = tx.send(DownloadEvent::Progress(
                format!("Downloading F{:03}...", fhr),
                pct,
            ));

            // Build URL based on model
            let url = match model.as_str() {
                "hrrr" => format!(
                    "https://nomads.ncep.noaa.gov/pub/data/nccf/com/hrrr/prod/hrrr.{}/conus/hrrr.t{}z.wrfprsf{:02}.grib2",
                    run.replace("/", "").replace("z", "").get(..8).unwrap_or("20250101"),
                    run.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<String>().chars().rev().collect::<String>(),
                    fhr,
                ),
                _ => {
                    let _ = tx.send(DownloadEvent::Progress(
                        format!("Model '{}' URL builder not yet implemented, using HRRR fallback", model),
                        pct,
                    ));
                    continue;
                }
            };

            let out_path = out_dir.join(format!("{}_{}_f{:03}.grib2", model, run.replace("/", "_").replace(" ", "_"), fhr));

            // Download using DownloadClient
            let client = match rustmet_core::download::DownloadClient::new() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(DownloadEvent::Error(format!("Client init failed: {e}")));
                    return;
                }
            };
            match client.get_bytes(&url) {
                Ok(data) => {
                    if let Err(e) = std::fs::write(&out_path, &data) {
                        let _ = tx.send(DownloadEvent::Error(format!("Write failed: {e}")));
                        continue;
                    }
                    let _ = tx.send(DownloadEvent::Progress(
                        format!("Saved F{:03}: {} ({:.1} MB)", fhr, out_path.display(), data.len() as f64 / 1e6),
                        (i + 1) as f32 / total as f32,
                    ));
                    // Send the first completed file for auto-open
                    if i == 0 {
                        let _ = tx.send(DownloadEvent::Complete(out_path));
                    }
                }
                Err(e) => {
                    let _ = tx.send(DownloadEvent::Error(format!("F{:03} failed: {e}", fhr)));
                }
            }
        }

        let _ = tx.send(DownloadEvent::Progress("All downloads complete".into(), 1.0));
    });
}
