use serde::Serialize;

/// Print a serializable value as JSON to stdout.
/// Uses compact format by default, pretty if requested.
pub fn print_json<T: Serialize>(value: &T, pretty: bool) {
    let json = if pretty {
        serde_json::to_string_pretty(value).expect("failed to serialize JSON")
    } else {
        serde_json::to_string(value).expect("failed to serialize JSON")
    };
    println!("{}", json);
}

/// Print an error as JSON to stderr and exit with code 1.
pub fn print_error(msg: &str) -> ! {
    eprintln!("{}", serde_json::json!({"error": msg}));
    std::process::exit(1);
}
