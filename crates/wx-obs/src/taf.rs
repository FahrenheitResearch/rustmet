use serde::{Serialize, Deserialize};
use crate::metar::{
    MetarTime, Wind, Visibility, WeatherPhenomenon, CloudLayer,
    parse_metar, // we'll reuse individual token parsers via a helper
};

/// TAF group type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TafGroupType {
    /// Base forecast period.
    Base,
    /// From — abrupt change at a specific time.
    FM,
    /// Temporary fluctuations.
    TEMPO,
    /// Gradual change (becoming).
    BECMG,
    /// Probability (PROB30, PROB40).
    PROB,
}

/// A single forecast group within a TAF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TafGroup {
    pub group_type: TafGroupType,
    pub from_time: Option<MetarTime>,
    pub to_time: Option<MetarTime>,
    pub probability: Option<u8>,
    pub wind: Option<Wind>,
    pub visibility: Option<Visibility>,
    pub weather: Vec<WeatherPhenomenon>,
    pub clouds: Vec<CloudLayer>,
}

/// Parsed Terminal Aerodrome Forecast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Taf {
    pub raw: String,
    pub station: String,
    pub issued: MetarTime,
    pub valid_from: MetarTime,
    pub valid_to: MetarTime,
    pub forecast_groups: Vec<TafGroup>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a raw TAF string into a `Taf` struct.
