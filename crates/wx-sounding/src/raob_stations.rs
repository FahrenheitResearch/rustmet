/// Upper-air radiosonde observation stations (US network).

#[derive(Debug, Clone)]
pub struct RaobStation {
    pub wmo: &'static str,
    pub icao: &'static str,
    pub name: &'static str,
    pub state: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f64,
}

/// Find a RAOB station by WMO number or ICAO identifier (case-insensitive).
pub fn find_raob_station(id: &str) -> Option<&'static RaobStation> {
    let id_upper = id.to_uppercase();
    RAOB_STATIONS.iter().find(|s| s.wmo == id_upper || s.icao == id_upper)
}

/// Find the nearest RAOB station to the given lat/lon using haversine distance.
pub fn nearest_raob_station(lat: f64, lon: f64) -> &'static RaobStation {
    let mut best = &RAOB_STATIONS[0];
    let mut best_dist = f64::MAX;
    for s in RAOB_STATIONS.iter() {
        let dlat = (s.lat - lat).to_radians();
        let dlon = (s.lon - lon).to_radians();
        let a = (dlat / 2.0).sin().powi(2)
            + lat.to_radians().cos() * s.lat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
        let d = 2.0 * a.sqrt().asin();
        if d < best_dist {
            best_dist = d;
            best = s;
        }
    }
    best
}

/// Return all RAOB stations.
pub fn all_raob_stations() -> &'static [RaobStation] {
    &RAOB_STATIONS
}

