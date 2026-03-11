use crate::state::AppState;
use crate::theme;
use rustmet_core::render::{SkewTConfig, SkewTData};
use std::collections::BTreeMap;

pub fn sounding_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    if state.grib.is_none() {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(egui::RichText::new("Skew-T Log-P Diagram").heading().color(theme::ACCENT));
                ui.add_space(16.0);
                ui.label(egui::RichText::new("Download or open a GRIB2 file with pressure-level data").color(theme::TEXT_DIM));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Use the \"HRRR Full F000\" preset for a complete sounding").color(theme::TEXT_DIM));
                ui.add_space(16.0);
                let btn = egui::Button::new(
                    egui::RichText::new("   Go to Download   ").color(egui::Color32::WHITE),
                ).fill(theme::ACCENT);
                if ui.add(btn).clicked() {
                    state.active_view = crate::state::View::Download;
                }
            });
        });
        return;
    }

    let grib = state.grib.as_ref().unwrap();

    // Count available pressure-level temperature messages
    let n_plevels = grib.messages.iter().filter(|m| {
        is_pressure_level(m.product.level_type) && m.product.parameter_category == 0 && m.product.parameter_number == 0
    }).count();

    // Top controls
    ui.horizontal(|ui| {
        ui.label("Grid point:");
        ui.label("i:");
        let max_i = state.field_nx.max(1).saturating_sub(1) as f32;
        let mut gi = state.sounding_grid_i as f32;
        if ui.add(egui::DragValue::new(&mut gi).speed(1.0).range(0.0..=max_i)).changed() {
            state.sounding_grid_i = gi as usize;
            state.sounding_texture = None;
        }
        ui.label("j:");
        let max_j = state.field_ny.max(1).saturating_sub(1) as f32;
        let mut gj = state.sounding_grid_j as f32;
        if ui.add(egui::DragValue::new(&mut gj).speed(1.0).range(0.0..=max_j)).changed() {
            state.sounding_grid_j = gj as usize;
            state.sounding_texture = None;
        }

        ui.separator();
        ui.label(egui::RichText::new(format!("{} pressure levels found", n_plevels)).small().color(theme::TEXT_DIM));

        if ui.button("Render Skew-T").clicked() {
            state.sounding_texture = None;
        }
    });

    ui.separator();

    if n_plevels < 3 {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(egui::RichText::new("Not enough pressure-level data for a sounding").color(theme::WARNING));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!(
                    "Found {} pressure-level temperature messages (need at least 3).\n\
                     This file may only contain surface data.\n\n\
                     Download \"HRRR Full F000\" to get all levels.",
                    n_plevels
                )).color(theme::TEXT_DIM));
                ui.add_space(16.0);
                let btn = egui::Button::new("Download full data").fill(theme::ACCENT);
                if ui.add(btn).clicked() {
                    state.active_view = crate::state::View::Download;
                }
            });
        });
        return;
    }

    // Extract and render
    if state.sounding_texture.is_none() {
        if let Some(sounding_data) = extract_sounding(grib, state.sounding_grid_i, state.sounding_grid_j) {
            let config = SkewTConfig {
                width: 900,
                height: 900,
                ..SkewTConfig::default()
            };
            let pixels = rustmet_core::render::render_skewt(&sounding_data, &config);
            let image = egui::ColorImage::from_rgba_unmultiplied([config.width as usize, config.height as usize], &pixels);
            let texture = ctx.load_texture("skewt", image, egui::TextureOptions::LINEAR);
            state.sounding_texture = Some(texture);
        }
    }

    if let Some(ref texture) = state.sounding_texture {
        let available = ui.available_size();
        let tex_size = texture.size_vec2();
        let scale = (available.x / tex_size.x).min(available.y / tex_size.y);
        let display_size = tex_size * scale;
        ui.centered_and_justified(|ui| {
            ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
        });
    }
}

/// Is this a pressure-related level type?
fn is_pressure_level(level_type: u8) -> bool {
    match level_type {
        100 => true,  // isobaric surface (Pa)
        105 => true,  // hybrid level (HRRR native)
        _ => false,
    }
}

