use crate::state::{AppState, RadarEvent};
use crate::theme;
use std::io::Read;
use std::sync::mpsc;

const NEXRAD_BASE_URL: &str = "https://unidata-nexrad-level2.s3.amazonaws.com";

const NEXRAD_SITES: &[(&str, &str, f64, f64)] = &[
    ("KTLX", "Oklahoma City, OK", 35.333, -97.278),
    ("KFWS", "Dallas/Fort Worth, TX", 32.573, -97.303),
    ("KLIX", "New Orleans, LA", 30.337, -89.826),
    ("KLOT", "Chicago, IL", 41.604, -88.085),
    ("KBMX", "Birmingham, AL", 33.172, -86.770),
    ("KMOB", "Mobile, AL", 30.679, -88.240),
    ("KJAX", "Jacksonville, FL", 30.485, -81.702),
    ("KTBW", "Tampa Bay, FL", 27.706, -82.402),
    ("KAMX", "Miami, FL", 25.611, -80.413),
    ("KGRK", "Fort Hood, TX", 30.722, -97.383),
    ("KLZK", "Little Rock, AR", 34.836, -92.262),
    ("KPAH", "Paducah, KY", 37.068, -88.772),
    ("KDVN", "Davenport, IA", 41.612, -90.581),
    ("KMPX", "Minneapolis, MN", 44.849, -93.566),
    ("KDTX", "Detroit, MI", 42.700, -83.472),
    ("KBOX", "Boston, MA", 41.956, -71.137),
    ("KOKX", "New York City, NY", 40.866, -72.864),
    ("KLWX", "Sterling, VA (DC)", 38.975, -77.478),
    ("KFCX", "Roanoke, VA", 37.024, -80.274),
    ("KMHX", "Morehead City, NC", 34.776, -76.876),
    ("KGSP", "Greenville, SC", 34.883, -82.220),
    ("KFFC", "Atlanta, GA", 33.364, -84.566),
    ("KEAX", "Kansas City, MO", 38.810, -94.264),
    ("KLSX", "St. Louis, MO", 38.699, -90.683),
    ("KSGF", "Springfield, MO", 37.235, -93.400),
    ("KDDC", "Dodge City, KS", 37.761, -99.969),
    ("KICT", "Wichita, KS", 37.655, -97.443),
    ("KIND", "Indianapolis, IN", 39.708, -86.280),
    ("KILN", "Wilmington, OH", 39.420, -83.822),
    ("KCLE", "Cleveland, OH", 41.413, -81.860),
    ("KPBZ", "Pittsburgh, PA", 40.532, -80.218),
    ("KDIX", "Philadelphia, PA", 39.947, -74.411),
    ("KENX", "Albany, NY", 42.587, -74.064),
    ("KBUF", "Buffalo, NY", 42.949, -78.737),
    ("KGYX", "Portland, ME", 43.891, -70.257),
    ("KDNR", "Denver, CO", 39.787, -104.546),
    ("KPUX", "Pueblo, CO", 38.460, -104.181),
    ("KSLC", "Salt Lake City, UT", 40.969, -111.930),
    ("KBOI", "Boise, ID", 43.491, -116.236),
    ("KMSX", "Missoula, MT", 47.041, -113.986),
    ("KATX", "Seattle, WA", 48.195, -122.496),
    ("KRTX", "Portland, OR", 45.715, -122.966),
    ("KMUX", "San Francisco, CA", 37.155, -121.898),
    ("KVBX", "Vandenberg, CA", 34.838, -120.397),
    ("KSOX", "Santa Ana Mtns, CA", 33.818, -117.636),
    ("KFGZ", "Flagstaff, AZ", 35.233, -111.198),
    ("KEMX", "Tucson, AZ", 31.893, -110.630),
    ("KABX", "Albuquerque, NM", 35.150, -106.824),
    ("KEWX", "Austin/San Antonio, TX", 29.704, -98.029),
    ("KHGX", "Houston, TX", 29.472, -95.079),
    ("KSHV", "Shreveport, LA", 32.451, -93.841),
    ("KJAN", "Jackson, MS", 32.318, -90.080),
    ("KMRX", "Knoxville, TN", 36.169, -83.402),
];

