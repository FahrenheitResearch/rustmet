use serde::Serialize;
use crate::output::{print_json, print_error};

#[derive(Serialize)]
struct DownloadResponse {
    status: &'static str,
    model: String,
    run: String,
    fhour: String,
    output: String,
    message: String,
}

pub fn run(model: &str, run: &str, fhour: &str, output: &str, pretty: bool) {
    let valid_models = ["hrrr", "gfs", "nam", "rap", "mrms"];
    if !valid_models.contains(&model) {
        print_error(&format!(
            "unknown model '{}'. Valid models: {}",
            model,
            valid_models.join(", ")
        ));
    }

    let resp = DownloadResponse {
        status: "stub",
        model: model.to_string(),
        run: run.to_string(),
        fhour: fhour.to_string(),
        output: output.to_string(),
        message: "Download not yet wired — will use rustmet-core download infrastructure".to_string(),
    };
    print_json(&resp, pretty);
}
