use eframe::egui;
use crate::state::{AppState, DownloadEvent};
use crate::theme;
use crate::panels;

pub struct WxApp {
    pub state: AppState,
}

impl WxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_dark_theme(&cc.egui_ctx);

        // Handle CLI arg: open file passed as first argument
        let mut state = AppState::default();
        if let Some(path) = std::env::args().nth(1) {
            let p = std::path::PathBuf::from(&path);
            if p.exists() {
                state.open_file(p);
            }
        }

        Self { state }
    }
}

impl eframe::App for WxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll download events
        self.poll_downloads();

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            panels::menu_bar(ui, &mut self.state);
        });

        // Bottom status bar
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(26.0)
            .show(ctx, |ui| {
                crate::widgets::status_bar(ui, &self.state);
            });

        // Left sidebar
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(300.0)
            .min_width(220.0)
            .max_width(500.0)
            .show(ctx, |ui| {
                panels::sidebar(ui, &mut self.state, ctx);
            });

        // Central panel — main view
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.active_view {
                crate::state::View::Map => panels::map_view(ui, &mut self.state, ctx),
                crate::state::View::Sounding => panels::sounding_view(ui, &mut self.state, ctx),
                crate::state::View::Hodograph => panels::hodograph_view(ui, &mut self.state, ctx),
                crate::state::View::Radar => panels::radar_panel(ui, &mut self.state),
                crate::state::View::Download => panels::download_panel(ui, &mut self.state),
                crate::state::View::Info => panels::info_panel(ui, &self.state),
            }
        });

        // Keyboard shortcuts
        self.handle_shortcuts(ctx);
    }
}

impl WxApp {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Ctrl+O: open file
            if i.key_pressed(egui::Key::O) && i.modifiers.command {
                panels::open_file_dialog(&mut self.state);
            }
            // Ctrl+E: export PNG
            if i.key_pressed(egui::Key::E) && i.modifiers.command {
                panels::export_png(&mut self.state);
            }
            // Arrow Down / Up: navigate messages
            if i.key_pressed(egui::Key::ArrowDown) && i.modifiers.is_none() {
                self.state.select_next_message();
            }
            if i.key_pressed(egui::Key::ArrowUp) && i.modifiers.is_none() {
                self.state.select_prev_message();
            }
            // Home: fit zoom
            if i.key_pressed(egui::Key::Home) {
                self.state.zoom = 1.0;
                self.state.pan = egui::Vec2::ZERO;
            }
            // +/-: zoom
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.state.zoom = (self.state.zoom * 1.25).min(20.0);
            }
            if i.key_pressed(egui::Key::Minus) {
                self.state.zoom = (self.state.zoom / 1.25).max(0.1);
            }
            // 1-6: switch views
            if i.key_pressed(egui::Key::Num1) { self.state.active_view = crate::state::View::Map; }
            if i.key_pressed(egui::Key::Num2) { self.state.active_view = crate::state::View::Sounding; }
            if i.key_pressed(egui::Key::Num3) { self.state.active_view = crate::state::View::Hodograph; }
            if i.key_pressed(egui::Key::Num4) { self.state.active_view = crate::state::View::Radar; }
            if i.key_pressed(egui::Key::Num5) { self.state.active_view = crate::state::View::Download; }
            if i.key_pressed(egui::Key::Num6) { self.state.active_view = crate::state::View::Info; }
            // R: toggle render mode
            if i.key_pressed(egui::Key::R) && !i.modifiers.command {
                self.state.render_mode = match self.state.render_mode {
                    crate::state::RenderMode::Raster => crate::state::RenderMode::FilledContour,
                    crate::state::RenderMode::FilledContour => crate::state::RenderMode::Raster,
                };
                self.state.needs_rerender = true;
            }
            // A: auto-range
            if i.key_pressed(egui::Key::A) && !i.modifiers.command {
                self.state.auto_range = !self.state.auto_range;
                if self.state.auto_range {
                    // Re-compute range
                    if let Some(ref vals) = self.state.field_values {
                        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                        for &v in vals {
                            if v.is_finite() { lo = lo.min(v); hi = hi.max(v); }
                        }
                        if lo < hi { self.state.vmin = lo; self.state.vmax = hi; }
                    }
                    self.state.needs_rerender = true;
                }
            }
        });
    }

    fn poll_downloads(&mut self) {
        // Collect events first to avoid borrow conflict
        let events: Vec<DownloadEvent> = self
            .state
            .dl_rx
            .as_ref()
            .map(|rx| {
                let mut evts = Vec::new();
                while let Ok(e) = rx.try_recv() {
                    evts.push(e);
                }
                evts
            })
            .unwrap_or_default();

        for event in events {
            match event {
                DownloadEvent::Progress(msg, pct) => {
                    self.state.dl_status.push(msg);
                    self.state.dl_progress = pct;
                }
                DownloadEvent::Complete(path) => {
                    self.state.dl_status.push(format!("Done: {}", path.display()));
                    self.state.dl_progress = 1.0;
                    self.state.dl_active = false;
                    self.state.open_file(path);
                    self.state.active_view = crate::state::View::Map;
                }
                DownloadEvent::Error(e) => {
                    self.state.dl_status.push(format!("ERROR: {e}"));
                    self.state.dl_active = false;
                }
            }
        }
    }
}
