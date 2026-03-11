use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use rustmet_core::grib2::{self, Grib2File, Grib2Message};

// ── Navigation ──────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Map,
    Sounding,
    Hodograph,
    Radar,
    Download,
    Info,
}

impl View {
    pub fn label(&self) -> &'static str {
        match self {
            View::Map => "Map View",
            View::Sounding => "Skew-T",
            View::Hodograph => "Hodograph",
            View::Radar => "Radar",
            View::Download => "Download",
            View::Info => "File Info",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            View::Map => "\u{1F5FA}",
            View::Sounding => "\u{1F321}",
            View::Hodograph => "\u{1F300}",
            View::Radar => "\u{1F4E1}",
            View::Download => "\u{2B07}",
            View::Info => "\u{2139}",
        }
    }
}

pub const ALL_VIEWS: &[View] = &[
    View::Map,
    View::Sounding,
    View::Hodograph,
    View::Radar,
    View::Download,
    View::Info,
];

// ── Render mode ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderMode {
    Raster,
    FilledContour,
}

// ── GRIB2 message metadata ─────────────────────────────────

pub struct MessageInfo {
    pub index: usize,
    pub name: String,
    pub units: String,
    pub level: String,
    pub forecast_hr: u32,
    pub nx: u32,
    pub ny: u32,
}

impl MessageInfo {
    pub fn from_message(index: usize, msg: &Grib2Message) -> Self {
        let name = grib2::parameter_name(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        )
        .to_string();
        let units = grib2::parameter_units(
            msg.discipline,
            msg.product.parameter_category,
            msg.product.parameter_number,
        )
        .to_string();
        let level = format!(
            "{} {:.0}",
            grib2::level_name(msg.product.level_type),
            msg.product.level_value
        );
        Self {
            index,
            name,
            units,
            level,
            forecast_hr: msg.product.forecast_time,
            nx: msg.grid.nx,
            ny: msg.grid.ny,
        }
    }
}

// ── Download status ─────────────────────────────────────────

pub enum DownloadEvent {
    Progress(String, f32),
    Complete(PathBuf),
    Error(String),
}

pub enum RadarEvent {
    Status(String),
    Data(Vec<u8>),
    Error(String),
}

// ── Colormap names ──────────────────────────────────────────

pub const COLORMAP_NAMES: &[&str] = &[
    "temperature",
    "dewpoint",
    "wind",
    "reflectivity",
    "cape",
    "relative_humidity",
    "vorticity",
    "pressure",
    "precipitation",
    "snow",
    "ice",
    "visibility",
    "cloud_cover",
    "helicity",
    "divergence",
    "theta_e",
    "nws_reflectivity",
    "nws_precip",
    "goes_ir",
];

// ── Application state ───────────────────────────────────────

pub struct AppState {
    // Navigation
    pub active_view: View,

    // File
    pub file_path: Option<PathBuf>,
    pub grib: Option<Grib2File>,
    pub messages: Vec<MessageInfo>,
    pub selected_msg: Option<usize>,

    // Unpacked field data (cached for current selection)
    pub field_values: Option<Vec<f64>>,
    pub field_nx: usize,
    pub field_ny: usize,
    pub scan_mode: u8,

    // Render
    pub field_texture: Option<egui::TextureHandle>,
    pub colormap_idx: usize,
    pub vmin: f64,
    pub vmax: f64,
    pub auto_range: bool,
    pub render_mode: RenderMode,
    pub needs_rerender: bool,
    pub contour_levels: usize,
    pub show_colorbar: bool,

    // Sounding / hodograph
    pub sounding_grid_i: usize,
    pub sounding_grid_j: usize,

    // Map interaction
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub dragging: bool,

    // Download
    pub dl_model: usize,
    pub dl_run: String,
    pub dl_fhours: String,
    pub dl_vars: String,
    pub dl_output: String,
    pub dl_status: Vec<String>,
    pub dl_progress: f32,
    pub dl_active: bool,
    pub dl_rx: Option<mpsc::Receiver<DownloadEvent>>,

    // Status bar
    pub status: String,
    pub hover_value: Option<f64>,
    pub hover_grid: Option<(usize, usize)>,

    // Recent files
    pub recent_files: Vec<PathBuf>,

    // Export
    pub last_export: Option<PathBuf>,

    // Radar
    pub radar_station_idx: usize,
    pub radar_file: Option<wx_radar::level2::Level2File>,
    pub radar_product_idx: usize,
    pub radar_sweep_idx: usize,
    pub radar_texture: Option<egui::TextureHandle>,
    pub radar_downloading: Arc<Mutex<bool>>,
    pub radar_rx: Option<mpsc::Receiver<RadarEvent>>,
    pub radar_status: String,
    pub radar_range_km: f64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_view: View::Download,
            file_path: None,
            grib: None,
            messages: Vec::new(),
            selected_msg: None,
            field_values: None,
            field_nx: 0,
            field_ny: 0,
            scan_mode: 0,
            field_texture: None,
            colormap_idx: 0,
            vmin: 0.0,
            vmax: 100.0,
            auto_range: true,
            render_mode: RenderMode::Raster,
            needs_rerender: false,
            contour_levels: 20,
            show_colorbar: true,
            sounding_grid_i: 0,
            sounding_grid_j: 0,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            dragging: false,
            dl_model: 0,
            dl_run: "latest".into(),
            dl_fhours: "0".into(),
            dl_vars: String::new(),
            dl_output: "./data".into(),
            dl_status: Vec::new(),
            dl_progress: 0.0,
            dl_active: false,
            dl_rx: None,
            status: "Ready".into(),
            hover_value: None,
            hover_grid: None,
            recent_files: Vec::new(),
            last_export: None,
            radar_station_idx: 0,
            radar_file: None,
            radar_product_idx: 0,
            radar_sweep_idx: 0,
            radar_texture: None,
            radar_downloading: Arc::new(Mutex::new(false)),
            radar_rx: None,
            radar_status: String::new(),
            radar_range_km: 0.0,
        }
    }
}

