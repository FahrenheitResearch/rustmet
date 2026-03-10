use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use numpy::{PyArray1, PyArray2};
use rustmet_core::{grib2, download, models};

// ──────────────────────────────────────────────────────────
// GribMessage — a single decoded GRIB2 field
// ──────────────────────────────────────────────────────────

#[pyclass]
#[derive(Clone)]
pub struct GribMessage {
    #[pyo3(get)]
    pub variable: String,
    #[pyo3(get)]
    pub level: String,
    #[pyo3(get)]
    pub level_type: String,
    #[pyo3(get)]
    pub level_value: f64,
    #[pyo3(get)]
    pub units: String,
    #[pyo3(get)]
    pub discipline: u8,
    #[pyo3(get)]
    pub parameter_category: u8,
    #[pyo3(get)]
    pub parameter_number: u8,
    #[pyo3(get)]
    pub forecast_time: u32,
    #[pyo3(get)]
    pub nx: u32,
    #[pyo3(get)]
    pub ny: u32,
    #[pyo3(get)]
    pub reference_time: String,
    // Grid info for lat/lon generation
    grid: grib2::GridDefinition,
    // Keep raw message for unpacking
    raw_msg: grib2::Grib2Message,
}

#[pymethods]
impl GribMessage {
    /// Unpack the data values as a 1D numpy array (f64).
    fn values<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let vals = grib2::unpack_message(&self.raw_msg)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        Ok(PyArray1::from_vec(py, vals))
    }

    /// Unpack as a 2D numpy array shaped (ny, nx).
    fn values_2d<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let vals = grib2::unpack_message(&self.raw_msg)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        let ny = self.ny as usize;
        let nx = self.nx as usize;
        if vals.len() != ny * nx {
            return Err(pyo3::exceptions::PyValueError::new_err(
                format!("Data has {} values but grid is {}x{} = {}", vals.len(), ny, nx, ny * nx)
            ));
        }
        Ok(PyArray2::from_vec2(py, &vals.chunks(nx).map(|c| c.to_vec()).collect::<Vec<_>>())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}", e)))?)
    }

    /// Get latitude array (1D or 2D depending on grid type).
    fn lats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let (lats, _) = grib2::grid_latlon(&self.grid);
        Ok(PyArray1::from_vec(py, lats))
    }

    /// Get longitude array (1D or 2D depending on grid type).
    fn lons<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let (_, lons) = grib2::grid_latlon(&self.grid);
        Ok(PyArray1::from_vec(py, lons))
    }

    fn __repr__(&self) -> String {
        format!("<GribMessage {} {} ({}x{})>", self.variable, self.level, self.nx, self.ny)
    }
}

fn msg_to_py(msg: &grib2::Grib2Message) -> GribMessage {
    let var_name = grib2::parameter_name(
        msg.discipline,
        msg.product.parameter_category,
        msg.product.parameter_number,
    );
    let var_units = grib2::parameter_units(
        msg.discipline,
        msg.product.parameter_category,
        msg.product.parameter_number,
    );
    let level_name = grib2::level_name(msg.product.level_type);

    GribMessage {
        variable: var_name.to_string(),
        level: format!("{} {}", msg.product.level_value, level_name),
        level_type: level_name.to_string(),
        level_value: msg.product.level_value,
        units: var_units.to_string(),
        discipline: msg.discipline,
        parameter_category: msg.product.parameter_category,
        parameter_number: msg.product.parameter_number,
        forecast_time: msg.product.forecast_time,
        nx: msg.grid.nx,
        ny: msg.grid.ny,
        reference_time: msg.reference_time.to_string(),
        grid: msg.grid.clone(),
        raw_msg: msg.clone(),
    }
}

// ──────────────────────────────────────────────────────────
// GribFile — parsed GRIB2 file with multiple messages
// ──────────────────────────────────────────────────────────

#[pyclass]
pub struct GribFile {
    messages: Vec<GribMessage>,
}

#[pymethods]
impl GribFile {
    /// Parse a GRIB2 file from a filesystem path.
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let grib = grib2::Grib2File::from_path(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))?;
        let messages: Vec<GribMessage> = grib.messages.iter().map(msg_to_py).collect();
        Ok(GribFile { messages })
    }

    /// Parse GRIB2 data from raw bytes.
    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        let grib = grib2::Grib2File::from_bytes(data)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        let messages: Vec<GribMessage> = grib.messages.iter().map(msg_to_py).collect();
        Ok(GribFile { messages })
    }

    /// Number of messages in the file.
    #[getter]
    fn num_messages(&self) -> usize {
        self.messages.len()
    }

    /// Get all messages as a list.
    #[getter]
    fn messages(&self) -> Vec<GribMessage> {
        self.messages.clone()
    }

    /// Find a message by variable name and optional level.
    /// Example: file.find("TMP", "2 m above ground")
    #[pyo3(signature = (variable, level=None))]
    fn find(&self, variable: &str, level: Option<&str>) -> Option<GribMessage> {
        let var_lower = variable.to_lowercase();
        self.messages.iter().find(|m| {
            let name_match = m.variable.to_lowercase().contains(&var_lower);
            if let Some(lev) = level {
                name_match && m.level.to_lowercase().contains(&lev.to_lowercase())
            } else {
                name_match
            }
        }).cloned()
    }

    /// Get a summary of all messages.
    fn inventory(&self) -> Vec<String> {
        self.messages.iter()
            .map(|m| format!("{}:{} [{}] ({}x{})", m.variable, m.level, m.units, m.nx, m.ny))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("<GribFile with {} messages>", self.messages.len())
    }

    fn __len__(&self) -> usize {
        self.messages.len()
    }
}

