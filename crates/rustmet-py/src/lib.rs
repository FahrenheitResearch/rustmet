use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use numpy::{PyArray1, PyArray2, PyReadonlyArray1};
use rayon::prelude::*;
use rustmet_core::{grib2, download, models, metfuncs, dynamics, composite, regrid, render};

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

    /// Merge another GribFile's messages into this one, returning a new GribFile.
    fn merge(&self, other: &GribFile) -> GribFile {
        let mut messages = self.messages.clone();
        messages.extend(other.messages.iter().cloned());
        GribFile { messages }
    }

    /// Extract specific messages by index (0-based), returning a new GribFile.
    fn subset(&self, indices: Vec<usize>) -> GribFile {
        let messages = indices
            .iter()
            .filter_map(|&i| self.messages.get(i).cloned())
            .collect();
        GribFile { messages }
    }

    /// Filter messages by variable name and optional level string.
    ///
    /// Returns a new GribFile containing only matching messages.
    #[pyo3(signature = (variable, level=None))]
    fn filter(&self, variable: &str, level: Option<&str>) -> GribFile {
        let var_lower = variable.to_lowercase();
        let messages = self.messages.iter().filter(|m| {
            let name_match = m.variable.to_lowercase().contains(&var_lower);
            if let Some(lev) = level {
                name_match && m.level.to_lowercase().contains(&lev.to_lowercase())
            } else {
                name_match
            }
        }).cloned().collect();
        GribFile { messages }
    }

    fn __repr__(&self) -> String {
        format!("<GribFile with {} messages>", self.messages.len())
    }

    fn __len__(&self) -> usize {
        self.messages.len()
    }
}

// ──────────────────────────────────────────────────────────
// GRIB2 Operations — field stats, smoothing, unit conversion, wind
// ──────────────────────────────────────────────────────────

/// Compute field statistics (min, max, mean, std_dev, count, nan_count).
///
/// Returns a dict with keys: min, max, mean, std_dev, count, nan_count.
#[pyfunction]
fn field_stats<'py>(py: Python<'py>, values: PyReadonlyArray1<f64>) -> PyResult<Bound<'py, PyDict>> {
    let slice = values.as_slice()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{}", e)))?;
    let stats = grib2::field_stats(slice);
    let dict = PyDict::new(py);
    dict.set_item("min", stats.min)?;
    dict.set_item("max", stats.max)?;
    dict.set_item("mean", stats.mean)?;
    dict.set_item("std_dev", stats.std_dev)?;
    dict.set_item("count", stats.count)?;
    dict.set_item("nan_count", stats.nan_count)?;
    Ok(dict)
}

/// Gaussian smooth a 2D field.
///
/// Args:
///     values: 1D numpy array of length nx*ny (row-major)
///     nx: Number of columns
///     ny: Number of rows
///     sigma: Gaussian kernel sigma in grid-point units
///
/// Returns: smoothed 1D numpy array
#[pyfunction]
fn smooth<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    sigma: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let slice = values.as_slice()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{}", e)))?;
    let result = grib2::smooth_gaussian(slice, nx, ny, sigma);
    Ok(PyArray1::from_vec(py, result))
}

/// Convert units for a numpy array.
///
/// Supports: K<->C<->F, m/s<->kt<->mph<->km/h, Pa<->hPa<->mb<->inHg,
///           m<->ft<->km, kg/m2<->mm<->in, m2/s2<->J/kg
///
/// Returns: converted numpy array (new allocation)
#[pyfunction]
fn convert_units<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    from_unit: &str,
    to_unit: &str,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let mut data: Vec<f64> = values.as_slice()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{}", e)))?
        .to_vec();
    grib2::convert_units(&mut data, from_unit, to_unit)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
    Ok(PyArray1::from_vec(py, data))
}

/// Compute wind speed and direction from U and V components.
///
/// Returns: (speed, direction) as tuple of numpy arrays.
/// Direction follows meteorological convention (degrees, 0=from north).
#[pyfunction]
fn wind_speed_dir<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>,
    v: PyReadonlyArray1<f64>,
) -> PyResult<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<f64>>)> {
    let u_slice = u.as_slice()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{}", e)))?;
    let v_slice = v.as_slice()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{}", e)))?;
    let (speed, dir) = grib2::wind_speed_dir(u_slice, v_slice);
    Ok((PyArray1::from_vec(py, speed), PyArray1::from_vec(py, dir)))
}

// ──────────────────────────────────────────────────────────
// Client — HTTP download client for operational model data
// ──────────────────────────────────────────────────────────


// Internal fetch helpers (free functions for thread safety with rayon)

fn fetch_single_impl(
    client: &download::DownloadClient, model: &str, run: &str,
    fhour: u32, product: &str, vars: &Option<Vec<String>>,
    source: &Option<String>,
) -> PyResult<GribFile> {
    let (date, hour) = resolve_run(client, model, run)?;
    let product_key = normalize_product(product);

    let var_strs: Option<Vec<&str>> = vars.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect());
    let forced_source = source.as_deref();

    let start = std::time::Instant::now();
    let result = download::fetch_with_fallback(
        client, model, &date, hour, &product_key, fhour,
        var_strs.as_deref(), forced_source,
    ).map_err(|e| pyo3::exceptions::PyIOError::new_err(
        format!("Download failed for f{:03}: {}", fhour, e)
    ))?;
    let elapsed = start.elapsed();
    eprintln!(
        "  Downloaded f{:03} from {} in {:.1}s",
        fhour, result.source_name, elapsed.as_secs_f64()
    );

    let grib = grib2::Grib2File::from_bytes(&result.data)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("GRIB2 parse error for f{:03}: {}", fhour, e)))?;
    Ok(GribFile { messages: grib.messages.iter().map(msg_to_py).collect() })
}

