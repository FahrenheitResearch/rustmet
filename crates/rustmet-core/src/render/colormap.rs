//! Standard meteorological colormaps.
//!
//! Each colormap is a slice of (value_fraction, r, g, b) control points where
//! value_fraction is in [0.0, 1.0]. Colors are linearly interpolated between
//! control points for smooth gradients.

/// A single control point: (normalized_position, r, g, b).
/// Position is in [0.0, 1.0] mapping linearly from vmin to vmax.
pub type ColorStop = (f64, u8, u8, u8);

/// Linearly interpolate a color from a colormap at a given normalized value t in [0, 1].
/// Returns (r, g, b). Values outside [0, 1] are clamped.
pub fn interpolate_color(colormap: &[ColorStop], value: f64) -> (u8, u8, u8) {
    if colormap.is_empty() {
        return (0, 0, 0);
    }
    if colormap.len() == 1 {
        return (colormap[0].1, colormap[0].2, colormap[0].3);
    }

    let t = value.clamp(0.0, 1.0);

    // Find the bracketing control points
    if t <= colormap[0].0 {
        return (colormap[0].1, colormap[0].2, colormap[0].3);
    }
    if t >= colormap[colormap.len() - 1].0 {
        let last = &colormap[colormap.len() - 1];
        return (last.1, last.2, last.3);
    }

    for i in 0..colormap.len() - 1 {
        let (t0, r0, g0, b0) = colormap[i];
        let (t1, r1, g1, b1) = colormap[i + 1];
        if t >= t0 && t <= t1 {
            let frac = if (t1 - t0).abs() < 1e-12 {
                0.0
            } else {
                (t - t0) / (t1 - t0)
            };
            let r = (r0 as f64 + (r1 as f64 - r0 as f64) * frac) as u8;
            let g = (g0 as f64 + (g1 as f64 - g0 as f64) * frac) as u8;
            let b = (b0 as f64 + (b1 as f64 - b0 as f64) * frac) as u8;
            return (r, g, b);
        }
    }

    let last = &colormap[colormap.len() - 1];
    (last.1, last.2, last.3)
}

// ============================================================
// Temperature colormap: cool blues -> white -> warm reds
// Designed for -40C to +50C / -40F to 120F
// ============================================================
pub static TEMPERATURE: &[ColorStop] = &[
    (0.000, 0x2b, 0x5d, 0x7e), // deep cold blue
    (0.111, 0x75, 0xa8, 0xb0),
    (0.222, 0xae, 0xe3, 0xdc),
    (0.278, 0xa0, 0xb8, 0xd6),
    (0.333, 0x96, 0x8b, 0xc5),
    (0.389, 0x82, 0x43, 0xb2), // purple for sub-zero
    (0.417, 0xa3, 0x43, 0xb3),
    (0.444, 0xf7, 0xf7, 0xff), // near-white at freezing
    (0.472, 0xa0, 0xb8, 0xd6),
    (0.500, 0x0f, 0x55, 0x75), // cool teal
    (0.556, 0x6d, 0x8c, 0x77),
    (0.611, 0xf8, 0xee, 0xa2), // warm yellow
    (0.667, 0xaa, 0x71, 0x4d), // warm brown
    (0.722, 0x5f, 0x00, 0x00), // dark red
    (0.778, 0x85, 0x2c, 0x40),
    (0.833, 0xb2, 0x8f, 0x85),
    (0.889, 0xe7, 0xe0, 0xda),
    (0.944, 0x95, 0x93, 0x91),
    (1.000, 0x45, 0x48, 0x44), // extreme heat gray
];

// ============================================================
// Precipitation colormap: white -> gray -> green -> blue -> purple -> red -> brown
// Designed for 0 to 15 inches
// ============================================================
pub static PRECIPITATION: &[ColorStop] = &[
    (0.000, 0xff, 0xff, 0xff), // white (no precip)
    (0.005, 0xdc, 0xdc, 0xdc),
    (0.020, 0xbe, 0xbe, 0xbe),
    (0.040, 0x9e, 0x9e, 0x9e),
    (0.060, 0x81, 0x81, 0x81), // gray trace
    (0.067, 0xb8, 0xf0, 0xc1), // light green
    (0.133, 0x15, 0x64, 0x71), // dark teal
    (0.200, 0x16, 0x4f, 0xba), // blue
    (0.333, 0xd8, 0xed, 0xf5), // light blue
    (0.400, 0xcf, 0xbd, 0xdd), // lavender
    (0.533, 0xa1, 0x34, 0xb1), // purple
    (0.600, 0xa4, 0x3c, 0x32), // red
    (0.733, 0xdd, 0x9c, 0x98), // pink
    (0.800, 0xf6, 0xf0, 0xa3), // yellow
    (0.900, 0x7e, 0x4b, 0x26), // brown
    (1.000, 0x54, 0x2f, 0x17), // dark brown
];