pub fn radar_panel(ui: &mut egui::Ui, state: &mut AppState) {
    // Poll radar download events
    poll_radar_events(state, ui.ctx());

    ui.heading("NEXRAD Radar");
    ui.add_space(4.0);

    // ── Station selector + download ────────────────
    ui.group(|ui| {
        ui.label(egui::RichText::new("STATION").small().strong().color(theme::ACCENT));
        ui.add_space(4.0);

        let current = NEXRAD_SITES[state.radar_station_idx];
        egui::ComboBox::from_id_salt("radar_station")
            .selected_text(format!("{} — {}", current.0, current.1))
            .width(300.0)
            .show_ui(ui, |ui| {
                for (i, &(id, loc, _, _)) in NEXRAD_SITES.iter().enumerate() {
                    ui.selectable_value(
                        &mut state.radar_station_idx, i,
                        format!("{} — {}", id, loc),
                    );
                }
            });

        ui.add_space(8.0);

        let is_downloading = *state.radar_downloading.lock().unwrap();
        let btn_text = if is_downloading { "Downloading..." } else { "Download Latest" };
        let btn = egui::Button::new(
            egui::RichText::new(btn_text).color(egui::Color32::WHITE),
        ).fill(if is_downloading { theme::TEXT_DIM } else { theme::ACCENT });

        if ui.add_enabled(!is_downloading, btn).clicked() {
            start_radar_download(state);
        }

        if !state.radar_status.is_empty() {
            ui.add_space(4.0);
            let color = if state.radar_status.starts_with("ERROR") { theme::ERROR } else { theme::TEXT_DIM };
            ui.label(egui::RichText::new(&state.radar_status).small().color(color));
        }
    });

    // ── Product + sweep selectors ──────────────────
    // Pre-extract data from radar_file to avoid borrow conflict
    let radar_info = state.radar_file.as_ref().map(|radar| {
        let products = radar.available_products();
        let sweep_info: Vec<(f32, usize)> = radar.sweeps.iter()
            .map(|s| (s.elevation_angle, s.radials.len()))
            .collect();
        let info_str = format!("{} | {} | {} sweeps",
            radar.station_id, radar.timestamp_string(), radar.sweeps.len());
        (products, sweep_info, info_str)
    });

    let mut needs_rerender = false;
    if let Some((products, sweep_info, info_str)) = radar_info {
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("CONTROLS").small().strong().color(theme::ACCENT));
            ui.add_space(4.0);

            // Product selector
            ui.horizontal(|ui| {
                ui.label("Product:");
                let current_product = products.get(state.radar_product_idx)
                    .map(|p| p.display_name())
                    .unwrap_or("REF");
                egui::ComboBox::from_id_salt("radar_product")
                    .selected_text(current_product)
                    .show_ui(ui, |ui| {
                        for (i, p) in products.iter().enumerate() {
                            if ui.selectable_value(&mut state.radar_product_idx, i, p.display_name()).changed() {
                                needs_rerender = true;
                            }
                        }
                    });
            });

            // Sweep/tilt selector
            ui.horizontal(|ui| {
                ui.label("Tilt:");
                let current_elev = sweep_info.get(state.radar_sweep_idx)
                    .map(|(e, _)| format!("{:.1}°", e))
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("radar_sweep")
                    .selected_text(format!("#{} ({})", state.radar_sweep_idx + 1, current_elev))
                    .show_ui(ui, |ui| {
                        for (i, (elev, n_radials)) in sweep_info.iter().enumerate() {
                            let label = format!("#{} — {:.1}° ({} radials)", i + 1, elev, n_radials);
                            if ui.selectable_value(&mut state.radar_sweep_idx, i, label).changed() {
                                needs_rerender = true;
                            }
                        }
                    });
            });

            ui.add_space(4.0);
            ui.label(egui::RichText::new(info_str).small().color(theme::TEXT_DIM));
        });

        if needs_rerender {
            render_radar(state, ui.ctx(), &products);
        }
    }

    ui.add_space(8.0);

    // ── PPI display ────────────────────────────────
    if let Some(ref texture) = state.radar_texture {
        let available = ui.available_size();
        let tex_size = texture.size_vec2();
        let fit = (available.x / tex_size.x).min(available.y / tex_size.y).min(1.0);
        let display_size = tex_size * fit;

        let (response, painter) = ui.allocate_painter(
            egui::vec2(available.x, display_size.y),
            egui::Sense::hover(),
        );
        let rect = response.rect;

        // Center the image
        let img_min = egui::pos2(
            rect.center().x - display_size.x / 2.0,
            rect.min.y,
        );
        let img_rect = egui::Rect::from_min_size(img_min, display_size);

        // Dark background
        painter.rect_filled(rect, 0.0, theme::DEEP_BG);

        // Draw PPI
        painter.image(
            texture.id(), img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // Range ring labels
        if state.radar_range_km > 0.0 {
            let font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
            let center = img_rect.center();
            let max_r = display_size.x / 2.0;
            for frac in [0.25, 0.5, 0.75, 1.0] {
                let r = max_r * frac as f32;
                let km = state.radar_range_km * frac;
                // Draw ring
                let ring_color = egui::Color32::from_white_alpha(30);
                let n_segs = 64;
                for s in 0..n_segs {
                    let a0 = s as f32 * std::f32::consts::TAU / n_segs as f32;
                    let a1 = (s + 1) as f32 * std::f32::consts::TAU / n_segs as f32;
                    painter.line_segment(
                        [
                            center + egui::vec2(r * a0.cos(), r * a0.sin()),
                            center + egui::vec2(r * a1.cos(), r * a1.sin()),
                        ],
                        egui::Stroke::new(0.5, ring_color),
                    );
                }
                // Label
                let label_pos = center + egui::vec2(4.0, -r);
                painter.text(label_pos, egui::Align2::LEFT_BOTTOM,
                    format!("{:.0} km", km), font.clone(),
                    egui::Color32::from_white_alpha(80));
            }
        }

        // Border
        painter.rect_stroke(img_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)));

        // Hover readout
        if let Some(pos) = response.hover_pos() {
            if img_rect.contains(pos) {
                let dx = pos.x - img_rect.center().x;
                let dy = img_rect.center().y - pos.y;
                let range_px = (dx * dx + dy * dy).sqrt();
                let max_r = display_size.x / 2.0;
                let range_km = state.radar_range_km * (range_px / max_r) as f64;
                let mut az = (dx.atan2(dy)).to_degrees();
                if az < 0.0 { az += 360.0; }

                // Crosshair
                let cc = egui::Color32::from_white_alpha(80);
                painter.line_segment(
                    [egui::pos2(pos.x, img_rect.min.y), egui::pos2(pos.x, img_rect.max.y)],
                    egui::Stroke::new(0.5, cc),
                );
                painter.line_segment(
                    [egui::pos2(img_rect.min.x, pos.y), egui::pos2(img_rect.max.x, pos.y)],
                    egui::Stroke::new(0.5, cc),
                );

                let font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
                let tip = format!("{:.0}° {:.1} km", az, range_km);
                let tip_pos = pos + egui::vec2(14.0, -18.0);
                let galley = painter.layout_no_wrap(tip.clone(), font.clone(), egui::Color32::WHITE);
                let bg = egui::Rect::from_min_size(
                    tip_pos - egui::vec2(2.0, galley.size().y + 1.0),
                    galley.size() + egui::vec2(4.0, 2.0),
                );
                painter.rect_filled(bg, 2.0, egui::Color32::from_black_alpha(180));
                painter.text(tip_pos, egui::Align2::LEFT_BOTTOM, &tip, font, egui::Color32::WHITE);
            }
        }
    } else if state.radar_file.is_none() {
        // No radar data yet
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(egui::RichText::new("Select a station and click Download Latest")
                    .color(theme::TEXT_DIM));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Real-time NEXRAD Level-II data from AWS (public, no auth)")
                    .small().color(theme::TEXT_DIM));
            });
        });
    }
}

