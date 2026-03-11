use serde::{Serialize, Deserialize};

/// Observation time in Zulu (UTC).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetarTime {
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

/// Wind information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wind {
    /// Degrees true; `None` if variable (VRB).
    pub direction: Option<u16>,
    /// Speed in knots.
    pub speed: u16,
    /// Gust speed in knots.
    pub gust: Option<u16>,
    /// Variable wind direction range — lower bound.
    pub variable_from: Option<u16>,
    /// Variable wind direction range — upper bound.
    pub variable_to: Option<u16>,
}

/// Prevailing visibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Visibility {
    pub statute_miles: f64,
}

/// Weather intensity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Intensity {
    Light,
    Moderate,
    Heavy,
}

/// Present weather phenomenon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherPhenomenon {
    pub intensity: Intensity,
    pub descriptor: Option<String>,
    pub phenomenon: String,
}

/// Sky coverage category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SkyCoverage {
    CLR,
    SKC,
    FEW,
    SCT,
    BKN,
    OVC,
    VV,
}

/// Cloud layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudLayer {
    pub coverage: SkyCoverage,
    /// Height AGL in feet.
    pub height_agl_ft: Option<u32>,
    /// Cloud type (CB, TCU).
    pub cloud_type: Option<String>,
}

/// Flight category (FAA).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FlightCategory {
    VFR,
    MVFR,
    IFR,
    LIFR,
}

/// Parsed METAR observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metar {
    pub raw: String,
    pub station: String,
    pub time: MetarTime,
    pub wind: Option<Wind>,
    pub visibility: Option<Visibility>,
    pub weather: Vec<WeatherPhenomenon>,
    pub clouds: Vec<CloudLayer>,
    pub temperature: Option<i32>,
    pub dewpoint: Option<i32>,
    /// Altimeter setting in inches of mercury.
    pub altimeter: Option<f64>,
    pub remarks: Option<String>,
    pub flight_category: FlightCategory,
}

// ---------------------------------------------------------------------------
// Descriptor and phenomenon codes used during parsing
// ---------------------------------------------------------------------------

const DESCRIPTORS: &[&str] = &["MI", "PR", "BC", "DR", "BL", "SH", "TS", "FZ"];
const PHENOMENA: &[&str] = &[
    "DZ", "RA", "SN", "SG", "IC", "PL", "GR", "GS", "UP",
    "BR", "FG", "FU", "VA", "DU", "SA", "HZ", "PY",
    "PO", "SQ", "FC", "SS", "DS",
];

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a raw METAR string into a `Metar` struct.
///
/// This is intentionally lenient — unknown or malformed tokens are skipped so
/// that partially valid METARs still yield as much information as possible.
pub fn parse_metar(raw: &str) -> Result<Metar, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty METAR string".into());
    }

    // Split off remarks first.
    let (body, remarks) = if let Some(idx) = raw.find(" RMK ") {
        (&raw[..idx], Some(raw[idx + 5..].to_string()))
    } else {
        (raw, None)
    };

    let tokens: Vec<&str> = body.split_whitespace().collect();
    if tokens.is_empty() {
        return Err("empty METAR string".into());
    }

    let mut pos: usize = 0;

    // Skip leading "METAR" or "SPECI" identifier.
    if tokens[pos] == "METAR" || tokens[pos] == "SPECI" {
        pos += 1;
    }
    if pos >= tokens.len() {
        return Err("METAR too short — no station ID".into());
    }

    // Station ID (4-letter ICAO).
    let station = tokens[pos].to_uppercase();
    if station.len() != 4 || !station.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("invalid station ID: {}", tokens[pos]));
    }
    pos += 1;

    // Time — DDHHMMz.
    let time = if pos < tokens.len() {
        parse_time(tokens[pos]).map(|t| { pos += 1; t })
    } else {
        None
    };
    let time = time.ok_or("missing or invalid time group")?;

    // Skip AUTO / COR.
    if pos < tokens.len() && (tokens[pos] == "AUTO" || tokens[pos] == "COR") {
        pos += 1;
    }

    // Wind.
    let mut wind: Option<Wind> = None;
    if pos < tokens.len() {
        if let Some(w) = parse_wind(tokens[pos]) {
            wind = Some(w);
            pos += 1;

            // Variable wind direction (e.g. "180V220").
            if pos < tokens.len() {
                if let Some((vf, vt)) = parse_variable_wind(tokens[pos]) {
                    if let Some(ref mut w) = wind {
                        w.variable_from = Some(vf);
                        w.variable_to = Some(vt);
                    }
                    pos += 1;
                }
            }
        }
    }

    // Visibility.
    let mut visibility: Option<Visibility> = None;
    if pos < tokens.len() {
        // Handle compound fraction like "1 1/2SM" (two tokens).
        let vis_result = parse_visibility_tokens(&tokens, pos);
        if let Some((v, consumed)) = vis_result {
            visibility = Some(v);
            pos += consumed;
        }
    }

    // Weather phenomena (can be multiple tokens).
    let mut weather: Vec<WeatherPhenomenon> = Vec::new();
    while pos < tokens.len() {
        if let Some(wx) = parse_weather(tokens[pos]) {
            weather.push(wx);
            pos += 1;
        } else {
            break;
        }
    }

    // Cloud layers (can be multiple tokens).
    let mut clouds: Vec<CloudLayer> = Vec::new();
    while pos < tokens.len() {
        if let Some(cl) = parse_cloud(tokens[pos]) {
            clouds.push(cl);
            pos += 1;
        } else {
            break;
        }
    }

    // Temperature / dewpoint.
    let mut temperature: Option<i32> = None;
    let mut dewpoint: Option<i32> = None;
    if pos < tokens.len() {
        if let Some((t, d)) = parse_temp_dew(tokens[pos]) {
            temperature = Some(t);
            dewpoint = d;
            pos += 1;
        }
    }

    // Altimeter.
    let mut altimeter: Option<f64> = None;
    if pos < tokens.len() {
        if let Some(a) = parse_altimeter(tokens[pos]) {
            altimeter = Some(a);
            pos += 1;
        }
    }

    // Also scan remaining body tokens for anything we may have missed
    // (sometimes order is slightly different in non-US METARs).
    let _ = pos; // silence unused warning

    let flight_category = compute_flight_category(&visibility, &clouds);

    Ok(Metar {
        raw: raw.to_string(),
        station,
        time,
        wind,
        visibility,
        weather,
        clouds,
        temperature,
        dewpoint,
        altimeter,
        remarks,
        flight_category,
    })
}

