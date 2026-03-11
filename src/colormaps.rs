/// Exact replicas of Solarpower07's custom colormaps from colormaps.py.
/// Each colormap uses the same hex colors, gradient definitions, and quantization counts.

/// RGBA color type
pub type Color = [u8; 4];

/// Parse a hex color string like "#ff00aa" to [R, G, B]
const fn hex(r: u8, g: u8, b: u8) -> [u8; 3] {
    [r, g, b]
}

/// Linearly interpolate between two RGB colors
fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [
        (a[0] as f64 + (b[0] as f64 - a[0] as f64) * t) as u8,
        (a[1] as f64 + (b[1] as f64 - a[1] as f64) * t) as u8,
        (a[2] as f64 + (b[2] as f64 - a[2] as f64) * t) as u8,
    ]
}

/// Sample N colors from a linear gradient defined by a list of color stops.
fn sample_gradient(colors: &[[u8; 3]], n: usize) -> Vec<[u8; 3]> {
    if n == 0 { return vec![]; }
    if n == 1 { return vec![colors[0]]; }
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let pos = t * (colors.len() - 1) as f64;
        let idx = (pos as usize).min(colors.len() - 2);
        let frac = pos - idx as f64;
        result.push(lerp_rgb(colors[idx], colors[idx + 1], frac));
    }
    result
}

/// Build a quantized colormap from multiple gradient segments (matching create_custom_cmap).
fn build_composite_colormap(segments: &[(&[[u8; 3]], usize)]) -> Vec<[u8; 3]> {
    let mut all_colors = Vec::new();
    for &(colors, quant) in segments {
        all_colors.extend(sample_gradient(colors, quant));
    }
    all_colors
}

/// Look up a color from a colormap with smooth linear interpolation (default).
pub fn lookup_quantized_ext(cmap: &[[u8; 3]], t: f64) -> Color {
    lookup_smooth(cmap, t)
}

/// Smooth linear interpolation between colormap entries (eliminates banding).
fn lookup_smooth(cmap: &[[u8; 3]], t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let max_idx = cmap.len() - 1;
    let pos = t * max_idx as f64;
    let lo = (pos as usize).min(max_idx.saturating_sub(1));
    let hi = (lo + 1).min(max_idx);
    let frac = pos - lo as f64;
    let c = lerp_rgb(cmap[lo], cmap[hi], frac);
    [c[0], c[1], c[2], 255]
}

/// Stepped/banded lookup (snaps to nearest index). Use only when discrete color bands are desired.
#[allow(dead_code)]
fn lookup_stepped(cmap: &[[u8; 3]], t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let idx = (t * (cmap.len() - 1) as f64).round() as usize;
    let idx = idx.min(cmap.len() - 1);
    let c = cmap[idx];
    [c[0], c[1], c[2], 255]
}

// Alias so all internal callers get smooth interpolation by default.
fn lookup_quantized(cmap: &[[u8; 3]], t: f64) -> Color {
    lookup_smooth(cmap, t)
}

// ============================================================
// WINDS COLORMAP
// ============================================================
const WINDS_COLORS: &[[u8; 3]] = &[
    hex(0xff, 0xff, 0xff), // #ffffff
    hex(0x87, 0xce, 0xfa), // #87cefa
    hex(0x6a, 0x5a, 0xcd), // #6a5acd
    hex(0xe6, 0x96, 0xdc), // #e696dc
    hex(0xc8, 0x5a, 0xbe), // #c85abe
    hex(0xa0, 0x14, 0x96), // #a01496
    hex(0xc8, 0x00, 0x28), // #c80028
    hex(0xdc, 0x28, 0x3c), // #dc283c
    hex(0xf0, 0x50, 0x50), // #f05050
    hex(0xfa, 0xf0, 0x64), // #faf064
    hex(0xdc, 0xbe, 0x46), // #dcbe46
    hex(0xbe, 0x8c, 0x28), // #be8c28
    hex(0xa0, 0x5a, 0x0a), // #a05a0a
];

pub fn winds_color(t: f64, n_segments: usize) -> Color {
    let cmap = sample_gradient(WINDS_COLORS, n_segments);
    lookup_quantized(&cmap, t)
}