// ──────────────────────────────────────────────────────────
// Client — HTTP download client for operational model data
// ──────────────────────────────────────────────────────────

#[pyclass]
pub struct Client {
    inner: download::DownloadClient,
    cache: download::Cache,
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (cache_dir=None))]
    fn new(cache_dir: Option<&str>) -> PyResult<Self> {
        let client = download::DownloadClient::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        let cache = if let Some(dir) = cache_dir {
            download::Cache::with_dir(std::path::PathBuf::from(dir))
        } else {
            download::Cache::new()
        };
        Ok(Client { inner: client, cache })
    }

    /// Download GRIB2 data for specific variables from a model run.
    ///
    /// Args:
    ///     model: Model name ("hrrr", "gfs", "nam", "rap")
    ///     run: Run time as "YYYY-MM-DD/HHz" (e.g. "2026-03-09/00z")
    ///     fhour: Forecast hour (default 0)
    ///     product: Model product ("prs", "sfc", "nat", "subh") — default "prs"
    ///     vars: List of variable patterns like ["TMP:2 m above ground", "CAPE:surface"]
    ///           If None, downloads all variables.
    ///
    /// Returns:
    ///     GribFile with decoded messages
    #[pyo3(signature = (model, run, fhour=0, product="prs", vars=None))]
    fn fetch(
        &self,
        model: &str,
        run: &str,
        fhour: u32,
        product: &str,
        vars: Option<Vec<String>>,
    ) -> PyResult<GribFile> {
        let (date, hour) = parse_run(run)?;
        let product_key = normalize_product(product);

        // Build URLs
        let (idx_url, grib_url) = model_urls(model, &date, hour, &product_key, fhour)?;

        // Fetch index
        let idx_text = self.inner.get_text(&idx_url)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(
                format!("Failed to fetch index: {}", e)
            ))?;
        let idx_entries = download::parse_idx(&idx_text);

        let data = if let Some(var_patterns) = &vars {
            // Selective download via byte ranges
            let mut selected: Vec<&download::IdxEntry> = Vec::new();
            for pat in var_patterns {
                let matches = download::find_entries(&idx_entries, pat);
                for m in matches {
                    if !selected.iter().any(|e| e.byte_offset == m.byte_offset) {
                        selected.push(m);
                    }
                }
            }

            if selected.is_empty() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    format!("No matching variables found for patterns: {:?}", var_patterns)
                ));
            }

            let ranges = download::byte_ranges(&idx_entries, &selected);
            self.inner.get_ranges(&grib_url, &ranges)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(
                    format!("Download failed: {}", e)
                ))?
        } else {
            // Download entire file
            self.inner.get_bytes(&grib_url)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(
                    format!("Download failed: {}", e)
                ))?
        };

        // Parse GRIB2
        let grib = grib2::Grib2File::from_bytes(&data)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(
                format!("GRIB2 parse error: {}", e)
            ))?;
        let messages: Vec<GribMessage> = grib.messages.iter().map(msg_to_py).collect();
        Ok(GribFile { messages })
    }

    /// Get the URL for a model's GRIB2 file.
    #[pyo3(signature = (model, run, fhour=0, product="prs"))]
    fn url(&self, model: &str, run: &str, fhour: u32, product: &str) -> PyResult<String> {
        let (date, hour) = parse_run(run)?;
        let product_key = normalize_product(product);
        let (_, grib_url) = model_urls(model, &date, hour, &product_key, fhour)?;
        Ok(grib_url)
    }

    /// Download raw bytes from a URL.
    fn get_bytes<'py>(&self, py: Python<'py>, url: &str) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        let data = self.inner.get_bytes(url)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))?;
        Ok(pyo3::types::PyBytes::new(py, &data))
    }

    /// Fetch and parse a .idx index file, return list of dicts.
    #[pyo3(signature = (model, run, fhour, product=None))]
    fn inventory<'py>(&self, py: Python<'py>, model: &str, run: &str, fhour: u32, product: Option<&str>) -> PyResult<Bound<'py, PyList>> {
        let (date, hour) = parse_run(run)?;
        let product_key = normalize_product(product.unwrap_or("prs"));
        let (idx_url, _) = model_urls(model, &date, hour, &product_key, fhour)?;

        let idx_text = self.inner.get_text(&idx_url)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))?;
        let entries = download::parse_idx(&idx_text);

        let list = PyList::empty(py);
        for entry in &entries {
            let dict = PyDict::new(py);
            dict.set_item("msg_num", entry.msg_num)?;
            dict.set_item("byte_offset", entry.byte_offset)?;
            dict.set_item("date", &entry.date)?;
            dict.set_item("variable", &entry.variable)?;
            dict.set_item("level", &entry.level)?;
            dict.set_item("forecast", &entry.forecast)?;
            list.append(dict)?;
        }
        Ok(list)
    }

    fn __repr__(&self) -> String {
        "<rustmet.Client>".to_string()
    }
}

