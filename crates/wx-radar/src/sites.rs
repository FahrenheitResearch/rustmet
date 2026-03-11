//! NEXRAD radar site database.
//!
//! Provides a static table of major NEXRAD (WSR-88D) sites and a lookup
//! function by site identifier.

use wx_field::RadarSite;

/// A selection of major NEXRAD sites with coordinates and elevation.
///
/// Coordinates and elevations sourced from NWS NEXRAD site documentation.
pub static SITES: &[(&str, &str, f64, f64, f64)] = &[
    ("KTLX", "Oklahoma City, OK",        35.3331, -97.2778,  370.0),
    ("KFWS", "Dallas/Fort Worth, TX",     32.5731, -97.3028,  208.0),
    ("KOUN", "Norman, OK (research)",     35.2456, -97.4622,  357.0),
    ("KOKX", "Brookhaven, NY",            40.8656, -72.8639,   26.0),
    ("KLSX", "St. Louis, MO",             38.6986, -90.6828,  185.0),
    ("KLIX", "New Orleans, LA",           30.3367, -89.8256,    8.0),
    ("KLOT", "Chicago, IL",               41.6044, -88.0847,  202.0),
    ("KFFC", "Atlanta, GA",               33.3636, -84.5658,  262.0),
    ("KAMX", "Miami, FL",                 25.6111, -80.4128,    4.0),
    ("KSOX", "Santa Ana Mtns, CA",        33.8178, -117.636,  923.0),
    ("KATX", "Seattle, WA",               48.1944, -122.496,  151.0),
    ("KPUX", "Pueblo, CO",                38.4594, -104.181, 1600.0),
];

/// Look up a NEXRAD site by its identifier (case-insensitive).
///
/// Returns `Some(RadarSite)` if the site is found in the built-in table,
/// or `None` otherwise.
///
/// # Example
/// ```
/// use wx_radar::sites::find_site;
/// let site = find_site("KTLX").unwrap();
/// assert_eq!(site.name, "Oklahoma City, OK");
/// ```
pub fn find_site(id: &str) -> Option<RadarSite> {
    let id_upper = id.to_uppercase();
    SITES.iter().find(|(sid, _, _, _, _)| *sid == id_upper).map(
        |(sid, name, lat, lon, elev)| RadarSite::new(*sid, *name, *lat, *lon, *elev),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_site_exists() {
        let site = find_site("KTLX").expect("KTLX should be in the site table");
        assert_eq!(site.id, "KTLX");
        assert_eq!(site.name, "Oklahoma City, OK");
        assert!((site.lat - 35.3331).abs() < 0.001);
        assert!((site.lon - (-97.2778)).abs() < 0.001);
    }

    #[test]
    fn test_find_site_case_insensitive() {
        assert!(find_site("ktlx").is_some());
        assert!(find_site("Ktlx").is_some());
    }

    #[test]
    fn test_find_site_not_found() {
        assert!(find_site("XYZZ").is_none());
    }

    #[test]
    fn test_sites_table_has_minimum_entries() {
        assert!(SITES.len() >= 10);
    }

    #[test]
    fn test_all_sites_findable() {
        for (id, _, _, _, _) in SITES {
            assert!(find_site(id).is_some(), "Site {} should be findable", id);
        }
    }
}