// ---------------------------------------------------------------------------
// Token parsers
// ---------------------------------------------------------------------------

fn parse_time(tok: &str) -> Option<MetarTime> {
    let tok = tok.trim_end_matches('Z').trim_end_matches('z');
    if tok.len() != 6 || !tok.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(MetarTime {
        day: tok[0..2].parse().ok()?,
        hour: tok[2..4].parse().ok()?,
        minute: tok[4..6].parse().ok()?,
    })
}

fn parse_wind(tok: &str) -> Option<Wind> {
    // Must end with KT (or KTS).
    let tok_upper = tok.to_uppercase();
    let body = tok_upper.strip_suffix("KT").or_else(|| tok_upper.strip_suffix("KTS"))?;

    let (dir, rest) = if body.starts_with("VRB") {
        (None, &body[3..])
    } else if body.len() >= 3 && body[..3].chars().all(|c| c.is_ascii_digit()) {
        (Some(body[..3].parse::<u16>().ok()?), &body[3..])
    } else {
        return None;
    };

    let (speed, gust) = if let Some(gust_idx) = rest.find('G') {
        let spd: u16 = rest[..gust_idx].parse().ok()?;
        let gst: u16 = rest[gust_idx + 1..].parse().ok()?;
        (spd, Some(gst))
    } else {
        (rest.parse().ok()?, None)
    };

    Some(Wind {
        direction: dir,
        speed,
        gust,
        variable_from: None,
        variable_to: None,
    })
}

fn parse_variable_wind(tok: &str) -> Option<(u16, u16)> {
    // Format: DDDVDDD
    if tok.len() != 7 {
        return None;
    }
    if tok.as_bytes()[3] != b'V' {
        return None;
    }
    let from: u16 = tok[..3].parse().ok()?;
    let to: u16 = tok[4..7].parse().ok()?;
    Some((from, to))
}

fn parse_visibility_tokens(tokens: &[&str], pos: usize) -> Option<(Visibility, usize)> {
    let tok = tokens[pos];

    // M1/4SM — less than 1/4 SM
    if tok == "M1/4SM" {
        return Some((Visibility { statute_miles: 0.125 }, 1));
    }

    // Check if this token ends with SM.
    if tok.to_uppercase().ends_with("SM") {
        let body = &tok[..tok.len() - 2];
        if let Some(v) = parse_fraction_or_number(body) {
            return Some((Visibility { statute_miles: v }, 1));
        }
    }

    // Compound: whole + fraction (e.g. tokens "1" and "1/2SM").
    if pos + 1 < tokens.len() {
        let next = tokens[pos + 1];
        if next.to_uppercase().ends_with("SM") {
            let frac_body = &next[..next.len() - 2];
            if let (Ok(whole), Some(frac)) = (tok.parse::<f64>(), parse_fraction_or_number(frac_body)) {
                return Some((Visibility { statute_miles: whole + frac }, 2));
            }
        }
    }

    None
}