fn start_radar_download(state: &mut AppState) {
    let station = NEXRAD_SITES[state.radar_station_idx].0.to_string();
    let (tx, rx) = mpsc::channel();
    state.radar_rx = Some(rx);
    *state.radar_downloading.lock().unwrap() = true;
    state.radar_status = format!("Downloading latest {} data...", station);

    let downloading = state.radar_downloading.clone();

    std::thread::spawn(move || {
        let result = download_latest_radar(&station, &tx);
        if let Err(e) = result {
            let _ = tx.send(RadarEvent::Error(e));
        }
        *downloading.lock().unwrap() = false;
    });
}

fn download_latest_radar(station: &str, tx: &mpsc::Sender<RadarEvent>) -> Result<(), String> {
    use chrono::{Datelike, Utc};

    let today = Utc::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);

    for date in [today, yesterday] {
        let prefix = format!(
            "{:04}/{:02}/{:02}/{}/",
            date.year(), date.month(), date.day(), station
        );
        let url = format!("{}?list-type=2&prefix={}", NEXRAD_BASE_URL, prefix);
        let _ = tx.send(RadarEvent::Status(format!("Listing {prefix}...")));

        let resp = ureq::get(&url).call().map_err(|e| format!("List error: {e}"))?;
        let body = resp.into_body().read_to_string().map_err(|e| format!("Read error: {e}"))?;

        let files = parse_s3_xml(&body);
        if files.is_empty() { continue; }

        // Download the latest (last sorted) file
        let latest = &files[files.len() - 1];
        let _ = tx.send(RadarEvent::Status(format!("Downloading {}...", latest.1)));

        let dl_url = format!("{}/{}", NEXRAD_BASE_URL, latest.0);
        let dl_resp = ureq::get(&dl_url).call().map_err(|e| format!("Download error: {e}"))?;

        let mut data = Vec::new();
        dl_resp.into_body().as_reader().read_to_end(&mut data).map_err(|e| format!("Read error: {e}"))?;

        // Decompress gzip if needed
        if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            use flate2::read::GzDecoder;
            use std::io::Read;
            let mut decoder = GzDecoder::new(&data[..]);
            let mut decompressed = Vec::new();
            if decoder.read_to_end(&mut decompressed).is_ok() {
                data = decompressed;
            }
        }

        let _ = tx.send(RadarEvent::Status(format!("Downloaded {} ({:.1} MB)", latest.1, data.len() as f64 / 1_048_576.0)));
        let _ = tx.send(RadarEvent::Data(data));
        return Ok(());
    }

    Err(format!("No NEXRAD files found for {station}"))
}