pub fn parse_taf(raw: &str) -> Result<Taf, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty TAF string".into());
    }

    let tokens: Vec<&str> = raw.split_whitespace().collect();
    if tokens.len() < 4 {
        return Err("TAF too short".into());
    }

    let mut pos: usize = 0;

    // Skip leading "TAF" and optional "AMD"/"COR".
    if tokens[pos].eq_ignore_ascii_case("TAF") {
        pos += 1;
    }
    if pos < tokens.len() && (tokens[pos].eq_ignore_ascii_case("AMD") || tokens[pos].eq_ignore_ascii_case("COR")) {
        pos += 1;
    }

    // Station.
    if pos >= tokens.len() {
        return Err("missing station".into());
    }
    let station = tokens[pos].to_uppercase();
    pos += 1;

    // Issued time: DDHHMMz.
    if pos >= tokens.len() {
        return Err("missing issued time".into());
    }
    let issued = parse_taf_time_6(tokens[pos]).ok_or("invalid issued time")?;
    pos += 1;

    // Valid period: DDHH/DDHH.
    if pos >= tokens.len() {
        return Err("missing valid period".into());
    }
    let (valid_from, valid_to) = parse_valid_period(tokens[pos]).ok_or("invalid valid period")?;
    pos += 1;

    // The rest is forecast groups.
    let mut groups: Vec<TafGroup> = Vec::new();

    // First, collect the base group tokens until we hit a group keyword.
    let base_start = pos;
    let mut base_end = pos;
    while base_end < tokens.len() && !is_group_keyword(tokens[base_end]) {
        base_end += 1;
    }

    if base_end > base_start {
        let base_tokens = &tokens[base_start..base_end];
        groups.push(parse_taf_group_tokens(TafGroupType::Base, None, None, None, base_tokens));
    }
    pos = base_end;

    // Subsequent groups.
    while pos < tokens.len() {
        let tok = tokens[pos];

        if tok.starts_with("FM") && tok.len() == 8 {
            // FM DDHHmm
            let time = parse_taf_time_6(&tok[2..]);
            pos += 1;
            let group_start = pos;
            while pos < tokens.len() && !is_group_keyword(tokens[pos]) {
                pos += 1;
            }
            groups.push(parse_taf_group_tokens(
                TafGroupType::FM, time.clone(), None, None, &tokens[group_start..pos],
            ));
        } else if tok.eq_ignore_ascii_case("TEMPO") || tok.eq_ignore_ascii_case("BECMG") {
            let gtype = if tok.eq_ignore_ascii_case("TEMPO") { TafGroupType::TEMPO } else { TafGroupType::BECMG };
            pos += 1;
            // Next token may be a time period DDHH/DDHH.
            let (from, to) = if pos < tokens.len() {
                if let Some((f, t)) = parse_valid_period(tokens[pos]) {
                    pos += 1;
                    (Some(f), Some(t))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            let group_start = pos;
            while pos < tokens.len() && !is_group_keyword(tokens[pos]) {
                pos += 1;
            }
            groups.push(parse_taf_group_tokens(gtype, from, to, None, &tokens[group_start..pos]));
        } else if tok.starts_with("PROB") {
            let prob: Option<u8> = tok[4..].parse().ok();
            pos += 1;
            // May be followed by TEMPO and time period.
            let gtype = if pos < tokens.len() && tokens[pos].eq_ignore_ascii_case("TEMPO") {
                pos += 1;
                TafGroupType::TEMPO
            } else {
                TafGroupType::PROB
            };
            let (from, to) = if pos < tokens.len() {
                if let Some((f, t)) = parse_valid_period(tokens[pos]) {
                    pos += 1;
                    (Some(f), Some(t))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            let group_start = pos;
            while pos < tokens.len() && !is_group_keyword(tokens[pos]) {
                pos += 1;
            }
            groups.push(parse_taf_group_tokens(gtype, from, to, prob, &tokens[group_start..pos]));
        } else {
            // Unknown token — skip.
            pos += 1;
        }
    }

    Ok(Taf {
        raw: raw.to_string(),
        station,
        issued,
        valid_from,
        valid_to,
        forecast_groups: groups,
    })
}

fn is_group_keyword(tok: &str) -> bool {
    tok.starts_with("FM") && tok.len() == 8
        || tok.eq_ignore_ascii_case("TEMPO")
        || tok.eq_ignore_ascii_case("BECMG")
        || tok.starts_with("PROB")
}

fn parse_taf_time_6(s: &str) -> Option<MetarTime> {
    let s = s.trim_end_matches('Z').trim_end_matches('z');
    if s.len() != 6 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(MetarTime {
        day: s[0..2].parse().ok()?,
        hour: s[2..4].parse().ok()?,
        minute: s[4..6].parse().ok()?,
    })
}

fn parse_valid_period(tok: &str) -> Option<(MetarTime, MetarTime)> {
    // Format: DDHH/DDHH
    let parts: Vec<&str> = tok.split('/').collect();
    if parts.len() != 2 {
        return None;
    }
    if parts[0].len() != 4 || parts[1].len() != 4 {
        return None;
    }
    let from = MetarTime {
        day: parts[0][0..2].parse().ok()?,
        hour: parts[0][2..4].parse().ok()?,
        minute: 0,
    };
    let to = MetarTime {
        day: parts[1][0..2].parse().ok()?,
        hour: parts[1][2..4].parse().ok()?,
        minute: 0,
    };
    Some((from, to))
}

/// Parse weather tokens within a TAF group reusing METAR token parsers.
/// We construct a fake METAR string and parse it, then extract the fields.
fn parse_taf_group_tokens(
    group_type: TafGroupType,
    from_time: Option<MetarTime>,
    to_time: Option<MetarTime>,
    probability: Option<u8>,
    tokens: &[&str],
) -> TafGroup {
    // Build a fake METAR to reuse the parser.
    let fake = format!("XXXX 010000Z {}", tokens.join(" "));
    let parsed = parse_metar(&fake);

    match parsed {
        Ok(m) => TafGroup {
            group_type,
            from_time,
            to_time,
            probability,
            wind: m.wind,
            visibility: m.visibility,
            weather: m.weather,
            clouds: m.clouds,
        },
        Err(_) => TafGroup {
            group_type,
            from_time,
            to_time,
            probability,
            wind: None,
            visibility: None,
            weather: Vec::new(),
            clouds: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_taf() {
        let raw = "TAF KOKC 112050Z 1121/1224 18012G20KT P6SM FEW250 FM120200 20008KT P6SM SCT040";
        let t = parse_taf(raw).unwrap();
        assert_eq!(t.station, "KOKC");
        assert_eq!(t.valid_from.day, 11);
        assert_eq!(t.valid_from.hour, 21);
        assert!(t.forecast_groups.len() >= 2);
        assert_eq!(t.forecast_groups[0].group_type, TafGroupType::Base);
        assert_eq!(t.forecast_groups[1].group_type, TafGroupType::FM);
    }
}