fn fetch_many_impl(
    client: &download::DownloadClient, model: &str, run: &str,
    fhours: &[u32], product: &str, vars: &Option<Vec<String>>,
    source: &Option<String>,
) -> PyResult<Vec<GribFile>> {
    let (date, hour) = resolve_run(client, model, run)?;
    let product_key = normalize_product(product);
    let model_lower = model.to_lowercase();
    let forced_source = source.as_deref();

    let results: Vec<Result<GribFile, String>> = fhours.par_iter().map(|&fh| {
        let var_strs: Option<Vec<&str>> = vars.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect());

        let start = std::time::Instant::now();
        let result = download::fetch_with_fallback(
            client, &model_lower, &date, hour, &product_key, fh,
            var_strs.as_deref(), forced_source,
        ).map_err(|e| format!("Download failed for f{:03}: {}", fh, e))?;
        let elapsed = start.elapsed();
        eprintln!(
            "  Downloaded f{:03} from {} in {:.1}s",
            fh, result.source_name, elapsed.as_secs_f64()
        );

        let grib = grib2::Grib2File::from_bytes(&result.data)
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
    ///     source: Force a specific data source ("aws", "google", "nomads", "azure").
    ///             If None, tries all sources in priority order with automatic fallback.
    ///
    /// Returns:
    ///     GribFile when fhour is a single int, list[GribFile] when fhour is a list.
    #[pyo3(signature = (model, run, fhour=None, product="prs", vars=None, source=None))]
    fn fetch(
        &self,
        py: Python<'_>,
        model: &str,
        run: &str,
        fhour: Option<PyObject>,
        product: &str,
        vars: Option<Vec<String>>,
        source: Option<String>,
    ) -> PyResult<PyObject> {
        let fhour_obj = match &fhour {
            Some(obj) => obj.clone_ref(py),
            None => 0i64.into_pyobject(py).unwrap().into_any().unbind(),
        };
        let is_list = fhour_obj.bind(py).is_instance_of::<PyList>();
        let fhours = parse_fhour(py, &fhour_obj)?;

        if fhours.len() == 1 && !is_list {
            // Single forecast hour — return a single GribFile (backward compatible)
            let result = fetch_single_impl(&self.inner, model, run, fhours[0], product, &vars, &source)?;
            Ok(result.into_pyobject(py)?.into_any().unbind())
        } else {
            // Multiple forecast hours — download in parallel, return list
            let results = fetch_many_impl(&self.inner, model, run, &fhours, product, &vars, &source)?;
            Ok(results.into_pyobject(py)?.into_any().unbind())
        }
    }

    /// List available data sources for a model with their availability status.
    ///
    /// Args:
    ///     model: Model name ("hrrr", "gfs", "nam", "rap", etc.)
    ///     run: Optional run time. If provided, probes each source for availability.
    ///          If None, returns source metadata without probing.
    ///     product: Product type (default "prs") — used when probing availability.
    ///
    /// Returns:
    ///     List of dicts with keys: name, priority, idx_available, max_age_hours,
    ///     and optionally "available" (bool) if run was provided.
    #[pyo3(signature = (model, run=None, product="prs"))]
    fn sources<'py>(
        &self,
        py: Python<'py>,
        model: &str,
        run: Option<&str>,
        product: &str,
    ) -> PyResult<Bound<'py, PyList>> {
        let srcs = download::model_sources(model);
        let list = PyList::empty(py);

        // If a run time is given, probe each source
        let probe_results: Option<Vec<(String, bool)>> = if let Some(run_str) = run {
            let (date, hour) = resolve_run(&self.inner, model, run_str)?;
            let product_key = normalize_product(product);
            Some(download::probe_sources(&self.inner, model, &date, hour, &product_key))
        } else {
            None
        };

        for (i, src) in srcs.iter().enumerate() {
            let dict = PyDict::new(py);
            dict.set_item("name", src.name)?;
            dict.set_item("priority", src.priority)?;
            dict.set_item("idx_available", src.idx_available)?;
            dict.set_item("max_age_hours", src.max_age_hours)?;
            if let Some(ref probes) = probe_results {
                if let Some((_, avail)) = probes.get(i) {
                    dict.set_item("available", *avail)?;
                }
            }
            list.append(dict)?;
        }
        Ok(list)
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
#[pyo3(signature = (model, run, fhour=None, product="prs", vars=None, source=None))]
fn fetch(
    py: Python<'_>,
    model: &str,
    run: &str,
    fhour: Option<PyObject>,
    product: &str,
    vars: Option<Vec<String>>,
    source: Option<String>,
) -> PyResult<PyObject> {
    let client = Client::new(None)?;
    client.fetch(py, model, run, fhour, product, vars, source)
}

/// List available data sources for a model.
///
/// Returns a list of source names (e.g., ["aws", "google", "nomads", "azure"]).
#[pyfunction]
fn model_data_sources(model: &str) -> Vec<String> {
    download::source_names(model).into_iter().map(|s| s.to_string()).collect()
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
        ("era5", "ECMWF Reanalysis v5 (0.25deg global, hourly, 1940-present, NetCDF on AWS)", 0.25, 0.25, 1440, 721, &["reanalysis"]),
        ("gefs", "Global Ensemble Forecast System (0.5deg global, 31 members, 6-hourly)", 0.50, 0.50, 720, 361, &["gens"]),
        ("hrrr_ak", "HRRR Alaska (3km Alaska domain, hourly)", 3000.0, 3000.0, 1299, 919, &["hrrrak", "hrrr-ak", "hrrr_alaska"]),
        ("cfs", "Climate Forecast System (T126 global, seasonal forecasts)", 0.9375, 0.9375, 384, 190, &["cfsv2"]),
        ("sref", "Short-Range Ensemble Forecast (40km CONUS, 26 members, 6-hourly)", 40635.0, 40635.0, 185, 129, &[]),
        ("wpc", "WPC Quantitative Precipitation Forecast (2.5km CONUS)", 2539.703, 2539.703, 2345, 1597, &["wpc_qpf"]),
        ("urma", "UnRestricted Mesoscale Analysis (2.5km CONUS, analysis only)", 2539.703, 2539.703, 2345, 1597, &[]),
        ("mrms", "Multi-Radar Multi-Sensor (1km CONUS, radar mosaics, gzipped)", 0.01, 0.01, 7000, 3500, &["radar"]),
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
        "era5" | "reanalysis" => {
            let year = &date[..4];
            let month = &date[4..6];
            let variable = if product.is_empty() || product == "prs" { "2m_temperature" } else { product };
            let data_url = models::Era5Config::aws_url(year, month, variable);
            Ok((data_url.clone(), data_url))
        }
        "gefs" | "gens" => {
            let (member, gefs_product) = if product.contains(':') {
                let parts: Vec<&str> = product.splitn(2, ':').collect();
                (parts[0], parts[1])
            } else if ["pgrb2a", "pgrb2b", "a", "b", "secondary", "prs", "sfc"].contains(&product) {
                ("c00", product)
            } else {
                (product, "pgrb2a")
            };
            let idx = models::GefsConfig::idx_url(date, hour, member, gefs_product, fhour);
            let grib = models::GefsConfig::aws_url(date, hour, member, gefs_product, fhour);
            Ok((idx, grib))
        }
        "hrrr_ak" | "hrrrak" | "hrrr-ak" | "hrrr_alaska" => {
            let idx = models::HrrrAkConfig::idx_url(date, hour, product, fhour);
            let grib = models::HrrrAkConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "cfs" | "cfsv2" => {
            let idx = models::CfsConfig::idx_url(date, hour, product, fhour);
            let grib = models::CfsConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "sref" => {
            let member = if product == "prs" || product.is_empty() { "mean" } else { product };
            let idx = models::SrefConfig::idx_url(date, hour, member, fhour);
            let grib = models::SrefConfig::nomads_url(date, hour, member, fhour);
            Ok((idx, grib))
        }
        "wpc" | "wpc_qpf" => {
            let grib = models::WpcConfig::url(date, hour, product, fhour);
            Ok((grib.clone(), grib))
        }
        "urma" => {
            let idx = models::UrmaConfig::idx_url(date, hour, product);
            let grib = models::UrmaConfig::aws_url(date, hour, product);
            Ok((idx, grib))
        }
        "mrms" | "radar" => {
            let mrms_product = if product == "prs" || product.is_empty() { "MergedReflectivityQCComposite" } else { product };
            let datetime = format!("{}-{:02}0000", date, hour);
            let grib = models::MrmsConfig::aws_url(mrms_product, "00.50", &datetime);
            Ok((grib.clone(), grib))
        }
        _ => Err(format!(
            "Unknown model '{}'. Supported: hrrr, gfs, nam, rap, ecmwf, nbm, rrfs, rtma, href, era5, gefs, hrrr_ak, cfs, sref, wpc, urma, mrms",
            model
        )),
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
        "era5" | "reanalysis" => {
            let year = &date[..4];
            let month = &date[4..6];
            let variable = if product.is_empty() || product == "prs" { "2m_temperature" } else { product };
            let data_url = models::Era5Config::aws_url(year, month, variable);
            Ok((data_url.clone(), data_url))
        }
        "gefs" | "gens" => {
            let (member, gefs_product) = if product.contains(':') {
                let parts: Vec<&str> = product.splitn(2, ':').collect();
                (parts[0], parts[1])
            } else if ["pgrb2a", "pgrb2b", "a", "b", "secondary", "prs", "sfc"].contains(&product) {
                ("c00", product)
            } else {
                (product, "pgrb2a")
            };
            let idx = models::GefsConfig::idx_url(date, hour, member, gefs_product, fhour);
            let grib = models::GefsConfig::aws_url(date, hour, member, gefs_product, fhour);
            Ok((idx, grib))
        }
        "hrrr_ak" | "hrrrak" | "hrrr-ak" | "hrrr_alaska" => {
            let idx = models::HrrrAkConfig::idx_url(date, hour, product, fhour);
            let grib = models::HrrrAkConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "cfs" | "cfsv2" => {
            let idx = models::CfsConfig::idx_url(date, hour, product, fhour);
            let grib = models::CfsConfig::aws_url(date, hour, product, fhour);
            Ok((idx, grib))
        }
        "sref" => {
            let member = if product == "prs" || product.is_empty() { "mean" } else { product };
            let idx = models::SrefConfig::idx_url(date, hour, member, fhour);
            let grib = models::SrefConfig::nomads_url(date, hour, member, fhour);
            Ok((idx, grib))
        }
        "wpc" | "wpc_qpf" => {
            let grib = models::WpcConfig::url(date, hour, product, fhour);
            Ok((grib.clone(), grib))
        }
        "urma" => {
            let idx = models::UrmaConfig::idx_url(date, hour, product);
            let grib = models::UrmaConfig::aws_url(date, hour, product);
            Ok((idx, grib))
        }
        "mrms" | "radar" => {
            let mrms_product = if product == "prs" || product.is_empty() { "MergedReflectivityQCComposite" } else { product };
            let datetime = format!("{}-{:02}0000", date, hour);
            let grib = models::MrmsConfig::aws_url(mrms_product, "00.50", &datetime);
            Ok((grib.clone(), grib))
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            format!("Unknown model '{}'. Supported: hrrr, gfs, nam, rap, ecmwf, nbm, rrfs, rtma, href, era5, gefs, hrrr_ak, cfs, sref, wpc, urma, mrms", model)
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
// Stability indices and new composite parameters
// ──────────────────────────────────────────────────────────

/// Significant Hail Parameter (SHIP).
///
/// All inputs are 1D numpy arrays (flattened 2D grids of size nx*ny).
#[pyfunction]
fn significant_hail_parameter<'py>(
    py: Python<'py>,
    cape: PyReadonlyArray1<f64>,
    shear06: PyReadonlyArray1<f64>,
    t500: PyReadonlyArray1<f64>,
    lr_700_500: PyReadonlyArray1<f64>,
    mr: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::significant_hail_parameter(
        cape.as_slice()?,
        shear06.as_slice()?,
        t500.as_slice()?,
        lr_700_500.as_slice()?,
        mr.as_slice()?,
        nx, ny,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Derecho Composite Parameter (DCP).
///
/// All inputs are 1D numpy arrays (flattened 2D grids of size nx*ny).
#[pyfunction]
fn derecho_composite_parameter<'py>(
    py: Python<'py>,
    dcape: PyReadonlyArray1<f64>,
    mu_cape: PyReadonlyArray1<f64>,
    shear06: PyReadonlyArray1<f64>,
    mu_mixing_ratio: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::derecho_composite_parameter(
        dcape.as_slice()?,
        mu_cape.as_slice()?,
        shear06.as_slice()?,
        mu_mixing_ratio.as_slice()?,
        nx, ny,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Enhanced Supercell Composite Parameter (SCP) with CIN penalty.
///
/// All inputs are 1D numpy arrays (flattened 2D grids of size nx*ny).
#[pyfunction]
fn supercell_composite_parameter<'py>(
    py: Python<'py>,
    mu_cape: PyReadonlyArray1<f64>,
    srh: PyReadonlyArray1<f64>,
    shear_06: PyReadonlyArray1<f64>,
    mu_cin: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::supercell_composite_parameter(
        mu_cape.as_slice()?,
        srh.as_slice()?,
        shear_06.as_slice()?,
        mu_cin.as_slice()?,
        nx, ny,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Critical Angle between storm-relative inflow and 0-500m shear vector.
///
/// All inputs are 1D numpy arrays (flattened 2D grids of size nx*ny).
/// Returns angle in degrees (0-180).
#[pyfunction]
fn critical_angle<'py>(
    py: Python<'py>,
    u_storm: PyReadonlyArray1<f64>,
    v_storm: PyReadonlyArray1<f64>,
    u_shear: PyReadonlyArray1<f64>,
    v_shear: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = composite::critical_angle(
        u_storm.as_slice()?,
        v_storm.as_slice()?,
        u_shear.as_slice()?,
        v_shear.as_slice()?,
        nx, ny,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Showalter Index: lift 850 hPa parcel to 500 hPa.
/// p in hPa, t and td in Celsius. Profiles surface-first (decreasing pressure).
#[pyfunction]
fn showalter_index(
    p: PyReadonlyArray1<f64>,
    t: PyReadonlyArray1<f64>,
    td: PyReadonlyArray1<f64>,
) -> PyResult<f64> {
    Ok(composite::showalter_index(
        p.as_slice()?,
        t.as_slice()?,
        td.as_slice()?,
    ))
}

/// Lifted Index: lift surface parcel to 500 hPa.
/// p in hPa, t and td in Celsius. Profiles surface-first (decreasing pressure).
#[pyfunction]
fn lifted_index(
    p: PyReadonlyArray1<f64>,
    t: PyReadonlyArray1<f64>,
    td: PyReadonlyArray1<f64>,
) -> PyResult<f64> {
    Ok(composite::lifted_index(
        p.as_slice()?,
        t.as_slice()?,
        td.as_slice()?,
    ))
}

/// K-Index: (T850 - T500) + Td850 - (T700 - Td700). All Celsius.
#[pyfunction]
fn k_index(t850: f64, t700: f64, t500: f64, td850: f64, td700: f64) -> f64 {
    composite::k_index(t850, t700, t500, td850, td700)
}

/// Total Totals Index: (T850 - T500) + (Td850 - T500). All Celsius.
#[pyfunction]
fn total_totals(t850: f64, t500: f64, td850: f64) -> f64 {
    composite::total_totals(t850, t500, td850)
}

/// Cross Totals: Td850 - T500. All Celsius.
#[pyfunction]
fn cross_totals(td850: f64, t500: f64) -> f64 {
    composite::cross_totals(td850, t500)
}

/// Vertical Totals: T850 - T500. All Celsius.
#[pyfunction]
fn vertical_totals(t850: f64, t500: f64) -> f64 {
    composite::vertical_totals(t850, t500)
}

/// SWEAT Index. tt=Total Totals, td850 in C, wspd in knots, wdir in degrees.
#[pyfunction]
fn sweat_index(tt: f64, td850: f64, wspd850: f64, wdir850: f64, wspd500: f64, wdir500: f64) -> f64 {
    composite::sweat_index(tt, td850, wspd850, wdir850, wspd500, wdir500)
}

/// Boyden Index: (Z700 - Z1000)/10 - T700 - 200. Heights in m, T in C.
#[pyfunction]
fn boyden_index(z1000: f64, z700: f64, t700: f64) -> f64 {
    composite::boyden_index(z1000, z700, t700)
}

/// Haines Index (Low Elevation). T in Celsius. Returns 2-6.
#[pyfunction]
fn haines_index(t_950: f64, t_850: f64, td_850: f64) -> u8 {
    composite::haines_index(t_950, t_850, td_850)
}

/// Fosberg Fire Weather Index. t_f in Fahrenheit, rh 0-100, wspd in mph.
#[pyfunction]
fn fosberg_fire_weather_index(t_f: f64, rh: f64, wspd_mph: f64) -> f64 {
    composite::fosberg_fire_weather_index(t_f, rh, wspd_mph)
}

/// Hot-Dry-Windy Index. t_c in Celsius, rh 0-100, wspd in m/s, vpd in hPa (0=auto).
#[pyfunction]
fn hot_dry_windy(t_c: f64, rh: f64, wspd_ms: f64, vpd: f64) -> f64 {
    composite::hot_dry_windy(t_c, rh, wspd_ms, vpd)
}

/// Bulk Richardson Number: CAPE / (0.5 * shear^2).
#[pyfunction]
fn bulk_richardson_number(cape: f64, shear_06_ms: f64) -> f64 {
    composite::bulk_richardson_number(cape, shear_06_ms)
}

/// Dendritic Growth Zone: returns (p_top, p_bottom) in hPa.
/// Profiles surface-first (decreasing pressure). T in Celsius, p in hPa.
#[pyfunction]
fn dendritic_growth_zone(
    t_profile: PyReadonlyArray1<f64>,
    p_profile: PyReadonlyArray1<f64>,
) -> PyResult<(f64, f64)> {
    Ok(composite::dendritic_growth_zone(
        t_profile.as_slice()?,
        p_profile.as_slice()?,
    ))
}

/// Check for warm nose (above-freezing layer above a sub-freezing surface).
/// Profiles surface-first. T in Celsius.
#[pyfunction]
fn warm_nose_check(
    t_profile: PyReadonlyArray1<f64>,
    p_profile: PyReadonlyArray1<f64>,
) -> PyResult<bool> {
    Ok(composite::warm_nose_check(
        t_profile.as_slice()?,
        p_profile.as_slice()?,
    ))
}

/// Freezing Rain Composite (0-1). precip_type: 0=none,1=rain,2=snow,3=sleet,4=FZRA.
#[pyfunction]
fn freezing_rain_composite(
    t_profile: PyReadonlyArray1<f64>,
    p_profile: PyReadonlyArray1<f64>,
    precip_type: u8,
) -> PyResult<f64> {
    Ok(composite::freezing_rain_composite(
        t_profile.as_slice()?,
        p_profile.as_slice()?,
        precip_type,
    ))
}

/// Convective Inhibition Depth (hPa). Profiles surface-first, p in hPa, t/td in C.
#[pyfunction]
fn convective_inhibition_depth(
    p: PyReadonlyArray1<f64>,
    t: PyReadonlyArray1<f64>,
    td: PyReadonlyArray1<f64>,
) -> PyResult<f64> {
    Ok(composite::convective_inhibition_depth(
        p.as_slice()?,
        t.as_slice()?,
        td.as_slice()?,
    ))
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

#[pyfunction]
#[pyo3(signature = (values, nx, ny, levels, colormap="temperature", width=None, height=None))]
fn render_filled_contours<'py>(py: Python<'py>, values: PyReadonlyArray1<f64>, nx: usize, ny: usize, levels: PyReadonlyArray1<f64>, colormap: &str, width: Option<u32>, height: Option<u32>) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let val_slice = values.as_slice()?;
    let lvl_slice = levels.as_slice()?;
    if val_slice.len() != nx * ny { return Err(pyo3::exceptions::PyValueError::new_err(format!("values has {} elements but nx*ny = {}", val_slice.len(), nx * ny))); }
    if lvl_slice.len() < 2 { return Err(pyo3::exceptions::PyValueError::new_err("Need at least 2 contour levels")); }
    let w = width.unwrap_or(nx as u32);
    let h = height.unwrap_or(ny as u32);
    let pixels = render::filled_contour::render_filled_contours(val_slice, nx, ny, lvl_slice, colormap, w, h);
    Ok(PyArray1::from_vec(py, pixels))
}

#[pyfunction]
#[pyo3(signature = (pixels, width, height, values, nx, ny, levels, r=0, g=0, b=0, line_width=1))]
fn overlay_contours_py<'py>(py: Python<'py>, pixels: PyReadonlyArray1<u8>, width: u32, height: u32, values: PyReadonlyArray1<f64>, nx: usize, ny: usize, levels: PyReadonlyArray1<f64>, r: u8, g: u8, b: u8, line_width: u32) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let pix_slice = pixels.as_slice()?;
    let val_slice = values.as_slice()?;
    let lvl_slice = levels.as_slice()?;
    let expected_len = (width as usize) * (height as usize) * 4;
    if pix_slice.len() != expected_len { return Err(pyo3::exceptions::PyValueError::new_err(format!("pixels len {} != width*height*4 = {}", pix_slice.len(), expected_len))); }
    if val_slice.len() != nx * ny { return Err(pyo3::exceptions::PyValueError::new_err(format!("values len {} != nx*ny = {}", val_slice.len(), nx * ny))); }
    let mut out = pix_slice.to_vec();
    let contours = render::contour::contour_lines(val_slice, nx, ny, lvl_slice);
    render::overlay::overlay_contours(&mut out, width, height, &contours, nx, ny, (r, g, b), line_width);
    Ok(PyArray1::from_vec(py, out))
}

#[pyfunction]
#[pyo3(signature = (pixels, width, height, u, v, nx, ny, skip=10, r=0, g=0, b=0, barb_length=15))]
fn overlay_wind_barbs_py<'py>(py: Python<'py>, pixels: PyReadonlyArray1<u8>, width: u32, height: u32, u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>, nx: usize, ny: usize, skip: usize, r: u8, g: u8, b: u8, barb_length: u32) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let pix_slice = pixels.as_slice()?;
    let u_slice = u.as_slice()?;
    let v_slice = v.as_slice()?;
    let expected_len = (width as usize) * (height as usize) * 4;
    if pix_slice.len() != expected_len { return Err(pyo3::exceptions::PyValueError::new_err(format!("pixels len {} != width*height*4 = {}", pix_slice.len(), expected_len))); }
    if u_slice.len() != nx * ny || v_slice.len() != nx * ny { return Err(pyo3::exceptions::PyValueError::new_err(format!("u/v must have nx*ny={} elements", nx * ny))); }
    let mut out = pix_slice.to_vec();
    render::overlay::overlay_wind_barbs(&mut out, width, height, u_slice, v_slice, nx, ny, skip, (r, g, b), barb_length);
    Ok(PyArray1::from_vec(py, out))
}

#[pyfunction]
#[pyo3(signature = (pixels, width, height, u, v, nx, ny, density=1.0, r=0, g=0, b=0))]
fn overlay_streamlines_py<'py>(py: Python<'py>, pixels: PyReadonlyArray1<u8>, width: u32, height: u32, u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>, nx: usize, ny: usize, density: f64, r: u8, g: u8, b: u8) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let pix_slice = pixels.as_slice()?;
    let u_slice = u.as_slice()?;
    let v_slice = v.as_slice()?;
    let expected_len = (width as usize) * (height as usize) * 4;
    if pix_slice.len() != expected_len { return Err(pyo3::exceptions::PyValueError::new_err(format!("pixels len {} != width*height*4 = {}", pix_slice.len(), expected_len))); }
    if u_slice.len() != nx * ny || v_slice.len() != nx * ny { return Err(pyo3::exceptions::PyValueError::new_err(format!("u/v must have nx*ny={} elements", nx * ny))); }
    let mut out = pix_slice.to_vec();
    render::overlay::overlay_streamlines(&mut out, width, height, u_slice, v_slice, nx, ny, density, (r, g, b));
    Ok(PyArray1::from_vec(py, out))
}



// Station model plot binding
#[pyfunction]
#[pyo3(signature = (stations, config=None))]
fn render_station_plot<'py>(
    py: Python<'py>,
    stations: &Bound<'py, PyList>,
    config: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyArray1<u8>>> {
    use render::station::{StationObs, StationPlotConfig};

    let mut obs_list: Vec<StationObs> = Vec::with_capacity(stations.len());
    for item in stations.iter() {
        let d = item.downcast::<PyDict>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("Each station must be a dict"))?;
        let lat: f64 = d.get_item("lat")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("Missing 'lat'"))?
            .extract()?;
        let lon: f64 = d.get_item("lon")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("Missing 'lon'"))?
            .extract()?;
        let temperature: Option<f64> = d.get_item("temperature")?.and_then(|v| v.extract().ok());
        let dewpoint: Option<f64> = d.get_item("dewpoint")?.and_then(|v| v.extract().ok());
        let wind_speed: Option<f64> = d.get_item("wind_speed")?.and_then(|v| v.extract().ok());
        let wind_direction: Option<f64> = d.get_item("wind_direction")?.and_then(|v| v.extract().ok());
        let pressure: Option<f64> = d.get_item("pressure")?.and_then(|v| v.extract().ok());
        let sky_cover: Option<u8> = d.get_item("sky_cover")?.and_then(|v| v.extract().ok());
        let weather: Option<u8> = d.get_item("weather")?.and_then(|v| v.extract().ok());
        let visibility: Option<f64> = d.get_item("visibility")?.and_then(|v| v.extract().ok());
        let pressure_tendency: Option<f64> = d.get_item("pressure_tendency")?.and_then(|v| v.extract().ok());
        obs_list.push(StationObs {
            lat, lon, temperature, dewpoint, wind_speed, wind_direction,
            pressure, sky_cover, weather, visibility, pressure_tendency,
        });
    }
    let mut cfg = StationPlotConfig::default();
    if let Some(cd) = config {
        if let Some(v) = cd.get_item("width")? { cfg.width = v.extract()?; }
        if let Some(v) = cd.get_item("height")? { cfg.height = v.extract()?; }
        if let Some(v) = cd.get_item("lon_min")? { cfg.lon_min = v.extract()?; }
        if let Some(v) = cd.get_item("lon_max")? { cfg.lon_max = v.extract()?; }
        if let Some(v) = cd.get_item("lat_min")? { cfg.lat_min = v.extract()?; }
        if let Some(v) = cd.get_item("lat_max")? { cfg.lat_max = v.extract()?; }
        if let Some(v) = cd.get_item("station_size")? { cfg.station_size = v.extract()?; }
        if let Some(v) = cd.get_item("font_size")? { cfg.font_size = v.extract()?; }
        if let Some(v) = cd.get_item("thinning_radius")? { cfg.thinning_radius = v.extract()?; }
        if let Some(v) = cd.get_item("bg_color")? {
            let c: Vec<u8> = v.extract()?;
            if c.len() == 4 { cfg.bg_color = (c[0], c[1], c[2], c[3]); }
        }
    }
    let pixels = render::station::render_station_plot(&obs_list, &cfg);
    Ok(PyArray1::from_vec(py, pixels))
}

