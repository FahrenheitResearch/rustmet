//! rustmet-wasm — WebAssembly bindings for the rustmet GRIB2 parser.
//!
//! Provides a JavaScript-friendly API for parsing GRIB2 files in the browser.
//! GRIB2 data is passed in as a `Uint8Array`, and decoded values are returned
//! as typed arrays or JSON strings.
//!
//! Build with: `wasm-pack build --target web`

use wasm_bindgen::prelude::*;
use rustmet_core::grib2;

/// A GRIB2 file parsed in WebAssembly memory.
///
/// Construct from a `Uint8Array` containing raw GRIB2 bytes,
/// then query individual messages for their data and metadata.
#[wasm_bindgen]
pub struct WasmGribFile {
    inner: grib2::Grib2File,
}

#[wasm_bindgen]
impl WasmGribFile {
    /// Parse a GRIB2 file from raw bytes (pass a `Uint8Array` from JavaScript).
    ///
    /// ```js
    /// const response = await fetch('data.grib2');
    /// const bytes = new Uint8Array(await response.arrayBuffer());
    /// const grib = new WasmGribFile(bytes);
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<WasmGribFile, JsError> {
        let inner = grib2::Grib2File::from_bytes(data)
            .map_err(|e| JsError::new(&format!("{}", e)))?;
        Ok(WasmGribFile { inner })
    }

    /// Number of messages (fields) in this GRIB2 file.
    #[wasm_bindgen(js_name = "messageCount")]
    pub fn message_count(&self) -> usize {
        self.inner.messages.len()
    }

    /// Decode the data values for message at `index`, returned as `Float64Array`.
    ///
    /// Values are fully unpacked and scaled (e.g., temperatures in Kelvin).
    /// Grid points with missing data are represented as `NaN`.
    pub fn values(&self, index: usize) -> Result<Vec<f64>, JsError> {
        let msg = self.get_message(index)?;
        grib2::unpack_message(msg)
            .map_err(|e| JsError::new(&format!("{}", e)))
    }

    /// Get the latitude of every grid point for message at `index`.
    ///
    /// Returns a `Float64Array` of length `nx * ny`, in row-major order.
    pub fn lats(&self, index: usize) -> Result<Vec<f64>, JsError> {
        let msg = self.get_message(index)?;
        let (lats, _) = grib2::grid_latlon(&msg.grid);
        Ok(lats)
    }

    /// Get the longitude of every grid point for message at `index`.
    ///
    /// Returns a `Float64Array` of length `nx * ny`, in row-major order.
    pub fn lons(&self, index: usize) -> Result<Vec<f64>, JsError> {
        let msg = self.get_message(index)?;
        let (_, lons) = grib2::grid_latlon(&msg.grid);
        Ok(lons)
    }

    /// Get metadata for a single message as a JSON string.
    ///
    /// Includes parameter name, units, level, grid dimensions, and reference time.
    #[wasm_bindgen(js_name = "messageInfo")]
    pub fn message_info(&self, index: usize) -> Result<String, JsError> {
        let msg = self.get_message(index)?;
        let name = grib2::parameter_name(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);
        let units = grib2::parameter_units(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);
        let level = grib2::level_name(msg.product.level_type);

        let info = serde_json::json!({
            "index": index,
            "parameter": name,
            "units": units,
            "level": level,
            "discipline": msg.discipline,
            "category": msg.product.parameter_category,
            "number": msg.product.parameter_number,
            "forecast_time": msg.product.forecast_time,
            "reference_time": msg.reference_time.to_string(),
            "grid_template": msg.grid.template,
            "nx": msg.grid.nx,
            "ny": msg.grid.ny,
            "lat1": msg.grid.lat1,
            "lon1": msg.grid.lon1,
            "lat2": msg.grid.lat2,
            "lon2": msg.grid.lon2,
            "data_template": msg.data_rep.template,
        });

        serde_json::to_string(&info)
            .map_err(|e| JsError::new(&format!("{}", e)))
    }

    /// Get a summary inventory of all messages as a JSON array.
    ///
    /// Each element contains the parameter name, units, level, and grid size.
    pub fn inventory(&self) -> String {
        let entries: Vec<serde_json::Value> = self.inner.messages.iter().enumerate().map(|(i, msg)| {
            let name = grib2::parameter_name(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);
            let units = grib2::parameter_units(msg.discipline, msg.product.parameter_category, msg.product.parameter_number);
            let level = grib2::level_name(msg.product.level_type);

            serde_json::json!({
                "index": i,
                "parameter": name,
                "units": units,
                "level": level,
                "nx": msg.grid.nx,
                "ny": msg.grid.ny,
                "forecast_time": msg.product.forecast_time,
                "reference_time": msg.reference_time.to_string(),
            })
        }).collect();

        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    }
}

impl WasmGribFile {
    fn get_message(&self, index: usize) -> Result<&grib2::Grib2Message, JsError> {
        self.inner.messages.get(index)
            .ok_or_else(|| JsError::new(&format!(
                "Message index {} out of range (file has {} messages)",
                index, self.inner.messages.len()
            )))
    }
}