/// Convert level_value to pressure in hPa.
fn level_to_pressure_hpa(level_type: u8, level_value: f64) -> f64 {
    match level_type {
        100 => {
            // Isobaric: value is in Pa, convert to hPa
            if level_value > 2000.0 { level_value / 100.0 } else { level_value }
        }
        105 => {
            // Hybrid level: level_value is the level number (1-50 typically)
            // Map to approximate pressure using standard HRRR hybrid levels
            // Level 1 ≈ surface (~1013 hPa), Level 50 ≈ top (~50 hPa)
            let lvl = level_value;
            if lvl <= 0.0 { return 1013.0; }
            if lvl >= 50.0 { return 50.0; }
            // Rough approximation: exponential spacing
            1013.0 * (-lvl * 0.06).exp()
        }
        _ => level_value,
    }
}

fn extract_sounding(
    grib: &rustmet_core::grib2::Grib2File,
    gi: usize,
    gj: usize,
) -> Option<SkewTData> {
    use rustmet_core::grib2;

    let mut temp_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut dew_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut rh_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut wspd_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut wdir_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut u_map: BTreeMap<i64, f64> = BTreeMap::new();
    let mut v_map: BTreeMap<i64, f64> = BTreeMap::new();

    for msg in &grib.messages {
        let lt = msg.product.level_type;
        if !is_pressure_level(lt) {
            continue;
        }
        let nx = msg.grid.nx as usize;
        let p_hpa = level_to_pressure_hpa(lt, msg.product.level_value);
        let p_key = (p_hpa * 100.0) as i64;

        let cat = msg.product.parameter_category;
        let num = msg.product.parameter_number;

        let get_val = |msg: &grib2::Grib2Message| -> Option<f64> {
            let vals = grib2::unpack_message_normalized(msg).ok()?;
            let idx = gj * nx + gi;
            vals.get(idx).copied()
        };

        match (cat, num) {
            (0, 0) => { // Temperature (K)
                if let Some(v) = get_val(msg) {
                    temp_map.insert(p_key, v - 273.15);
                }
            }
            (0, 6) => { // Dewpoint temperature (K)
                if let Some(v) = get_val(msg) {
                    dew_map.insert(p_key, v - 273.15);
                }
            }
            (0, 1) => { // Specific humidity or RH
                if let Some(v) = get_val(msg) {
                    rh_map.insert(p_key, v);
                }
            }
            (2, 2) => { // U-wind (m/s)
                if let Some(v) = get_val(msg) {
                    u_map.insert(p_key, v);
                }
            }
            (2, 3) => { // V-wind (m/s)
                if let Some(v) = get_val(msg) {
                    v_map.insert(p_key, v);
                }
            }
            _ => {}
        }
    }

    if temp_map.len() < 3 {
        return None;
    }

    let mut pressure = Vec::new();
    let mut temperature = Vec::new();
    let mut dewpoint = Vec::new();
    let mut wind_speed = Vec::new();
    let mut wind_dir = Vec::new();

    // Sort by descending pressure (surface first)
    let keys: Vec<i64> = temp_map.keys().rev().cloned().collect();
    for &k in &keys {
        let p = k as f64 / 100.0;
        let t = *temp_map.get(&k)?;

        // Try dewpoint, else compute from RH
        let td = if let Some(&td) = dew_map.get(&k) {
            td
        } else if let Some(&rh) = rh_map.get(&k) {
            // Approximate Td from T and RH using Magnus formula
            let rh_frac = if rh > 2.0 { rh / 100.0 } else { rh }; // handle 0-1 vs 0-100
            let a = 17.625;
            let b = 243.04;
            let gamma = (rh_frac.max(0.01)).ln() + (a * t) / (b + t);
            (b * gamma) / (a - gamma)
        } else {
            t - 15.0 // last resort fallback
        };

        pressure.push(p);
        temperature.push(t);
        dewpoint.push(td);

        // Wind
        if let (Some(&u), Some(&v)) = (u_map.get(&k), v_map.get(&k)) {
            let spd = (u * u + v * v).sqrt() * 1.94384; // m/s -> kt
            let dir = (270.0 - v.atan2(u).to_degrees()).rem_euclid(360.0);
            wind_speed.push(spd);
            wind_dir.push(dir);
        }
    }

    Some(SkewTData {
        pressure,
        temperature,
        dewpoint,
        wind_speed: if wind_speed.len() >= 3 { Some(wind_speed) } else { None },
        wind_dir: if wind_dir.len() >= 3 { Some(wind_dir) } else { None },
    })
}