// Cross-section renderer binding
#[pyfunction]
#[pyo3(signature = (values_2d, pressures, distances, colormap="temperature", vmin=0.0, vmax=1.0, width=800, height=600, p_min=100.0, p_max=1000.0))]
fn render_cross_section<'py>(
    py: Python<'py>,
    values_2d: &Bound<'py, PyList>,
    pressures: PyReadonlyArray1<f64>,
    distances: PyReadonlyArray1<f64>,
    colormap: &str,
    vmin: f64,
    vmax: f64,
    width: u32,
    height: u32,
    p_min: f64,
    p_max: f64,
) -> PyResult<Bound<'py, PyArray1<u8>>> {
    use render::cross_section::{CrossSectionConfig, CrossSectionData};
    let pres_slice = pressures.as_slice()?;
    let dist_slice = distances.as_slice()?;
    let mut values: Vec<Vec<f64>> = Vec::with_capacity(values_2d.len());
    for item in values_2d.iter() {
        let row: Vec<f64> = item.extract()?;
        values.push(row);
    }
    let data = CrossSectionData {
        values,
        pressure_levels: pres_slice.to_vec(),
        distances: dist_slice.to_vec(),
    };
    let config = CrossSectionConfig { width, height, p_min, p_max };
    let pixels = render::cross_section::render_cross_section(&data, &config, colormap, vmin, vmax);
    Ok(PyArray1::from_vec(py, pixels))
}