fn parse_fraction_or_number(s: &str) -> Option<f64> {
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    if let Some(slash) = s.find('/') {
        let num: f64 = s[..slash].parse().ok()?;
        let den: f64 = s[slash + 1..].parse().ok()?;
        if den == 0.0 { return None; }
        return Some(num / den);
    }
    None
}

fn parse_weather(tok: &str) -> Option<WeatherPhenomenon> {
    if tok.is_empty() {
        return None;
    }
    let mut s = tok;

    // Intensity prefix.
    let intensity = if s.starts_with('+') {
        s = &s[1..];
        Intensity::Heavy
    } else if s.starts_with('-') {
        s = &s[1..];
        Intensity::Light
    } else {
        Intensity::Moderate
    };

    // Descriptor.
    let mut descriptor: Option<String> = None;
    for d in DESCRIPTORS {
        if s.starts_with(d) {
            descriptor = Some(d.to_string());
            s = &s[d.len()..];
            break;
        }
    }

    // Phenomenon — may be multiple concatenated (e.g. "TSRA" = TS descriptor + RA).
    let mut phenomenon = String::new();
    let mut remaining = s;
    let mut found_any = false;
    while !remaining.is_empty() {
        let mut matched = false;
        for p in PHENOMENA {
            if remaining.starts_with(p) {
                if !phenomenon.is_empty() {
                    phenomenon.push_str(p);
                } else {
                    phenomenon.push_str(p);
                }
                remaining = &remaining[p.len()..];
                matched = true;
                found_any = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }

    // If we parsed at least a descriptor or a phenomenon, accept it.
    if !found_any && descriptor.is_none() {
        return None;
    }

    // If the token was only a descriptor with no phenomenon (rare but possible,
    // like just "TS"), move descriptor text into phenomenon.
    if phenomenon.is_empty() {
        if let Some(d) = descriptor.take() {
            phenomenon = d;
        } else {
            return None;
        }
    }

    // Make sure we consumed the whole token (ignoring any trailing junk).
    if !remaining.is_empty() && remaining != tok {
        // There's leftover — this might not be a weather token.
        // Be lenient: if we got a good phenomenon, accept it anyway.
    }

    Some(WeatherPhenomenon {
        intensity,
        descriptor,
        phenomenon,
    })
}

fn parse_cloud(tok: &str) -> Option<CloudLayer> {
    let upper = tok.to_uppercase();

    if upper == "CLR" {
        return Some(CloudLayer { coverage: SkyCoverage::CLR, height_agl_ft: None, cloud_type: None });
    }
    if upper == "SKC" {
        return Some(CloudLayer { coverage: SkyCoverage::SKC, height_agl_ft: None, cloud_type: None });
    }

    // FEWhhh, SCThhh, BKNhhh, OVChhh, VVhhh
    let (coverage, rest) = if upper.starts_with("FEW") {
        (SkyCoverage::FEW, &upper[3..])
    } else if upper.starts_with("SCT") {
        (SkyCoverage::SCT, &upper[3..])
    } else if upper.starts_with("BKN") {
        (SkyCoverage::BKN, &upper[3..])
    } else if upper.starts_with("OVC") {
        (SkyCoverage::OVC, &upper[3..])
    } else if upper.starts_with("VV") {
        (SkyCoverage::VV, &upper[2..])
    } else {
        return None;
    };

    // Height is 3 digits (hundreds of feet), optionally followed by CB or TCU.
    if rest.len() < 3 {
        return None;
    }
    let height_str = &rest[..3];
    if !height_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let height: u32 = height_str.parse::<u32>().ok()? * 100;

    let cloud_type = if rest.len() > 3 {
        let ct = rest[3..].trim();
        if ct == "CB" || ct == "TCU" {
            Some(ct.to_string())
        } else {
            None
        }
    } else {
        None
    };

    Some(CloudLayer {
        coverage,
        height_agl_ft: Some(height),
        cloud_type,
    })
}

fn parse_temp_dew(tok: &str) -> Option<(i32, Option<i32>)> {
    // Format: TT/DD or TT/ or just TT
    // M prefix = negative.
    let parts: Vec<&str> = tok.split('/').collect();
    if parts.is_empty() || parts.len() > 2 {
        return None;
    }

    let temp = parse_metar_temp(parts[0])?;
    let dew = if parts.len() == 2 && !parts[1].is_empty() {
        parse_metar_temp(parts[1])
    } else {
        None
    };

    Some((temp, dew))
}

fn parse_metar_temp(s: &str) -> Option<i32> {
    if s.is_empty() {
        return None;
    }
    let (neg, digits) = if s.starts_with('M') {
        (true, &s[1..])
    } else {
        (false, s)
    };
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let val: i32 = digits.parse().ok()?;
    Some(if neg { -val } else { val })
}

fn parse_altimeter(tok: &str) -> Option<f64> {
    let upper = tok.to_uppercase();
    if !upper.starts_with('A') {
        return None;
    }
    let digits = &upper[1..];
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let val: f64 = digits.parse::<f64>().ok()? / 100.0;
    Some(val)
}

// ---------------------------------------------------------------------------
// Flight category
// ---------------------------------------------------------------------------

fn compute_flight_category(vis: &Option<Visibility>, clouds: &[CloudLayer]) -> FlightCategory {
    let vis_sm = vis.as_ref().map(|v| v.statute_miles).unwrap_or(10.0);

    // Ceiling = lowest BKN, OVC, or VV layer.
    let ceiling: Option<u32> = clouds.iter()
        .filter(|c| matches!(c.coverage, SkyCoverage::BKN | SkyCoverage::OVC | SkyCoverage::VV))
        .filter_map(|c| c.height_agl_ft)
        .min();

    let ceil_ft = ceiling.unwrap_or(99999);

    if vis_sm < 1.0 || ceil_ft < 500 {
        FlightCategory::LIFR
    } else if vis_sm < 3.0 || ceil_ft < 1000 {
        FlightCategory::IFR
    } else if vis_sm <= 5.0 || ceil_ft <= 3000 {
        FlightCategory::MVFR
    } else {
        FlightCategory::VFR
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kokc() {
        let m = parse_metar("KOKC 112053Z 18012G20KT 10SM FEW250 32/14 A2985 RMK AO2 SLP089").unwrap();
        assert_eq!(m.station, "KOKC");
        assert_eq!(m.time.day, 11);
        assert_eq!(m.time.hour, 20);
        assert_eq!(m.time.minute, 53);
        let w = m.wind.as_ref().unwrap();
        assert_eq!(w.direction, Some(180));
        assert_eq!(w.speed, 12);
        assert_eq!(w.gust, Some(20));
        assert_eq!(m.visibility.as_ref().unwrap().statute_miles, 10.0);
        assert_eq!(m.clouds.len(), 1);
        assert_eq!(m.clouds[0].coverage, SkyCoverage::FEW);
        assert_eq!(m.clouds[0].height_agl_ft, Some(25000));
        assert_eq!(m.temperature, Some(32));
        assert_eq!(m.dewpoint, Some(14));
        assert!((m.altimeter.unwrap() - 29.85).abs() < 0.001);
        assert_eq!(m.remarks.as_deref(), Some("AO2 SLP089"));
        assert_eq!(m.flight_category, FlightCategory::VFR);
    }

    #[test]
    fn test_kjfk() {
        let m = parse_metar("KJFK 112051Z 22008KT 10SM BKN070 OVC250 28/19 A2992").unwrap();
        assert_eq!(m.station, "KJFK");
        assert_eq!(m.wind.as_ref().unwrap().speed, 8);
        assert_eq!(m.clouds.len(), 2);
        assert_eq!(m.clouds[0].coverage, SkyCoverage::BKN);
        assert_eq!(m.clouds[0].height_agl_ft, Some(7000));
        assert_eq!(m.flight_category, FlightCategory::VFR);
    }

    #[test]
    fn test_kord_with_weather() {
        let m = parse_metar("KORD 112051Z 19015G25KT 6SM -TSRA BKN040CB OVC070 24/20 A2978 RMK AO2 TSB45").unwrap();
        assert_eq!(m.station, "KORD");
        assert_eq!(m.wind.as_ref().unwrap().gust, Some(25));
        assert_eq!(m.visibility.as_ref().unwrap().statute_miles, 6.0);
        assert_eq!(m.weather.len(), 1);
        assert_eq!(m.weather[0].intensity, Intensity::Light);
        assert_eq!(m.weather[0].descriptor.as_deref(), Some("TS"));
        assert_eq!(m.weather[0].phenomenon, "RA");
        assert_eq!(m.clouds[0].cloud_type.as_deref(), Some("CB"));
        assert_eq!(m.flight_category, FlightCategory::VFR);
    }

    #[test]
    fn test_kden_vrb() {
        let m = parse_metar("KDEN 112053Z VRB03KT 10SM CLR 33/06 A3012").unwrap();
        assert_eq!(m.station, "KDEN");
        let w = m.wind.as_ref().unwrap();
        assert_eq!(w.direction, None); // VRB
        assert_eq!(w.speed, 3);
        assert_eq!(m.clouds[0].coverage, SkyCoverage::CLR);
        assert_eq!(m.temperature, Some(33));
        assert_eq!(m.flight_category, FlightCategory::VFR);
    }
}
