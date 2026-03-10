use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use numpy::{PyArray1, PyArray2, PyReadonlyArray1};
use rayon::prelude::*;
use rustmet_core::{grib2, download, models, metfuncs, composite, render};

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
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(PyArray1::from_vec(py, vals))
    }

    /// Unpack as a 2D numpy array shaped (ny, nx).
    fn values_2d<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let vals = grib2::unpack_message(&self.raw_msg)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
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
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        let messages: Vec<GribMessage> = grib.messages.iter().map(msg_to_py).collect();
        Ok(GribFile { messages })
    }

    /// Parse GRIB2 data from raw bytes.
    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        let grib = grib2::Grib2File::from_bytes(data)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
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

    /// Search messages by human-readable query (fuzzy match).
    ///
    /// Supports patterns like:
    ///   - "temperature" -- any temperature variable
    ///   - "temperature 2m" -- TMP at 2m above ground
    ///   - "wind 10m" -- UGRD/VGRD at 10m
    ///   - "cape" -- CAPE
    ///   - "500mb height" -- HGT at 500 mb
    ///   - "rh" -- Relative Humidity (alias)
    ///
    /// Returns a list of GribMessage objects ranked by relevance.
    fn search(&self, query: &str) -> Vec<GribMessage> {
        let raw_msgs: Vec<grib2::Grib2Message> = self.messages.iter().map(|m| m.raw_msg.clone()).collect();
        let results = grib2::search_messages(&raw_msgs, query);
        results.iter().map(|m| msg_to_py(m)).collect()
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


// Internal fetch helpers (free functions for thread safety with rayon)

fn fetch_single_impl(
    client: &download::DownloadClient, model: &str, run: &str,
    fhour: u32, product: &str, vars: &Option<Vec<String>>,
) -> PyResult<GribFile> {
    let (date, hour) = resolve_run(client, model, run)?;
    let product_key = normalize_product(product);
    let (idx_url, grib_url) = model_urls(model, &date, hour, &product_key, fhour)?;

    let idx_text = client.get_text(&idx_url)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Failed to fetch index for f{:03}: {}", fhour, e)))?;
    let idx_entries = download::parse_idx(&idx_text);

    let data = if let Some(var_patterns) = vars {
        let mut selected: Vec<&download::IdxEntry> = Vec::new();
        for pat in var_patterns {
            for m in download::find_entries(&idx_entries, pat) {
                if !selected.iter().any(|e| e.byte_offset == m.byte_offset) { selected.push(m); }
            }
        }
        if selected.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                format!("No matching variables found for patterns: {:?}", var_patterns)));
        }
        let ranges = download::byte_ranges(&idx_entries, &selected);
        client.get_ranges(&grib_url, &ranges)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Download failed for f{:03}: {}", fhour, e)))?
    } else {
        client.get_bytes(&grib_url)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Download failed for f{:03}: {}", fhour, e)))?
    };

    let grib = grib2::Grib2File::from_bytes(&data)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("GRIB2 parse error for f{:03}: {}", fhour, e)))?;
    Ok(GribFile { messages: grib.messages.iter().map(msg_to_py).collect() })
}

fn fetch_many_impl(
    client: &download::DownloadClient, model: &str, run: &str,
    fhours: &[u32], product: &str, vars: &Option<Vec<String>>,
) -> PyResult<Vec<GribFile>> {
    let (date, hour) = resolve_run(client, model, run)?;
    let product_key = normalize_product(product);
    let model_lower = model.to_lowercase();

    let results: Vec<Result<GribFile, String>> = fhours.par_iter().map(|&fh| {
        let (idx_url, grib_url) = model_urls_inner(&model_lower, &date, hour, &product_key, fh)?;
        let idx_text = client.get_text(&idx_url)
            .map_err(|e| format!("Failed to fetch index for f{:03}: {}", fh, e))?;
        let idx_entries = download::parse_idx(&idx_text);
        let data = if let Some(var_patterns) = vars {
            let mut selected: Vec<&download::IdxEntry> = Vec::new();
            for pat in var_patterns {
                for m in download::find_entries(&idx_entries, pat) {
                    if !selected.iter().any(|e| e.byte_offset == m.byte_offset) { selected.push(m); }
                }
            }
            if selected.is_empty() {
                return Err(format!("No matching variables found for f{:03}: {:?}", fh, var_patterns));
            }
            let ranges = download::byte_ranges(&idx_entries, &selected);
            client.get_ranges(&grib_url, &ranges)
                .map_err(|e| format!("Download failed for f{:03}: {}", fh, e))?
        } else {
            client.get_bytes(&grib_url)
                .map_err(|e| format!("Download failed for f{:03}: {}", fh, e))?
        };
        let grib = grib2::Grib2File::from_bytes(&data)
            .map_err(|e| format!("GRIB2 parse error for f{:03}: {}", fh, e))?;
        Ok(GribFile { messages: grib.messages.iter().map(msg_to_py).collect() })
    }).collect();

    results.into_iter()
        .map(|r| r.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e)))
        .collect()
}

