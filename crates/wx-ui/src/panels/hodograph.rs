use crate::state::AppState;
use crate::theme;
use rustmet_core::render::{HodographConfig, HodographData};

pub fn hodograph_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    if state.grib.is_none() {
        centered_message(ui, "Open a GRIB2 file with wind data at multiple pressure levels");
        return;
    }

    let grib = state.grib.as_ref().unwrap();

    // Controls
    ui.horizontal(|ui| {
        ui.label("Grid point:");
        ui.label("i:");
        let mut gi = state.sounding_grid_i as f32;
        if ui.add(egui::DragValue::new(&mut gi).speed(1.0).range(0.0..=(state.field_nx.saturating_sub(1) as f32))).changed() {
            state.sounding_grid_i = gi as usize;
            state.hodograph_texture = None;
        }
        ui.label("j:");
        let mut gj = state.sounding_grid_j as f32;
        if ui.add(egui::DragValue::new(&mut gj).speed(1.0).range(0.0..=(state.field_ny.saturating_sub(1) as f32))).changed() {
            state.sounding_grid_j = gj as usize;
            state.hodograph_texture = None;
        }

        if ui.button("Render Hodograph").clicked() {
            state.hodograph_texture = None;
        }
    });

    ui.separator();

    // Render hodograph
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
        } else {
            centered_message(ui, "Could not extract wind profile.\nFile needs U and V wind at multiple pressure levels.");
            return;
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

fn extract_hodograph(
    grib: &rustmet_core::grib2::Grib2File,
    gi: usize,
    gj: usize,
) -> Option<HodographData> {
    use rustmet_core::grib2;

    let mut u_map: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();
    let mut v_map: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();

    for msg in &grib.messages {
        if msg.product.level_type != 100 {
            continue;
        }
        let nx = msg.grid.nx as usize;
        let p_pa = msg.product.level_value;
        let p_hpa = if p_pa > 2000.0 { p_pa / 100.0 } else { p_pa };
        let p_key = (p_hpa * 100.0) as i64;
        let cat = msg.product.parameter_category;
        let num = msg.product.parameter_number;

        // U-wind (cat=2, num=2)
        if cat == 2 && num == 2 {
            if let Ok(vals) = grib2::unpack_message(msg) {
                let idx = gj * nx + gi;
                if idx < vals.len() {
                    u_map.insert(p_key, vals[idx] * 1.94384); // m/s -> kt
                }
            }
        }
        // V-wind (cat=2, num=3)
        if cat == 2 && num == 3 {
            if let Ok(vals) = grib2::unpack_message(msg) {
                let idx = gj * nx + gi;
                if idx < vals.len() {
                    v_map.insert(p_key, vals[idx] * 1.94384);
                }
            }
        }
    }

    if u_map.len() < 3 {
        return None;
    }

    let mut pressure = Vec::new();
    let mut u_wind = Vec::new();
    let mut v_wind = Vec::new();

    // Descending pressure (surface first)
    let keys: Vec<i64> = u_map.keys().rev().cloned().collect();
    for &k in &keys {
        if let (Some(&u), Some(&v)) = (u_map.get(&k), v_map.get(&k)) {
            pressure.push(k as f64 / 100.0);
            u_wind.push(u);
            v_wind.push(v);
        }
    }

    if pressure.len() < 3 {
        return None;
    }

    Some(HodographData {
        pressure,
        u_wind,
        v_wind,
    })
}

fn centered_message(ui: &mut egui::Ui, msg: &str) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.label(egui::RichText::new(msg).color(theme::TEXT_DIM));
        });
    });
}