/// Render a Skew-T Log-P diagram from sounding data.
///
/// Args:
///     pressure: 1D numpy float64 array of pressure levels (hPa), surface to top
///     temperature: 1D numpy float64 array of temperatures (°C)
///     dewpoint: 1D numpy float64 array of dewpoints (°C)
///     wind_speed: Optional 1D numpy float64 array of wind speeds (knots)
///     wind_dir: Optional 1D numpy float64 array of wind directions (degrees)
///     width: Image width in pixels (default 800)
///     height: Image height in pixels (default 800)
///
/// Returns:
///     numpy uint8 array, length height*width*4 (RGBA row-major)
#[pyfunction]
#[pyo3(signature = (pressure, temperature, dewpoint, wind_speed=None, wind_dir=None, width=800, height=800))]
fn render_skewt_py<'py>(
    py: Python<'py>,
    pressure: PyReadonlyArray1<f64>,
    temperature: PyReadonlyArray1<f64>,
    dewpoint: PyReadonlyArray1<f64>,
    wind_speed: Option<PyReadonlyArray1<f64>>,
    wind_dir: Option<PyReadonlyArray1<f64>>,
    width: u32,
    height: u32,
) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let data = render::skewt::SkewTData {
        pressure: pressure.as_slice()?.to_vec(),
        temperature: temperature.as_slice()?.to_vec(),
        dewpoint: dewpoint.as_slice()?.to_vec(),
        wind_speed: wind_speed.map(|ws| ws.as_slice().unwrap_or(&[]).to_vec()),
        wind_dir: wind_dir.map(|wd| wd.as_slice().unwrap_or(&[]).to_vec()),
    };
    let config = render::skewt::SkewTConfig {
        width,
        height,
        ..Default::default()
    };
    let pixels = render::skewt::render_skewt(&data, &config);
    Ok(PyArray1::from_vec(py, pixels))
}