fn parse_s3_xml(xml: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for contents in xml.split("<Contents>").skip(1) {
        let end = contents.find("</Contents>").unwrap_or(contents.len());
        let block = &contents[..end];

        let key = extract_xml_tag(block, "Key").unwrap_or_default();
        let display = key.rsplit('/').next().unwrap_or(&key).to_string();

        if key.is_empty() || display.ends_with("_MDM") || display.ends_with(".md") {
            continue;
        }
        files.push((key, display));
    }
    files.sort();
    files
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml.find(&close)?;
    Some(xml[start..end].to_string())
}

fn poll_radar_events(state: &mut AppState, ctx: &egui::Context) {
    let events: Vec<RadarEvent> = state.radar_rx.as_ref()
        .map(|rx| {
            let mut evts = Vec::new();
            while let Ok(e) = rx.try_recv() { evts.push(e); }
            evts
        })
        .unwrap_or_default();

    for event in events {
        match event {
            RadarEvent::Status(msg) => {
                state.radar_status = msg;
                ctx.request_repaint();
            }
            RadarEvent::Data(data) => {
                state.radar_status = "Parsing Level-II data...".into();
                match wx_radar::level2::Level2File::parse(&data) {
                    Ok(file) => {
                        let n_sweeps = file.sweeps.len();
                        let products = file.available_products();
                        let product_names: Vec<&str> = products.iter().map(|p| p.short_name()).collect();
                        state.radar_status = format!(
                            "{} | {} sweeps | Products: {}",
                            file.timestamp_string(), n_sweeps, product_names.join(", "),
                        );
                        state.radar_sweep_idx = 0;
                        state.radar_product_idx = 0;
                        state.radar_file = Some(file);

                        // Render the first sweep
                        let products = state.radar_file.as_ref().unwrap().available_products();
                        render_radar(state, ctx, &products);
                    }
                    Err(e) => {
                        state.radar_status = format!("ERROR: Parse failed: {e}");
                    }
                }
                ctx.request_repaint();
            }
            RadarEvent::Error(e) => {
                state.radar_status = format!("ERROR: {e}");
                ctx.request_repaint();
            }
        }
    }
}

fn render_radar(state: &mut AppState, ctx: &egui::Context, products: &[wx_radar::products::RadarProduct]) {
    let Some(ref radar) = state.radar_file else { return };
    let Some(sweep) = radar.sweeps.get(state.radar_sweep_idx) else { return };
    let Some(&product) = products.get(state.radar_product_idx) else { return };

    let image_size = 800u32;
    if let Some(rendered) = wx_radar::render::render_ppi(sweep, product, image_size) {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [rendered.size as usize, rendered.size as usize],
            &rendered.pixels,
        );
        let texture = ctx.load_texture("radar_ppi", image, egui::TextureOptions::LINEAR);
        state.radar_texture = Some(texture);
        state.radar_range_km = rendered.range_km;
    }
}
