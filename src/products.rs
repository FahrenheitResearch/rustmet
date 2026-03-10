/// Product definitions for weather plotting.
///
/// Re-exports `GribProduct` from rustmet-core, plus rendering-specific types.

// Re-export core GRIB product types
pub use rustmet_core::products::{GribProduct, GRIB_PRODUCTS, find_grib_product, list_grib_products};

// ============================================================
// Full Plot Product Definitions (rendering types)
// ============================================================

/// How to render the field data
#[derive(Clone, Debug)]
pub enum RenderStyle {
    /// Filled contours (most common)
    FilledContour,
    /// Raster/pcolormesh style (simulated IR, ML metrics, max omega)
    Raster,
}

/// Overlay type
#[derive(Clone, Debug)]
pub enum Overlay {
    /// Wind barbs at a given level
    WindBarbs(FieldLevel),
    /// Geopotential height contours at a given level
    HeightContours(FieldLevel),
    /// MSLP contours
    MslpContours,
    /// UH > threshold contour + shaded fill
    UhOverlay { duration: UhDuration, threshold: f64 },
    /// Reflectivity > threshold contour + shaded fill
    ReflectivityOverlay { threshold: f64 },
}

#[derive(Clone, Debug)]
pub enum FieldLevel {
    Surface,
    PressureMb(f64),
}

#[derive(Clone, Copy, Debug)]
pub enum UhDuration {
    OneHour,
    Run,
}

/// What data does this product need?
#[derive(Clone, Debug)]
pub enum ProductData {
    // Surface fields
    SurfaceTemperature { unit: TempUnit },
    SurfaceDewpoint { unit: TempUnit },
    SurfaceRH,
    SurfaceWindSpeed,

    // Upper air fields
    UpperWindSpeed { level: f64 },
    UpperTemperature { level: f64, unit: TempUnit },
    UpperRH { level: f64 },
    UpperVorticity { level: f64 },
    GeopHeightAnomaly { level: f64 },

    // Severe/convective
    Cape { parcel: ParcelType },
    ThreeCape { parcel: ParcelType },
    Srh { top_km: f64 },
    Stp,
    Ehi { top_km: f64 },
    LapseRate { bottom_km: f64, top_km: f64 },
    LapseRatePressure { bottom_mb: f64, top_mb: f64 },

    // Radar
    CompositeReflectivity,
    UpdatedHelicity { duration: UhDuration },

    // Other
    SimulatedIR,
    MaxOmega,
    Precipitation { range: PrecipRange, unit: PrecipUnit },
}

#[derive(Clone, Debug)]
pub enum TempUnit { Celsius, Fahrenheit }

#[derive(Clone, Copy, Debug)]
pub enum ParcelType { SB, ML, MU }

#[derive(Clone, Debug)]
pub enum PrecipRange { Total, OneHour, SixHour }

#[derive(Clone, Debug)]
pub enum PrecipUnit { Inches }

/// A complete plot product definition
#[derive(Clone, Debug)]
pub struct Product {
    pub name: &'static str,
    pub product_name_fn: fn(f64) -> String, // takes pressure level, returns title string
    pub data: ProductData,
    pub render_style: RenderStyle,
    pub contour_min: f64,
    pub contour_max: f64,
    pub contour_step: f64,
    pub cbar_min: f64,
    pub cbar_max: f64,
    pub cbar_step: f64,
    pub colormap_id: &'static str,
    pub overlays: Vec<Overlay>,
    /// Non-uniform contour levels (if set, overrides min/max/step)
    pub custom_levels: Option<Vec<f64>>,
    pub custom_cbar_ticks: Option<Vec<f64>>,
}

