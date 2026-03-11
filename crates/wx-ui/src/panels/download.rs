use crate::state::{AppState, DownloadEvent};
use crate::theme;
use std::sync::mpsc;

const MODELS: &[&str] = &["hrrr", "gfs", "nam", "rap"];
const PRODUCTS: &[&str] = &["wrfprs", "wrfsfc", "wrfnat"];

struct Preset {
    label: &'static str,
    desc: &'static str,
    model: usize,
    fhours: &'static str,
    vars: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        label: "HRRR Surface Analysis",
        desc: "Latest HRRR F000 — temperature, dewpoint, wind, MSLP, reflectivity",
        model: 0,
        fhours: "0",
        vars: "TMP:2 m,DPT:2 m,UGRD:10 m,VGRD:10 m,MSLMA,REFC",
    },
    Preset {
        label: "HRRR Severe Weather",
        desc: "Latest HRRR F000-F018 — CAPE, SRH, reflectivity, UH",
        model: 0,
        fhours: "0-18",
        vars: "CAPE:surface,CIN:surface,HLCY:0-3000,HLCY:0-1000,REFC,MXUPHL",
    },
    Preset {
        label: "HRRR Full F000",
        desc: "Latest HRRR F000 — all variables (large, ~100+ MB)",
        model: 0,
        fhours: "0",
        vars: "",
    },
    Preset {
        label: "GFS Global F000-F024",
        desc: "Latest GFS 0.25deg — first 24 hours, all variables",
        model: 1,
        fhours: "0,6,12,18,24",
        vars: "",
    },
    Preset {
        label: "RAP Analysis",
        desc: "Latest RAP F000 — all variables",
        model: 3,
        fhours: "0",
        vars: "",
    },
];