/// Render a hodograph from sounding wind data.
///
/// Args:
///     pressure: 1D numpy float64 array of pressure levels (hPa)
///     u_wind: 1D numpy float64 array of U-wind components (knots)
///     v_wind: 1D numpy float64 array of V-wind components (knots)
///     size: Square image size in pixels (default 400)
///
/// Returns:
///     numpy uint8 array, length size*size*4 (RGBA row-major)
#[pyfunction]
#[pyo3(signature = (pressure, u_wind, v_wind, size=400))]
fn render_hodograph_py<'py>(
    py: Python<'py>,
    pressure: PyReadonlyArray1<f64>,
    u_wind: PyReadonlyArray1<f64>,
    v_wind: PyReadonlyArray1<f64>,
    size: u32,
) -> PyResult<Bound<'py, PyArray1<u8>>> {
    let data = render::hodograph::HodographData {
        pressure: pressure.as_slice()?.to_vec(),
        u_wind: u_wind.as_slice()?.to_vec(),
        v_wind: v_wind.as_slice()?.to_vec(),
    };
    let config = render::hodograph::HodographConfig {
        size,
        ..Default::default()
    };
    let pixels = render::hodograph::render_hodograph(&data, &config);
    Ok(PyArray1::from_vec(py, pixels))
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
// Dynamics / kinematics (2D grid operations)
// ──────────────────────────────────────────────────────────

