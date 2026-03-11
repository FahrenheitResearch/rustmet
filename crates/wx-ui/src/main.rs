mod app;
mod state;
mod theme;
mod panels;
mod widgets;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("WxView — Atmospheric Analysis Engine"),
        ..Default::default()
    };

    eframe::run_native(
        "WxView",
        options,
        Box::new(|cc| Ok(Box::new(app::WxApp::new(cc)))),
    )
}
