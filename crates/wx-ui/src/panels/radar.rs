use crate::state::AppState;
use crate::theme;

const NEXRAD_SITES: &[(&str, &str, f64, f64)] = &[
    ("KTLX", "Oklahoma City, OK", 35.333, -97.278),
    ("KFWS", "Dallas/Fort Worth, TX", 32.573, -97.303),
    ("KLIX", "New Orleans, LA", 30.337, -89.826),
    ("KLOT", "Chicago, IL", 41.604, -88.085),
    ("KBMX", "Birmingham, AL", 33.172, -86.770),
    ("KMOB", "Mobile, AL", 30.679, -88.240),
    ("KJAX", "Jacksonville, FL", 30.485, -81.702),
    ("KTBW", "Tampa Bay, FL", 27.706, -82.402),
    ("KAMX", "Miami, FL", 25.611, -80.413),
    ("KGRK", "Fort Hood, TX", 30.722, -97.383),
    ("KLZK", "Little Rock, AR", 34.836, -92.262),
    ("KPAH", "Paducah, KY", 37.068, -88.772),
    ("KDVN", "Davenport, IA", 41.612, -90.581),
    ("KMPX", "Minneapolis, MN", 44.849, -93.566),
    ("KDTX", "Detroit, MI", 42.700, -83.472),
    ("KBOX", "Boston, MA", 41.956, -71.137),
    ("KOKX", "New York City, NY", 40.866, -72.864),
    ("KLWX", "Sterling, VA (DC)", 38.975, -77.478),
    ("KFCX", "Roanoke, VA", 37.024, -80.274),
    ("KMHX", "Morehead City, NC", 34.776, -76.876),
    ("KGSP", "Greenville, SC", 34.883, -82.220),
    ("KFFC", "Atlanta, GA", 33.364, -84.566),
    ("KEAX", "Kansas City, MO", 38.810, -94.264),
    ("KLSX", "St. Louis, MO", 38.699, -90.683),
    ("KSGF", "Springfield, MO", 37.235, -93.400),
    ("KDDC", "Dodge City, KS", 37.761, -99.969),
    ("KICT", "Wichita, KS", 37.655, -97.443),
    ("KIND", "Indianapolis, IN", 39.708, -86.280),
    ("KILN", "Wilmington, OH", 39.420, -83.822),
    ("KCLE", "Cleveland, OH", 41.413, -81.860),
    ("KPBZ", "Pittsburgh, PA", 40.532, -80.218),
    ("KDIX", "Philadelphia, PA", 39.947, -74.411),
    ("KENX", "Albany, NY", 42.587, -74.064),
    ("KBUF", "Buffalo, NY", 42.949, -78.737),
    ("KGYX", "Portland, ME", 43.891, -70.257),
    ("KDNR", "Denver, CO", 39.787, -104.546),
    ("KPUX", "Pueblo, CO", 38.460, -104.181),
    ("KSLC", "Salt Lake City, UT", 40.969, -111.930),
    ("KBOI", "Boise, ID", 43.491, -116.236),
    ("KMSX", "Missoula, MT", 47.041, -113.986),
    ("KATX", "Seattle, WA", 48.195, -122.496),
    ("KRTX", "Portland, OR", 45.715, -122.966),
    ("KMUX", "San Francisco, CA", 37.155, -121.898),
    ("KVBX", "Vandenberg, CA", 34.838, -120.397),
    ("KSOX", "Santa Ana Mtns, CA", 33.818, -117.636),
    ("KFGZ", "Flagstaff, AZ", 35.233, -111.198),
    ("KEMX", "Tucson, AZ", 31.893, -110.630),
    ("KABX", "Albuquerque, NM", 35.150, -106.824),
];

pub fn radar_panel(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("NEXRAD Radar");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Live NEXRAD Level-II radar data viewer").color(theme::TEXT_DIM));
    ui.add_space(12.0);

    // Status
    ui.group(|ui| {
        ui.label(egui::RichText::new("STATUS").small().strong().color(theme::ACCENT));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(
            "NEXRAD radar integration is connected to the wx-radar crate.\n\
             Real-time Level-II data download and rendering coming in the next release.\n\n\
             For now, GRIB2 composite reflectivity from HRRR/RAP is available in Map View."
        ).color(theme::TEXT_DIM));
        ui.add_space(8.0);
        let btn = egui::Button::new(
            egui::RichText::new("Download HRRR Reflectivity").color(egui::Color32::WHITE),
        ).fill(theme::ACCENT);
        if ui.add(btn).clicked() {
            state.dl_model = 0; // HRRR
            state.dl_fhours = "0".to_string();
            state.dl_vars = "REFC".to_string();
            state.dl_run = "latest".to_string();
            state.active_view = crate::state::View::Download;
        }
    });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    // NEXRAD site map
    ui.label(egui::RichText::new("NEXRAD SITES").small().strong().color(theme::TEXT_DIM));
    ui.add_space(4.0);

    // Search filter
    ui.horizontal(|ui| {
        ui.label("Search:");
        // Simple static filter - just show the full list
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("radar_sites")
            .striped(true)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("ID").strong().small());
                ui.label(egui::RichText::new("Location").strong().small());
                ui.label(egui::RichText::new("Lat").strong().small());
                ui.label(egui::RichText::new("Lon").strong().small());
                ui.end_row();

                for &(id, location, lat, lon) in NEXRAD_SITES {
                    ui.label(egui::RichText::new(id).small().color(theme::ACCENT).family(egui::FontFamily::Monospace));
                    ui.label(egui::RichText::new(location).small());
                    ui.label(egui::RichText::new(format!("{:.2}", lat)).small().color(theme::TEXT_DIM));
                    ui.label(egui::RichText::new(format!("{:.2}", lon)).small().color(theme::TEXT_DIM));
                    ui.end_row();
                }
            });
    });
}
