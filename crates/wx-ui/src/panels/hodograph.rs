use crate::state::AppState;
use crate::theme;
use rustmet_core::render::{HodographConfig, HodographData};
use std::collections::BTreeMap;

pub fn hodograph_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    if state.grib.is_none() {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(egui::RichText::new("Wind Hodograph").heading().color(theme::ACCENT));
                ui.add_space(16.0);
                ui.label(egui::RichText::new("Download or open a GRIB2 file with wind data at pressure levels").color(theme::TEXT_DIM));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Use \"HRRR Full F000\" for complete wind profiles").color(theme::TEXT_DIM));
                ui.add_space(16.0);
                let btn = egui::Button::new("   Go to Download   ").fill(theme::ACCENT);
                if ui.add(btn).clicked() {
                    state.active_view = crate::state::View::Download;
                }
            });
        });
        return;
    }

    let grib = state.grib.as_ref().unwrap();

    // Count wind messages
    let n_wind = grib.messages.iter().filter(|m| {
        is_pressure_level(m.product.level_type) && m.product.parameter_category == 2 && m.product.parameter_number == 2
    }).count();

    // Controls
    ui.horizontal(|ui| {
        ui.label("Grid point:");
        ui.label("i:");
        let max_i = state.field_nx.max(1).saturating_sub(1) as f32;
        let mut gi = state.sounding_grid_i as f32;
        if ui.add(egui::DragValue::new(&mut gi).speed(1.0).range(0.0..=max_i)).changed() {
            state.sounding_grid_i = gi as usize;
            state.hodograph_texture = None;
        }
        ui.label("j:");
        let max_j = state.field_ny.max(1).saturating_sub(1) as f32;
        let mut gj = state.sounding_grid_j as f32;
        if ui.add(egui::DragValue::new(&mut gj).speed(1.0).range(0.0..=max_j)).changed() {
            state.sounding_grid_j = gj as usize;
            state.hodograph_texture = None;
        }

        ui.separator();
        ui.label(egui::RichText::new(format!("{} wind levels found", n_wind)).small().color(theme::TEXT_DIM));

        if ui.button("Render Hodograph").clicked() {
            state.hodograph_texture = None;
        }
    });

    ui.separator();

    if n_wind < 3 {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(egui::RichText::new("Not enough wind data for a hodograph").color(theme::WARNING));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!(
                    "Found {} wind levels (need at least 3).\n\
                     Download \"HRRR Full F000\" for complete profiles.",
                    n_wind
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

    // Render
    if state.hodograph_texture.is_none() {
        if let Some(hodo_data) = extract_hodograph(grib, state.sounding_grid_i, state.sounding_grid_j) {
            let config = HodographConfig {
                size: 600,
                ..HodographConfig::default()
            };
            let pixels = rustmet_core::render::render_hodograph(&hodo_data, &config);
            let image = egui::ColorImage::from_rgba_unmultiplied([config.size as usize, config.size as usize], &pixels);
            let texture = ctx.load_texture("hodograph", image, egui::TextureOptions::LINEAR);
            state.hodograph_texture = Some(texture);
        }
    }

    if let Some(ref texture) = state.hodograph_texture {
        let available = ui.available_size();
        let tex_size = texture.size_vec2();
        let scale = (available.x / tex_size.x).min(available.y / tex_size.y);
        let display_size = tex_size * scale;
        ui.centered_and_justified(|ui| {
            ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
        });
    }
}

fn is_pressure_level(level_type: u8) -> bool {
    level_type == 100 || level_type == 105
}

fn level_to_pressure_hpa(level_type: u8, level_value: f64) -> f64 {
    match level_type {
        100 => if level_value > 2000.0 { level_value / 100.0 } else { level_value },
        105 => {
            let lvl = level_value;
            if lvl <= 0.0 { return 1013.0; }
            if lvl >= 50.0 { return 50.0; }
            1013.0 * (-lvl * 0.06).exp()
        }
        _ => level_value,
    }
}

fn extract_hodograph(
    grib: &rustmet_core::grib2::Grib2File,
    gi: usize,
    gj: usize,
) -> Option<HodographData> {
    use rustmet_core::grib2;

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
            (2, 2) => { if let Some(v) = get_val(msg) { u_map.insert(p_key, v * 1.94384); } }
            (2, 3) => { if let Some(v) = get_val(msg) { v_map.insert(p_key, v * 1.94384); } }
            _ => {}
        }
    }

    if u_map.len() < 3 {
        return None;
    }

    let mut pressure = Vec::new();
    let mut u_wind = Vec::new();
    let mut v_wind = Vec::new();

    let keys: Vec<i64> = u_map.keys().rev().cloned().collect();
    for &k in &keys {
        if let (Some(&u), Some(&v)) = (u_map.get(&k), v_map.get(&k)) {
            pressure.push(k as f64 / 100.0);
            u_wind.push(u);
            v_wind.push(v);
        }
    }

    if pressure.len() < 3 { return None; }

    Some(HodographData { pressure, u_wind, v_wind })
}