#[pyclass]
pub struct Client {
    inner: download::DownloadClient,
}

#[pymethods]
impl Client {
    /// Create a new download client with disk caching enabled.
    ///
    /// Args:
    ///     cache_dir: Optional path for the download cache. If provided,
    ///                caching uses that directory. If omitted/None, caching
    ///                is enabled at the platform default location
    ///                (~/.cache/rustmet/ on Linux/macOS,
    ///                 %LOCALAPPDATA%/rustmet/cache/ on Windows).
    #[new]
    #[pyo3(signature = (cache_dir=None))]
    fn new(cache_dir: Option<&str>) -> PyResult<Self> {
        let client = download::DownloadClient::new_with_cache(cache_dir)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Client { inner: client })
    }

    /// Clear all cached downloads.
    fn clear_cache(&self) {
        if let Some(cache) = self.inner.cache() {
            cache.clear();
        }
    }

    /// Return the total size of the download cache in bytes.
    fn cache_size(&self) -> u64 {
        self.inner.cache().map_or(0, |c| c.size())
    }

    /// Return the cache directory path, or None if caching is disabled.
    fn cache_dir(&self) -> Option<String> {
        self.inner.cache().map(|c| c.dir().to_string_lossy().into_owned())
    }

    /// Download GRIB2 data for specific variables from a model run.
    ///
    /// Args:
    ///     model: Model name ("hrrr", "gfs", "nam", "rap")
    ///     run: Run time as "YYYY-MM-DD/HHz" (e.g. "2026-03-09/00z")
    ///     fhour: Forecast hour (int) or list of forecast hours (list[int]).
    ///            When a list is given, downloads are parallelized and a list of
    ///            GribFile objects is returned. Default 0.
    ///     product: Model product ("prs", "sfc", "nat", "subh") — default "prs"
    ///     vars: List of variable patterns like ["TMP:2 m above ground", "CAPE:surface"]
    ///           If None, downloads all variables.
    ///
    /// Returns:
    ///     GribFile when fhour is a single int, list[GribFile] when fhour is a list.
    #[pyo3(signature = (model, run, fhour=None, product="prs", vars=None))]
    fn fetch(
        &self,
        py: Python<'_>,
        model: &str,
        run: &str,
        fhour: Option<PyObject>,
        product: &str,
        vars: Option<Vec<String>>,
    ) -> PyResult<PyObject> {
        let fhour_obj = match &fhour {
            Some(obj) => obj.clone_ref(py),
            None => 0i64.into_pyobject(py).unwrap().into_any().unbind(),
        };
        let is_list = fhour_obj.bind(py).is_instance_of::<PyList>();
        let fhours = parse_fhour(py, &fhour_obj)?;

        if fhours.len() == 1 && !is_list {
            // Single forecast hour — return a single GribFile (backward compatible)
            let result = fetch_single_impl(&self.inner, model, run, fhours[0], product, &vars)?;
            Ok(result.into_pyobject(py)?.into_any().unbind())
        } else {
            // Multiple forecast hours — download in parallel, return list
            let results = fetch_many_impl(&self.inner, model, run, &fhours, product, &vars)?;
            Ok(results.into_pyobject(py)?.into_any().unbind())
        }
    }

    /// Get the URL for a model's GRIB2 file.
    #[pyo3(signature = (model, run, fhour=0, product="prs"))]
    fn url(&self, model: &str, run: &str, fhour: u32, product: &str) -> PyResult<String> {
        let (date, hour) = resolve_run(&self.inner, model, run)?;
        let product_key = normalize_product(product);
        let (_, grib_url) = model_urls(model, &date, hour, &product_key, fhour)?;
        Ok(grib_url)
    }

    /// Download raw bytes from a URL.
    fn get_bytes<'py>(&self, py: Python<'py>, url: &str) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        let data = self.inner.get_bytes(url)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        Ok(pyo3::types::PyBytes::new(py, &data))
    }

    /// Fetch and parse a .idx index file, return list of dicts.
    #[pyo3(signature = (model, run, fhour, product=None))]
    fn inventory<'py>(&self, py: Python<'py>, model: &str, run: &str, fhour: u32, product: Option<&str>) -> PyResult<Bound<'py, PyList>> {
        let (date, hour) = resolve_run(&self.inner, model, run)?;
        let product_key = normalize_product(product.unwrap_or("prs"));
        let (idx_url, _) = model_urls(model, &date, hour, &product_key, fhour)?;

        let idx_text = self.inner.get_text(&idx_url)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
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
/// fhour can be a single int or a list of ints. When a list is given,
/// downloads are parallelized and a list of GribFile objects is returned.
///
/// Example:
///     data = rustmet.fetch("hrrr", "2026-03-09/00z", vars=["TMP:2 m above ground"])
///     series = rustmet.fetch("hrrr", "2026-03-09/00z", fhour=[0,1,2,3])
#[pyfunction]
#[pyo3(signature = (model, run, fhour=None, product="prs", vars=None))]
fn fetch(
    py: Python<'_>,
    model: &str,
    run: &str,
    fhour: Option<PyObject>,
    product: &str,
    vars: Option<Vec<String>>,
) -> PyResult<PyObject> {
    let client = Client::new(None)?;
    client.fetch(py, model, run, fhour, product, vars)
}