// ============================================================
// TEMPERATURE COLORMAP
// ============================================================
const TEMP_COLORS: &[[u8; 3]] = &[
    hex(0x2b, 0x5d, 0x7e), // -60F
    hex(0x75, 0xa8, 0xb0),
    hex(0xae, 0xe3, 0xdc),
    hex(0xa0, 0xb8, 0xd6), // -30F
    hex(0x96, 0x8b, 0xc5),
    hex(0x82, 0x43, 0xb2),
    hex(0xa3, 0x43, 0xb3), // 0F
    hex(0xf7, 0xf7, 0xff),
    hex(0xa0, 0xb8, 0xd6),
    hex(0x0f, 0x55, 0x75), // 30F
    hex(0x6d, 0x8c, 0x77),
    hex(0xf8, 0xee, 0xa2),
    hex(0xaa, 0x71, 0x4d), // 60F
    hex(0x5f, 0x00, 0x00),
    hex(0x85, 0x2c, 0x40),
    hex(0xb2, 0x8f, 0x85), // 90F
    hex(0xe7, 0xe0, 0xda),
    hex(0x95, 0x93, 0x91),
    hex(0x45, 0x48, 0x44), // 120F
];

pub fn temperature_color(t: f64, n_segments: usize) -> Color {
    let cmap = sample_gradient(TEMP_COLORS, n_segments);
    lookup_quantized(&cmap, t)
}

pub fn temperature_color_cropped(t: f64, n_segments: usize, crop_start_f: f64, crop_end_f: f64) -> Color {
    // Crop the colormap to a temperature range in Fahrenheit
    let start_idx = ((crop_start_f + 60.0) / 180.0 * (TEMP_COLORS.len() - 1) as f64) as usize;
    let end_idx = ((crop_end_f + 60.0) / 180.0 * (TEMP_COLORS.len() - 1) as f64) as usize;
    let cropped: Vec<[u8; 3]> = TEMP_COLORS[start_idx..=end_idx.min(TEMP_COLORS.len()-1)].to_vec();
    let cmap = sample_gradient(&cropped, n_segments);
    lookup_quantized(&cmap, t)
}

// ============================================================
// DEW POINT COLORMAP (complex multi-gradient)
// ============================================================
pub fn dewpoint_colormap(dry_points: usize, moist_points: usize) -> Vec<[u8; 3]> {
    let dry_gradient: &[[u8; 3]] = &[
        hex(0x99, 0x6f, 0x4f), hex(0x4d, 0x42, 0x36), hex(0xf2, 0xf2, 0xd8),
    ];
    let moist_gradients: &[&[[u8; 3]]] = &[
        &[hex(0xe3, 0xf3, 0xe6), hex(0x64, 0xc4, 0x61)],
        &[hex(0x32, 0xae, 0x32), hex(0x08, 0x4d, 0x06)],
        &[hex(0x66, 0xa3, 0xad), hex(0x12, 0x29, 0x2a)],
        &[hex(0x66, 0x67, 0x9d), hex(0x2b, 0x1e, 0x63)],
        &[hex(0x71, 0x42, 0x70), hex(0xa2, 0x73, 0x82)],
    ];
    let moist_each = moist_points / moist_gradients.len();
    let mut segments: Vec<(&[[u8; 3]], usize)> = vec![(dry_gradient, dry_points)];
    for g in moist_gradients {
        segments.push((g, moist_each));
    }
    build_composite_colormap(&segments)
}

pub fn dewpoint_color(t: f64, dry_points: usize, moist_points: usize) -> Color {
    let cmap = dewpoint_colormap(dry_points, moist_points);
    lookup_quantized(&cmap, t)
}

// ============================================================
// RELATIVE HUMIDITY COLORMAP
// ============================================================
pub fn rh_colormap() -> Vec<[u8; 3]> {
    let seg1: &[[u8; 3]] = &[
        hex(0xa5, 0x73, 0x4d), hex(0x38, 0x2f, 0x28), hex(0x6e, 0x65, 0x59),
        hex(0xa5, 0x9b, 0x8e), hex(0xdd, 0xd1, 0xc3),
    ];
    let seg2: &[[u8; 3]] = &[hex(0xc8, 0xd7, 0xc0), hex(0x00, 0x4a, 0x2f)];
    let seg3: &[[u8; 3]] = &[hex(0x00, 0x41, 0x23), hex(0x28, 0x58, 0x8c)];
    build_composite_colormap(&[(seg1, 40), (seg2, 50), (seg3, 10)])
}

pub fn rh_color(t: f64) -> Color {
    let cmap = rh_colormap();
    lookup_quantized(&cmap, t)
}