impl AppState {
    /// Open a GRIB2 file and populate message list.
    pub fn open_file(&mut self, path: PathBuf) {
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.status = format!("Error reading file: {e}");
                return;
            }
        };
        match grib2::Grib2File::from_bytes(&data) {
            Ok(grib) => {
                self.messages = grib
                    .messages
                    .iter()
                    .enumerate()
                    .map(|(i, m)| MessageInfo::from_message(i, m))
                    .collect();
                let n = self.messages.len();
                self.grib = Some(grib);
                self.file_path = Some(path.clone());
                self.selected_msg = None;
                self.field_texture = None;
                self.field_values = None;
                self.status = format!("Opened: {} ({n} messages)", path.display());

                // Add to recent files
                self.recent_files.retain(|p| p != &path);
                self.recent_files.insert(0, path);
                if self.recent_files.len() > 10 {
                    self.recent_files.truncate(10);
                }
            }
            Err(e) => {
                self.status = format!("GRIB2 parse error: {e}");
            }
        }
    }

    /// Select a message and trigger rendering.
    pub fn select_message(&mut self, idx: usize) {
        if self.selected_msg == Some(idx) {
            return;
        }
        self.selected_msg = Some(idx);
        self.unpack_selected();
        self.needs_rerender = true;
    }

    /// Unpack the selected message's field data.
    fn unpack_selected(&mut self) {
        let Some(idx) = self.selected_msg else { return };
        let Some(ref grib) = self.grib else { return };
        if idx >= grib.messages.len() {
            return;
        }
        let msg = &grib.messages[idx];
        self.scan_mode = msg.grid.scan_mode;
        match grib2::unpack_message_normalized(msg) {
            Ok(values) => {
                self.field_nx = msg.grid.nx as usize;
                self.field_ny = msg.grid.ny as usize;
                if self.auto_range {
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for &v in &values {
                        if v.is_finite() {
                            lo = lo.min(v);
                            hi = hi.max(v);
                        }
                    }
                    if lo < hi {
                        self.vmin = lo;
                        self.vmax = hi;
                    }
                }
                self.field_values = Some(values);
                self.status = format!(
                    "{} @ {} | {}x{} | range: {:.1}..{:.1} {}",
                    self.messages[idx].name,
                    self.messages[idx].level,
                    self.field_nx,
                    self.field_ny,
                    self.vmin,
                    self.vmax,
                    self.messages[idx].units,
                );
            }
            Err(e) => {
                self.field_values = None;
                self.status = format!("Unpack error: {e}");
            }
        }
    }

    pub fn select_next_message(&mut self) {
        if self.messages.is_empty() {
            return;
        }
        let next = match self.selected_msg {
            Some(i) if i + 1 < self.messages.len() => i + 1,
            None => 0,
            Some(i) => i,
        };
        self.select_message(next);
    }

    pub fn select_prev_message(&mut self) {
        if self.messages.is_empty() {
            return;
        }
        let prev = match self.selected_msg {
            Some(i) if i > 0 => i - 1,
            None => 0,
            Some(i) => i,
        };
        self.select_message(prev);
    }

    pub fn colormap_name(&self) -> &'static str {
        COLORMAP_NAMES[self.colormap_idx % COLORMAP_NAMES.len()]
    }

    /// Guess a good default colormap based on the current message name.
    pub fn auto_colormap(&mut self) {
        let Some(idx) = self.selected_msg else { return };
        if idx >= self.messages.len() {
            return;
        }
        let name = self.messages[idx].name.to_lowercase();
        let cmap = if name.contains("temperature") || name.contains("tmp") {
            "temperature"
        } else if name.contains("dew") || name.contains("dpt") {
            "dewpoint"
        } else if name.contains("wind") || name.contains("gust") || name.contains("ugrd") || name.contains("vgrd") {
            "wind"
        } else if name.contains("refl") || name.contains("dbz") {
            "reflectivity"
        } else if name.contains("cape") {
            "cape"
        } else if name.contains("rh") || name.contains("relative") {
            "relative_humidity"
        } else if name.contains("vort") {
            "vorticity"
        } else if name.contains("pres") || name.contains("mslp") || name.contains("hgt") {
            "pressure"
        } else if name.contains("precip") || name.contains("apcp") || name.contains("qpf") {
            "precipitation"
        } else if name.contains("snow") {
            "snow"
        } else if name.contains("ice") {
            "ice"
        } else if name.contains("vis") {
            "visibility"
        } else if name.contains("cloud") || name.contains("tcc") {
            "cloud_cover"
        } else if name.contains("heli") || name.contains("srh") {
            "helicity"
        } else if name.contains("div") {
            "divergence"
        } else if name.contains("theta") {
            "theta_e"
        } else {
            "temperature"
        };
        if let Some(pos) = COLORMAP_NAMES.iter().position(|&n| n == cmap) {
            self.colormap_idx = pos;
        }
    }
}