// ============================================================
// Wind speed colormap: white -> blue -> purple -> red -> yellow -> brown
// Designed for 0-175 kt
// ============================================================
pub static WIND: &[ColorStop] = &[
    (0.000, 0xff, 0xff, 0xff), // calm white
    (0.083, 0x87, 0xce, 0xfa), // light sky blue
    (0.167, 0x6a, 0x5a, 0xcd), // slate blue
    (0.250, 0xe6, 0x96, 0xdc), // orchid
    (0.333, 0xc8, 0x5a, 0xbe), // medium orchid
    (0.417, 0xa0, 0x14, 0x96), // dark magenta
    (0.500, 0xc8, 0x00, 0x28), // crimson
    (0.583, 0xdc, 0x28, 0x3c), // red
    (0.667, 0xf0, 0x50, 0x50), // coral
    (0.750, 0xfa, 0xf0, 0x64), // khaki
    (0.833, 0xdc, 0xbe, 0x46), // dark khaki
    (0.917, 0xbe, 0x8c, 0x28), // dark goldenrod
    (1.000, 0xa0, 0x5a, 0x0a), // saddle brown
];

// ============================================================
// Reflectivity colormap: white -> blue -> green -> yellow -> red -> purple -> gray
// Designed for 5-75 dBZ
// ============================================================
pub static REFLECTIVITY: &[ColorStop] = &[
    (0.000, 0xff, 0xff, 0xff),
    (0.038, 0xf2, 0xf6, 0xfc),
    (0.077, 0xd9, 0xe3, 0xf4),
    (0.115, 0xb0, 0xc6, 0xe6),
    (0.154, 0x8a, 0xa7, 0xda),
    (0.192, 0x64, 0x8b, 0xcb),
    (0.231, 0x39, 0x6d, 0xc1), // blues
    (0.269, 0x13, 0x50, 0xb4),
    (0.308, 0x0d, 0x4f, 0x5d),
    (0.346, 0x43, 0x73, 0x6f),
    (0.385, 0x77, 0x98, 0x7b), // greens
    (0.423, 0xa8, 0xbf, 0x8b),
    (0.462, 0xfd, 0xf2, 0x73), // yellow
    (0.500, 0xf2, 0xd4, 0x5a),
    (0.538, 0xee, 0xb2, 0x47),
    (0.577, 0xe1, 0x93, 0x2d), // orange
    (0.615, 0xd9, 0x75, 0x17),
    (0.654, 0xcd, 0x54, 0x03),
    (0.692, 0xcd, 0x00, 0x02), // red
    (0.731, 0xa1, 0x02, 0x06),
    (0.769, 0x75, 0x03, 0x0b),
    (0.808, 0x9e, 0x37, 0xab), // purple
    (0.846, 0x83, 0x25, 0x9d),
    (0.885, 0x60, 0x14, 0x90),
    (0.923, 0x81, 0x81, 0x81), // gray (extreme)
    (0.962, 0xb3, 0xb3, 0xb3),
    (1.000, 0xe8, 0xe8, 0xe8),
];

// ============================================================
// CAPE colormap: gray -> teal -> yellow -> orange -> red -> purple -> pink -> rose
// Designed for 0-8000 J/kg
// ============================================================
pub static CAPE: &[ColorStop] = &[
    (0.000, 0xff, 0xff, 0xff), // white
    (0.071, 0x69, 0x69, 0x69), // gray
    (0.143, 0x37, 0x53, 0x6a), // steel blue
    (0.214, 0xa7, 0xc8, 0xce), // powder blue
    (0.286, 0xe9, 0xdd, 0x96), // khaki
    (0.357, 0xe1, 0x6f, 0x02), // dark orange
    (0.429, 0xdc, 0x41, 0x10), // red-orange
    (0.500, 0x8b, 0x09, 0x50), // dark magenta
    (0.571, 0x73, 0x08, 0x8a), // dark violet
    (0.643, 0xda, 0x99, 0xe7), // plum
    (0.714, 0xe9, 0xbe, 0xc3), // misty rose
    (0.786, 0xb2, 0x44, 0x5a), // palevioletred
    (0.857, 0x89, 0x3d, 0x48), // dark rose
    (1.000, 0xbc, 0x91, 0x95), // rosy brown
];

