use thiserror::Error;

#[derive(Error, Debug)]
pub enum RustmetError {
    #[error("GRIB2 parse error: {0}")]
    Parse(String),

    #[error("GRIB2 unpack error: {0}")]
    Unpack(String),

    #[error("Unsupported template {template}: {detail}")]
    UnsupportedTemplate { template: u16, detail: String },

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("HTTP status {code}: {url}")]
    HttpStatus { code: u16, url: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("No data: {0}")]
    NoData(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

pub type Result<T> = std::result::Result<T, RustmetError>;