/// Generate the default set of products for a severe weather plotting preset.
pub fn default_products() -> Vec<Product> {
    let mut products = Vec::new();

    // ============ SURFACE ============

    products.push(Product {
        name: "Surface Temperature",
        product_name_fn: |_| "Surface Temperature (\u{00b0}F), MSLP (mb), 10m AGL Wind (kt)".into(),
        data: ProductData::SurfaceTemperature { unit: TempUnit::Fahrenheit },
        render_style: RenderStyle::FilledContour,
        contour_min: -60.0, contour_max: 120.0, contour_step: 1.0,
        cbar_min: -60.0, cbar_max: 120.0, cbar_step: 10.0,
        colormap_id: "temperature_f",
        overlays: vec![
            Overlay::WindBarbs(FieldLevel::Surface),
            Overlay::MslpContours,
        ],
        custom_levels: None, custom_cbar_ticks: None,
    });

    products.push(Product {
        name: "Surface Dewpoint",
        product_name_fn: |_| "Surface Dewpoint (\u{00b0}F), MSLP (mb), 10m AGL Wind (kt)".into(),
        data: ProductData::SurfaceDewpoint { unit: TempUnit::Fahrenheit },
        render_style: RenderStyle::FilledContour,
        contour_min: -40.0, contour_max: 90.0, contour_step: 1.0,
        cbar_min: -40.0, cbar_max: 90.0, cbar_step: 10.0,
        colormap_id: "dewpoint_f",
        overlays: vec![
            Overlay::WindBarbs(FieldLevel::Surface),
            Overlay::MslpContours,
        ],
        custom_levels: None, custom_cbar_ticks: None,
    });

    products.push(Product {
        name: "Surface Relative Humidity",
        product_name_fn: |_| "Surface Relative Humidity (%), MSLP (mb), 10m AGL Wind (kt)".into(),
        data: ProductData::SurfaceRH,
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 100.0, contour_step: 1.0,
        cbar_min: 0.0, cbar_max: 100.0, cbar_step: 10.0,
        colormap_id: "rh",
        overlays: vec![
            Overlay::WindBarbs(FieldLevel::Surface),
            Overlay::MslpContours,
        ],
        custom_levels: None, custom_cbar_ticks: None,
    });

    products.push(Product {
        name: "Surface Winds and MSLP",
        product_name_fn: |_| "Surface MSLP (mb), 10m AGL Wind (kt)".into(),
        data: ProductData::SurfaceWindSpeed,
        render_style: RenderStyle::FilledContour,
        contour_min: 10.0, contour_max: 70.0, contour_step: 1.0,
        cbar_min: 10.0, cbar_max: 70.0, cbar_step: 5.0,
        colormap_id: "winds_sfc",
        overlays: vec![
            Overlay::WindBarbs(FieldLevel::Surface),
            Overlay::MslpContours,
        ],
        custom_levels: None, custom_cbar_ticks: None,
    });

    // ============ UPPER AIR ============
    for &(level, wind_range, wind_step) in &[
        (250.0, (25.0, 175.0), 10.0),
        (350.0, (20.0, 160.0), 10.0),
        (500.0, (20.0, 140.0), 5.0),
        (700.0, (15.0, 100.0), 5.0),
        (850.0, (15.0, 80.0), 5.0),
    ] {
        let lev = level as u32;
        products.push(Product {
            name: match lev {
                250 => "250mb Winds",
                350 => "350mb Winds",
                500 => "500mb Winds",
                700 => "700mb Winds",
                850 => "850mb Winds",
                _ => "Winds",
            },
            product_name_fn: match lev {
                250 => |_| "250 mb Height (dam), Wind (kt)".into(),
                350 => |_| "350 mb Height (dam), Wind (kt)".into(),
                500 => |_| "500 mb Height (dam), Wind (kt)".into(),
                700 => |_| "700 mb Height (dam), Wind (kt)".into(),
                850 => |_| "850 mb Height (dam), Wind (kt)".into(),
                _ => |_| "Wind (kt)".into(),
            },
            data: ProductData::UpperWindSpeed { level },
            render_style: RenderStyle::FilledContour,
            contour_min: wind_range.0, contour_max: wind_range.1, contour_step: 1.0,
            cbar_min: wind_range.0, cbar_max: wind_range.1, cbar_step: wind_step,
            colormap_id: "winds",
            overlays: vec![
                Overlay::WindBarbs(FieldLevel::PressureMb(level)),
                Overlay::HeightContours(FieldLevel::PressureMb(level)),
            ],
            custom_levels: None, custom_cbar_ticks: None,
        });
    }

    // Upper air temperatures
    for &(level, _crop_start, _crop_end, c_min, c_max) in &[
        (250.0, -40.0, 70.0, -70.0, -30.0),
        (500.0, -40.0, 70.0, -50.0, 5.0),
        (700.0, -40.0, 90.0, -40.0, 25.0),
    ] {
        let lev = level as u32;
        products.push(Product {
            name: match lev {
                250 => "250mb Temperature",
                500 => "500mb Temperature",
                700 => "700mb Temperature",
                _ => "Temperature",
            },
            product_name_fn: match lev {
                250 => |_| "250 mb Temperature (\u{00b0}C), Height (dam), Wind (kt)".into(),
                500 => |_| "500 mb Temperature (\u{00b0}C), Height (dam), Wind (kt)".into(),
                700 => |_| "700 mb Temperature (\u{00b0}C), Height (dam), Wind (kt)".into(),
                _ => |_| "Temperature (\u{00b0}C)".into(),
            },
            data: ProductData::UpperTemperature { level, unit: TempUnit::Celsius },
            render_style: RenderStyle::FilledContour,
            contour_min: c_min, contour_max: c_max, contour_step: 1.0,
            cbar_min: c_min, cbar_max: c_max, cbar_step: 5.0,
            colormap_id: match lev {
                250 => "temperature_250",
                500 => "temperature_500",
                700 => "temperature_700",
                _ => "temperature_c",
            },
            overlays: vec![
                Overlay::WindBarbs(FieldLevel::PressureMb(level)),
                Overlay::HeightContours(FieldLevel::PressureMb(level)),
            ],
            custom_levels: None, custom_cbar_ticks: None,
        });
    }

    // 700mb RH
    products.push(Product {
        name: "700mb Relative Humidity",
        product_name_fn: |_| "700 mb Relative Humidity (%), Height (dam), Wind (kt)".into(),
        data: ProductData::UpperRH { level: 700.0 },
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 100.0, contour_step: 1.0,
        cbar_min: 0.0, cbar_max: 100.0, cbar_step: 10.0,
        colormap_id: "rh",
        overlays: vec![
            Overlay::WindBarbs(FieldLevel::PressureMb(700.0)),
            Overlay::HeightContours(FieldLevel::PressureMb(700.0)),
        ],
        custom_levels: None, custom_cbar_ticks: None,
    });

    // 500mb Height Anomaly
    products.push(Product {
        name: "500mb Height Anomaly",
        product_name_fn: |_| "500 mb Height Anomaly (dam) (based on 1990-2020 climatology)".into(),
        data: ProductData::GeopHeightAnomaly { level: 500.0 },
        render_style: RenderStyle::FilledContour,
        contour_min: -40.0, contour_max: 40.0, contour_step: 1.0,
        cbar_min: -40.0, cbar_max: 40.0, cbar_step: 10.0,
        colormap_id: "geopot_anomaly",
        overlays: vec![
            Overlay::HeightContours(FieldLevel::PressureMb(500.0)),
        ],
        custom_levels: None, custom_cbar_ticks: None,
    });

    // ============ RADAR / REFLECTIVITY ============
    products.push(Product {
        name: "Composite Reflectivity",
        product_name_fn: |_| "Composite Reflectivity (dBZ)".into(),
        data: ProductData::CompositeReflectivity,
        render_style: RenderStyle::FilledContour,
        contour_min: 5.0, contour_max: 70.0, contour_step: 2.5,
        cbar_min: 5.0, cbar_max: 70.0, cbar_step: 5.0,
        colormap_id: "reflectivity",
        overlays: vec![],
        custom_levels: None, custom_cbar_ticks: None,
    });

    // Composite Reflectivity + UH overlays
    for &(uh_thresh, dur_name, dur) in &[
        (75.0, "1h", UhDuration::OneHour),
        (75.0, "run", UhDuration::Run),
        (25.0, "1h", UhDuration::OneHour),
        (25.0, "run", UhDuration::Run),
    ] {
        let name_str: &'static str = match (uh_thresh as u32, dur_name) {
            (75, "1h") => "Composite Reflectivity, 1h Max Updraft Helicity",
            (75, "run") => "Composite Reflectivity, Run Max Updraft Helicity",
            (25, "1h") => "Composite Reflectivity, 1h Max Updraft Helicity (25)",
            (25, "run") => "Composite Reflectivity, Run Max Updraft Helicity (25)",
            _ => "Composite Reflectivity + UH",
        };
        products.push(Product {
            name: name_str,
            product_name_fn: match (uh_thresh as u32, dur_name) {
                (75, "1h") => |_| "Composite Reflectivity (dBZ), 1h Max Updraft Helicity > 75 (m\u{00b2}/s\u{00b2})".into(),
                (75, "run") => |_| "Composite Reflectivity (dBZ), Run Max Updraft Helicity > 75 (m\u{00b2}/s\u{00b2})".into(),
                (25, "1h") => |_| "Composite Reflectivity (dBZ), 1h Max Updraft Helicity > 25 (m\u{00b2}/s\u{00b2})".into(),
                (25, "run") => |_| "Composite Reflectivity (dBZ), Run Max Updraft Helicity > 25 (m\u{00b2}/s\u{00b2})".into(),
                _ => |_| "Composite Reflectivity (dBZ)".into(),
            },
            data: ProductData::CompositeReflectivity,
            render_style: RenderStyle::FilledContour,
            contour_min: 5.0, contour_max: 70.0, contour_step: 2.5,
            cbar_min: 5.0, cbar_max: 70.0, cbar_step: 5.0,
            colormap_id: "reflectivity",
            overlays: vec![Overlay::UhOverlay { duration: dur, threshold: uh_thresh }],
            custom_levels: None, custom_cbar_ticks: None,
        });
    }

    // Max UH Swath
    products.push(Product {
        name: "Run Max Updraft Helicity",
        product_name_fn: |_| "Run Max Updraft Helicity (m\u{00b2}/s\u{00b2})".into(),
        data: ProductData::UpdatedHelicity { duration: UhDuration::Run },
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 400.0, contour_step: 5.0,
        cbar_min: 0.0, cbar_max: 400.0, cbar_step: 20.0,
        colormap_id: "uh",
        overlays: vec![],
        custom_levels: None,
        custom_cbar_ticks: {
            let mut ticks: Vec<f64> = (0..200).step_by(10).map(|x| x as f64).collect();
            ticks.extend((200..=400).step_by(20).map(|x| x as f64));
            Some(ticks)
        },
    });

    // ============ ENVIRONMENTAL PARAMETERS ============

    // CAPE variants
    for &(parcel, name, _title) in &[
        (ParcelType::SB, "Surface-Based CAPE", "Surface-Based CAPE (J Kg\u{207b}\u{00b9})"),
        (ParcelType::ML, "Mixed-Layer CAPE", "Mixed-Layer CAPE (J Kg\u{207b}\u{00b9})"),
        (ParcelType::MU, "Most Unstable CAPE", "Most Unstable CAPE (J Kg\u{207b}\u{00b9})"),
    ] {
        products.push(Product {
            name,
            product_name_fn: match parcel {
                ParcelType::SB => |_| "Surface-Based CAPE (J Kg\u{207b}\u{00b9})".into(),
                ParcelType::ML => |_| "Mixed-Layer CAPE (J Kg\u{207b}\u{00b9})".into(),
                ParcelType::MU => |_| "Most Unstable CAPE (J Kg\u{207b}\u{00b9})".into(),
            },
            data: ProductData::Cape { parcel },
            render_style: RenderStyle::FilledContour,
            contour_min: 0.0, contour_max: 8000.0, contour_step: 100.0,
            cbar_min: 0.0, cbar_max: 8000.0, cbar_step: 500.0,
            colormap_id: "cape",
            overlays: vec![],
            custom_levels: None, custom_cbar_ticks: None,
        });
    }

    // 0-3km MLCAPE
    products.push(Product {
        name: "0-3km MLCAPE",
        product_name_fn: |_| "0-3 km Mixed-Layer CAPE (J Kg\u{207b}\u{00b9})".into(),
        data: ProductData::ThreeCape { parcel: ParcelType::ML },
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 500.0, contour_step: 5.0,
        cbar_min: 0.0, cbar_max: 500.0, cbar_step: 25.0,
        colormap_id: "three_cape",
        overlays: vec![],
        custom_levels: {
            let mut levels: Vec<f64> = (0..300).step_by(5).map(|x| x as f64).collect();
            levels.extend((300..=500).step_by(20).map(|x| x as f64));
            Some(levels)
        },
        custom_cbar_ticks: {
            let mut ticks: Vec<f64> = (0..300).step_by(25).map(|x| x as f64).collect();
            ticks.extend((300..=500).step_by(100).map(|x| x as f64));
            Some(ticks)
        },
    });

    // Lapse rates
    products.push(Product {
        name: "0-3km Lapse Rate",
        product_name_fn: |_| "0-3km Lapse Rate (\u{00b0}C/km)".into(),
        data: ProductData::LapseRate { bottom_km: 0.0, top_km: 3.0 },
        render_style: RenderStyle::FilledContour,
        contour_min: 2.0, contour_max: 10.0, contour_step: 0.1,
        cbar_min: 2.0, cbar_max: 10.0, cbar_step: 1.0,
        colormap_id: "lapse_rate",
        overlays: vec![],
        custom_levels: None, custom_cbar_ticks: None,
    });

    products.push(Product {
        name: "700-500mb Lapse Rate",
        product_name_fn: |_| "700-500mb Lapse Rate (\u{00b0}C/km)".into(),
        data: ProductData::LapseRatePressure { bottom_mb: 700.0, top_mb: 500.0 },
        render_style: RenderStyle::FilledContour,
        contour_min: 2.0, contour_max: 10.0, contour_step: 0.1,
        cbar_min: 2.0, cbar_max: 10.0, cbar_step: 1.0,
        colormap_id: "lapse_rate",
        overlays: vec![],
        custom_levels: None, custom_cbar_ticks: None,
    });

    // SRH
    for &(top_km, name) in &[(1.0, "0-1km SRH"), (3.0, "0-3km SRH")] {
        products.push(Product {
            name,
            product_name_fn: match top_km as u32 {
                1 => |_| "0-1km Storm Relative Helicity (m\u{00b2}/s\u{00b2})".into(),
                _ => |_| "0-3km Storm Relative Helicity (m\u{00b2}/s\u{00b2})".into(),
            },
            data: ProductData::Srh { top_km },
            render_style: RenderStyle::FilledContour,
            contour_min: 0.0, contour_max: 1000.0, contour_step: 10.0,
            cbar_min: 0.0, cbar_max: 1000.0, cbar_step: 50.0,
            colormap_id: "srh",
            overlays: vec![],
            custom_levels: None, custom_cbar_ticks: None,
        });
    }

    // EHI
    for &(top_km, name) in &[(1.0, "0-1km Energy Helicity Index"), (3.0, "0-3km Energy Helicity Index")] {
        products.push(Product {
            name,
            product_name_fn: match top_km as u32 {
                1 => |_| "0-1km Energy Helicity Index".into(),
                _ => |_| "0-3km Energy Helicity Index".into(),
            },
            data: ProductData::Ehi { top_km },
            render_style: RenderStyle::FilledContour,
            contour_min: 0.0, contour_max: 16.0, contour_step: 0.1,
            cbar_min: 0.0, cbar_max: 16.0, cbar_step: 1.0,
            colormap_id: "ehi",
            overlays: vec![],
            custom_levels: {
                let mut levels: Vec<f64> = Vec::new();
                let mut v = 0.0;
                while v < 2.0 { levels.push(v); v += 0.1; }
                while v <= 16.2 { levels.push(v); v += 0.2; }
                Some(levels)
            },
            custom_cbar_ticks: {
                let mut ticks: Vec<f64> = Vec::new();
                let mut v = 0.0;
                while v < 2.0 { ticks.push(v); v += 0.5; }
                while v <= 16.2 { ticks.push(v); v += 1.0; }
                Some(ticks)
            },
        });
    }

    // STP
    products.push(Product {
        name: "Significant Tornado Parameter",
        product_name_fn: |_| "Significant Tornado Parameter (STP)".into(),
        data: ProductData::Stp,
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 10.0, contour_step: 0.1,
        cbar_min: 0.0, cbar_max: 10.0, cbar_step: 1.0,
        colormap_id: "stp",
        overlays: vec![],
        custom_levels: None, custom_cbar_ticks: None,
    });

    // ============ SIMULATED SATELLITE ============
    products.push(Product {
        name: "Simulated IR Satellite",
        product_name_fn: |_| "Simulated IR Satellite (Brightness Temp \u{00b0}C)".into(),
        data: ProductData::SimulatedIR,
        render_style: RenderStyle::Raster,
        contour_min: -90.0, contour_max: 41.0, contour_step: 1.0,
        cbar_min: -90.0, cbar_max: 41.0, cbar_step: 10.0,
        colormap_id: "sim_ir",
        overlays: vec![],
        custom_levels: None,
        custom_cbar_ticks: {
            let mut ticks: Vec<f64> = (-90..-20).step_by(10).map(|x| x as f64).collect();
            ticks.extend((-20..=40).step_by(20).map(|x| x as f64));
            Some(ticks)
        },
    });

    // ============ MAX OMEGA ============
    products.push(Product {
        name: "Column Max Vertical Velocity",
        product_name_fn: |_| "Column Max Vertical Velocity (Pa/s)".into(),
        data: ProductData::MaxOmega,
        render_style: RenderStyle::Raster,
        contour_min: -20.0, contour_max: 30.0, contour_step: 1.0,
        cbar_min: -20.0, cbar_max: 30.0, cbar_step: 5.0,
        colormap_id: "relvort",
        overlays: vec![],
        custom_levels: None, custom_cbar_ticks: None,
    });

    // ============ PRECIPITATION ============
    products.push(Product {
        name: "Total Precipitation",
        product_name_fn: |_| "Total QPF (in)".into(),
        data: ProductData::Precipitation { range: PrecipRange::Total, unit: PrecipUnit::Inches },
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 15.0, contour_step: 0.05,
        cbar_min: 0.0, cbar_max: 15.0, cbar_step: 1.0,
        colormap_id: "precip_in",
        overlays: vec![],
        custom_levels: {
            let mut levels = vec![0.0, 0.01, 0.03, 0.05, 0.075];
            let mut v = 0.1; while v < 1.0 { levels.push(v); v += 0.05; }
            v = 1.0; while v < 2.0 { levels.push(v); v += 0.1; }
            v = 2.0; while v < 4.0 { levels.push(v); v += 0.25; }
            levels.extend_from_slice(&[4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 15.0]);
            Some(levels)
        },
        custom_cbar_ticks: Some(vec![
            0.01, 0.05, 0.1, 0.3, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 9.0, 15.0,
        ]),
    });

    products.push(Product {
        name: "1h Precipitation",
        product_name_fn: |_| "1h QPF (in)".into(),
        data: ProductData::Precipitation { range: PrecipRange::OneHour, unit: PrecipUnit::Inches },
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 15.0, contour_step: 0.05,
        cbar_min: 0.0, cbar_max: 15.0, cbar_step: 1.0,
        colormap_id: "precip_in",
        overlays: vec![],
        custom_levels: {
            let mut levels = vec![0.0, 0.01, 0.03, 0.05, 0.075];
            let mut v = 0.1; while v < 1.0 { levels.push(v); v += 0.05; }
            v = 1.0; while v < 2.0 { levels.push(v); v += 0.1; }
            v = 2.0; while v < 4.0 { levels.push(v); v += 0.25; }
            levels.extend_from_slice(&[4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 15.0]);
            Some(levels)
        },
        custom_cbar_ticks: Some(vec![
            0.01, 0.05, 0.1, 0.3, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 9.0, 15.0,
        ]),
    });

    products.push(Product {
        name: "6h Precipitation",
        product_name_fn: |_| "6h QPF (in)".into(),
        data: ProductData::Precipitation { range: PrecipRange::SixHour, unit: PrecipUnit::Inches },
        render_style: RenderStyle::FilledContour,
        contour_min: 0.0, contour_max: 15.0, contour_step: 0.05,
        cbar_min: 0.0, cbar_max: 15.0, cbar_step: 1.0,
        colormap_id: "precip_in",
        overlays: vec![],
        custom_levels: {
            let mut levels = vec![0.0, 0.01, 0.03, 0.05, 0.075];
            let mut v = 0.1; while v < 1.0 { levels.push(v); v += 0.05; }
            v = 1.0; while v < 2.0 { levels.push(v); v += 0.1; }
            v = 2.0; while v < 4.0 { levels.push(v); v += 0.25; }
            levels.extend_from_slice(&[4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 15.0]);
            Some(levels)
        },
        custom_cbar_ticks: Some(vec![
            0.01, 0.05, 0.1, 0.3, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 9.0, 15.0,
        ]),
    });

    products
}
