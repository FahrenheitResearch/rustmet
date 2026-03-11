use crate::state::AppState;
use crate::theme;
use rustmet_core::render::{SkewTConfig, SkewTData};

pub fn sounding_view(ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context) {
    if state.grib.is_none() {
        centered_message(ui, "Open a GRIB2 file with multiple pressure levels to view soundings");
        return;
    }

    let grib = state.grib.as_ref().unwrap();

    // Top controls
    ui.horizontal(|ui| {
        ui.label("Grid point:");
        ui.label("i:");
        let mut gi = state.sounding_grid_i as f32;
        if ui.add(egui::DragValue::new(&mut gi).speed(1.0).range(0.0..=(state.field_nx.saturating_sub(1) as f32))).changed() {
            state.sounding_grid_i = gi as usize;
            state.sounding_texture = None;
        }
        ui.label("j:");
        let mut gj = state.sounding_grid_j as f32;
        if ui.add(egui::DragValue::new(&mut gj).speed(1.0).range(0.0..=(state.field_ny.saturating_sub(1) as f32))).changed() {
            state.sounding_grid_j = gj as usize;
            state.sounding_texture = None;
        }

        if ui.button("Render Skew-T").clicked() {
            state.sounding_texture = None; // force re-render
        }
    });

    ui.separator();

    // Extract sounding data from multi-level GRIB2 messages
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
        } else {
            centered_message(ui, "Could not extract sounding data.\nFile needs temperature and dewpoint at multiple pressure levels.");
            return;
        }
    }

    // Display the skew-T texture
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

/// Try to extract a vertical sounding from multi-level GRIB2 data.
fn extract_sounding(
    grib: &rustmet_core::grib2::Grib2File,
    gi: usize,
    gj: usize,
) -> Option<SkewTData> {
    use rustmet_core::grib2;

    // Find temperature messages at pressure levels (level_type == 100 = isobaric)
    let mut pressure_levels: Vec<f64> = Vec::new();
    let mut temperatures: Vec<f64> = Vec::new();
    let mut dewpoints: Vec<f64> = Vec::new();

    // Collect temperature data (category=0, number=0 = temperature)
    let mut temp_map: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();
    let mut dew_map: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();

    for msg in &grib.messages {
        if msg.product.level_type != 100 {
            continue; // not isobaric
        }
        let nx = msg.grid.nx as usize;
        let p_pa = msg.product.level_value;
        let p_hpa = if p_pa > 2000.0 { p_pa / 100.0 } else { p_pa }; // handle Pa vs hPa
        let p_key = (p_hpa * 100.0) as i64; // sort key

        let cat = msg.product.parameter_category;
        let num = msg.product.parameter_number;

        // Temperature (cat=0, num=0)
        if cat == 0 && num == 0 {
            if let Ok(vals) = grib2::unpack_message(msg) {
                let idx = gj * nx + gi;
                if idx < vals.len() {
                    let t_k = vals[idx];
                    temp_map.insert(p_key, t_k - 273.15); // K -> C
                }
            }
        }

        // Dewpoint (cat=0, num=6) or RH (cat=0, num=1)
        if cat == 0 && num == 6 {
            if let Ok(vals) = grib2::unpack_message(msg) {
                let idx = gj * nx + gi;
                if idx < vals.len() {
                    let td_k = vals[idx];
                    dew_map.insert(p_key, td_k - 273.15);
                }
            }
        }
    }

    if temp_map.len() < 3 {
        return None;
    }

    // Build sorted (descending pressure = surface first) arrays
    let keys: Vec<i64> = temp_map.keys().rev().cloned().collect();
    for &k in &keys {
        let p = k as f64 / 100.0;
        let t = *temp_map.get(&k)?;
        pressure_levels.push(p);
        temperatures.push(t);
        dewpoints.push(dew_map.get(&k).copied().unwrap_or(t - 15.0)); // fallback: Td = T - 15
    }

    Some(SkewTData {
        pressure: pressure_levels,
        temperature: temperatures,
        dewpoint: dewpoints,
        wind_speed: None,
        wind_dir: None,
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