pub fn download_panel(ui: &mut egui::Ui, state: &mut AppState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // ── Quick Start ──────────────────────
        ui.add_space(12.0);
        ui.heading("Download Weather Data");
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Get real-time model data from NOAA").color(theme::TEXT_DIM));
        ui.add_space(16.0);

        // Quick-start preset buttons
        ui.label(egui::RichText::new("QUICK START").small().strong().color(theme::ACCENT));
        ui.add_space(4.0);

        let preset_clicked: Option<usize> = {
            let mut clicked = None;
            for (i, preset) in PRESETS.iter().enumerate() {
                ui.horizontal(|ui| {
                    let btn = egui::Button::new(
                        egui::RichText::new(preset.label)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .min_size(egui::vec2(240.0, 32.0))
                    .fill(theme::ACCENT.linear_multiply(0.35));

                    if ui.add_enabled(!state.dl_active, btn).clicked() {
                        clicked = Some(i);
                    }
                    ui.label(egui::RichText::new(preset.desc).small().color(theme::TEXT_DIM));
                });
            }
            clicked
        };

        if let Some(i) = preset_clicked {
            let preset = &PRESETS[i];
            state.dl_model = preset.model;
            state.dl_fhours = preset.fhours.to_string();
            state.dl_vars = preset.vars.to_string();
            state.dl_run = "latest".to_string();
            start_download(state);
        }

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(12.0);

        // ── Custom Download ──────────────────
        ui.label(egui::RichText::new("CUSTOM DOWNLOAD").small().strong().color(theme::TEXT_DIM));
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.set_width(ui.available_width().min(600.0));

            // Model selector — big buttons
            ui.horizontal(|ui| {
                ui.label("Model:");
                for (i, &model) in MODELS.iter().enumerate() {
                    let selected = state.dl_model == i;
                    let btn = egui::Button::new(
                        egui::RichText::new(model.to_uppercase())
                            .strong()
                            .color(if selected { egui::Color32::WHITE } else { theme::TEXT_DIM }),
                    )
                    .fill(if selected { theme::ACCENT.linear_multiply(0.4) } else { egui::Color32::TRANSPARENT })
                    .min_size(egui::vec2(60.0, 28.0));
                    if ui.add(btn).clicked() {
                        state.dl_model = i;
                    }
                }
            });

            ui.add_space(8.0);

            // Run time
            ui.horizontal(|ui| {
                ui.label("Run:         ");
                let w = ui.available_width().min(300.0);
                ui.add(egui::TextEdit::singleline(&mut state.dl_run).desired_width(w));
            });
            ui.label(
                egui::RichText::new("     YYYYMMDD/HHz  or  \"latest\"")
                    .small()
                    .color(theme::TEXT_DIM),
            );

            ui.add_space(4.0);

            // Forecast hours
            ui.horizontal(|ui| {
                ui.label("F-hours:     ");
                let w = ui.available_width().min(300.0);
                ui.add(egui::TextEdit::singleline(&mut state.dl_fhours).desired_width(w));
            });
            ui.label(
                egui::RichText::new("     Range \"0-18\"  or  list \"0,6,12,18\"")
                    .small()
                    .color(theme::TEXT_DIM),
            );

            ui.add_space(4.0);

            // Variable filter
            ui.horizontal(|ui| {
                ui.label("Variables:   ");
                let w = ui.available_width().min(300.0);
                ui.add(egui::TextEdit::singleline(&mut state.dl_vars).desired_width(w));
            });
            ui.label(
                egui::RichText::new("     e.g. \"TMP:2 m,CAPE:surface,REFC\"  (empty = all)")
                    .small()
                    .color(theme::TEXT_DIM),
            );

            ui.add_space(4.0);

            // Output directory
            ui.horizontal(|ui| {
                ui.label("Output dir:  ");
                let w = ui.available_width().min(250.0) - 80.0;
                ui.add(egui::TextEdit::singleline(&mut state.dl_output).desired_width(w));
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
                let btn = egui::Button::new(
                    egui::RichText::new("   Download   ")
                        .strong()
                        .size(15.0)
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::ACCENT)
                .min_size(egui::vec2(160.0, 36.0));

                if ui.add_enabled(can_download, btn).clicked() {
                    start_download(state);
                }

                if state.dl_active {
                    ui.add_space(8.0);
                    ui.spinner();
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", state.dl_progress * 100.0))
                            .strong()
                            .color(theme::ACCENT),
                    );
                }
            });
        });

        ui.add_space(16.0);

        // ── Progress log ─────────────────────
        if !state.dl_status.is_empty() {
            ui.separator();
            ui.add_space(4.0);

            if state.dl_active {
                ui.add(
                    egui::ProgressBar::new(state.dl_progress)
                        .show_percentage()
                        .animate(true),
                );
                ui.add_space(4.0);
            }

            ui.label(egui::RichText::new("LOG").small().color(theme::TEXT_DIM));
            egui::ScrollArea::vertical()
                .max_height(250.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &state.dl_status {
                        let color = if line.starts_with("ERROR") || line.contains("failed") {
                            theme::ERROR
                        } else if line.starts_with("Done") || line.contains("Saved") || line.contains("complete") {
                            theme::SUCCESS
                        } else {
                            theme::TEXT_DIM
                        };
                        ui.label(egui::RichText::new(line).small().color(color).family(egui::FontFamily::Monospace));
                    }
                });
        }
    });
}

fn resolve_latest_run(model: &str) -> (String, u32) {
    // Use current UTC time, back off ~2 hours for data availability
    let now = chrono::Utc::now() - chrono::Duration::hours(2);
    let date = now.format("%Y%m%d").to_string();
    let hour = match model {
        "gfs" => (now.hour() / 6) * 6,      // GFS runs every 6h
        "nam" => (now.hour() / 6) * 6,      // NAM every 6h
        _ => now.hour(),                      // HRRR/RAP hourly
    };
    (date, hour)
}