static RAOB_STATIONS: [RaobStation; 92] = [
    // 1-5: Florida
    RaobStation { wmo: "72201", icao: "KEY", name: "Key West", state: "FL", lat: 24.55, lon: -81.75, elevation_m: 1.0 },
    RaobStation { wmo: "72202", icao: "MFL", name: "Miami", state: "FL", lat: 25.75, lon: -80.38, elevation_m: 5.0 },
    RaobStation { wmo: "72206", icao: "JAX", name: "Jacksonville", state: "FL", lat: 30.50, lon: -81.70, elevation_m: 10.0 },
    RaobStation { wmo: "72210", icao: "TBW", name: "Tampa Bay", state: "FL", lat: 27.70, lon: -82.40, elevation_m: 13.0 },
    RaobStation { wmo: "72214", icao: "TLH", name: "Tallahassee", state: "FL", lat: 30.45, lon: -84.30, elevation_m: 53.0 },
    // 6-10: Southeast
    RaobStation { wmo: "72215", icao: "VPS", name: "Valparaiso / Eglin AFB", state: "FL", lat: 30.48, lon: -86.52, elevation_m: 26.0 },
    RaobStation { wmo: "72230", icao: "BMX", name: "Birmingham", state: "AL", lat: 33.17, lon: -86.77, elevation_m: 178.0 },
    RaobStation { wmo: "72233", icao: "FFC", name: "Peachtree City", state: "GA", lat: 33.37, lon: -84.57, elevation_m: 246.0 },
    RaobStation { wmo: "72235", icao: "JAN", name: "Jackson", state: "MS", lat: 32.32, lon: -90.08, elevation_m: 101.0 },
    RaobStation { wmo: "72240", icao: "LIX", name: "Slidell", state: "LA", lat: 30.33, lon: -89.83, elevation_m: 8.0 },
    // 11-15: Southeast cont.
    RaobStation { wmo: "72248", icao: "SHV", name: "Shreveport", state: "LA", lat: 32.45, lon: -93.85, elevation_m: 79.0 },
    RaobStation { wmo: "72208", icao: "CHS", name: "Charleston", state: "SC", lat: 32.90, lon: -80.03, elevation_m: 15.0 },
    RaobStation { wmo: "72311", icao: "AHN", name: "Athens", state: "GA", lat: 33.95, lon: -83.33, elevation_m: 246.0 },
    RaobStation { wmo: "72293", icao: "MIA", name: "Miami NWS", state: "FL", lat: 25.75, lon: -80.38, elevation_m: 5.0 },
    RaobStation { wmo: "72327", icao: "BNA", name: "Nashville", state: "TN", lat: 36.25, lon: -86.57, elevation_m: 180.0 },
    // 16-20: Gulf Coast / South Texas
    RaobStation { wmo: "72250", icao: "BRO", name: "Brownsville", state: "TX", lat: 25.92, lon: -97.42, elevation_m: 7.0 },
    RaobStation { wmo: "72251", icao: "CRP", name: "Corpus Christi", state: "TX", lat: 27.77, lon: -97.50, elevation_m: 14.0 },
    RaobStation { wmo: "72261", icao: "DRT", name: "Del Rio", state: "TX", lat: 29.37, lon: -100.92, elevation_m: 313.0 },
    RaobStation { wmo: "72265", icao: "MAF", name: "Midland", state: "TX", lat: 31.95, lon: -102.18, elevation_m: 873.0 },
    RaobStation { wmo: "72340", icao: "LCH", name: "Lake Charles", state: "LA", lat: 30.12, lon: -93.22, elevation_m: 10.0 },
    // 21-25: Texas / Oklahoma / Arkansas
    RaobStation { wmo: "72353", icao: "OUN", name: "Norman", state: "OK", lat: 35.18, lon: -97.44, elevation_m: 362.0 },
    RaobStation { wmo: "72357", icao: "LMN", name: "Lamont", state: "OK", lat: 36.61, lon: -97.49, elevation_m: 315.0 },
    RaobStation { wmo: "72363", icao: "AMA", name: "Amarillo", state: "TX", lat: 35.23, lon: -101.70, elevation_m: 1099.0 },
    RaobStation { wmo: "72364", icao: "FWD", name: "Fort Worth", state: "TX", lat: 32.83, lon: -97.30, elevation_m: 196.0 },
    RaobStation { wmo: "72318", icao: "LZK", name: "Little Rock", state: "AR", lat: 34.83, lon: -92.26, elevation_m: 78.0 },
    // 26-30: Southwest / New Mexico / Arizona
    RaobStation { wmo: "72365", icao: "ABQ", name: "Albuquerque", state: "NM", lat: 35.04, lon: -106.62, elevation_m: 1619.0 },
    RaobStation { wmo: "72274", icao: "TUS", name: "Tucson", state: "AZ", lat: 32.23, lon: -110.95, elevation_m: 779.0 },
    RaobStation { wmo: "72376", icao: "FGZ", name: "Flagstaff", state: "AZ", lat: 35.23, lon: -111.82, elevation_m: 2192.0 },
    RaobStation { wmo: "74389", icao: "EPZ", name: "Santa Teresa", state: "NM", lat: 31.87, lon: -106.70, elevation_m: 1252.0 },
    RaobStation { wmo: "74455", icao: "TWC", name: "Tucson NWS", state: "AZ", lat: 32.23, lon: -110.95, elevation_m: 779.0 },
    // 31-35: Nevada / California
    RaobStation { wmo: "74494", icao: "VEF", name: "Las Vegas", state: "NV", lat: 36.05, lon: -115.18, elevation_m: 640.0 },
    RaobStation { wmo: "72388", icao: "VBG", name: "Vandenberg AFB", state: "CA", lat: 34.75, lon: -120.57, elevation_m: 121.0 },
    RaobStation { wmo: "72393", icao: "NKX", name: "San Diego / Miramar", state: "CA", lat: 32.85, lon: -117.12, elevation_m: 128.0 },
    RaobStation { wmo: "72493", icao: "OAK", name: "Oakland", state: "CA", lat: 37.75, lon: -122.22, elevation_m: 6.0 },
    RaobStation { wmo: "72489", icao: "REV", name: "Reno", state: "NV", lat: 39.57, lon: -119.80, elevation_m: 1516.0 },
    // 36-40: Mid-Atlantic
    RaobStation { wmo: "72305", icao: "MHX", name: "Morehead City", state: "NC", lat: 34.78, lon: -76.88, elevation_m: 11.0 },
    RaobStation { wmo: "72317", icao: "GSO", name: "Greensboro", state: "NC", lat: 36.08, lon: -79.95, elevation_m: 277.0 },
    RaobStation { wmo: "72326", icao: "RNK", name: "Blacksburg", state: "VA", lat: 37.21, lon: -80.41, elevation_m: 654.0 },
    RaobStation { wmo: "72402", icao: "WAL", name: "Wallops Island", state: "VA", lat: 37.93, lon: -75.48, elevation_m: 13.0 },
    RaobStation { wmo: "72403", icao: "IAD", name: "Sterling", state: "VA", lat: 38.98, lon: -77.48, elevation_m: 86.0 },
    // 41-45: Ohio Valley
    RaobStation { wmo: "72426", icao: "ILN", name: "Wilmington", state: "OH", lat: 39.42, lon: -83.82, elevation_m: 317.0 },
    RaobStation { wmo: "72440", icao: "ILX", name: "Lincoln", state: "IL", lat: 40.15, lon: -89.34, elevation_m: 178.0 },
    RaobStation { wmo: "72520", icao: "PIT", name: "Pittsburgh", state: "PA", lat: 40.53, lon: -80.23, elevation_m: 357.0 },
    RaobStation { wmo: "72476", icao: "SGF", name: "Springfield", state: "MO", lat: 37.24, lon: -93.40, elevation_m: 387.0 },
    RaobStation { wmo: "72456", icao: "TOP", name: "Topeka", state: "KS", lat: 39.07, lon: -95.62, elevation_m: 270.0 },
    // 46-50: Central Plains
    RaobStation { wmo: "72451", icao: "DDC", name: "Dodge City", state: "KS", lat: 37.77, lon: -99.97, elevation_m: 790.0 },
    RaobStation { wmo: "72469", icao: "DNR", name: "Denver", state: "CO", lat: 39.77, lon: -104.88, elevation_m: 1625.0 },
    RaobStation { wmo: "72558", icao: "OAX", name: "Omaha / Valley", state: "NE", lat: 41.32, lon: -96.37, elevation_m: 350.0 },
    RaobStation { wmo: "72562", icao: "LBF", name: "North Platte", state: "NE", lat: 41.13, lon: -100.68, elevation_m: 849.0 },
    RaobStation { wmo: "72550", icao: "DVN", name: "Davenport", state: "IA", lat: 41.62, lon: -90.58, elevation_m: 229.0 },
    // 51-55: Northeast
    RaobStation { wmo: "72501", icao: "OKX", name: "Upton", state: "NY", lat: 40.87, lon: -72.87, elevation_m: 20.0 },
    RaobStation { wmo: "72518", icao: "ALB", name: "Albany", state: "NY", lat: 42.70, lon: -73.83, elevation_m: 93.0 },
    RaobStation { wmo: "72528", icao: "BUF", name: "Buffalo", state: "NY", lat: 42.93, lon: -78.73, elevation_m: 218.0 },
    RaobStation { wmo: "72606", icao: "PWM", name: "Portland", state: "ME", lat: 43.65, lon: -70.32, elevation_m: 19.0 },
    RaobStation { wmo: "72607", icao: "GYX", name: "Gray", state: "ME", lat: 43.89, lon: -70.26, elevation_m: 125.0 },
    // 56-60: New England / Northeast cont.
    RaobStation { wmo: "72612", icao: "CHH", name: "Chatham", state: "MA", lat: 41.67, lon: -69.97, elevation_m: 16.0 },
    RaobStation { wmo: "72712", icao: "CAR", name: "Caribou", state: "ME", lat: 46.87, lon: -68.02, elevation_m: 191.0 },
    RaobStation { wmo: "72513", icao: "PBZ", name: "Pittsburgh NWS", state: "PA", lat: 40.53, lon: -80.22, elevation_m: 357.0 },
    RaobStation { wmo: "72514", icao: "AVP", name: "Wilkes-Barre / Scranton", state: "PA", lat: 41.34, lon: -75.73, elevation_m: 289.0 },
    RaobStation { wmo: "72634", icao: "ABR", name: "Aberdeen", state: "SD", lat: 45.45, lon: -98.42, elevation_m: 396.0 },
    // 61-65: Great Lakes
    RaobStation { wmo: "72532", icao: "DTX", name: "White Lake / Detroit", state: "MI", lat: 42.70, lon: -83.47, elevation_m: 329.0 },
    RaobStation { wmo: "72546", icao: "GRB", name: "Green Bay", state: "WI", lat: 44.48, lon: -88.13, elevation_m: 214.0 },
    RaobStation { wmo: "72645", icao: "MPX", name: "Chanhassen", state: "MN", lat: 44.85, lon: -93.57, elevation_m: 287.0 },
    RaobStation { wmo: "72654", icao: "DLH", name: "Duluth", state: "MN", lat: 46.83, lon: -92.18, elevation_m: 432.0 },
    RaobStation { wmo: "72655", icao: "FSD", name: "Sioux Falls", state: "SD", lat: 43.58, lon: -96.73, elevation_m: 435.0 },
    // 66-70: Northern Plains
    RaobStation { wmo: "72632", icao: "BIS", name: "Bismarck", state: "ND", lat: 46.77, lon: -100.75, elevation_m: 506.0 },
    RaobStation { wmo: "72649", icao: "UNR", name: "Rapid City", state: "SD", lat: 44.07, lon: -103.21, elevation_m: 1030.0 },
    RaobStation { wmo: "72659", icao: "GGW", name: "Glasgow", state: "MT", lat: 48.21, lon: -106.63, elevation_m: 694.0 },
    RaobStation { wmo: "72662", icao: "TFX", name: "Great Falls", state: "MT", lat: 47.46, lon: -111.38, elevation_m: 1130.0 },
    RaobStation { wmo: "72672", icao: "RIW", name: "Riverton", state: "WY", lat: 43.06, lon: -108.48, elevation_m: 1694.0 },
    // 71-75: Rockies / Northwest
    RaobStation { wmo: "72572", icao: "SLC", name: "Salt Lake City", state: "UT", lat: 40.77, lon: -111.97, elevation_m: 1288.0 },
    RaobStation { wmo: "72576", icao: "GJT", name: "Grand Junction", state: "CO", lat: 39.12, lon: -108.53, elevation_m: 1475.0 },
    RaobStation { wmo: "72582", icao: "BOI", name: "Boise", state: "ID", lat: 43.57, lon: -116.22, elevation_m: 874.0 },
    RaobStation { wmo: "72764", icao: "MSO", name: "Missoula", state: "MT", lat: 46.92, lon: -114.09, elevation_m: 972.0 },
    RaobStation { wmo: "72747", icao: "ELY", name: "Ely", state: "NV", lat: 39.28, lon: -114.85, elevation_m: 1909.0 },
    // 76-80: Pacific Northwest
    RaobStation { wmo: "72597", icao: "MFR", name: "Medford", state: "OR", lat: 42.37, lon: -122.87, elevation_m: 405.0 },
    RaobStation { wmo: "72694", icao: "SLE", name: "Salem", state: "OR", lat: 44.92, lon: -123.02, elevation_m: 61.0 },
    RaobStation { wmo: "72776", icao: "OTX", name: "Spokane", state: "WA", lat: 47.68, lon: -117.63, elevation_m: 728.0 },
    RaobStation { wmo: "72786", icao: "UIL", name: "Quillayute", state: "WA", lat: 47.95, lon: -124.55, elevation_m: 56.0 },
    RaobStation { wmo: "72797", icao: "SEW", name: "Seattle", state: "WA", lat: 47.45, lon: -122.31, elevation_m: 122.0 },
    // 81-85: Idaho / Twin Falls, International Falls, misc
    RaobStation { wmo: "72681", icao: "TWF", name: "Twin Falls", state: "ID", lat: 42.48, lon: -114.48, elevation_m: 1264.0 },
    RaobStation { wmo: "72747", icao: "INL", name: "International Falls", state: "MN", lat: 48.57, lon: -93.40, elevation_m: 361.0 },
    RaobStation { wmo: "72768", icao: "GEG", name: "Spokane Airport", state: "WA", lat: 47.62, lon: -117.53, elevation_m: 721.0 },
    RaobStation { wmo: "72445", icao: "SPI", name: "Springfield", state: "IL", lat: 39.84, lon: -89.68, elevation_m: 178.0 },
    RaobStation { wmo: "72528", icao: "BUF", name: "Buffalo", state: "NY", lat: 42.93, lon: -78.73, elevation_m: 218.0 },
    // 86-92: Alaska
    RaobStation { wmo: "70261", icao: "ANC", name: "Anchorage", state: "AK", lat: 61.17, lon: -150.02, elevation_m: 40.0 },
    RaobStation { wmo: "70273", icao: "FAI", name: "Fairbanks", state: "AK", lat: 64.82, lon: -147.87, elevation_m: 138.0 },
    RaobStation { wmo: "70316", icao: "JNU", name: "Juneau", state: "AK", lat: 58.37, lon: -134.58, elevation_m: 7.0 },
    RaobStation { wmo: "70350", icao: "YAK", name: "Yakutat", state: "AK", lat: 59.52, lon: -139.67, elevation_m: 9.0 },
    RaobStation { wmo: "70219", icao: "BET", name: "Bethel", state: "AK", lat: 60.78, lon: -161.80, elevation_m: 40.0 },
    RaobStation { wmo: "70200", icao: "ADQ", name: "Kodiak", state: "AK", lat: 57.75, lon: -152.50, elevation_m: 34.0 },
    RaobStation { wmo: "70133", icao: "OTZ", name: "Kotzebue", state: "AK", lat: 66.87, lon: -162.63, elevation_m: 5.0 },
];
