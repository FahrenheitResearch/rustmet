//! Sounding profile endpoint — fetches Wyoming radiosonde data and renders
//! SkewT-LogP diagrams as SVG.
//!
//! GET /api/sounding/{station}          -> SVG (default)
//! GET /api/sounding/{station}?format=json -> raw JSON data
//! GET /api/sounding/{station}?hour=0   -> 00Z sounding (default: most recent)

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// Re-export the types we need from wx-sounding
use wx_sounding::{Sounding, compute_indices};

// ---------------------------------------------------------------------------
// Sounding cache -- 1 hour TTL
// ---------------------------------------------------------------------------

struct CachedSounding {
    data: Sounding,
    fetched: Instant,
}

pub struct SoundingCache {
    entries: RwLock<HashMap<String, CachedSounding>>,
    ttl: Duration,
}

impl SoundingCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub async fn get(&self, key: &str) -> Option<Sounding> {
        let map = self.entries.read().await;
        if let Some(entry) = map.get(key) {
            if entry.fetched.elapsed() < self.ttl {
                return Some(entry.data.clone());
            }
        }
        None
    }

    pub async fn put(&self, key: String, data: Sounding) {
        let mut map = self.entries.write().await;
        // Evict expired entries periodically
        if map.len() > 100 {
            let ttl = self.ttl;
            map.retain(|_, v| v.fetched.elapsed() < ttl);
        }
        map.insert(key, CachedSounding {
            data,
            fetched: Instant::now(),
        });
    }
}

// ---------------------------------------------------------------------------
// Fetch sounding (blocking Wyoming HTTP in spawn_blocking)
// ---------------------------------------------------------------------------

pub async fn fetch_sounding_cached(
    cache: &SoundingCache,
    station: &str,
    hour: Option<u32>,
) -> Result<Sounding, String> {
    // Build cache key
    let now = chrono::Utc::now();
    let key = format!("{}:{}:{}", station, now.format("%Y%m%d"), hour.unwrap_or(99));

    // Check cache
    if let Some(cached) = cache.get(&key).await {
        return Ok(cached);
    }

    // Fetch from Wyoming (blocking HTTP via ureq)
    let station_owned = station.to_string();
    let sounding = tokio::task::spawn_blocking(move || {
        let mut snd = match hour {
            Some(0) => wx_sounding::fetch_latest_00z(&station_owned),
            Some(12) => wx_sounding::fetch_latest_12z(&station_owned),
            Some(h) => {
                let now = chrono::Utc::now();
                let year = now.format("%Y").to_string().parse::<i32>().unwrap_or(2024);
                let month = now.format("%m").to_string().parse::<u32>().unwrap_or(1);
                let day = now.format("%d").to_string().parse::<u32>().unwrap_or(1);
                wx_sounding::fetch_sounding(&station_owned, year, month, day, h)
            }
            None => {
                // Auto-detect: try most recent synoptic time
                let hour_utc = now.format("%H").to_string().parse::<u32>().unwrap_or(0);
                if hour_utc >= 15 {
                    wx_sounding::fetch_latest_12z(&station_owned)
                } else if hour_utc >= 3 {
                    wx_sounding::fetch_latest_00z(&station_owned)
                } else {
                    wx_sounding::fetch_latest_12z(&station_owned)
                }
            }
        }?;

        // Compute derived indices
        compute_indices(&mut snd);
        Ok::<Sounding, String>(snd)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    // Cache it
    cache.put(key, sounding.clone()).await;
    Ok(sounding)
}

// ---------------------------------------------------------------------------
// SVG SkewT-LogP renderer
// ---------------------------------------------------------------------------

/// Helper: format an SVG attribute with a hex color without triggering Rust
/// raw string delimiter issues. We use format!() with {} placeholders for
/// colors instead of embedding `"#hex"` inside `r#"..."#` strings.
fn svg_line(x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str, width: &str) -> String {
    format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"{}\"/>",
        x1, y1, x2, y2, stroke, width
    )
}

fn svg_line_cap(x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str, width: &str) -> String {
    format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
        x1, y1, x2, y2, stroke, width
    )
}