// ============================================================
// RELATIVE VORTICITY COLORMAP
// ============================================================
const RVORT_COLORS: &[[u8; 3]] = &[
    hex(0x32, 0x32, 0x32), // -40
    hex(0x4d, 0x4d, 0x4d),
    hex(0x70, 0x70, 0x70),
    hex(0x8a, 0x8a, 0x8a),
    hex(0xa1, 0xa1, 0xa1),
    hex(0xc0, 0xc0, 0xc0),
    hex(0xd6, 0xd6, 0xd6),
    hex(0xe5, 0xe5, 0xe5),
    hex(0xff, 0xff, 0xff), // 0
    hex(0xfd, 0xd2, 0x44),
    hex(0xfe, 0xa0, 0x00),
    hex(0xf1, 0x67, 0x02),
    hex(0xda, 0x24, 0x22),
    hex(0xab, 0x02, 0x9b),
    hex(0x78, 0x00, 0x8f),
    hex(0x44, 0x00, 0x8b),
    hex(0x00, 0x01, 0x60),
    hex(0x24, 0x44, 0x88),
    hex(0x4f, 0x85, 0xb2),
    hex(0x73, 0xca, 0xdb),
    hex(0x91, 0xff, 0xfd), // 60
];

pub fn relvort_color(t: f64, n_segments: usize) -> Color {
    let cmap = sample_gradient(RVORT_COLORS, n_segments);
    lookup_quantized(&cmap, t)
}

// ============================================================
// SIMULATED IR COLORMAP
// ============================================================
pub fn sim_ir_colormap() -> Vec<[u8; 3]> {
    let seg1: &[[u8; 3]] = &[hex(0x7f, 0x01, 0x7f), hex(0xe3, 0x6f, 0xbe)];
    let seg2: &[[u8; 3]] = &[
        hex(0xff, 0xff, 0xff), hex(0x00, 0x00, 0x00), hex(0xfd, 0x01, 0x00),
        hex(0xfc, 0xff, 0x05), hex(0x03, 0xfd, 0x03), hex(0x01, 0x00, 0x77),
        hex(0x0f, 0xf6, 0xef),
    ];
    let seg3: &[[u8; 3]] = &[hex(0xff, 0xff, 0xff), hex(0x00, 0x00, 0x00)];
    build_composite_colormap(&[(seg1, 10), (seg2, 60), (seg3, 60)])
}

pub fn sim_ir_color(t: f64) -> Color {
    let cmap = sim_ir_colormap();
    lookup_quantized(&cmap, t)
}

// ============================================================
// COMPOSITE COLORMAP (used by CAPE, SRH, STP, EHI, LR, UH, ML metrics)
// ============================================================
fn composite_colormap_base(quantizations: [usize; 7]) -> Vec<[u8; 3]> {
    let seg0: &[[u8; 3]] = &[hex(0xff, 0xff, 0xff), hex(0x69, 0x69, 0x69)];
    let seg1: &[[u8; 3]] = &[hex(0x37, 0x53, 0x6a), hex(0xa7, 0xc8, 0xce)];
    let seg2: &[[u8; 3]] = &[hex(0xe9, 0xdd, 0x96), hex(0xe1, 0x6f, 0x02)];
    let seg3: &[[u8; 3]] = &[hex(0xdc, 0x41, 0x10), hex(0x8b, 0x09, 0x50)];
    let seg4: &[[u8; 3]] = &[hex(0x73, 0x08, 0x8a), hex(0xda, 0x99, 0xe7)];
    let seg5: &[[u8; 3]] = &[hex(0xe9, 0xbe, 0xc3), hex(0xb2, 0x44, 0x5a)];
    let seg6: &[[u8; 3]] = &[hex(0x89, 0x3d, 0x48), hex(0xbc, 0x91, 0x95)];
    build_composite_colormap(&[
        (seg0, quantizations[0]),
        (seg1, quantizations[1]),
        (seg2, quantizations[2]),
        (seg3, quantizations[3]),
        (seg4, quantizations[4]),
        (seg5, quantizations[5]),
        (seg6, quantizations[6]),
    ])
}

// Pre-built composite colormaps
pub fn cape_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([10, 10, 10, 10, 10, 10, 20])
}
pub fn three_cape_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([10, 10, 10, 10, 10, 10, 40])
}
pub fn ehi_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([10, 10, 20, 20, 20, 40, 40])
}
pub fn srh_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([10, 10, 10, 10, 10, 10, 40])
}
pub fn stp_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([10, 10, 10, 10, 10, 10, 40])
}
pub fn lr_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([40, 10, 10, 10, 10, 0, 0])
}
pub fn uh_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([10, 10, 10, 10, 20, 20, 0])
}
pub fn ml_metric_colormap() -> Vec<[u8; 3]> {
    composite_colormap_base([10, 10, 10, 10, 10, 10, 10])
}