/// Compute ∂f/∂x using centered finite differences (forward/backward at edges).
#[pyfunction]
fn gradient_x<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::gradient_x(values.as_slice()?, nx, ny, dx);
    Ok(PyArray1::from_vec(py, result))
}

/// Compute ∂f/∂y using centered finite differences (forward/backward at edges).
#[pyfunction]
fn gradient_y<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::gradient_y(values.as_slice()?, nx, ny, dy);
    Ok(PyArray1::from_vec(py, result))
}

/// Laplacian ∇²f = ∂²f/∂x² + ∂²f/∂y².
#[pyfunction]
fn grid_laplacian<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::laplacian(values.as_slice()?, nx, ny, dx, dy);
    Ok(PyArray1::from_vec(py, result))
}

/// Horizontal divergence: ∂u/∂x + ∂v/∂y.
#[pyfunction]
fn divergence<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::divergence(u.as_slice()?, v.as_slice()?, nx, ny, dx, dy);
    Ok(PyArray1::from_vec(py, result))
}

/// Relative vorticity: ∂v/∂x - ∂u/∂y.
#[pyfunction]
fn vorticity<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::vorticity(u.as_slice()?, v.as_slice()?, nx, ny, dx, dy);
    Ok(PyArray1::from_vec(py, result))
}

/// Absolute vorticity: relative vorticity + Coriolis.
#[pyfunction]
fn absolute_vorticity<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    lats: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::absolute_vorticity(
        u.as_slice()?, v.as_slice()?, lats.as_slice()?,
        nx, ny, dx, dy,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Coriolis parameter f = 2Ω sin(φ).
#[pyfunction]
fn coriolis_parameter(lat_deg: f64) -> f64 {
    dynamics::coriolis_parameter(lat_deg)
}

/// Stretching deformation: ∂u/∂x - ∂v/∂y.
#[pyfunction]
fn stretching_deformation<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::stretching_deformation(u.as_slice()?, v.as_slice()?, nx, ny, dx, dy);
    Ok(PyArray1::from_vec(py, result))
}

/// Shearing deformation: ∂v/∂x + ∂u/∂y.
#[pyfunction]
fn shearing_deformation<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::shearing_deformation(u.as_slice()?, v.as_slice()?, nx, ny, dx, dy);
    Ok(PyArray1::from_vec(py, result))
}

/// Total deformation: √(stretching² + shearing²).
#[pyfunction]
fn total_deformation<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::total_deformation(u.as_slice()?, v.as_slice()?, nx, ny, dx, dy);
    Ok(PyArray1::from_vec(py, result))
}