fn svg_polyline(points: &str, stroke: &str, width: &str, extra: &str) -> String {
    format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" {}/>",
        points, stroke, width, extra
    )
}

fn svg_text(x: f64, y: f64, fill: &str, font: &str, size: &str, anchor: &str, text: &str) -> String {
    format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{}\" font-family=\"{}\" font-size=\"{}\" text-anchor=\"{}\">{}</text>",
        x, y, fill, font, size, anchor, text
    )
}

fn svg_text_extra(x: f64, y: f64, fill: &str, font: &str, size: &str, anchor: &str, extra: &str, text: &str) -> String {
    format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{}\" font-family=\"{}\" font-size=\"{}\" text-anchor=\"{}\" {}>{}</text>",
        x, y, fill, font, size, anchor, extra, text
    )
}

/// Render a complete SkewT-LogP diagram as an SVG string.
///
/// Professional dark-theme SkewT with hodograph inset, wind barbs, and indices panel.
/// Total SVG dimensions: 900x700 (main diagram) + right-side indices panel.
pub fn render_skewt_svg(sounding: &Sounding, _width: u32, _height: u32) -> String {
    // Fixed professional dimensions
    let total_w: f64 = 1100.0;
    let total_h: f64 = 750.0;

    // Main SkewT plot area
    let margin_left = 72.0;
    let margin_right = 60.0; // space for wind barbs before indices panel
    let margin_top = 52.0;
    let margin_bottom = 58.0;
    let skewt_w = 660.0; // main plot width
    let plot_w = skewt_w - margin_left - margin_right;
    let plot_h = total_h - margin_top - margin_bottom;

    // Indices panel on the right
    let panel_x = skewt_w + 10.0;
    let panel_w = total_w - panel_x - 10.0;

    let p_min: f64 = 100.0;
    let p_max: f64 = 1050.0;
    let t_min: f64 = -40.0;
    let t_max: f64 = 50.0;
    let skew_factor: f64 = 0.9;

    let ln_p_max = p_max.ln();
    let ln_p_min = p_min.ln();
    let ln_range = ln_p_max - ln_p_min;
    let t_range = t_max - t_min;

    // Coordinate transforms -- logarithmic pressure, skewed temperature
    let p_to_y = |p: f64| -> f64 {
        let frac = (ln_p_max - p.max(p_min).min(p_max).ln()) / ln_range;
        margin_top + frac * plot_h
    };

    let tp_to_x = |t: f64, p: f64| -> f64 {
        let y = p_to_y(p);
        let frac_from_bottom = 1.0 - (y - margin_top) / plot_h;
        let t_frac = (t - t_min) / t_range;
        margin_left + (t_frac + skew_factor * frac_from_bottom) * plot_w / (1.0 + skew_factor)
    };

    let mut svg = String::with_capacity(96_000);

    // SVG header
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        total_w as u32, total_h as u32, total_w as u32, total_h as u32
    ));

    // Background
    svg.push_str(&format!(
        "<rect width=\"{}\" height=\"{}\" fill=\"#111116\"/>",
        total_w as u32, total_h as u32
    ));

    // Clipping path for SkewT plot area
    svg.push_str(&format!(
        "<defs><clipPath id=\"plot\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs>",
        margin_left, margin_top, plot_w, plot_h
    ));

    // ================================================================
    // Background grid (clipped to plot area)
    // ================================================================
    svg.push_str("<g clip-path=\"url(#plot)\">");

    // --- Dry adiabats (orange/brown, thin) ---
    for theta_c in (-40..=200).step_by(10) {
        let theta = (theta_c + 273) as f64;
        let mut points = Vec::new();
        let mut p = p_max;
        while p >= p_min {
            let t_c = theta * (p / 1000.0_f64).powf(0.286) - 273.15;
            let x = tp_to_x(t_c, p);
            let y = p_to_y(p);
            points.push(format!("{:.1},{:.1}", x, y));
            p -= 20.0;
        }
        if points.len() >= 2 {
            svg.push_str(&svg_polyline(
                &points.join(" "),
                "rgba(180,120,60,0.18)",
                "0.6",
                "stroke-dasharray=\"6,3\"",
            ));
        }
    }

    // --- Moist adiabats (green, thin dashed) ---
    for tw_start in (-20..=36).step_by(4) {
        let mut t_c = tw_start as f64;
        let mut p = p_max;
        let mut points = Vec::new();
        while p >= p_min {
            let x = tp_to_x(t_c, p);
            let y = p_to_y(p);
            points.push(format!("{:.1},{:.1}", x, y));
            let es = 6.112 * (17.67 * t_c / (t_c + 243.5)).exp();
            let rs = 0.622 * es / (p - es).max(0.001);
            let lv = 2.501e6_f64;
            let cp = 1004.0_f64;
            let rd = 287.0_f64;
            let t_k = t_c + 273.15;
            let gamma_m = (rd * t_k / (cp * p * 100.0))
                * (1.0 + lv * rs / (rd * t_k))
                / (1.0 + lv * lv * rs / (cp * 461.5 * t_k * t_k));
            let dp = 20.0;
            t_c -= gamma_m * dp * 100.0;
            p -= dp;
        }
        if points.len() >= 2 {
            svg.push_str(&svg_polyline(
                &points.join(" "),
                "rgba(60,180,90,0.18)",
                "0.6",
                "stroke-dasharray=\"4,4\"",
            ));
        }
    }

    // --- Mixing ratio lines (cyan/teal, very thin dashed) ---
    let mix_ratios: &[f64] = &[0.4, 1.0, 2.0, 4.0, 7.0, 10.0, 16.0, 24.0];
    for &w_g in mix_ratios {
        let mut points = Vec::new();
        let mut p = p_max;
        while p >= 200.0 {
            let w_kg = w_g / 1000.0;
            let es = w_kg * p / (0.622 + w_kg);
            let td = if es > 0.0 {
                let ln_es = (es / 6.112).ln();
                243.5 * ln_es / (17.67 - ln_es)
            } else {
                -80.0
            };
            let x = tp_to_x(td, p);
            let y = p_to_y(p);
            points.push(format!("{:.1},{:.1}", x, y));
            p -= 50.0;
        }
        if points.len() >= 2 {
            svg.push_str(&svg_polyline(
                &points.join(" "),
                "rgba(80,200,200,0.15)",
                "0.4",
                "stroke-dasharray=\"3,5\"",
            ));
        }
    }

    // --- Isotherms (every 10 deg C, skewed) ---
    let mut t = -90.0_f64;
    while t <= 60.0 {
        let x_bot = tp_to_x(t, p_max);
        let y_bot = p_to_y(p_max);
        let x_top = tp_to_x(t, p_min);
        let y_top = p_to_y(p_min);
        if (t - 0.0).abs() < 0.1 {
            // 0 deg C isotherm -- prominent blue
            svg.push_str(&svg_line(x_bot, y_bot, x_top, y_top, "rgba(80,140,255,0.55)", "1.0"));
        } else {
            svg.push_str(&svg_line(x_bot, y_bot, x_top, y_top, "rgba(255,255,255,0.08)", "0.5"));
        }
        t += 10.0;
    }

    // --- Isobars (horizontal, at standard levels) ---
    let isobars: &[f64] = &[1000.0, 925.0, 850.0, 700.0, 500.0, 400.0, 300.0, 250.0, 200.0, 150.0, 100.0];
    for &p in isobars {
        if p < p_min || p > p_max {
            continue;
        }
        let y = p_to_y(p);
        let alpha = if p == 500.0 || p == 850.0 || p == 300.0 { "0.20" } else { "0.12" };
        svg.push_str(&svg_line(
            margin_left, y, margin_left + plot_w, y,
            &format!("rgba(255,255,255,{})", alpha), "0.5",
        ));
    }

    svg.push_str("</g>"); // end clipped grid

    // ================================================================
    // Data traces (clipped)
    // ================================================================
    svg.push_str("<g clip-path=\"url(#plot)\">");

    let levels = &sounding.levels;
    if levels.len() >= 2 {
        // Temperature trace -- THICK RED
        let temp_points: Vec<String> = levels
            .iter()
            .filter(|l| l.pressure >= p_min && l.pressure <= p_max)
            .map(|l| {
                format!("{:.1},{:.1}", tp_to_x(l.temperature, l.pressure), p_to_y(l.pressure))
            })
            .collect();
        if temp_points.len() >= 2 {
            // Glow effect
            svg.push_str(&svg_polyline(
                &temp_points.join(" "),
                "rgba(255,50,50,0.25)",
                "7",
                "stroke-linejoin=\"round\" stroke-linecap=\"round\"",
            ));
            svg.push_str(&svg_polyline(
                &temp_points.join(" "),
                "#ff2222",
                "3.0",
                "stroke-linejoin=\"round\" stroke-linecap=\"round\"",
            ));
        }

        // Dewpoint trace -- THICK GREEN
        let dew_points: Vec<String> = levels
            .iter()
            .filter(|l| l.pressure >= p_min && l.pressure <= p_max)
            .map(|l| {
                format!("{:.1},{:.1}", tp_to_x(l.dewpoint, l.pressure), p_to_y(l.pressure))
            })
            .collect();
        if dew_points.len() >= 2 {
            // Glow effect
            svg.push_str(&svg_polyline(
                &dew_points.join(" "),
                "rgba(50,220,50,0.25)",
                "7",
                "stroke-linejoin=\"round\" stroke-linecap=\"round\"",
            ));
            svg.push_str(&svg_polyline(
                &dew_points.join(" "),
                "#22cc22",
                "3.0",
                "stroke-linejoin=\"round\" stroke-linecap=\"round\"",
            ));
        }
    }

    svg.push_str("</g>"); // end data traces

    // ================================================================
    // Wind barbs on the right side of the SkewT
    // ================================================================
    let barb_x = margin_left + plot_w + 30.0;
    let mut last_y = -100.0_f64;
    for level in levels {
        if level.pressure < p_min || level.pressure > p_max {
            continue;
        }
        let y = p_to_y(level.pressure);
        if (y - last_y).abs() < 20.0 {
            continue;
        }
        last_y = y;
        svg.push_str(&render_wind_barb_svg(barb_x, y, level.wind_speed, level.wind_dir));
        svg.push_str(&svg_line(
            margin_left + plot_w, y, barb_x - 14.0, y,
            "rgba(255,255,255,0.06)", "0.5",
        ));
    }

    // ================================================================
    // Plot border
    // ================================================================
    svg.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"rgba(160,160,200,0.4)\" stroke-width=\"1\"/>",
        margin_left, margin_top, plot_w, plot_h
    ));

    // ================================================================
    // Axis labels
    // ================================================================

    // Pressure labels (left side)
    for &p in isobars {
        if p < p_min || p > p_max {
            continue;
        }
        let y = p_to_y(p);
        svg.push_str(&svg_text_extra(
            margin_left - 6.0, y, "#8888aa", "monospace", "10", "end",
            "dominant-baseline=\"middle\"",
            &format!("{}", p as i32),
        ));
    }

    // Temperature labels (bottom, along skewed isotherms)
    let mut t = -30.0_f64;
    while t <= 40.0 {
        let x = tp_to_x(t, p_max);
        if x > margin_left + 5.0 && x < margin_left + plot_w - 5.0 {
            svg.push_str(&svg_text(
                x, margin_top + plot_h + 14.0, "#8888aa", "monospace", "9", "middle",
                &format!("{}°", t as i32),
            ));
        }
        t += 10.0;
    }

    // Axis titles
    svg.push_str(&svg_text(
        margin_left + plot_w / 2.0, total_h - 8.0,
        "#999ab0", "'Courier New',monospace", "10", "middle",
        "Temperature (\u{00B0}C)",
    ));
    svg.push_str(&svg_text_extra(
        14.0, margin_top + plot_h / 2.0,
        "#999ab0", "'Courier New',monospace", "10", "middle",
        &format!("transform=\"rotate(-90,14,{:.1})\"", margin_top + plot_h / 2.0),
        "Pressure (hPa)",
    ));

    // ================================================================
    // Title bar
    // ================================================================
    let title = if sounding.station_name.is_empty() {
        format!("{} \u{2014} {}", sounding.station, sounding.time)
    } else {
        format!("{} ({}) \u{2014} {}", sounding.station_name, sounding.station, sounding.time)
    };
    svg.push_str(&svg_text_extra(
        skewt_w / 2.0, 18.0, "#ddddf0", "'Courier New',monospace", "14", "middle",
        "font-weight=\"bold\" letter-spacing=\"1\"",
        &svg_escape(&title),
    ));
    svg.push_str(&svg_text(
        skewt_w / 2.0, 35.0, "#666680", "'Courier New',monospace", "10", "middle",
        "SkewT-LogP Diagram",
    ));

    // Legend (top of plot, inside)
    let leg_y = margin_top + 16.0;
    svg.push_str(&svg_line(margin_left + 8.0, leg_y, margin_left + 28.0, leg_y, "#ff2222", "3"));
    svg.push_str(&svg_text(margin_left + 32.0, leg_y + 4.0, "#ff6666", "'Courier New',monospace", "9", "start", "T"));
    svg.push_str(&svg_line(margin_left + 48.0, leg_y, margin_left + 68.0, leg_y, "#22cc22", "3"));
    svg.push_str(&svg_text(margin_left + 72.0, leg_y + 4.0, "#66cc66", "'Courier New',monospace", "9", "start", "Td"));

    // ================================================================
    // Hodograph inset (top-left area, 180x180)
    // ================================================================
    {
        let hodo_cx = panel_x + panel_w / 2.0;
        let hodo_cy = margin_top + 105.0;
        let hodo_r = 85.0;

        // Background circle
        svg.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.0}\" fill=\"rgba(15,15,25,0.9)\" stroke=\"rgba(100,100,140,0.3)\" stroke-width=\"0.5\"/>",
            hodo_cx, hodo_cy, hodo_r + 5.0
        ));

        // Concentric circles at 20kt intervals (max ~120kt visible)
        let max_wind = 120.0_f64;
        let kt_intervals = [20.0, 40.0, 60.0, 80.0, 100.0, 120.0];
        for &kt in &kt_intervals {
            let r = kt / max_wind * hodo_r;
            svg.push_str(&format!(
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" fill=\"none\" stroke=\"rgba(255,255,255,0.08)\" stroke-width=\"0.5\"/>",
                hodo_cx, hodo_cy, r
            ));
            // Label
            svg.push_str(&svg_text(
                hodo_cx + r + 2.0, hodo_cy - 2.0,
                "rgba(255,255,255,0.2)", "monospace", "7", "start",
                &format!("{}", kt as i32),
            ));
        }

        // Cross hairs
        svg.push_str(&svg_line(hodo_cx - hodo_r, hodo_cy, hodo_cx + hodo_r, hodo_cy, "rgba(255,255,255,0.06)", "0.5"));
        svg.push_str(&svg_line(hodo_cx, hodo_cy - hodo_r, hodo_cx, hodo_cy + hodo_r, "rgba(255,255,255,0.06)", "0.5"));

        // Plot wind vectors as a connected trace, color-coded by height
        // Below 3km: red/orange, 3-6km: green, 6-9km: cyan, 9+km: purple
        let wind_pts: Vec<(f64, f64, f64)> = levels
            .iter()
            .filter(|l| l.pressure >= 100.0 && l.pressure <= 1050.0 && l.wind_speed > 0.0)
            .map(|l| {
                let dir_rad = l.wind_dir.to_radians();
                let spd = l.wind_speed.min(max_wind);
                let scale = spd / max_wind * hodo_r;
                let x = hodo_cx + scale * dir_rad.sin();
                let y = hodo_cy - scale * dir_rad.cos();
                (x, y, l.height)
            })
            .collect();

        if wind_pts.len() >= 2 {
            // Draw segments color-coded by height AGL
            let sfc_elev = sounding.elevation_m;
            for i in 0..wind_pts.len() - 1 {
                let (x1, y1, h1) = wind_pts[i];
                let (x2, y2, _) = wind_pts[i + 1];
                let agl = (h1 - sfc_elev).max(0.0);
                let color = if agl < 1000.0 {
                    "#ff4444" // 0-1km red
                } else if agl < 3000.0 {
                    "#ff8800" // 1-3km orange
                } else if agl < 6000.0 {
                    "#44cc44" // 3-6km green
                } else if agl < 9000.0 {
                    "#44bbdd" // 6-9km cyan
                } else {
                    "#aa66ee" // 9+km purple
                };
                svg.push_str(&svg_line_cap(x1, y1, x2, y2, color, "1.8"));
            }
        }

        // Hodograph label
        svg.push_str(&svg_text(
            hodo_cx, hodo_cy - hodo_r - 10.0,
            "#8888aa", "'Courier New',monospace", "9", "middle",
            "Hodograph (kt)",
        ));
    }

    // ================================================================
    // Indices panel (right side)
    // ================================================================
    {
        let idx = &sounding.indices;
        let px = panel_x + 8.0;
        let mut py = margin_top + 210.0;
        let line_h = 15.0;

        // Panel background
        svg.push_str(&format!(
            "<rect x=\"{:.0}\" y=\"{:.0}\" width=\"{:.0}\" height=\"{:.0}\" rx=\"4\" fill=\"rgba(18,18,30,0.95)\" stroke=\"rgba(80,80,120,0.3)\" stroke-width=\"0.5\"/>",
            panel_x, py - 18.0, panel_w, 480.0
        ));

        // Section: Thermodynamic
        svg.push_str(&svg_text_extra(
            px, py, "#aaaacc", "'Courier New',monospace", "10", "start",
            "font-weight=\"bold\" letter-spacing=\"1\"",
            "THERMODYNAMIC",
        ));
        py += line_h + 4.0;

        let thermo_lines = [
            ("SBCAPE", format!("{:.0}", idx.sbcape), "J/kg"),
            ("SBCIN", format!("{:.0}", idx.sbcin), "J/kg"),
            ("MLCAPE", format!("{:.0}", idx.mlcape), "J/kg"),
            ("MLCIN", format!("{:.0}", idx.mlcin), "J/kg"),
            ("MUCAPE", format!("{:.0}", idx.mucape), "J/kg"),
            ("LI", format!("{:+.1}", idx.li), ""),
        ];
        for (label, val, unit) in &thermo_lines {
            // Color CAPE values by magnitude
            let val_color = if *label == "SBCAPE" || *label == "MLCAPE" || *label == "MUCAPE" {
                let v: f64 = val.parse().unwrap_or(0.0);
                if v >= 2500.0 { "#ff4444" }
                else if v >= 1000.0 { "#ffaa33" }
                else if v >= 500.0 { "#cccc44" }
                else { "#8888bb" }
            } else if *label == "LI" {
                let v: f64 = val.parse().unwrap_or(0.0);
                if v <= -6.0 { "#ff4444" }
                else if v <= -3.0 { "#ffaa33" }
                else { "#8888bb" }
            } else {
                "#8888bb"
            };
            svg.push_str(&svg_text(px, py, "#777799", "monospace", "9", "start", label));
            svg.push_str(&svg_text(px + 80.0, py, val_color, "monospace", "9", "end", val));
            if !unit.is_empty() {
                svg.push_str(&svg_text(px + 82.0, py, "#555570", "monospace", "8", "start", unit));
            }
            py += line_h;
        }

        py += 8.0;

        // Section: Levels
        svg.push_str(&svg_text_extra(
            px, py, "#aaaacc", "'Courier New',monospace", "10", "start",
            "font-weight=\"bold\" letter-spacing=\"1\"",
            "LEVELS",
        ));
        py += line_h + 4.0;

        let level_lines = [
            ("LCL", format!("{:.0}", idx.lcl_m), "m AGL"),
            ("LFC", format!("{:.0}", idx.lfc_m), "m AGL"),
            ("EL", format!("{:.0}", idx.el_m), "m AGL"),
        ];
        for (label, val, unit) in &level_lines {
            svg.push_str(&svg_text(px, py, "#777799", "monospace", "9", "start", label));
            svg.push_str(&svg_text(px + 80.0, py, "#8888bb", "monospace", "9", "end", val));
            svg.push_str(&svg_text(px + 82.0, py, "#555570", "monospace", "8", "start", unit));
            py += line_h;
        }

        py += 8.0;

        // Section: Severe Indices
        svg.push_str(&svg_text_extra(
            px, py, "#aaaacc", "'Courier New',monospace", "10", "start",
            "font-weight=\"bold\" letter-spacing=\"1\"",
            "INDICES",
        ));
        py += line_h + 4.0;

        let severe_lines = [
            ("K-Index", format!("{:.0}", idx.k_index)),
            ("TotTots", format!("{:.0}", idx.total_totals)),
            ("SWEAT", format!("{:.0}", idx.sweat)),
            ("PW", format!("{:.1} mm", idx.pw_mm)),
        ];
        for (label, val) in &severe_lines {
            svg.push_str(&svg_text(px, py, "#777799", "monospace", "9", "start", label));
            svg.push_str(&svg_text(px + 80.0, py, "#8888bb", "monospace", "9", "end", &svg_escape(val)));
            py += line_h;
        }

        py += 8.0;

        // Section: Kinematics
        svg.push_str(&svg_text_extra(
            px, py, "#aaaacc", "'Courier New',monospace", "10", "start",
            "font-weight=\"bold\" letter-spacing=\"1\"",
            "KINEMATICS",
        ));
        py += line_h + 4.0;

        let kin_lines: Vec<(&str, String, &str)> = vec![
            ("SRH 0-1", format!("{:.0}", idx.srh_01), "m\u{00B2}/s\u{00B2}"),
            ("SRH 0-3", format!("{:.0}", idx.srh_03), "m\u{00B2}/s\u{00B2}"),
            ("BS 0-1", format!("{:.0}", idx.bulk_shear_01), "kt"),
            ("BS 0-6", format!("{:.0}", idx.bulk_shear_06), "kt"),
        ];
        for (label, val, unit) in &kin_lines {
            // Color SRH by magnitude
            let val_color = if label.starts_with("SRH") {
                let v: f64 = val.parse().unwrap_or(0.0);
                if v >= 300.0 { "#ff4444" }
                else if v >= 150.0 { "#ffaa33" }
                else if v >= 100.0 { "#cccc44" }
                else { "#8888bb" }
            } else {
                "#8888bb"
            };
            svg.push_str(&svg_text(px, py, "#777799", "monospace", "9", "start", label));
            svg.push_str(&svg_text(px + 80.0, py, val_color, "monospace", "9", "end", val));
            svg.push_str(&svg_text(px + 82.0, py, "#555570", "monospace", "8", "start", unit));
            py += line_h;
        }

        py += 8.0;

        // Section: Composite
        svg.push_str(&svg_text_extra(
            px, py, "#aaaacc", "'Courier New',monospace", "10", "start",
            "font-weight=\"bold\" letter-spacing=\"1\"",
            "COMPOSITE",
        ));
        py += line_h + 4.0;

        let stp_color = if idx.stp >= 4.0 { "#ff4444" }
            else if idx.stp >= 1.0 { "#ffaa33" }
            else { "#8888bb" };
        svg.push_str(&svg_text(px, py, "#777799", "monospace", "9", "start", "STP"));
        svg.push_str(&svg_text(px + 80.0, py, stp_color, "monospace", "9", "end", &format!("{:.1}", idx.stp)));

        // Hodograph color legend at bottom of panel
        py += line_h * 2.5;
        svg.push_str(&svg_text(px, py, "#666680", "monospace", "8", "start", "Hodograph colors:"));
        py += 12.0;
        let hodo_legend = [
            ("#ff4444", "0-1 km"),
            ("#ff8800", "1-3 km"),
            ("#44cc44", "3-6 km"),
            ("#44bbdd", "6-9 km"),
            ("#aa66ee", "9+ km"),
        ];
        for (color, label) in &hodo_legend {
            svg.push_str(&format!(
                "<rect x=\"{:.0}\" y=\"{:.0}\" width=\"10\" height=\"3\" rx=\"1\" fill=\"{}\"/>",
                px, py - 3.0, color
            ));
            svg.push_str(&svg_text(px + 14.0, py, "#666680", "monospace", "8", "start", label));
            py += 11.0;
        }
    }

    svg.push_str("</svg>");
    svg
}