// ============================================================
// Relative humidity colormap: brown (dry) -> green -> blue (moist)
// Designed for 0-100%
// ============================================================
pub static RELATIVE_HUMIDITY: &[ColorStop] = &[
    (0.000, 0xa5, 0x73, 0x4d), // brown (dry)
    (0.100, 0x38, 0x2f, 0x28), // dark brown
    (0.200, 0x6e, 0x65, 0x59), // dim gray
    (0.300, 0xa5, 0x9b, 0x8e), // gray
    (0.400, 0xdd, 0xd1, 0xc3), // light gray
    (0.450, 0xc8, 0xd7, 0xc0), // pale green
    (0.700, 0x00, 0x4a, 0x2f), // dark green
    (0.900, 0x00, 0x41, 0x23), // darker green
    (1.000, 0x28, 0x58, 0x8c), // steel blue (saturated)
];

// ============================================================
// Vorticity / generic diverging colormap: gray -> white -> yellow -> red -> purple -> blue -> cyan
// ============================================================
pub static VORTICITY: &[ColorStop] = &[
    (0.000, 0x32, 0x32, 0x32), // dark gray (negative)
    (0.100, 0x70, 0x70, 0x70),
    (0.200, 0xa1, 0xa1, 0xa1),
    (0.300, 0xd6, 0xd6, 0xd6),
    (0.400, 0xff, 0xff, 0xff), // white (zero)
    (0.450, 0xfd, 0xd2, 0x44), // yellow
    (0.500, 0xfe, 0xa0, 0x00), // orange
    (0.550, 0xf1, 0x67, 0x02), // dark orange
    (0.600, 0xda, 0x24, 0x22), // red
    (0.650, 0xab, 0x02, 0x9b), // magenta
    (0.700, 0x78, 0x00, 0x8f), // purple
    (0.750, 0x44, 0x00, 0x8b), // dark purple
    (0.800, 0x00, 0x01, 0x60), // navy
    (0.850, 0x24, 0x44, 0x88), // steel blue
    (0.900, 0x4f, 0x85, 0xb2), // cadet blue
    (0.950, 0x73, 0xca, 0xdb), // medium turquoise
    (1.000, 0x91, 0xff, 0xfd), // cyan
];

/// Look up a named colormap. Returns None if the name is not recognized.
pub fn get_colormap(name: &str) -> Option<&'static [ColorStop]> {
    match name {
        "temperature" | "temp" | "temperature_f" | "temperature_c"
            | "temperature_250" | "temperature_500" | "temperature_700"
            | "dewpoint" | "dewpoint_f" => Some(TEMPERATURE),
        "precipitation" | "precip" | "precip_in" | "rain" => Some(PRECIPITATION),
        "wind" | "winds" | "winds_sfc" => Some(WIND),
        "reflectivity" | "refl" | "dbz" => Some(REFLECTIVITY),
        "cape" | "three_cape" | "stp" | "ehi" | "srh" | "uh"
            | "lapse_rate" | "ml_metric" => Some(CAPE),
        "relative_humidity" | "rh" => Some(RELATIVE_HUMIDITY),
        "vorticity" | "relvort" | "geopot_anomaly" => Some(VORTICITY),
        _ => None,
    }
}

/// List all available colormap names.
pub fn list_colormaps() -> &'static [&'static str] {
    &[
        "temperature",
        "precipitation",
        "wind",
        "reflectivity",
        "cape",
        "relative_humidity",
        "vorticity",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_endpoints() {
        let (r, g, b) = interpolate_color(TEMPERATURE, 0.0);
        assert_eq!((r, g, b), (0x2b, 0x5d, 0x7e));

        let (r, g, b) = interpolate_color(TEMPERATURE, 1.0);
        assert_eq!((r, g, b), (0x45, 0x48, 0x44));
    }

    #[test]
    fn test_interpolate_midpoint() {
        let (r, g, b) = interpolate_color(TEMPERATURE, 0.5);
        // Should be the control point at 0.500
        assert_eq!((r, g, b), (0x0f, 0x55, 0x75));
    }

    #[test]
    fn test_clamp_out_of_range() {
        let below = interpolate_color(TEMPERATURE, -0.5);
        let at_zero = interpolate_color(TEMPERATURE, 0.0);
        assert_eq!(below, at_zero);

        let above = interpolate_color(TEMPERATURE, 1.5);
        let at_one = interpolate_color(TEMPERATURE, 1.0);
        assert_eq!(above, at_one);
    }

    #[test]
    fn test_get_colormap() {
        assert!(get_colormap("temperature").is_some());
        assert!(get_colormap("wind").is_some());
        assert!(get_colormap("nonexistent").is_none());
    }
}