/// Advection of a scalar field: -u(∂s/∂x) - v(∂s/∂y).
#[pyfunction]
fn grid_advection<'py>(
    py: Python<'py>,
    scalar: PyReadonlyArray1<f64>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::advection(
        scalar.as_slice()?, u.as_slice()?, v.as_slice()?,
        nx, ny, dx, dy,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Temperature advection: -u(∂T/∂x) - v(∂T/∂y).
#[pyfunction]
fn temperature_advection<'py>(
    py: Python<'py>,
    t: PyReadonlyArray1<f64>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::temperature_advection(
        t.as_slice()?, u.as_slice()?, v.as_slice()?,
        nx, ny, dx, dy,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Moisture advection: -u(∂q/∂x) - v(∂q/∂y).
#[pyfunction]
fn moisture_advection<'py>(
    py: Python<'py>,
    q: PyReadonlyArray1<f64>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::moisture_advection(
        q.as_slice()?, u.as_slice()?, v.as_slice()?,
        nx, ny, dx, dy,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// 2D Petterssen frontogenesis function.
#[pyfunction]
fn frontogenesis_2d<'py>(
    py: Python<'py>,
    theta: PyReadonlyArray1<f64>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::frontogenesis_2d(
        theta.as_slice()?, u.as_slice()?, v.as_slice()?,
        nx, ny, dx, dy,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Q-vector components (Q1, Q2). Returns a dict with keys 'q1' and 'q2'.
#[pyfunction]
fn q_vector<'py>(
    py: Python<'py>,
    t: PyReadonlyArray1<f64>,
    u_geo: PyReadonlyArray1<f64>, v_geo: PyReadonlyArray1<f64>,
    p_hpa: f64,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let (q1, q2) = dynamics::q_vector(
        t.as_slice()?, u_geo.as_slice()?, v_geo.as_slice()?,
        p_hpa, nx, ny, dx, dy,
    );
    let dict = PyDict::new(py);
    dict.set_item("q1", PyArray1::from_vec(py, q1))?;
    dict.set_item("q2", PyArray1::from_vec(py, q2))?;
    Ok(dict)
}

/// Q-vector convergence: -2∇·Q.
#[pyfunction]
fn q_vector_convergence<'py>(
    py: Python<'py>,
    q1: PyReadonlyArray1<f64>, q2: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::q_vector_convergence(
        q1.as_slice()?, q2.as_slice()?, nx, ny, dx, dy,
    );
    Ok(PyArray1::from_vec(py, result))
}

/// Wind speed: √(u² + v²).
#[pyfunction]
fn wind_speed<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::wind_speed(u.as_slice()?, v.as_slice()?);
    Ok(PyArray1::from_vec(py, result))
}

/// Meteorological wind direction (degrees, 0=N, 90=E).
#[pyfunction]
fn wind_direction<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = dynamics::wind_direction(u.as_slice()?, v.as_slice()?);
    Ok(PyArray1::from_vec(py, result))
}

/// Wind components (u, v) from speed and direction. Returns dict with 'u' and 'v'.
#[pyfunction]
fn wind_components<'py>(
    py: Python<'py>,
    speed: PyReadonlyArray1<f64>, direction: PyReadonlyArray1<f64>,
) -> PyResult<Bound<'py, PyDict>> {
    let (u, v) = dynamics::wind_components(speed.as_slice()?, direction.as_slice()?);
    let dict = PyDict::new(py);
    dict.set_item("u", PyArray1::from_vec(py, u))?;
    dict.set_item("v", PyArray1::from_vec(py, v))?;
    Ok(dict)
}

/// Geostrophic wind from geopotential height. Returns dict with 'u_geo' and 'v_geo'.
#[pyfunction]
fn geostrophic_wind<'py>(
    py: Python<'py>,
    height: PyReadonlyArray1<f64>, lats: PyReadonlyArray1<f64>,
    nx: usize, ny: usize, dx: f64, dy: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let (ug, vg) = dynamics::geostrophic_wind(
        height.as_slice()?, lats.as_slice()?, nx, ny, dx, dy,
    );
    let dict = PyDict::new(py);
    dict.set_item("u_geo", PyArray1::from_vec(py, ug))?;
    dict.set_item("v_geo", PyArray1::from_vec(py, vg))?;
    Ok(dict)
}

/// Ageostrophic wind: (u - u_geo, v - v_geo). Returns dict with 'u_ageo' and 'v_ageo'.
#[pyfunction]
fn ageostrophic_wind<'py>(
    py: Python<'py>,
    u: PyReadonlyArray1<f64>, v: PyReadonlyArray1<f64>,
    u_geo: PyReadonlyArray1<f64>, v_geo: PyReadonlyArray1<f64>,
) -> PyResult<Bound<'py, PyDict>> {
    let (ua, va) = dynamics::ageostrophic_wind(
        u.as_slice()?, v.as_slice()?, u_geo.as_slice()?, v_geo.as_slice()?,
    );
    let dict = PyDict::new(py);
    dict.set_item("u_ageo", PyArray1::from_vec(py, ua))?;
    dict.set_item("v_ageo", PyArray1::from_vec(py, va))?;
    Ok(dict)
}


// ------------------------------------------------------
// Regridding / Interpolation
// ------------------------------------------------------

/// Regrid data from source grid to a regular lat/lon target grid.
#[pyfunction]
#[pyo3(signature = (values, src_lats, src_lons, src_nx, src_ny, target_lat_min, target_lat_max, target_lon_min, target_lon_max, resolution, method = "bilinear"))]
#[allow(clippy::too_many_arguments)]
fn regrid_data<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    src_lats: PyReadonlyArray1<f64>,
    src_lons: PyReadonlyArray1<f64>,
    src_nx: usize,
    src_ny: usize,
    target_lat_min: f64,
    target_lat_max: f64,
    target_lon_min: f64,
    target_lon_max: f64,
    resolution: f64,
    method: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let interp = regrid::InterpMethod::from_str_loose(method)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
            format!("Unknown interpolation method '{}'. Use: bilinear, nearest, bicubic, budget", method)
        ))?;

    let target = regrid::GridSpec::regular(target_lat_min, target_lat_max, target_lon_min, target_lon_max, resolution);

    let result = regrid::regrid(
        values.as_slice()?,
        src_lats.as_slice()?,
        src_lons.as_slice()?,
        src_nx, src_ny,
        &target, interp,
    );

    let dict = PyDict::new(py);
    let target_lats = target.lats();
    let target_lons = target.lons();
    dict.set_item("values", PyArray2::from_vec2(py, &result.chunks(target.nx).map(|c| c.to_vec()).collect::<Vec<_>>())
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}", e)))?)?;
    dict.set_item("lats", PyArray1::from_vec(py, target_lats))?;
    dict.set_item("lons", PyArray1::from_vec(py, target_lons))?;
    dict.set_item("nx", target.nx)?;
    dict.set_item("ny", target.ny)?;
    Ok(dict)
}

/// Interpolate gridded data to a set of target lat/lon points.
#[pyfunction]
#[pyo3(signature = (values, lats, lons, nx, ny, target_lats, target_lons, method = "bilinear"))]
#[allow(clippy::too_many_arguments)]
fn interpolate_to_points<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    lats: PyReadonlyArray1<f64>,
    lons: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    target_lats: PyReadonlyArray1<f64>,
    target_lons: PyReadonlyArray1<f64>,
    method: &str,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let interp = regrid::InterpMethod::from_str_loose(method)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
            format!("Unknown interpolation method '{}'. Use: bilinear, nearest, bicubic, budget", method)
        ))?;

    let result = regrid::interpolate_points(
        values.as_slice()?,
        lats.as_slice()?,
        lons.as_slice()?,
        nx, ny,
        target_lats.as_slice()?,
        target_lons.as_slice()?,
        interp,
    );

    Ok(PyArray1::from_vec(py, result))
}