/// Render a GRIB2 message to RGBA pixels for display in a canvas.
///
/// Maps data values to colors using a linear scale between `vmin` and `vmax`.
/// Returns a `Uint8Array` of length `nx * ny * 4` (RGBA).
///
/// Supported colormaps: `"viridis"`, `"turbo"`, `"inferno"`, `"magma"`,
/// `"plasma"`, `"coolwarm"`, `"grayscale"`.
///
/// NaN values are rendered as fully transparent (alpha = 0).
#[wasm_bindgen(js_name = "renderToRgba")]
pub fn render_to_rgba(
    grib: &WasmGribFile,
    index: usize,
    colormap: &str,
    vmin: f64,
    vmax: f64,
) -> Result<Vec<u8>, JsError> {
    let msg = grib.get_message(index)?;
    let values = grib2::unpack_message(msg)
        .map_err(|e| JsError::new(&format!("{}", e)))?;

    let nx = msg.grid.nx as usize;
    let ny = msg.grid.ny as usize;
    let n = nx * ny;

    let mut rgba = vec![0u8; n * 4];
    let range = vmax - vmin;
    let inv_range = if range.abs() < 1e-15 { 0.0 } else { 1.0 / range };

    for i in 0..values.len().min(n) {
        let v = values[i];
        if v.is_nan() {
            // Transparent pixel
            continue;
        }

        let t = ((v - vmin) * inv_range).clamp(0.0, 1.0);
        let (r, g, b) = colormap_lookup(colormap, t);

        let offset = i * 4;
        rgba[offset] = r;
        rgba[offset + 1] = g;
        rgba[offset + 2] = b;
        rgba[offset + 3] = 255;
    }

    Ok(rgba)
}

/// Map a normalized value [0, 1] to an RGB color using the specified colormap.
fn colormap_lookup(name: &str, t: f64) -> (u8, u8, u8) {
    match name {
        "turbo" => turbo(t),
        "inferno" => inferno(t),
        "coolwarm" => coolwarm(t),
        "grayscale" | "gray" => {
            let v = (t * 255.0) as u8;
            (v, v, v)
        }
        // Default to viridis
        _ => viridis(t),
    }
}

/// Viridis colormap (simplified 5-stop approximation).
fn viridis(t: f64) -> (u8, u8, u8) {
    // Key stops: dark purple -> blue -> teal -> green -> yellow
    let colors: [(f64, f64, f64); 5] = [
        (0.267, 0.004, 0.329),
        (0.282, 0.141, 0.458),
        (0.127, 0.566, 0.551),
        (0.369, 0.789, 0.383),
        (0.993, 0.906, 0.144),
    ];
    interpolate_colormap(&colors, t)
}

/// Turbo colormap (simplified 6-stop approximation).
fn turbo(t: f64) -> (u8, u8, u8) {
    let colors: [(f64, f64, f64); 6] = [
        (0.190, 0.072, 0.232),
        (0.085, 0.532, 0.872),
        (0.163, 0.835, 0.600),
        (0.565, 0.940, 0.264),
        (0.928, 0.644, 0.103),
        (0.640, 0.120, 0.023),
    ];
    interpolate_colormap(&colors, t)
}

/// Inferno colormap (simplified 5-stop approximation).
fn inferno(t: f64) -> (u8, u8, u8) {
    let colors: [(f64, f64, f64); 5] = [
        (0.001, 0.000, 0.014),
        (0.329, 0.059, 0.404),
        (0.716, 0.215, 0.330),
        (0.978, 0.557, 0.035),
        (0.988, 0.998, 0.645),
    ];
    interpolate_colormap(&colors, t)
}

/// Cool-to-warm diverging colormap.
fn coolwarm(t: f64) -> (u8, u8, u8) {
    let colors: [(f64, f64, f64); 5] = [
        (0.230, 0.299, 0.754),
        (0.552, 0.691, 0.996),
        (0.866, 0.866, 0.866),
        (0.956, 0.604, 0.486),
        (0.706, 0.016, 0.150),
    ];
    interpolate_colormap(&colors, t)
}

/// Linearly interpolate through a list of RGB color stops.
fn interpolate_colormap(colors: &[(f64, f64, f64)], t: f64) -> (u8, u8, u8) {
    let n = colors.len();
    if n == 0 {
        return (0, 0, 0);
    }
    if n == 1 || t <= 0.0 {
        let (r, g, b) = colors[0];
        return ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
    }
    if t >= 1.0 {
        let (r, g, b) = colors[n - 1];
        return ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
    }

    let segment = t * (n - 1) as f64;
    let idx = segment as usize;
    let frac = segment - idx as f64;

    let (r0, g0, b0) = colors[idx];
    let (r1, g1, b1) = colors[(idx + 1).min(n - 1)];

    let r = r0 + frac * (r1 - r0);
    let g = g0 + frac * (g1 - g0);
    let b = b0 + frac * (b1 - b0);

    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Initialize panic hook for better error messages in the browser console.
///
/// Call this once from JavaScript before using any other functions.
#[wasm_bindgen(js_name = "initPanicHook")]
pub fn init_panic_hook() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}