/// Parse a local GRIB2 file.
///
/// Example:
///     grib = rustmet.open("path/to/file.grib2")
#[pyfunction]
fn open(path: &str) -> PyResult<GribFile> {
    GribFile::open(path)
}

/// Search a GribFile's messages by human-readable query.
///
/// Example:
///     results = rustmet.search(grib, "temperature 2m")
///     results = rustmet.search(grib, "500mb height")
#[pyfunction]
fn search(grib: &GribFile, query: &str) -> Vec<GribMessage> {
    grib.search(query)
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

/// List all available weather models with metadata.
///
/// Returns a list of dicts with keys: name, description, grid_dx, grid_dy, grid_nx, grid_ny, aliases.
#[pyfunction]
fn available_models(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let list = PyList::empty(py);

    let model_info: &[(&str, &str, f64, f64, u32, u32, &[&str])] = &[
        ("hrrr", "High-Resolution Rapid Refresh (3km CONUS, hourly)", 3000.0, 3000.0, 1799, 1059, &[]),
        ("gfs", "Global Forecast System (0.25deg global, 6-hourly)", 0.25, 0.25, 1440, 721, &[]),
        ("nam", "North American Mesoscale (12km CONUS, 6-hourly)", 12190.58, 12190.58, 614, 428, &[]),
        ("rap", "Rapid Refresh (13km North America, hourly)", 13545.09, 13545.09, 451, 337, &[]),
        ("ecmwf", "ECMWF IFS Open Data (0.25deg global, 00/12z)", 0.25, 0.25, 1440, 721, &["ifs"]),
        ("nbm", "National Blend of Models (2.5km CONUS, hourly)", 2539.703, 2539.703, 2345, 1597, &["blend"]),
        ("rrfs", "Rapid Refresh Forecast System (3km CONUS)", 3000.0, 3000.0, 1799, 1059, &[]),
        ("rtma", "Real-Time Mesoscale Analysis (2.5km CONUS, analysis only)", 2539.703, 2539.703, 2345, 1597, &[]),
        ("href", "High-Resolution Ensemble Forecast (3km CONUS, 6-hourly)", 3000.0, 3000.0, 1799, 1059, &[]),
    ];

    for &(name, desc, dx, dy, nx, ny, aliases) in model_info {
        let dict = PyDict::new(py);
        dict.set_item("name", name)?;
        dict.set_item("description", desc)?;
        dict.set_item("grid_dx", dx)?;
        dict.set_item("grid_dy", dy)?;
        dict.set_item("grid_nx", nx)?;
        dict.set_item("grid_ny", ny)?;
        dict.set_item("aliases", aliases.to_vec())?;
        list.append(dict)?;
    }
    Ok(list)
}

// ──────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────

/// Parse fhour from Python: accepts int, list[int], or None (defaults to [0]).
fn parse_fhour(py: Python<'_>, obj: &PyObject) -> PyResult<Vec<u32>> {
    let bound = obj.bind(py);

    // Try extracting as a single integer first
    if let Ok(val) = bound.extract::<u32>() {
        return Ok(vec![val]);
    }

    // Try extracting as a list of integers
    if let Ok(list) = bound.downcast::<PyList>() {
        let mut hours = Vec::with_capacity(list.len());
        for item in list.iter() {
            let h: u32 = item.extract().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "fhour list elements must be integers"
                )
            })?;
            hours.push(h);
        }
        if hours.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "fhour list must not be empty"
            ));
        }
        return Ok(hours);
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "fhour must be an int or a list of ints"
    ))
}