/// Extract a cross-section along a great-circle path.
#[pyfunction]
#[pyo3(signature = (values, lats, lons, nx, ny, start_lat, start_lon, end_lat, end_lon, n_points = 100, method = "bilinear"))]
#[allow(clippy::too_many_arguments)]
fn cross_section_native<'py>(
    py: Python<'py>,
    values: PyReadonlyArray1<f64>,
    lats: PyReadonlyArray1<f64>,
    lons: PyReadonlyArray1<f64>,
    nx: usize,
    ny: usize,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    n_points: usize,
    method: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let interp = regrid::InterpMethod::from_str_loose(method)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
            format!("Unknown interpolation method '{}'. Use: bilinear, nearest, bicubic, budget", method)
        ))?;

    let (vals, dists) = regrid::cross_section_data(
        values.as_slice()?,
        lats.as_slice()?,
        lons.as_slice()?,
        nx, ny,
        (start_lat, start_lon),
        (end_lat, end_lon),
        n_points,
        interp,
    );

    let dict = PyDict::new(py);
    dict.set_item("values", PyArray1::from_vec(py, vals))?;
    dict.set_item("distances_km", PyArray1::from_vec(py, dists))?;
    Ok(dict)
}

/// Interpolate a 3D field to a specific vertical level.
#[pyfunction]
#[pyo3(signature = (values_3d, levels, target_level, nx, ny, nz, log_interp = false))]
fn interpolate_vertical_py<'py>(
    py: Python<'py>,
    values_3d: PyReadonlyArray1<f64>,
    levels: PyReadonlyArray1<f64>,
    target_level: f64,
    nx: usize,
    ny: usize,
    nz: usize,
    log_interp: bool,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let result = regrid::interpolate_vertical(
        values_3d.as_slice()?,
        levels.as_slice()?,
        target_level,
        nx, ny, nz,
        log_interp,
    );
    Ok(PyArray1::from_vec(py, result))
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
    // Stability indices and composite parameters
    m.add_function(wrap_pyfunction!(significant_hail_parameter, m)?)?;
    m.add_function(wrap_pyfunction!(derecho_composite_parameter, m)?)?;
    m.add_function(wrap_pyfunction!(supercell_composite_parameter, m)?)?;
    m.add_function(wrap_pyfunction!(critical_angle, m)?)?;
    m.add_function(wrap_pyfunction!(showalter_index, m)?)?;
    m.add_function(wrap_pyfunction!(lifted_index, m)?)?;
    m.add_function(wrap_pyfunction!(k_index, m)?)?;
    m.add_function(wrap_pyfunction!(total_totals, m)?)?;
    m.add_function(wrap_pyfunction!(cross_totals, m)?)?;
    m.add_function(wrap_pyfunction!(vertical_totals, m)?)?;
    m.add_function(wrap_pyfunction!(sweat_index, m)?)?;
    m.add_function(wrap_pyfunction!(boyden_index, m)?)?;
    m.add_function(wrap_pyfunction!(haines_index, m)?)?;
    m.add_function(wrap_pyfunction!(fosberg_fire_weather_index, m)?)?;
    m.add_function(wrap_pyfunction!(hot_dry_windy, m)?)?;
    m.add_function(wrap_pyfunction!(bulk_richardson_number, m)?)?;
    m.add_function(wrap_pyfunction!(dendritic_growth_zone, m)?)?;
    m.add_function(wrap_pyfunction!(warm_nose_check, m)?)?;
    m.add_function(wrap_pyfunction!(freezing_rain_composite, m)?)?;
    m.add_function(wrap_pyfunction!(convective_inhibition_depth, m)?)?;
    // Rendering functions
    m.add_function(wrap_pyfunction!(render_map, m)?)?;
    m.add_function(wrap_pyfunction!(render_array, m)?)?;
    m.add_function(wrap_pyfunction!(save_png, m)?)?;
    m.add_function(wrap_pyfunction!(colormaps, m)?)?;
    m.add_function(wrap_pyfunction!(render_filled_contours, m)?)?;
    m.add_function(wrap_pyfunction!(overlay_contours_py, m)?)?;
    m.add_function(wrap_pyfunction!(overlay_wind_barbs_py, m)?)?;
    m.add_function(wrap_pyfunction!(overlay_streamlines_py, m)?)?;
    // Station model and cross-section rendering
    m.add_function(wrap_pyfunction!(render_station_plot, m)?)?;
    m.add_function(wrap_pyfunction!(render_cross_section, m)?)?;
    // Skew-T and Hodograph rendering
    m.add_function(wrap_pyfunction!(render_skewt_py, m)?)?;
    m.add_function(wrap_pyfunction!(render_hodograph_py, m)?)?;
    // Dynamics / kinematics (2D grid operations)
    m.add_function(wrap_pyfunction!(gradient_x, m)?)?;
    m.add_function(wrap_pyfunction!(gradient_y, m)?)?;
    m.add_function(wrap_pyfunction!(grid_laplacian, m)?)?;
    m.add_function(wrap_pyfunction!(divergence, m)?)?;
    m.add_function(wrap_pyfunction!(vorticity, m)?)?;
    m.add_function(wrap_pyfunction!(absolute_vorticity, m)?)?;
    m.add_function(wrap_pyfunction!(coriolis_parameter, m)?)?;
    m.add_function(wrap_pyfunction!(stretching_deformation, m)?)?;
    m.add_function(wrap_pyfunction!(shearing_deformation, m)?)?;
    m.add_function(wrap_pyfunction!(total_deformation, m)?)?;
    m.add_function(wrap_pyfunction!(grid_advection, m)?)?;
    m.add_function(wrap_pyfunction!(temperature_advection, m)?)?;
    m.add_function(wrap_pyfunction!(moisture_advection, m)?)?;
    m.add_function(wrap_pyfunction!(frontogenesis_2d, m)?)?;
    m.add_function(wrap_pyfunction!(q_vector, m)?)?;
    m.add_function(wrap_pyfunction!(q_vector_convergence, m)?)?;
    m.add_function(wrap_pyfunction!(wind_speed, m)?)?;
    m.add_function(wrap_pyfunction!(wind_direction, m)?)?;
    m.add_function(wrap_pyfunction!(wind_components, m)?)?;
    m.add_function(wrap_pyfunction!(geostrophic_wind, m)?)?;
    m.add_function(wrap_pyfunction!(ageostrophic_wind, m)?)?;
    // GRIB2 field operations
    m.add_function(wrap_pyfunction!(field_stats, m)?)?;
    m.add_function(wrap_pyfunction!(smooth, m)?)?;
    m.add_function(wrap_pyfunction!(convert_units, m)?)?;
    m.add_function(wrap_pyfunction!(wind_speed_dir, m)?)?;
    // Data source / fallback functions
    m.add_function(wrap_pyfunction!(model_data_sources, m)?)?;
    // Regridding / Interpolation
    m.add_function(wrap_pyfunction!(regrid_data, m)?)?;
    m.add_function(wrap_pyfunction!(interpolate_to_points, m)?)?;
    m.add_function(wrap_pyfunction!(cross_section_native, m)?)?;
    m.add_function(wrap_pyfunction!(interpolate_vertical_py, m)?)?;
    m.add("__version__", "0.1.0")?;
    Ok(())
}
