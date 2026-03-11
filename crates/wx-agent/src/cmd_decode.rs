use serde::Serialize;
use rustmet_core::grib2::{Grib2File, parameter_name, parameter_units, level_name, unpack_message_normalized};
use crate::output::{print_json, print_error};

#[derive(Serialize)]
struct MessageInfo {
    index: usize,
    name: &'static str,
    units: &'static str,
    level: String,
    nx: u32,
    ny: u32,
}

#[derive(Serialize)]
struct ListResponse {
    file: String,
    messages: Vec<MessageInfo>,
}

#[derive(Serialize)]
struct PointResponse {
    index: usize,
    name: &'static str,
    grid_i: usize,
    grid_j: usize,
    value: f64,
}

pub fn run(file: &str, list: bool, message: Option<usize>, point: Option<&str>, pretty: bool) {
    // Read the GRIB2 file
    let data = match std::fs::read(file) {
        Ok(d) => d,
        Err(e) => print_error(&format!("failed to read '{}': {}", file, e)),
    };

    let grib = match Grib2File::from_bytes(&data) {
        Ok(g) => g,
        Err(e) => print_error(&format!("failed to parse GRIB2 '{}': {}", file, e)),
    };

    if list {
        let messages: Vec<MessageInfo> = grib.messages.iter().enumerate().map(|(i, msg)| {
            let name = parameter_name(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);
            let units = parameter_units(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);
            let level_str = level_name(msg.product.level_type);
            let level = if msg.product.level_value != 0.0 {
                format!("{} {}", level_str, msg.product.level_value)
            } else {
                level_str.to_string()
            };
            MessageInfo {
                index: i,
                name,
                units,
                level,
                nx: msg.grid.nx,
                ny: msg.grid.ny,
            }
        }).collect();

        let display_file = std::path::Path::new(file)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string());

        let resp = ListResponse {
            file: display_file,
            messages,
        };
        print_json(&resp, pretty);
        return;
    }

    if let Some(msg_idx) = message {
        if msg_idx >= grib.messages.len() {
            print_error(&format!(
                "message index {} out of range (file has {} messages)",
                msg_idx, grib.messages.len()
            ));
        }

        let msg = &grib.messages[msg_idx];
        let name = parameter_name(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);

        if let Some(pt) = point {
            let parts: Vec<&str> = pt.split(',').collect();
            if parts.len() != 2 {
                print_error("--point must be in format I,J (e.g. --point 100,200)");
            }
            let gi: usize = parts[0].parse().unwrap_or_else(|_| {
                print_error("--point I value must be an integer");
            });
            let gj: usize = parts[1].parse().unwrap_or_else(|_| {
                print_error("--point J value must be an integer");
            });

            let values = match unpack_message_normalized(msg) {
                Ok(v) => v,
                Err(e) => print_error(&format!("failed to unpack message {}: {}", msg_idx, e)),
            };

            let nx = msg.grid.nx as usize;
            let ny = msg.grid.ny as usize;
            if gi >= nx || gj >= ny {
                print_error(&format!(
                    "point ({},{}) out of grid bounds ({}x{})",
                    gi, gj, nx, ny
                ));
            }

            let idx = gj * nx + gi;
            if idx >= values.len() {
                print_error(&format!(
                    "computed index {} out of range for {} values",
                    idx, values.len()
                ));
            }

            let resp = PointResponse {
                index: msg_idx,
                name,
                grid_i: gi,
                grid_j: gj,
                value: values[idx],
            };
            print_json(&resp, pretty);
        } else {
            // No --point given, just show message metadata
            let units = parameter_units(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);
            let level_str = level_name(msg.product.level_type);
            let level = if msg.product.level_value != 0.0 {
                format!("{} {}", level_str, msg.product.level_value)
            } else {
                level_str.to_string()
            };
            let info = MessageInfo {
                index: msg_idx,
                name,
                units,
                level,
                nx: msg.grid.nx,
                ny: msg.grid.ny,
            };
            print_json(&info, pretty);
        }
        return;
    }

    print_error("must specify --list or --message N");
}