/// Inner URL builder that takes an already-lowercased model name (for use in rayon closures).
fn model_urls_inner(model: &str, date: &str, hour: u32, product: &str, fhour: u32)
    -> Result<(String, String), String>
{
    match model {
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
        "ecmwf" | "ifs" => {
            let idx = models::EcmwfConfig::idx_url(date, hour, product, fhour);
            let grib = models::EcmwfConfig::open_data_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "nbm" | "blend" => {
            let idx = models::NbmConfig::idx_url(date, hour, product, fhour);
            let grib = models::NbmConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "rrfs" => {
            let idx = models::RrfsConfig::idx_url(date, hour, product, fhour);
            let grib = models::RrfsConfig::nomads_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "rtma" => {
            let idx = models::RtmaConfig::idx_url(date, hour, product);
            let grib = models::RtmaConfig::aws_url(date, hour, product);
            Ok((idx, grib))
        }
        "href" => {
            let idx = models::HrefConfig::idx_url(date, hour, product, fhour);
            let grib = models::HrefConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        _ => Err(format!("Unknown model '{}'. Supported: hrrr, gfs, nam, rap, ecmwf, nbm, rrfs, rtma, href", model)),
    }
}

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

/// Resolve run string: if "latest", probe AWS for the latest available run.
/// Otherwise parse the date/hour from the string.
fn resolve_run(client: &download::DownloadClient, model: &str, run: &str) -> PyResult<(String, u32)> {
    if run.eq_ignore_ascii_case("latest") {
        models::find_latest_run(client, model)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
    } else {
        parse_run(run)
    }
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
        "ecmwf" | "ifs" => {
            let idx = models::EcmwfConfig::idx_url(date, hour, product, fhour);
            let grib = models::EcmwfConfig::open_data_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "nbm" | "blend" => {
            let idx = models::NbmConfig::idx_url(date, hour, product, fhour);
            let grib = models::NbmConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "rrfs" => {
            let idx = models::RrfsConfig::idx_url(date, hour, product, fhour);
            let grib = models::RrfsConfig::nomads_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "rtma" => {
            let idx = models::RtmaConfig::idx_url(date, hour, product);
            let grib = models::RtmaConfig::aws_url(date, hour, product);
            Ok((idx, grib))
        }
        "href" => {
            let idx = models::HrefConfig::idx_url(date, hour, product, fhour);
            let grib = models::HrefConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            format!("Unknown model '{}'. Supported: hrrr, gfs, nam, rap, ecmwf, nbm, rrfs, rtma, href", model)
        )),
    }
}

// ──────────────────────────────────────────────────────────
// Thermodynamic functions (from metfuncs)
// ──────────────────────────────────────────────────────────

/// LCL temperature from temperature and dewpoint (both Celsius). Returns Celsius.
#[pyfunction]
fn lcltemp(t_celsius: f64, td_celsius: f64) -> f64 {
    metfuncs::lcltemp(t_celsius, td_celsius)
}

/// Equivalent potential temperature (Celsius).
/// p_hpa: pressure in hPa, t_celsius: temperature in C, td_celsius: dewpoint in C.
#[pyfunction]
fn thetae(p_hpa: f64, t_celsius: f64, td_celsius: f64) -> f64 {
    metfuncs::thetae(p_hpa, t_celsius, td_celsius)
}

/// Mixing ratio (g/kg) at given pressure (hPa) and temperature (Celsius).
#[pyfunction]
fn mixratio(p_hpa: f64, t_celsius: f64) -> f64 {
    metfuncs::mixratio(p_hpa, t_celsius)
}

/// Dewpoint (Celsius) from mixing ratio (kg/kg) and pressure (hPa).
#[pyfunction]
fn dewpoint_from_q(q_kgkg: f64, p_hpa: f64) -> f64 {
    composite::dewpoint_from_q(q_kgkg, p_hpa)
}

// ──────────────────────────────────────────────────────────
// Composite severe weather parameters (numpy array functions)
// ──────────────────────────────────────────────────────────

/// Compute CAPE, CIN, LCL, and LFC for every grid point.
///
/// All 3D arrays are flattened [nz][ny][nx]. 2D arrays are [ny][nx].
/// Returns dict with keys 'cape', 'cin', 'lcl', 'lfc' each as numpy array.
#[pyfunction]
#[pyo3(signature = (pressure_3d, temperature_c_3d, qvapor_3d, height_agl_3d, psfc, t2, q2, nx, ny, nz, parcel_type="sb"))]
fn compute_cape_cin<'py>(
    py: Python<'py>,
    pressure_3d: PyReadonlyArray1<f64>,
    temperature_c_3d: PyReadonlyArray1<f64>,
    qvapor_3d: PyReadonlyArray1<f64>,
    height_agl_3d: PyReadonlyArray1<f64>,
    psfc: PyReadonlyArray1<f64>,
    t2: PyReadonlyArray1<f64>,
    q2: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    nz: usize,
    parcel_type: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let (cape, cin, lcl, lfc) = composite::compute_cape_cin(
        pressure_3d.as_slice()?,
        temperature_c_3d.as_slice()?,
        qvapor_3d.as_slice()?,
        height_agl_3d.as_slice()?,
        psfc.as_slice()?,
        t2.as_slice()?,
        q2.as_slice()?,
        nx, ny, nz,
        parcel_type,
    );
    let dict = PyDict::new(py);
    dict.set_item("cape", PyArray1::from_vec(py, cape))?;
    dict.set_item("cin", PyArray1::from_vec(py, cin))?;
    dict.set_item("lcl", PyArray1::from_vec(py, lcl))?;
    dict.set_item("lfc", PyArray1::from_vec(py, lfc))?;
    Ok(dict)
}

/// Compute Storm Relative Helicity (m^2/s^2) for every grid point.
///
/// 3D arrays are flattened [nz][ny][nx]. Returns 1D numpy array of size ny*nx.
#[pyfunction]
fn compute_srh<'py>(
    py: Python<'py>,
    u_3d: PyReadonlyArray1<f64>,
    v_3d: PyReadonlyArray1<f64>,
    height_agl_3d: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    nz: usize,
    top_m: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::compute_srh(
        u_3d.as_slice()?,
        v_3d.as_slice()?,
        height_agl_3d.as_slice()?,
        nx, ny, nz,
        top_m,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Compute bulk wind shear magnitude (m/s) between bottom_m and top_m AGL.
///
/// 3D arrays are flattened [nz][ny][nx]. Returns 1D numpy array of size ny*nx.
#[pyfunction]
fn compute_shear<'py>(
    py: Python<'py>,
    u_3d: PyReadonlyArray1<f64>,
    v_3d: PyReadonlyArray1<f64>,
    height_agl_3d: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    nz: usize,
    bottom_m: f64,
    top_m: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::compute_shear(
        u_3d.as_slice()?,
        v_3d.as_slice()?,
        height_agl_3d.as_slice()?,
        nx, ny, nz,
        bottom_m, top_m,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Significant Tornado Parameter (STP).
///
/// All inputs are 1D numpy arrays of the same size (ny*nx).
#[pyfunction]
fn compute_stp<'py>(
    py: Python<'py>,
    cape: PyReadonlyArray1<f64>,
    lcl: PyReadonlyArray1<f64>,
    srh_1km: PyReadonlyArray1<f64>,
    shear_6km: PyReadonlyArray1<f64>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::compute_stp(
        cape.as_slice()?,
        lcl.as_slice()?,
        srh_1km.as_slice()?,
        shear_6km.as_slice()?,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Energy Helicity Index: EHI = (CAPE * SRH) / 160000.
///
/// All inputs are 1D numpy arrays of the same size.
#[pyfunction]
fn compute_ehi<'py>(
    py: Python<'py>,
    cape: PyReadonlyArray1<f64>,
    srh: PyReadonlyArray1<f64>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::compute_ehi(
        cape.as_slice()?,
        srh.as_slice()?,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Supercell Composite Parameter: SCP = (MUCAPE/1000) * (SRH_3km/50) * (SHEAR_6km/40).
///
/// All inputs are 1D numpy arrays of the same size.
#[pyfunction]
fn compute_scp<'py>(
    py: Python<'py>,
    mucape: PyReadonlyArray1<f64>,
    srh_3km: PyReadonlyArray1<f64>,
    shear_6km: PyReadonlyArray1<f64>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::compute_scp(
        mucape.as_slice()?,
        srh_3km.as_slice()?,
        shear_6km.as_slice()?,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Lapse rate (C/km) between two heights in km AGL.
///
/// 3D arrays are flattened [nz][ny][nx]. Returns 1D numpy array of size ny*nx.
#[pyfunction]
fn compute_lapse_rate<'py>(
    py: Python<'py>,
    temperature_c_3d: PyReadonlyArray1<f64>,
    qvapor_3d: PyReadonlyArray1<f64>,
    height_agl_3d: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    nz: usize,
    bottom_km: f64,
    top_km: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::compute_lapse_rate(
        temperature_c_3d.as_slice()?,
        qvapor_3d.as_slice()?,
        height_agl_3d.as_slice()?,
        nx, ny, nz,
        bottom_km, top_km,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Precipitable water (mm).
///
/// 3D arrays are flattened [nz][ny][nx]. Returns 1D numpy array of size ny*nx.
#[pyfunction]
fn compute_pw<'py>(
    py: Python<'py>,
    qvapor_3d: PyReadonlyArray1<f64>,
    pressure_3d: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    nz: usize,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::compute_pw(
        qvapor_3d.as_slice()?,
        pressure_3d.as_slice()?,
        nx, ny, nz,
    );
    Ok(PyArray1::from_vec(py, result))
}

// ──────────────────────────────────────────────────────────
// Rendering functions
// ──────────────────────────────────────────────────────────

/// Render a GribMessage as an RGBA pixel buffer.
///
/// Returns a 1D numpy uint8 array of length ny*nx*4 (RGBA row-major).
/// Reshape in Python with `.reshape((msg.ny, msg.nx, 4))`.
/// NaN grid values become transparent pixels (alpha=0).
///
/// Args:
///     msg: GribMessage to render
///     colormap: Colormap name ("temperature", "wind", "reflectivity",
///               "cape", "relative_humidity", "precipitation", "vorticity")
///     vmin: Minimum value for colormap scaling. Auto-detected if None.
///     vmax: Maximum value for colormap scaling. Auto-detected if None.
///
/// Returns:
///     numpy uint8 array, length ny*nx*4
#[pyfunction]
#[pyo3(signature = (msg, colormap="temperature", vmin=None, vmax=None))]
fn render_map<'py>(
    py: Python<'py>,
    msg: &GribMessage,
    colormap: &str,
    vmin: Option<f64>,
    vmax: Option<f64>,
) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let values = grib2::unpack_message(&msg.raw_msg)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let nx = msg.nx as usize;
    let ny = msg.ny as usize;

    let (actual_vmin, actual_vmax) = match (vmin, vmax) {
        (Some(lo), Some(hi)) => (lo, hi),
        _ => {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for &v in &values {
                if !v.is_nan() {
                    if v < lo { lo = v; }
                    if v > hi { hi = v; }
                }
            }
            (vmin.unwrap_or(lo), vmax.unwrap_or(hi))
        }
    };

    let pixels = render::raster::render_raster(&values, nx, ny, colormap, actual_vmin, actual_vmax);
    Ok(PyArray1::from_vec(py, pixels))
}

/// Render raw f64 values as an RGBA pixel buffer.
///
/// Args:
///     values: 1D numpy float64 array (row-major, ny*nx elements)
///     nx: Grid width
///     ny: Grid height
///     colormap: Colormap name
///     vmin: Minimum value for colormap scaling
///     vmax: Maximum value for colormap scaling
///
/// Returns:
///     numpy uint8 array, length ny*nx*4
#[pyfunction]
fn render_array<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    colormap: &str,
    vmin: f64,
    vmax: f64,
) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let slice = values.as_slice()?;
    if slice.len() != nx * ny {
        return Err(pyo3::exceptions::PyValueError::new_err(
            format!("values has {} elements but nx*ny = {}", slice.len(), nx * ny)
        ));
    }

    let pixels = render::raster::render_raster(slice, nx, ny, colormap, vmin, vmax);
    Ok(PyArray1::from_vec(py, pixels))
}

/// Save an RGBA pixel buffer as a PNG file.
///
/// Args:
///     pixels: Flat uint8 array of length ny*nx*4 (RGBA row-major)
///     nx: Image width
///     ny: Image height
///     path: Output file path
#[pyfunction]
fn save_png(pixels: PyReadonlyArray1<u8>, nx: usize, ny: usize, path: &str) -> PyResult<()> {
    let slice = pixels.as_slice()?;
    render::encode::write_png(slice, nx as u32, ny as u32, path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))
}

/// List available colormap names.
#[pyfunction]
fn colormaps() -> Vec<&'static str> {
    render::colormap::list_colormaps().to_vec()
}

// ──────────────────────────────────────────────────────────
// Streaming fetch — delivers messages via Python callback
// ──────────────────────────────────────────────────────────

/// Streaming fetch that downloads and decodes GRIB2 data, calling a Python
/// callback for each message as it becomes available during download.
///
/// The callback receives a single `GribMessage` argument. This allows
/// processing (e.g., rendering maps) to overlap with the download.
///
/// Args:
///     model: Model name ("hrrr", "gfs", "nam", "rap")
///     run: Run time as "YYYY-MM-DD/HHz"
///     fhour: Forecast hour (default 0)
///     product: Model product ("prs", "sfc", "nat", "subh") -- default "prs"
///     vars: Optional list of variable patterns to filter download
///     callback: Python callable that receives (GribMessage) for each decoded field
///
/// Returns:
///     List of all GribMessage objects decoded (also passed to callback if provided).
#[pyfunction]
#[pyo3(signature = (model, run, fhour=0, product="prs", vars=None, callback=None))]
fn fetch_streaming(
    py: Python<'_>,
    model: &str,
    run: &str,
    fhour: u32,
    product: &str,
    vars: Option<Vec<String>>,
    callback: Option<PyObject>,
) -> PyResult<Vec<GribMessage>> {
    let (date, hour) = parse_run(run)?;
    let product_key = normalize_product(product);
    let (idx_url, grib_url) = model_urls(model, &date, hour, &product_key, fhour)?;

    let client = download::DownloadClient::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}", e)))?;

    let idx_text = client.get_text(&idx_url)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(
            format!("Failed to fetch index: {}", e)
        ))?;
    let idx_entries = download::parse_idx(&idx_text);

    let ranges = if let Some(ref var_patterns) = vars {
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
        download::byte_ranges(&idx_entries, &selected)
    } else {
        vec![(0, u64::MAX)]
    };

    let mut all_messages: Vec<GribMessage> = Vec::new();

    download::fetch_streaming(&client, &grib_url, &ranges, |msg, _values| {
        let py_msg = msg_to_py(&msg);

        if let Some(ref cb) = callback {
            Python::with_gil(|inner_py| {
                if let Err(e) = cb.call1(inner_py, (py_msg.clone(),)) {
                    eprintln!("Streaming callback error: {}", e);
                }
            });
        }

        all_messages.push(py_msg);
    }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;

    Ok(all_messages)
}

// ──────────────────────────────────────────────────────────
// Module definition
// ──────────────────────────────────────────────────────────

// ──────────────────────────────────────────────────────────
// Grib2Writer — create GRIB2 files from data arrays
// ──────────────────────────────────────────────────────────

#[pyclass(name = "Grib2Writer")]
struct PyGrib2Writer {
    messages: Vec<grib2::writer::MessageBuilder>,
}

#[pymethods]
impl PyGrib2Writer {
    #[new]
    fn new() -> Self {
        PyGrib2Writer {
            messages: Vec::new(),
        }
    }

    /// Add a field/message to the GRIB2 file.
    ///
    /// Args:
    ///     values: 1D numpy array of data values (ny * nx elements)
    ///     discipline: WMO discipline (0=Meteorological, default 0)
    ///     parameter_category: WMO parameter category (default 0)
    ///     parameter_number: WMO parameter number (default 0)
    ///     level_type: Type of level (103=height above ground, 100=isobaric, 1=surface)
    ///     level_value: Level value (e.g., 2.0 for 2m)
    ///     grid_template: Grid template (0=lat/lon, 30=Lambert Conformal)
    ///     nx, ny: Grid dimensions
    ///     lat1, lon1: First grid point coordinates (degrees)
    ///     lat2, lon2: Last grid point coordinates (degrees, for lat/lon grids)
    ///     dx, dy: Grid spacing (degrees for lat/lon, meters for Lambert)
    ///     scan_mode: Scanning mode flags (default 0)
    ///     latin1, latin2: Standard parallels for Lambert (degrees)
    ///     lov: Orientation longitude for Lambert (degrees)
    ///     bits_per_value: Packing precision (default 16)
    ///     reference_time: Reference time as "YYYY-MM-DD HH:MM:SS"
    ///     forecast_time: Forecast time in hours (default 0)
    ///     center: Originating center ID (default 0)
    ///     bitmap: Optional 1D boolean numpy array for missing value mask
    #[pyo3(signature = (
        values,
        discipline = 0,
        parameter_category = 0,
        parameter_number = 0,
        level_type = 103,
        level_value = 2.0,
        grid_template = 0,
        nx = 1,
        ny = 1,
        lat1 = 0.0,
        lon1 = 0.0,
        lat2 = 0.0,
        lon2 = 0.0,
        dx = 1.0,
        dy = 1.0,
        scan_mode = 0,
        latin1 = 0.0,
        latin2 = 0.0,
        lov = 0.0,
        bits_per_value = 16,
        reference_time = "2000-01-01 00:00:00",
        forecast_time = 0,
        center = 0,
        bitmap = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn add_field(
        &mut self,
        values: PyReadonlyArray1<f64>,
        discipline: u8,
        parameter_category: u8,
        parameter_number: u8,
        level_type: u8,
        level_value: f64,
        grid_template: u16,
        nx: u32,
        ny: u32,
        lat1: f64,
        lon1: f64,
        lat2: f64,
        lon2: f64,
        dx: f64,
        dy: f64,
        scan_mode: u8,
        latin1: f64,
        latin2: f64,
        lov: f64,
        bits_per_value: u8,
        reference_time: &str,
        forecast_time: u32,
        center: u16,
        bitmap: Option<PyReadonlyArray1<bool>>,
    ) -> PyResult<()> {
        let vals = values.as_slice()?.to_vec();

        let dt = chrono::NaiveDateTime::parse_from_str(reference_time, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(
                format!("Invalid reference_time '{}': {}. Use 'YYYY-MM-DD HH:MM:SS'", reference_time, e)
            ))?;

        let grid = grib2::GridDefinition {
            template: grid_template,
            nx,
            ny,
            lat1,
            lon1,
            lat2,
            lon2,
            dx,
            dy,
            latin1,
            latin2,
            lov,
            scan_mode,
            ..grib2::GridDefinition::default()
        };

        let product = grib2::ProductDefinition {
            template: 0,
            parameter_category,
            parameter_number,
            generating_process: 2,
            forecast_time,
            time_range_unit: 1, // Hour
            level_type,
            level_value,
        };

        let mut builder = grib2::writer::MessageBuilder::new(discipline, vals)
            .grid(grid)
            .product(product)
            .center(center, 0)
            .reference_time(dt)
            .packing(grib2::writer::PackingMethod::Simple { bits_per_value });

        if let Some(bm) = bitmap {
            builder = builder.bitmap(bm.as_slice()?.to_vec());
        }

        self.messages.push(builder);
        Ok(())
    }

    /// Write the GRIB2 file to bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        let mut writer = grib2::Grib2Writer::new();
        for msg in &self.messages {
            writer = writer.add_message(msg.clone());
        }
        let data = writer.to_bytes()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        Ok(pyo3::types::PyBytes::new(py, &data))
    }

    /// Write the GRIB2 file to disk.
    fn write(&self, path: &str) -> PyResult<()> {
        let mut writer = grib2::Grib2Writer::new();
        for msg in &self.messages {
            writer = writer.add_message(msg.clone());
        }
        writer.write_file(path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
    }

    /// Number of messages/fields added so far.
    #[getter]
    fn num_messages(&self) -> usize {
        self.messages.len()
    }

    fn __repr__(&self) -> String {
        format!("<Grib2Writer with {} messages>", self.messages.len())
    }
}

#[pymodule]
fn _rustmet(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<GribFile>()?;
    m.add_class::<GribMessage>()?;
    m.add_class::<Client>()?;
    m.add_class::<PyGrib2Writer>()?;
    m.add_function(wrap_pyfunction!(fetch, m)?)?;
    m.add_function(wrap_pyfunction!(fetch_streaming, m)?)?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(search, m)?)?;
    m.add_function(wrap_pyfunction!(products, m)?)?;
    m.add_function(wrap_pyfunction!(available_models, m)?)?;
    // Thermodynamic scalar functions
    m.add_function(wrap_pyfunction!(lcltemp, m)?)?;
    m.add_function(wrap_pyfunction!(thetae, m)?)?;
    m.add_function(wrap_pyfunction!(mixratio, m)?)?;
    m.add_function(wrap_pyfunction!(dewpoint_from_q, m)?)?;
    // Composite severe weather parameters (numpy array functions)
    m.add_function(wrap_pyfunction!(compute_cape_cin, m)?)?;
    m.add_function(wrap_pyfunction!(compute_srh, m)?)?;
    m.add_function(wrap_pyfunction!(compute_shear, m)?)?;
    m.add_function(wrap_pyfunction!(compute_stp, m)?)?;
    m.add_function(wrap_pyfunction!(compute_ehi, m)?)?;
    m.add_function(wrap_pyfunction!(compute_scp, m)?)?;
    m.add_function(wrap_pyfunction!(compute_lapse_rate, m)?)?;
    m.add_function(wrap_pyfunction!(compute_pw, m)?)?;
    // Rendering functions
    m.add_function(wrap_pyfunction!(render_map, m)?)?;
    m.add_function(wrap_pyfunction!(render_array, m)?)?;
    m.add_function(wrap_pyfunction!(save_png, m)?)?;
    m.add_function(wrap_pyfunction!(colormaps, m)?)?;
    m.add("__version__", "0.1.0")?;
    Ok(())
}