fn start_download(state: &mut AppState) {
    let model = MODELS[state.dl_model].to_string();
    let run_str = state.dl_run.clone();
    let fhours_str = state.dl_fhours.clone();
    let vars_str = state.dl_vars.clone();
    let output = state.dl_output.clone();

    let (tx, rx) = mpsc::channel::<DownloadEvent>();
    state.dl_rx = Some(rx);
    state.dl_active = true;
    state.dl_progress = 0.0;
    state.dl_status.clear();

    std::thread::spawn(move || {
        // Resolve run time
        let (date, hour) = if run_str.trim().eq_ignore_ascii_case("latest") {
            let (d, h) = resolve_latest_run(&model);
            let _ = tx.send(DownloadEvent::Progress(
                format!("Resolved latest {} run: {} {:02}z", model.to_uppercase(), d, h),
                0.0,
            ));
            (d, h)
        } else {
            // Parse "YYYYMMDD/HHz" or "YYYYMMDD/HH"
            let cleaned = run_str.replace('/', "").replace('z', "").replace('Z', "");
            if cleaned.len() >= 10 {
                let d = cleaned[..8].to_string();
                let h: u32 = cleaned[8..10].parse().unwrap_or(0);
                (d, h)
            } else if cleaned.len() >= 8 {
                (cleaned[..8].to_string(), 0)
            } else {
                let _ = tx.send(DownloadEvent::Error(format!("Invalid run time: '{}'", run_str)));
                return;
            }
        };

        // Parse forecast hours
        let hours: Vec<u32> = if fhours_str.contains('-') {
            let parts: Vec<&str> = fhours_str.split('-').collect();
            if parts.len() == 2 {
                let start: u32 = parts[0].trim().parse().unwrap_or(0);
                let end: u32 = parts[1].trim().parse().unwrap_or(0);
                (start..=end).collect()
            } else {
                vec![0]
            }
        } else {
            fhours_str.split(',').filter_map(|s| s.trim().parse().ok()).collect()
        };

        if hours.is_empty() {
            let _ = tx.send(DownloadEvent::Error("No forecast hours specified".into()));
            return;
        }

        let _ = tx.send(DownloadEvent::Progress(
            format!("{} {} {:02}z  |  F{:?}  |  {} vars",
                model.to_uppercase(), date, hour, hours,
                if vars_str.is_empty() { "all".to_string() } else { vars_str.clone() }),
            0.0,
        ));

        // Create download client
        let client = match rustmet_core::download::DownloadClient::new() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error(format!("HTTP client init failed: {e}")));
                return;
            }
        };

        let out_dir = std::path::PathBuf::from(&output);
        let _ = std::fs::create_dir_all(&out_dir);

        // Parse variable patterns
        let var_patterns: Vec<&str> = if vars_str.is_empty() {
            Vec::new()
        } else {
            vars_str.split(',').map(|s| s.trim()).collect()
        };
        let vars_opt: Option<Vec<&str>> = if var_patterns.is_empty() {
            None
        } else {
            Some(var_patterns)
        };

        let total = hours.len();
        let mut first_file: Option<std::path::PathBuf> = None;

        for (i, &fhr) in hours.iter().enumerate() {
            let pct = i as f32 / total as f32;
            let _ = tx.send(DownloadEvent::Progress(
                format!("Fetching F{:03} from multiple sources...", fhr),
                pct,
            ));

            let product = if model == "hrrr" { "wrfprs" } else { "prs" };

            match rustmet_core::download::fetch_with_fallback(
                &client,
                &model,
                &date,
                hour,
                product,
                fhr,
                vars_opt.as_deref(),
                None,
            ) {
                Ok(result) => {
                    let out_path = out_dir.join(format!(
                        "{}_{}_{:02}z_f{:03}.grib2",
                        model, date, hour, fhr
                    ));
                    if let Err(e) = std::fs::write(&out_path, &result.data) {
                        let _ = tx.send(DownloadEvent::Error(format!("Write failed: {e}")));
                        continue;
                    }
                    let mb = result.data.len() as f64 / 1e6;
                    let _ = tx.send(DownloadEvent::Progress(
                        format!("F{:03}  {:.1} MB  via {}  ->  {}",
                            fhr, mb, result.source_name,
                            out_path.file_name().unwrap_or_default().to_string_lossy()),
                        (i + 1) as f32 / total as f32,
                    ));
                    if first_file.is_none() {
                        first_file = Some(out_path);
                    }
                }
                Err(e) => {
                    let _ = tx.send(DownloadEvent::Error(format!("F{:03} failed: {e}", fhr)));
                }
            }
        }

        let _ = tx.send(DownloadEvent::Progress(
            format!("All {} downloads complete", total),
            1.0,
        ));

        // Auto-open the first downloaded file
        if let Some(path) = first_file {
            let _ = tx.send(DownloadEvent::Complete(path));
        }
    });
}

use chrono::Timelike;