/// Render a single wind barb as SVG elements.
fn render_wind_barb_svg(cx: f64, cy: f64, speed: f64, direction: f64) -> String {
    let mut s = String::new();

    if speed < 2.5 {
        // Calm: circle
        s.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"none\" stroke=\"#aaaacc\" stroke-width=\"1.5\"/>",
            cx, cy
        ));
        return s;
    }

    let staff_len = 28.0_f64;
    let barb_len = 14.0_f64;

    // Staff points into the wind (FROM direction)
    let dir_rad = (direction + 180.0).to_radians();
    let tip_x = cx + staff_len * dir_rad.sin();
    let tip_y = cy - staff_len * dir_rad.cos();

    // Staff line
    s.push_str(&svg_line_cap(cx, cy, tip_x, tip_y, "#aaaacc", "1.5"));

    let mut remaining = speed;
    let mut pos = 0.0_f64;

    // Perpendicular direction for barbs
    let staff_dx = cx - tip_x;
    let staff_dy = cy - tip_y;
    let staff_mag = (staff_dx * staff_dx + staff_dy * staff_dy).sqrt();
    let perp_x = -staff_dy / staff_mag * barb_len;
    let perp_y = staff_dx / staff_mag * barb_len;

    // Flags (50 kt triangles)
    while remaining >= 47.5 {
        let p0 = pos;
        let p1 = pos + 0.15;
        let fx0 = tip_x + p0 * staff_dx;
        let fy0 = tip_y + p0 * staff_dy;
        let fx1 = tip_x + p1 * staff_dx;
        let fy1 = tip_y + p1 * staff_dy;
        s.push_str(&format!(
            "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"#aaaacc\"/>",
            fx0, fy0, fx0 + perp_x, fy0 + perp_y, fx1, fy1
        ));
        remaining -= 50.0;
        pos += 0.18;
    }

    // Full barbs (10 kt)
    while remaining >= 7.5 {
        let bx = tip_x + pos * staff_dx;
        let by = tip_y + pos * staff_dy;
        s.push_str(&svg_line_cap(bx, by, bx + perp_x, by + perp_y, "#aaaacc", "1.5"));
        remaining -= 10.0;
        pos += 0.12;
    }

    // Half barb (5 kt)
    if remaining >= 2.5 {
        let bx = tip_x + pos * staff_dx;
        let by = tip_y + pos * staff_dy;
        s.push_str(&svg_line_cap(bx, by, bx + perp_x * 0.5, by + perp_y * 0.5, "#aaaacc", "1.5"));
    }

    s
}