// ──────────────────────────────────────────────────────────
// Module-level convenience functions
// ──────────────────────────────────────────────────────────

/// Quick fetch: download and decode GRIB2 data in one call.
///
/// Example:
///     data = rustmet.fetch("hrrr", "2026-03-09/00z", vars=["TMP:2 m above ground"])
#[pyfunction]
#[pyo3(signature = (model, run, fhour=0, product="prs", vars=None))]
fn fetch(
    model: &str,
    run: &str,
    fhour: u32,
    product: &str,
    vars: Option<Vec<String>>,
) -> PyResult<GribFile> {
    let client = Client::new(None)?;
    client.fetch(model, run, fhour, product, vars)
}

/// Parse a local GRIB2 file.
///
/// Example:
///     grib = rustmet.open("path/to/file.grib2")
#[pyfunction]
fn open(path: &str) -> PyResult<GribFile> {
    GribFile::open(path)
}

/// List available products with their variable patterns.
#[pyfunction]
fn products(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let list = PyList::empty(py);
    for p in rustmet_core::products::GRIB_PRODUCTS {
        let dict = PyDict::new(py);
        dict.set_item("name", p.name)?;
        dict.set_item("vars", p.grib_vars.to_vec())?;
        dict.set_item("colormap", p.colormap)?;
        dict.set_item("range", (p.range.0, p.range.1))?;
        dict.set_item("units", p.units)?;
        dict.set_item("description", p.description)?;
        list.append(dict)?;
    }
    Ok(list)
}

// ──────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────

fn parse_run(s: &str) -> PyResult<(String, u32)> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Run time must be 'YYYY-MM-DD/HHz' format"
        ));
    }
    let date_str = parts[0].replace('-', "");
    if date_str.len() != 8 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            format!("Invalid date '{}', expected YYYY-MM-DD or YYYYMMDD", parts[0])
        ));
    }
    let hour: u32 = parts.get(1)
        .map(|h| h.trim_end_matches('z').trim_end_matches('Z').parse().unwrap_or(0))
        .unwrap_or(0);
    Ok((date_str, hour))
}

fn normalize_product(product: &str) -> String {
    match product.to_lowercase().as_str() {
        "prs" | "pressure" | "wrfprs" => "wrfprs".to_string(),
        "sfc" | "surface" | "wrfsfc" => "wrfsfc".to_string(),
        "nat" | "native" | "wrfnat" => "wrfnat".to_string(),
        "subh" | "subhourly" | "wrfsubh" => "wrfsubh".to_string(),
        other => other.to_string(),
    }
}

fn model_urls(model: &str, date: &str, hour: u32, product: &str, fhour: u32)
    -> PyResult<(String, String)>
{
    match model.to_lowercase().as_str() {
        "hrrr" => {
            let idx = models::HrrrConfig::idx_url(date, hour, product, fhour);
            let grib = models::HrrrConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "gfs" => {
            let idx = models::GfsConfig::idx_url(date, hour, fhour);
            let grib = models::GfsConfig::aws_url(date, hour, fhour);
            Ok((idx, grib))
        }
        "nam" => {
            let idx = models::NamConfig::idx_url(date, hour, fhour);
            let grib = models::NamConfig::aws_url(date, hour, fhour);
            Ok((idx, grib))
        }
        "rap" => {
            let idx = models::RapConfig::idx_url(date, hour, fhour);
            let grib = models::RapConfig::aws_url(date, hour, fhour);
            Ok((idx, grib))
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            format!("Unknown model '{}'. Supported: hrrr, gfs, nam, rap", model)
        )),
    }
}

// ──────────────────────────────────────────────────────────
// Module definition
// ──────────────────────────────────────────────────────────

#[pymodule]
fn _rustmet(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<GribFile>()?;
    m.add_class::<GribMessage>()?;
    m.add_class::<Client>()?;
    m.add_function(wrap_pyfunction!(fetch, m)?)?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(products, m)?)?;
    m.add("__version__", "0.1.0")?;
    Ok(())
}
