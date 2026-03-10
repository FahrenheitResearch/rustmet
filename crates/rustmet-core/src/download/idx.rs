/// A single entry from a GRIB2 .idx index file.
///
/// Format: `msg_num:byte_offset:d=YYYYMMDDHH:variable:level:forecast_time:`
#[derive(Debug, Clone)]
pub struct IdxEntry {
    pub msg_num: u32,
    pub byte_offset: u64,
    pub date: String,
    pub variable: String,
    pub level: String,
    pub forecast: String,
}

/// Parse the text content of a GRIB2 .idx file into a list of entries.
pub fn parse_idx(text: &str) -> Vec<IdxEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(7, ':').collect();
        if parts.len() < 6 {
            continue;
        }
        let msg_num = match parts[0].parse::<u32>() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let byte_offset = match parts[1].parse::<u64>() {
            Ok(n) => n,
            Err(_) => continue,
        };
        // parts[2] is like "d=2026031012"
        let date = parts[2]
            .strip_prefix("d=")
            .unwrap_or(parts[2])
            .to_string();
        let variable = parts[3].to_string();
        let level = parts[4].to_string();
        let forecast = parts[5].trim_end_matches(':').to_string();

        entries.push(IdxEntry {
            msg_num,
            byte_offset,
            date,
            variable,
            level,
            forecast,
        });
    }
    entries
}

/// Find entries matching a search pattern.
///
/// Pattern format: `"VAR:level"` — matches variable name exactly and level as a substring.
/// If the pattern contains no colon, it matches only the variable name.
///
/// Examples:
/// - `"TMP:2 m above ground"` matches TMP at 2m
/// - `"CAPE:surface"` matches surface CAPE
/// - `"REFC"` matches any REFC entry
/// - `"MXUPHL"` matches any max updraft helicity entry
pub fn find_entries<'a>(entries: &'a [IdxEntry], pattern: &str) -> Vec<&'a IdxEntry> {
    let (var_pat, level_pat) = if let Some(idx) = pattern.find(':') {
        (&pattern[..idx], Some(&pattern[idx + 1..]))
    } else {
        (pattern, None)
    };

    entries
        .iter()
        .filter(|e| {
            if e.variable != var_pat {
                return false;
            }
            if let Some(lp) = level_pat {
                e.level.contains(lp)
            } else {
                true
            }
        })
        .collect()
}

/// Compute byte ranges for downloading specific entries from a GRIB2 file.
///
/// Each entry's data spans from its byte_offset to the next entry's byte_offset - 1.
/// The last selected entry extends to the end of the file (represented as u64::MAX).
///
/// The `entries` slice must be the full sorted list of idx entries so that the
/// "next entry" byte offset can be determined.
pub fn byte_ranges(entries: &[IdxEntry], selected: &[&IdxEntry]) -> Vec<(u64, u64)> {
    let mut ranges = Vec::with_capacity(selected.len());

    for sel in selected {
        let start = sel.byte_offset;

        // Find the next entry by looking for the entry with the next message number
        let end = entries
            .iter()
            .find(|e| e.byte_offset > start)
            .map(|e| e.byte_offset - 1)
            .unwrap_or(u64::MAX);

        ranges.push((start, end));
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_IDX: &str = "\
1:0:d=2026031012:TMP:2 m above ground:anl:
2:47843:d=2026031012:TMP:surface:anl:
3:96542:d=2026031012:SPFH:2 m above ground:anl:
4:143210:d=2026031012:CAPE:surface:anl:
5:200000:d=2026031012:REFC:entire atmosphere:anl:
";

    #[test]
    fn test_parse_idx() {
        let entries = parse_idx(SAMPLE_IDX);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].msg_num, 1);
        assert_eq!(entries[0].byte_offset, 0);
        assert_eq!(entries[0].date, "2026031012");
        assert_eq!(entries[0].variable, "TMP");
        assert_eq!(entries[0].level, "2 m above ground");
        assert_eq!(entries[0].forecast, "anl");

        assert_eq!(entries[1].byte_offset, 47843);
        assert_eq!(entries[1].variable, "TMP");
        assert_eq!(entries[1].level, "surface");
    }

    #[test]
    fn test_find_entries() {
        let entries = parse_idx(SAMPLE_IDX);
        let found = find_entries(&entries, "TMP:2 m above ground");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].msg_num, 1);

        let found = find_entries(&entries, "TMP:surface");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].msg_num, 2);

        // Match all TMP entries
        let found = find_entries(&entries, "TMP");
        assert_eq!(found.len(), 2);

        let found = find_entries(&entries, "CAPE:surface");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_byte_ranges() {
        let entries = parse_idx(SAMPLE_IDX);
        let selected = find_entries(&entries, "TMP:2 m above ground");
        let ranges = byte_ranges(&entries, &selected);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], (0, 47842));

        let selected = find_entries(&entries, "REFC");
        let ranges = byte_ranges(&entries, &selected);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].0, 200000);
        assert_eq!(ranges[0].1, u64::MAX); // last entry
    }
}