// ============================================================
// REFLECTIVITY COLORMAP (PW Style, listed directly)
// ============================================================
pub const REFLECTIVITY_COLORS: &[[u8; 3]] = &[
    hex(0xff, 0xff, 0xff),
    hex(0xf2, 0xf6, 0xfc),
    hex(0xd9, 0xe3, 0xf4),
    hex(0xb0, 0xc6, 0xe6),
    hex(0x8a, 0xa7, 0xda),
    hex(0x64, 0x8b, 0xcb),
    hex(0x39, 0x6d, 0xc1),
    hex(0x13, 0x50, 0xb4),
    hex(0x0d, 0x4f, 0x5d),
    hex(0x43, 0x73, 0x6f),
    hex(0x77, 0x98, 0x7b),
    hex(0xa8, 0xbf, 0x8b),
    hex(0xfd, 0xf2, 0x73),
    hex(0xf2, 0xd4, 0x5a),
    hex(0xee, 0xb2, 0x47),
    hex(0xe1, 0x93, 0x2d),
    hex(0xd9, 0x75, 0x17),
    hex(0xcd, 0x54, 0x03),
    hex(0xcd, 0x00, 0x02),
    hex(0xa1, 0x02, 0x06),
    hex(0x75, 0x03, 0x0b),
    hex(0x9e, 0x37, 0xab),
    hex(0x83, 0x25, 0x9d),
    hex(0x60, 0x14, 0x90),
    hex(0x81, 0x81, 0x81),
    hex(0xb3, 0xb3, 0xb3),
    hex(0xe8, 0xe8, 0xe8),
];

pub fn reflectivity_color(t: f64) -> Color {
    lookup_smooth(REFLECTIVITY_COLORS, t)
}

// ============================================================
// GEOPOTENTIAL HEIGHT ANOMALY COLORMAP
// ============================================================
const GEOPOT_ANOM_COLORS: &[[u8; 3]] = &[
    hex(0xc9, 0xf2, 0xfc), // -40
    hex(0xe6, 0x84, 0xf4),
    hex(0x73, 0x21, 0x64),
    hex(0x7b, 0x2b, 0x8d),
    hex(0x8a, 0x41, 0xd6),
    hex(0x25, 0x3f, 0xba),
    hex(0x70, 0x89, 0xcb),
    hex(0xc0, 0xd5, 0xe8),
    hex(0xff, 0xff, 0xff), // 0
    hex(0xfb, 0xcf, 0xa1),
    hex(0xfc, 0x98, 0x4b),
    hex(0xb8, 0x38, 0x00),
    hex(0xa3, 0x24, 0x1a),
    hex(0x5e, 0x14, 0x25),
    hex(0x42, 0x29, 0x3e),
    hex(0x55, 0x7b, 0x75),
    hex(0xdd, 0xd5, 0xcf), // 40
];

pub fn geopot_anomaly_color(t: f64, n_segments: usize) -> Color {
    let cmap = sample_gradient(GEOPOT_ANOM_COLORS, n_segments);
    lookup_quantized(&cmap, t)
}

// ============================================================
// PRECIPITATION COLORMAP (inches)
// ============================================================
pub fn precip_colormap_in() -> Vec<[u8; 3]> {
    let seg0: &[[u8; 3]] = &[hex(0xff, 0xff, 0xff), hex(0xff, 0xff, 0xff)];
    let seg1: &[[u8; 3]] = &[
        hex(0xdc, 0xdc, 0xdc), hex(0xbe, 0xbe, 0xbe),
        hex(0x9e, 0x9e, 0x9e), hex(0x81, 0x81, 0x81),
    ];
    let seg2: &[[u8; 3]] = &[hex(0xb8, 0xf0, 0xc1), hex(0x15, 0x64, 0x71)];
    let seg3: &[[u8; 3]] = &[hex(0x16, 0x4f, 0xba), hex(0xd8, 0xed, 0xf5)];
    let seg4: &[[u8; 3]] = &[hex(0xcf, 0xbd, 0xdd), hex(0xa1, 0x34, 0xb1)];
    let seg5: &[[u8; 3]] = &[hex(0xa4, 0x3c, 0x32), hex(0xdd, 0x9c, 0x98)];
    let seg6: &[[u8; 3]] = &[hex(0xf6, 0xf0, 0xa3), hex(0x7e, 0x4b, 0x26), hex(0x54, 0x2f, 0x17)];
    build_composite_colormap(&[
        (seg0, 1), (seg1, 9), (seg2, 40), (seg3, 50),
        (seg4, 100), (seg5, 200), (seg6, 1100),
    ])
}

pub fn precip_color_in(t: f64) -> Color {
    let cmap = precip_colormap_in();
    lookup_quantized(&cmap, t)
}

// ============================================================
// SHADED OVERLAY (transparent to semi-transparent black)
// ============================================================
#[allow(dead_code)]
pub fn shaded_overlay_color(t: f64) -> Color {
    let alpha = (t.clamp(0.0, 1.0) * 96.0) as u8; // 0x00 to 0x60
    [0, 0, 0, alpha]
}