/// Escape special XML characters in text content.
fn svg_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Convert a Sounding to JSON for the ?format=json endpoint.
pub fn sounding_to_json(sounding: &Sounding) -> serde_json::Value {
    serde_json::json!({
        "station": sounding.station,
        "station_name": sounding.station_name,
        "lat": sounding.lat,
        "lon": sounding.lon,
        "elevation_m": sounding.elevation_m,
        "time": sounding.time,
        "levels": sounding.levels.iter().map(|l| {
            serde_json::json!({
                "pressure": l.pressure,
                "height": l.height,
                "temperature": l.temperature,
                "dewpoint": l.dewpoint,
                "wind_dir": l.wind_dir,
                "wind_speed": l.wind_speed,
            })
        }).collect::<Vec<_>>(),
        "indices": {
            "sbcape": sounding.indices.sbcape,
            "sbcin": sounding.indices.sbcin,
            "mlcape": sounding.indices.mlcape,
            "mlcin": sounding.indices.mlcin,
            "mucape": sounding.indices.mucape,
            "mucin": sounding.indices.mucin,
            "lcl_m": sounding.indices.lcl_m,
            "lfc_m": sounding.indices.lfc_m,
            "el_m": sounding.indices.el_m,
            "li": sounding.indices.li,
            "total_totals": sounding.indices.total_totals,
            "k_index": sounding.indices.k_index,
            "pw_mm": sounding.indices.pw_mm,
            "srh_01": sounding.indices.srh_01,
            "srh_03": sounding.indices.srh_03,
            "bulk_shear_06": sounding.indices.bulk_shear_06,
            "stp": sounding.indices.stp,
        }
    })
}
