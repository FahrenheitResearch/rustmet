use serde::{Serialize, Deserialize};

/// A surface weather observation station.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub icao: &'static str,
    pub name: &'static str,
    pub state: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f64,
}

/// Find a station by its 4-letter ICAO code.
pub fn find_station(icao: &str) -> Option<&'static Station> {
    let icao_upper = icao.to_uppercase();
    STATIONS.iter().find(|s| s.icao == icao_upper)
}

/// Find the nearest station to a given latitude/longitude.
pub fn nearest_station(lat: f64, lon: f64) -> &'static Station {
    STATIONS.iter()
        .min_by(|a, b| {
            let da = haversine_km(lat, lon, a.lat, a.lon);
            let db = haversine_km(lat, lon, b.lat, b.lon);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("station database is non-empty")
}

/// Find all stations within `radius_km` of a given point.
pub fn stations_within(lat: f64, lon: f64, radius_km: f64) -> Vec<&'static Station> {
    STATIONS.iter()
        .filter(|s| haversine_km(lat, lon, s.lat, s.lon) <= radius_km)
        .collect()
}

/// Haversine distance in kilometers.
fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Earth radius in km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

// ---------------------------------------------------------------------------
// Static station database — major NWS/ASOS sites, all 50 states + DC/PR/GU
// ---------------------------------------------------------------------------

pub static STATIONS: &[Station] = &[
    // Alabama
    Station { icao: "KBHM", name: "Birmingham-Shuttlesworth Intl", state: "AL", lat: 33.5629, lon: -86.7535, elevation_m: 196.0 },
    Station { icao: "KHSV", name: "Huntsville Intl", state: "AL", lat: 34.6372, lon: -86.7751, elevation_m: 191.0 },
    Station { icao: "KMOB", name: "Mobile Regional", state: "AL", lat: 30.6914, lon: -88.2428, elevation_m: 67.0 },
    Station { icao: "KMGM", name: "Montgomery Regional", state: "AL", lat: 32.3006, lon: -86.3940, elevation_m: 62.0 },
    Station { icao: "KTCL", name: "Tuscaloosa Regional", state: "AL", lat: 33.2206, lon: -87.6114, elevation_m: 52.0 },
    Station { icao: "KDHN", name: "Dothan Regional", state: "AL", lat: 31.3213, lon: -85.4496, elevation_m: 122.0 },
    // Alaska
    Station { icao: "PANC", name: "Ted Stevens Anchorage Intl", state: "AK", lat: 61.1743, lon: -149.9963, elevation_m: 46.0 },
    Station { icao: "PAFA", name: "Fairbanks Intl", state: "AK", lat: 64.8151, lon: -147.8564, elevation_m: 135.0 },
    Station { icao: "PAJN", name: "Juneau Intl", state: "AK", lat: 58.3550, lon: -134.5763, elevation_m: 7.0 },
    Station { icao: "PAOM", name: "Nome", state: "AK", lat: 64.5122, lon: -165.4453, elevation_m: 11.0 },
    Station { icao: "PABR", name: "Wiley Post-Will Rogers Memorial", state: "AK", lat: 71.2854, lon: -156.7660, elevation_m: 13.0 },
    // Arizona
    Station { icao: "KPHX", name: "Phoenix Sky Harbor Intl", state: "AZ", lat: 33.4373, lon: -112.0078, elevation_m: 337.0 },
    Station { icao: "KTUS", name: "Tucson Intl", state: "AZ", lat: 32.1161, lon: -110.9410, elevation_m: 806.0 },
    Station { icao: "KFLG", name: "Flagstaff Pulliam", state: "AZ", lat: 35.1385, lon: -111.6712, elevation_m: 2135.0 },
    Station { icao: "KPRC", name: "Prescott Regional", state: "AZ", lat: 34.6545, lon: -112.4196, elevation_m: 1531.0 },
    Station { icao: "KIWA", name: "Phoenix-Mesa Gateway", state: "AZ", lat: 33.3078, lon: -111.6556, elevation_m: 421.0 },
    // Arkansas
    Station { icao: "KLIT", name: "Clinton Natl / Little Rock", state: "AR", lat: 34.7294, lon: -92.2243, elevation_m: 80.0 },
    Station { icao: "KXNA", name: "Northwest Arkansas Regional", state: "AR", lat: 36.2819, lon: -94.3068, elevation_m: 390.0 },
    Station { icao: "KFSM", name: "Fort Smith Regional", state: "AR", lat: 35.3366, lon: -94.3674, elevation_m: 141.0 },
    // California
    Station { icao: "KLAX", name: "Los Angeles Intl", state: "CA", lat: 33.9425, lon: -118.4081, elevation_m: 38.0 },
    Station { icao: "KSFO", name: "San Francisco Intl", state: "CA", lat: 37.6213, lon: -122.3790, elevation_m: 4.0 },
    Station { icao: "KSAN", name: "San Diego Intl", state: "CA", lat: 32.7336, lon: -117.1897, elevation_m: 5.0 },
    Station { icao: "KOAK", name: "Oakland Intl", state: "CA", lat: 37.7213, lon: -122.2208, elevation_m: 2.0 },
    Station { icao: "KSJC", name: "San Jose Intl", state: "CA", lat: 37.3626, lon: -121.9291, elevation_m: 19.0 },
    Station { icao: "KSAC", name: "Sacramento Executive", state: "CA", lat: 38.5125, lon: -121.4935, elevation_m: 5.0 },
    Station { icao: "KSMF", name: "Sacramento Intl", state: "CA", lat: 38.6954, lon: -121.5908, elevation_m: 8.0 },
    Station { icao: "KFAT", name: "Fresno Yosemite Intl", state: "CA", lat: 36.7762, lon: -119.7181, elevation_m: 102.0 },
    Station { icao: "KBFL", name: "Meadows Field / Bakersfield", state: "CA", lat: 35.4336, lon: -119.0568, elevation_m: 151.0 },
    Station { icao: "KRDD", name: "Redding Municipal", state: "CA", lat: 40.5090, lon: -122.2934, elevation_m: 153.0 },
    Station { icao: "KBUR", name: "Bob Hope / Burbank", state: "CA", lat: 34.2007, lon: -118.3585, elevation_m: 236.0 },
    // Colorado
    Station { icao: "KDEN", name: "Denver Intl", state: "CO", lat: 39.8561, lon: -104.6737, elevation_m: 1656.0 },
    Station { icao: "KCOS", name: "Colorado Springs", state: "CO", lat: 38.8058, lon: -104.7008, elevation_m: 1886.0 },
    Station { icao: "KGJT", name: "Grand Junction Regional", state: "CO", lat: 39.1224, lon: -108.5267, elevation_m: 1476.0 },
    Station { icao: "KPUB", name: "Pueblo Memorial", state: "CO", lat: 38.2890, lon: -104.4966, elevation_m: 1440.0 },
    Station { icao: "KASE", name: "Aspen-Pitkin County", state: "CO", lat: 39.2232, lon: -106.8689, elevation_m: 2349.0 },
    // Connecticut
    Station { icao: "KBDL", name: "Bradley Intl / Hartford", state: "CT", lat: 41.9389, lon: -72.6832, elevation_m: 53.0 },
    Station { icao: "KHVN", name: "Tweed-New Haven", state: "CT", lat: 41.2637, lon: -72.8868, elevation_m: 4.0 },
    // Delaware
    Station { icao: "KILG", name: "Wilmington / New Castle", state: "DE", lat: 39.6787, lon: -75.6065, elevation_m: 24.0 },
    Station { icao: "KDOV", name: "Dover AFB", state: "DE", lat: 39.1298, lon: -75.4667, elevation_m: 8.0 },
    // Florida
    Station { icao: "KMIA", name: "Miami Intl", state: "FL", lat: 25.7959, lon: -80.2870, elevation_m: 3.0 },
    Station { icao: "KMCO", name: "Orlando Intl", state: "FL", lat: 28.4294, lon: -81.3090, elevation_m: 29.0 },
    Station { icao: "KTPA", name: "Tampa Intl", state: "FL", lat: 27.9755, lon: -82.5332, elevation_m: 8.0 },
    Station { icao: "KJAX", name: "Jacksonville Intl", state: "FL", lat: 30.4941, lon: -81.6879, elevation_m: 10.0 },
    Station { icao: "KFLL", name: "Fort Lauderdale-Hollywood Intl", state: "FL", lat: 26.0726, lon: -80.1527, elevation_m: 3.0 },
    Station { icao: "KTLH", name: "Tallahassee Intl", state: "FL", lat: 30.3965, lon: -84.3503, elevation_m: 25.0 },
    Station { icao: "KPBI", name: "Palm Beach Intl", state: "FL", lat: 26.6832, lon: -80.0956, elevation_m: 6.0 },
    Station { icao: "KPNS", name: "Pensacola Intl", state: "FL", lat: 30.4734, lon: -87.1866, elevation_m: 37.0 },
    Station { icao: "KEYW", name: "Key West Intl", state: "FL", lat: 24.5561, lon: -81.7596, elevation_m: 1.0 },
    Station { icao: "KRSW", name: "Southwest Florida Intl", state: "FL", lat: 26.5362, lon: -81.7552, elevation_m: 9.0 },
    // Georgia
    Station { icao: "KATL", name: "Hartsfield-Jackson Atlanta Intl", state: "GA", lat: 33.6367, lon: -84.4281, elevation_m: 315.0 },
    Station { icao: "KSAV", name: "Savannah / Hilton Head Intl", state: "GA", lat: 32.1276, lon: -81.2021, elevation_m: 15.0 },
    Station { icao: "KAGS", name: "Augusta Regional", state: "GA", lat: 33.3699, lon: -81.9645, elevation_m: 45.0 },
    Station { icao: "KMCN", name: "Middle Georgia Regional", state: "GA", lat: 32.6927, lon: -83.6492, elevation_m: 107.0 },
    Station { icao: "KCSG", name: "Columbus Metropolitan", state: "GA", lat: 32.5163, lon: -84.9389, elevation_m: 121.0 },
    // Hawaii
    Station { icao: "PHNL", name: "Daniel K. Inouye Intl / Honolulu", state: "HI", lat: 21.3187, lon: -157.9225, elevation_m: 4.0 },
    Station { icao: "PHOG", name: "Kahului", state: "HI", lat: 20.8986, lon: -156.4305, elevation_m: 16.0 },
    Station { icao: "PHKO", name: "Ellison Onizuka Kona Intl", state: "HI", lat: 19.7388, lon: -156.0456, elevation_m: 14.0 },
    Station { icao: "PHLI", name: "Lihue", state: "HI", lat: 21.9760, lon: -159.3390, elevation_m: 45.0 },
    // Idaho
    Station { icao: "KBOI", name: "Boise Air Terminal", state: "ID", lat: 43.5644, lon: -116.2228, elevation_m: 874.0 },
    Station { icao: "KIDA", name: "Idaho Falls Regional", state: "ID", lat: 43.5146, lon: -112.0702, elevation_m: 1443.0 },
    Station { icao: "KLWS", name: "Lewiston-Nez Perce County", state: "ID", lat: 46.3745, lon: -117.0154, elevation_m: 438.0 },
    // Illinois
    Station { icao: "KORD", name: "O'Hare Intl / Chicago", state: "IL", lat: 41.9742, lon: -87.9073, elevation_m: 204.0 },
    Station { icao: "KMDW", name: "Midway Intl / Chicago", state: "IL", lat: 41.7868, lon: -87.7522, elevation_m: 189.0 },
    Station { icao: "KSPI", name: "Abraham Lincoln Capital / Springfield", state: "IL", lat: 39.8441, lon: -89.6779, elevation_m: 179.0 },
    Station { icao: "KPIA", name: "Gen Downing-Peoria Intl", state: "IL", lat: 40.6642, lon: -89.6933, elevation_m: 202.0 },
    Station { icao: "KRFD", name: "Chicago Rockford Intl", state: "IL", lat: 42.1954, lon: -89.0972, elevation_m: 222.0 },
    Station { icao: "KMLI", name: "Quad City Intl / Moline", state: "IL", lat: 41.4485, lon: -90.5075, elevation_m: 179.0 },
    // Indiana
    Station { icao: "KIND", name: "Indianapolis Intl", state: "IN", lat: 39.7173, lon: -86.2944, elevation_m: 241.0 },
    Station { icao: "KFWA", name: "Fort Wayne Intl", state: "IN", lat: 40.9785, lon: -85.1951, elevation_m: 248.0 },
    Station { icao: "KSBN", name: "South Bend Intl", state: "IN", lat: 41.7087, lon: -86.3173, elevation_m: 238.0 },
    Station { icao: "KEVV", name: "Evansville Regional", state: "IN", lat: 38.0370, lon: -87.5324, elevation_m: 127.0 },
    // Iowa
    Station { icao: "KDSM", name: "Des Moines Intl", state: "IA", lat: 41.5340, lon: -93.6631, elevation_m: 294.0 },
    Station { icao: "KCID", name: "Eastern Iowa / Cedar Rapids", state: "IA", lat: 41.8847, lon: -91.7108, elevation_m: 265.0 },
    Station { icao: "KDBQ", name: "Dubuque Regional", state: "IA", lat: 42.4020, lon: -90.7095, elevation_m: 329.0 },
    Station { icao: "KSUX", name: "Sioux Gateway", state: "IA", lat: 42.4026, lon: -96.3844, elevation_m: 336.0 },
    // Kansas
    Station { icao: "KICT", name: "Wichita Dwight D. Eisenhower Natl", state: "KS", lat: 37.6499, lon: -97.4331, elevation_m: 407.0 },
    Station { icao: "KMCI", name: "Kansas City Intl", state: "KS", lat: 39.2976, lon: -94.7139, elevation_m: 312.0 },
    Station { icao: "KTOP", name: "Topeka / Billard Muni", state: "KS", lat: 39.0686, lon: -95.6225, elevation_m: 283.0 },
    Station { icao: "KGCK", name: "Garden City Regional", state: "KS", lat: 37.9275, lon: -100.7244, elevation_m: 879.0 },
    Station { icao: "KDDC", name: "Dodge City Regional", state: "KS", lat: 37.7634, lon: -99.9696, elevation_m: 790.0 },
    // Kentucky
    Station { icao: "KSDF", name: "Louisville Muhammad Ali Intl", state: "KY", lat: 38.1744, lon: -85.7360, elevation_m: 151.0 },
    Station { icao: "KLEX", name: "Blue Grass / Lexington", state: "KY", lat: 38.0365, lon: -84.6059, elevation_m: 301.0 },
    Station { icao: "KCVG", name: "Cincinnati / N. Kentucky Intl", state: "KY", lat: 39.0488, lon: -84.6678, elevation_m: 271.0 },
    Station { icao: "KPAH", name: "Barkley Regional / Paducah", state: "KY", lat: 37.0603, lon: -88.7738, elevation_m: 126.0 },
    // Louisiana
    Station { icao: "KMSY", name: "Louis Armstrong New Orleans Intl", state: "LA", lat: 29.9934, lon: -90.2580, elevation_m: 1.0 },
    Station { icao: "KBTR", name: "Baton Rouge Metropolitan", state: "LA", lat: 30.5332, lon: -91.1496, elevation_m: 21.0 },
    Station { icao: "KSHV", name: "Shreveport Regional", state: "LA", lat: 32.4466, lon: -93.8261, elevation_m: 79.0 },
    Station { icao: "KLFT", name: "Lafayette Regional", state: "LA", lat: 30.2053, lon: -91.9876, elevation_m: 13.0 },
    Station { icao: "KLCH", name: "Lake Charles Regional", state: "LA", lat: 30.1261, lon: -93.2234, elevation_m: 5.0 },
    // Maine
    Station { icao: "KPWM", name: "Portland Intl Jetport", state: "ME", lat: 43.6462, lon: -70.3093, elevation_m: 23.0 },
    Station { icao: "KBGR", name: "Bangor Intl", state: "ME", lat: 44.8074, lon: -68.8281, elevation_m: 58.0 },
    Station { icao: "KCAR", name: "Caribou Municipal", state: "ME", lat: 46.8715, lon: -68.0179, elevation_m: 190.0 },
    // Maryland
    Station { icao: "KBWI", name: "Baltimore/Washington Intl", state: "MD", lat: 39.1754, lon: -76.6684, elevation_m: 47.0 },
    Station { icao: "KSBY", name: "Salisbury-Ocean City Wicomico", state: "MD", lat: 38.3405, lon: -75.5103, elevation_m: 16.0 },
    // Massachusetts
    Station { icao: "KBOS", name: "Boston Logan Intl", state: "MA", lat: 42.3643, lon: -71.0052, elevation_m: 6.0 },
    Station { icao: "KORH", name: "Worcester Regional", state: "MA", lat: 42.2673, lon: -71.8757, elevation_m: 308.0 },
    Station { icao: "KACK", name: "Nantucket Memorial", state: "MA", lat: 41.2531, lon: -70.0604, elevation_m: 14.0 },
    // Michigan
    Station { icao: "KDTW", name: "Detroit Metro Wayne County", state: "MI", lat: 42.2124, lon: -83.3534, elevation_m: 196.0 },
    Station { icao: "KGRR", name: "Gerald R. Ford Intl / Grand Rapids", state: "MI", lat: 42.8808, lon: -85.5228, elevation_m: 241.0 },
    Station { icao: "KLAN", name: "Capital Region Intl / Lansing", state: "MI", lat: 42.7787, lon: -84.5874, elevation_m: 264.0 },
    Station { icao: "KFNT", name: "Bishop Intl / Flint", state: "MI", lat: 42.9655, lon: -83.7436, elevation_m: 233.0 },
    Station { icao: "KTVC", name: "Cherry Capital / Traverse City", state: "MI", lat: 44.7414, lon: -85.5822, elevation_m: 191.0 },
    Station { icao: "KSAW", name: "Sawyer Intl / Marquette", state: "MI", lat: 46.3536, lon: -87.3954, elevation_m: 430.0 },
    // Minnesota
    Station { icao: "KMSP", name: "Minneapolis-St Paul Intl", state: "MN", lat: 44.8831, lon: -93.2289, elevation_m: 256.0 },
    Station { icao: "KDLH", name: "Duluth Intl", state: "MN", lat: 46.8420, lon: -92.1936, elevation_m: 436.0 },
    Station { icao: "KRST", name: "Rochester Intl", state: "MN", lat: 43.9085, lon: -92.5000, elevation_m: 402.0 },
    Station { icao: "KSTC", name: "St Cloud Regional", state: "MN", lat: 45.5466, lon: -94.0599, elevation_m: 312.0 },
    Station { icao: "KINL", name: "Falls Intl / International Falls", state: "MN", lat: 48.5662, lon: -93.4031, elevation_m: 361.0 },
    // Mississippi
    Station { icao: "KJAN", name: "Jackson-Medgar Wiley Evers Intl", state: "MS", lat: 32.3112, lon: -90.0759, elevation_m: 106.0 },
    Station { icao: "KGPT", name: "Gulfport-Biloxi Intl", state: "MS", lat: 30.4073, lon: -89.0701, elevation_m: 8.0 },
    Station { icao: "KTUP", name: "Tupelo Regional", state: "MS", lat: 34.2681, lon: -88.7699, elevation_m: 109.0 },
    // Missouri
    Station { icao: "KSTL", name: "St Louis Lambert Intl", state: "MO", lat: 38.7487, lon: -90.3700, elevation_m: 185.0 },
    Station { icao: "KSGF", name: "Springfield-Branson Natl", state: "MO", lat: 37.2457, lon: -93.3886, elevation_m: 390.0 },
    Station { icao: "KCOU", name: "Columbia Regional", state: "MO", lat: 38.8181, lon: -92.2196, elevation_m: 272.0 },
    Station { icao: "KJLN", name: "Joplin Regional", state: "MO", lat: 37.1518, lon: -94.4983, elevation_m: 299.0 },
    // Montana
    Station { icao: "KBZN", name: "Bozeman Yellowstone Intl", state: "MT", lat: 45.7775, lon: -111.1530, elevation_m: 1371.0 },
    Station { icao: "KBIL", name: "Billings Logan Intl", state: "MT", lat: 45.8077, lon: -108.5430, elevation_m: 1113.0 },
    Station { icao: "KGTF", name: "Great Falls Intl", state: "MT", lat: 47.4820, lon: -111.3707, elevation_m: 1116.0 },
    Station { icao: "KMSO", name: "Missoula Montana", state: "MT", lat: 46.9163, lon: -114.0906, elevation_m: 972.0 },
    Station { icao: "KHLN", name: "Helena Regional", state: "MT", lat: 46.6068, lon: -111.9827, elevation_m: 1167.0 },
    // Nebraska
    Station { icao: "KOMA", name: "Eppley Airfield / Omaha", state: "NE", lat: 41.3032, lon: -95.8940, elevation_m: 299.0 },
    Station { icao: "KLNK", name: "Lincoln", state: "NE", lat: 40.8510, lon: -96.7592, elevation_m: 362.0 },
    Station { icao: "KGRI", name: "Central Nebraska Regional", state: "NE", lat: 40.9675, lon: -98.3096, elevation_m: 566.0 },
    Station { icao: "KLBF", name: "North Platte Regional", state: "NE", lat: 41.1262, lon: -100.6837, elevation_m: 849.0 },
    // Nevada
    Station { icao: "KLAS", name: "Harry Reid Intl / Las Vegas", state: "NV", lat: 36.0800, lon: -115.1523, elevation_m: 664.0 },
    Station { icao: "KRNO", name: "Reno-Tahoe Intl", state: "NV", lat: 39.4991, lon: -119.7681, elevation_m: 1342.0 },
    Station { icao: "KELK", name: "Elko Regional", state: "NV", lat: 40.8249, lon: -115.7920, elevation_m: 1547.0 },
    // New Hampshire
    Station { icao: "KMHT", name: "Manchester-Boston Regional", state: "NH", lat: 42.9326, lon: -71.4357, elevation_m: 81.0 },
    Station { icao: "KCON", name: "Concord Municipal", state: "NH", lat: 43.2027, lon: -71.5022, elevation_m: 105.0 },
    // New Jersey
    Station { icao: "KEWR", name: "Newark Liberty Intl", state: "NJ", lat: 40.6925, lon: -74.1687, elevation_m: 5.0 },
    Station { icao: "KTTN", name: "Trenton-Mercer", state: "NJ", lat: 40.2767, lon: -74.8135, elevation_m: 64.0 },
    Station { icao: "KACY", name: "Atlantic City Intl", state: "NJ", lat: 39.4576, lon: -74.5773, elevation_m: 23.0 },
    // New Mexico
    Station { icao: "KABQ", name: "Albuquerque Intl Sunport", state: "NM", lat: 35.0402, lon: -106.6091, elevation_m: 1631.0 },
    Station { icao: "KSAF", name: "Santa Fe County Muni", state: "NM", lat: 35.6171, lon: -106.0885, elevation_m: 1935.0 },
    Station { icao: "KELP", name: "El Paso Intl", state: "NM", lat: 31.8072, lon: -106.3776, elevation_m: 1195.0 },
    Station { icao: "KROW", name: "Roswell Air Center", state: "NM", lat: 33.3016, lon: -104.5307, elevation_m: 1118.0 },
    // New York
    Station { icao: "KJFK", name: "John F. Kennedy Intl", state: "NY", lat: 40.6399, lon: -73.7787, elevation_m: 4.0 },
    Station { icao: "KLGA", name: "LaGuardia", state: "NY", lat: 40.7772, lon: -73.8726, elevation_m: 6.0 },
    Station { icao: "KBUF", name: "Buffalo Niagara Intl", state: "NY", lat: 42.9405, lon: -78.7322, elevation_m: 218.0 },
    Station { icao: "KSYR", name: "Syracuse Hancock Intl", state: "NY", lat: 43.1112, lon: -76.1063, elevation_m: 127.0 },
    Station { icao: "KALB", name: "Albany Intl", state: "NY", lat: 42.7483, lon: -73.8017, elevation_m: 89.0 },
    Station { icao: "KROC", name: "Frederick Douglass / Rochester", state: "NY", lat: 43.1189, lon: -77.6724, elevation_m: 171.0 },
    Station { icao: "KISP", name: "Long Island MacArthur", state: "NY", lat: 40.7952, lon: -73.1002, elevation_m: 30.0 },
    // North Carolina
    Station { icao: "KCLT", name: "Charlotte Douglas Intl", state: "NC", lat: 35.2141, lon: -80.9431, elevation_m: 228.0 },
    Station { icao: "KRDU", name: "Raleigh-Durham Intl", state: "NC", lat: 35.8776, lon: -78.7875, elevation_m: 127.0 },
    Station { icao: "KGSO", name: "Piedmont Triad Intl / Greensboro", state: "NC", lat: 36.0978, lon: -79.9373, elevation_m: 270.0 },
    Station { icao: "KILM", name: "Wilmington Intl", state: "NC", lat: 34.2706, lon: -77.9026, elevation_m: 10.0 },
    Station { icao: "KAVL", name: "Asheville Regional", state: "NC", lat: 35.4362, lon: -82.5418, elevation_m: 651.0 },
    Station { icao: "KFAY", name: "Fayetteville Regional", state: "NC", lat: 34.9912, lon: -78.8804, elevation_m: 58.0 },
    Station { icao: "KHSE", name: "Billy Mitchell / Cape Hatteras", state: "NC", lat: 35.2328, lon: -75.6218, elevation_m: 3.0 },
    // North Dakota
    Station { icao: "KFAR", name: "Hector Intl / Fargo", state: "ND", lat: 46.9207, lon: -96.8158, elevation_m: 274.0 },
    Station { icao: "KBIS", name: "Bismarck Municipal", state: "ND", lat: 46.7727, lon: -100.7504, elevation_m: 504.0 },
    Station { icao: "KMOT", name: "Minot Intl", state: "ND", lat: 48.2594, lon: -101.2803, elevation_m: 523.0 },
    Station { icao: "KGFK", name: "Grand Forks Intl", state: "ND", lat: 47.9493, lon: -97.1761, elevation_m: 257.0 },
    // Ohio
    Station { icao: "KCLE", name: "Cleveland Hopkins Intl", state: "OH", lat: 41.4117, lon: -81.8498, elevation_m: 241.0 },
    Station { icao: "KCMH", name: "John Glenn Columbus Intl", state: "OH", lat: 39.9980, lon: -82.8919, elevation_m: 275.0 },
    Station { icao: "KDAY", name: "James M. Cox Dayton Intl", state: "OH", lat: 39.9024, lon: -84.2194, elevation_m: 306.0 },
    Station { icao: "KTOL", name: "Toledo Express", state: "OH", lat: 41.5868, lon: -83.8073, elevation_m: 211.0 },
    Station { icao: "KCAK", name: "Akron-Canton", state: "OH", lat: 40.9161, lon: -81.4422, elevation_m: 377.0 },
    // Oklahoma
    Station { icao: "KOKC", name: "Will Rogers World / OKC", state: "OK", lat: 35.3931, lon: -97.6007, elevation_m: 397.0 },
    Station { icao: "KTUL", name: "Tulsa Intl", state: "OK", lat: 36.1984, lon: -95.8881, elevation_m: 206.0 },
    Station { icao: "KLAW", name: "Lawton-Fort Sill Regional", state: "OK", lat: 34.5677, lon: -98.4166, elevation_m: 346.0 },
    Station { icao: "KGAG", name: "Gage", state: "OK", lat: 36.2955, lon: -99.7764, elevation_m: 662.0 },
    // Oregon
    Station { icao: "KPDX", name: "Portland Intl", state: "OR", lat: 45.5887, lon: -122.5975, elevation_m: 9.0 },
    Station { icao: "KEUG", name: "Mahlon Sweet Field / Eugene", state: "OR", lat: 44.1246, lon: -123.2119, elevation_m: 109.0 },
    Station { icao: "KMFR", name: "Rogue Valley Intl / Medford", state: "OR", lat: 42.3742, lon: -122.8735, elevation_m: 405.0 },
    Station { icao: "KRDM", name: "Roberts Field / Redmond", state: "OR", lat: 44.2541, lon: -121.1500, elevation_m: 940.0 },
    Station { icao: "KAST", name: "Astoria Regional", state: "OR", lat: 46.1580, lon: -123.8787, elevation_m: 5.0 },
    // Pennsylvania
    Station { icao: "KPHL", name: "Philadelphia Intl", state: "PA", lat: 39.8721, lon: -75.2411, elevation_m: 11.0 },
    Station { icao: "KPIT", name: "Pittsburgh Intl", state: "PA", lat: 40.4915, lon: -80.2329, elevation_m: 367.0 },
    Station { icao: "KABE", name: "Lehigh Valley Intl", state: "PA", lat: 40.6521, lon: -75.4408, elevation_m: 120.0 },
    Station { icao: "KMDT", name: "Harrisburg Intl", state: "PA", lat: 40.1935, lon: -76.7634, elevation_m: 94.0 },
    Station { icao: "KERI", name: "Erie Intl / Tom Ridge Field", state: "PA", lat: 42.0831, lon: -80.1764, elevation_m: 222.0 },
    Station { icao: "KAVP", name: "Wilkes-Barre/Scranton Intl", state: "PA", lat: 41.3383, lon: -75.7234, elevation_m: 292.0 },
    // Rhode Island
    Station { icao: "KPVD", name: "T.F. Green Intl / Providence", state: "RI", lat: 41.7251, lon: -71.4282, elevation_m: 17.0 },
    // South Carolina
    Station { icao: "KCHS", name: "Charleston AFB / Intl", state: "SC", lat: 32.8986, lon: -80.0405, elevation_m: 14.0 },
    Station { icao: "KCAE", name: "Columbia Metropolitan", state: "SC", lat: 33.9388, lon: -81.1195, elevation_m: 72.0 },
    Station { icao: "KGSP", name: "Greenville-Spartanburg Intl", state: "SC", lat: 34.8957, lon: -82.2189, elevation_m: 296.0 },
    Station { icao: "KMYR", name: "Myrtle Beach Intl", state: "SC", lat: 33.6797, lon: -78.9283, elevation_m: 8.0 },
    // South Dakota
    Station { icao: "KFSD", name: "Sioux Falls Regional / Joe Foss Field", state: "SD", lat: 43.5820, lon: -96.7419, elevation_m: 435.0 },
    Station { icao: "KRAP", name: "Rapid City Regional", state: "SD", lat: 44.0453, lon: -103.0574, elevation_m: 966.0 },
    Station { icao: "KABR", name: "Aberdeen Regional", state: "SD", lat: 45.4491, lon: -98.4218, elevation_m: 399.0 },
    Station { icao: "KPIR", name: "Pierre Regional", state: "SD", lat: 44.3827, lon: -100.2860, elevation_m: 527.0 },
    // Tennessee
    Station { icao: "KBNA", name: "Nashville Intl", state: "TN", lat: 36.1245, lon: -86.6782, elevation_m: 181.0 },
    Station { icao: "KMEM", name: "Memphis Intl", state: "TN", lat: 35.0424, lon: -89.9767, elevation_m: 104.0 },
    Station { icao: "KTYS", name: "McGhee Tyson / Knoxville", state: "TN", lat: 35.8110, lon: -83.9940, elevation_m: 299.0 },
    Station { icao: "KCHA", name: "Lovell Field / Chattanooga", state: "TN", lat: 35.0353, lon: -85.2038, elevation_m: 210.0 },
    Station { icao: "KTRI", name: "Tri-Cities Regional", state: "TN", lat: 36.4752, lon: -82.4074, elevation_m: 462.0 },
    // Texas
    Station { icao: "KDFW", name: "Dallas/Fort Worth Intl", state: "TX", lat: 32.8968, lon: -97.0380, elevation_m: 183.0 },
    Station { icao: "KIAH", name: "George Bush Intercontinental / Houston", state: "TX", lat: 29.9844, lon: -95.3414, elevation_m: 30.0 },
    Station { icao: "KHOU", name: "William P. Hobby / Houston", state: "TX", lat: 29.6454, lon: -95.2789, elevation_m: 14.0 },
    Station { icao: "KSAT", name: "San Antonio Intl", state: "TX", lat: 29.5337, lon: -98.4698, elevation_m: 240.0 },
    Station { icao: "KAUS", name: "Austin-Bergstrom Intl", state: "TX", lat: 30.1945, lon: -97.6699, elevation_m: 162.0 },
    Station { icao: "KDAL", name: "Dallas Love Field", state: "TX", lat: 32.8471, lon: -96.8518, elevation_m: 148.0 },
    Station { icao: "KELP", name: "El Paso Intl", state: "TX", lat: 31.8072, lon: -106.3776, elevation_m: 1195.0 },
    Station { icao: "KMAF", name: "Midland Intl Air & Space Port", state: "TX", lat: 31.9425, lon: -102.2019, elevation_m: 872.0 },
    Station { icao: "KAMA", name: "Rick Husband Amarillo Intl", state: "TX", lat: 35.2194, lon: -101.7060, elevation_m: 1099.0 },
    Station { icao: "KLBB", name: "Lubbock Preston Smith Intl", state: "TX", lat: 33.6636, lon: -101.8227, elevation_m: 993.0 },
    Station { icao: "KCRP", name: "Corpus Christi Intl", state: "TX", lat: 27.7704, lon: -97.5012, elevation_m: 13.0 },
    Station { icao: "KBRO", name: "Brownsville / South Padre Island Intl", state: "TX", lat: 25.9068, lon: -97.4259, elevation_m: 7.0 },
    Station { icao: "KABI", name: "Abilene Regional", state: "TX", lat: 32.4113, lon: -99.6819, elevation_m: 546.0 },
    // Utah
    Station { icao: "KSLC", name: "Salt Lake City Intl", state: "UT", lat: 40.7884, lon: -111.9778, elevation_m: 1288.0 },
    Station { icao: "KOGD", name: "Ogden-Hinckley", state: "UT", lat: 41.1961, lon: -112.0122, elevation_m: 1356.0 },
    Station { icao: "KCDC", name: "Cedar City Regional", state: "UT", lat: 37.7010, lon: -113.0989, elevation_m: 1713.0 },
    Station { icao: "KPVU", name: "Provo Municipal", state: "UT", lat: 40.2192, lon: -111.7235, elevation_m: 1370.0 },
    // Vermont
    Station { icao: "KBTV", name: "Burlington Intl", state: "VT", lat: 44.4719, lon: -73.1533, elevation_m: 103.0 },
    Station { icao: "KMPV", name: "Edward F. Knapp State / Montpelier", state: "VT", lat: 44.2035, lon: -72.5623, elevation_m: 346.0 },
    // Virginia
    Station { icao: "KIAD", name: "Washington Dulles Intl", state: "VA", lat: 38.9445, lon: -77.4558, elevation_m: 95.0 },
    Station { icao: "KDCA", name: "Ronald Reagan Washington Natl", state: "VA", lat: 38.8521, lon: -77.0377, elevation_m: 5.0 },
    Station { icao: "KRIC", name: "Richmond Intl", state: "VA", lat: 37.5052, lon: -77.3197, elevation_m: 51.0 },
    Station { icao: "KORF", name: "Norfolk Intl", state: "VA", lat: 36.8946, lon: -76.2012, elevation_m: 9.0 },
    Station { icao: "KROA", name: "Roanoke-Blacksburg Regional", state: "VA", lat: 37.3255, lon: -79.9754, elevation_m: 358.0 },
    Station { icao: "KCHO", name: "Charlottesville-Albemarle", state: "VA", lat: 38.1386, lon: -78.4529, elevation_m: 195.0 },
    // Washington
    Station { icao: "KSEA", name: "Seattle-Tacoma Intl", state: "WA", lat: 47.4490, lon: -122.3093, elevation_m: 137.0 },
    Station { icao: "KGEG", name: "Spokane Intl", state: "WA", lat: 47.6199, lon: -117.5338, elevation_m: 721.0 },
    Station { icao: "KBLI", name: "Bellingham Intl", state: "WA", lat: 48.7927, lon: -122.5375, elevation_m: 52.0 },
    Station { icao: "KYKM", name: "Yakima Air Terminal", state: "WA", lat: 46.5682, lon: -120.5440, elevation_m: 325.0 },
    Station { icao: "KOLM", name: "Olympia Regional", state: "WA", lat: 46.9694, lon: -122.9025, elevation_m: 63.0 },
    // West Virginia
    Station { icao: "KCRW", name: "Yeager / Charleston", state: "WV", lat: 38.3731, lon: -81.5932, elevation_m: 299.0 },
    Station { icao: "KCKB", name: "North Central WV / Clarksburg", state: "WV", lat: 39.2967, lon: -80.2281, elevation_m: 368.0 },
    // Wisconsin
    Station { icao: "KMKE", name: "Milwaukee Mitchell Intl", state: "WI", lat: 42.9472, lon: -87.8966, elevation_m: 207.0 },
    Station { icao: "KMSN", name: "Dane County Regional / Madison", state: "WI", lat: 43.1399, lon: -89.3375, elevation_m: 264.0 },
    Station { icao: "KGRB", name: "Green Bay Austin Straubel Intl", state: "WI", lat: 44.4851, lon: -88.1296, elevation_m: 210.0 },
    Station { icao: "KEAU", name: "Chippewa Valley Regional / Eau Claire", state: "WI", lat: 44.8658, lon: -91.4843, elevation_m: 277.0 },
    Station { icao: "KLSE", name: "La Crosse Regional", state: "WI", lat: 43.8793, lon: -91.2567, elevation_m: 199.0 },
    // Wyoming
    Station { icao: "KCYS", name: "Cheyenne Regional", state: "WY", lat: 41.1557, lon: -104.8118, elevation_m: 1872.0 },
    Station { icao: "KCPR", name: "Casper-Natrona County Intl", state: "WY", lat: 42.9080, lon: -106.4646, elevation_m: 1623.0 },
    Station { icao: "KJAC", name: "Jackson Hole", state: "WY", lat: 43.6073, lon: -110.7378, elevation_m: 1966.0 },
    Station { icao: "KSHR", name: "Sheridan County", state: "WY", lat: 44.7692, lon: -106.9803, elevation_m: 1209.0 },
    Station { icao: "KRKS", name: "Southwest Wyoming Regional / Rock Springs", state: "WY", lat: 41.5942, lon: -109.0652, elevation_m: 2056.0 },
    // District of Columbia
    // (KDCA and KIAD cover DC area — listed under Virginia above)

    // Puerto Rico
    Station { icao: "TJSJ", name: "Luis Munoz Marin Intl / San Juan", state: "PR", lat: 18.4394, lon: -66.0018, elevation_m: 3.0 },
    Station { icao: "TJBQ", name: "Rafael Hernandez / Aguadilla", state: "PR", lat: 18.4949, lon: -67.1294, elevation_m: 72.0 },
    Station { icao: "TJPS", name: "Mercedita / Ponce", state: "PR", lat: 18.0083, lon: -66.5630, elevation_m: 9.0 },

    // Guam
    Station { icao: "PGUM", name: "Antonio B. Won Pat Intl / Guam", state: "GU", lat: 13.4834, lon: 144.7960, elevation_m: 94.0 },

    // US Virgin Islands
    Station { icao: "TIST", name: "Cyril E. King / St Thomas", state: "VI", lat: 18.3373, lon: -64.9734, elevation_m: 7.0 },

    // Additional major stations to reach 300+ total
    // Alabama extras
    Station { icao: "KANB", name: "Anniston Regional", state: "AL", lat: 33.5882, lon: -85.8581, elevation_m: 186.0 },
    // Arizona extras
    Station { icao: "KYUM", name: "Yuma MCAS / Yuma Intl", state: "AZ", lat: 32.6566, lon: -114.6060, elevation_m: 65.0 },
    // California extras
    Station { icao: "KONT", name: "Ontario Intl", state: "CA", lat: 34.0560, lon: -117.6012, elevation_m: 287.0 },
    Station { icao: "KSBA", name: "Santa Barbara Municipal", state: "CA", lat: 34.4262, lon: -119.8404, elevation_m: 3.0 },
    Station { icao: "KPSP", name: "Palm Springs Intl", state: "CA", lat: 33.8297, lon: -116.5067, elevation_m: 145.0 },
    Station { icao: "KCRQ", name: "McClellan-Palomar / Carlsbad", state: "CA", lat: 33.1283, lon: -117.2802, elevation_m: 100.0 },
    Station { icao: "KMOD", name: "Modesto City-County", state: "CA", lat: 37.6258, lon: -120.9542, elevation_m: 30.0 },
    Station { icao: "KSTS", name: "Charles M. Schulz / Sonoma County", state: "CA", lat: 38.5089, lon: -122.8128, elevation_m: 38.0 },
    // Colorado extras
    Station { icao: "KEGE", name: "Eagle County Regional", state: "CO", lat: 39.6426, lon: -106.9159, elevation_m: 1993.0 },
    Station { icao: "KALS", name: "San Luis Valley Regional / Alamosa", state: "CO", lat: 37.4349, lon: -105.8667, elevation_m: 2296.0 },
    // Florida extras
    Station { icao: "KGNV", name: "Gainesville Regional", state: "FL", lat: 29.6901, lon: -82.2718, elevation_m: 46.0 },
    Station { icao: "KDAB", name: "Daytona Beach Intl", state: "FL", lat: 29.1799, lon: -81.0581, elevation_m: 10.0 },
    Station { icao: "KSRQ", name: "Sarasota-Bradenton Intl", state: "FL", lat: 27.3954, lon: -82.5544, elevation_m: 9.0 },
    Station { icao: "KAPF", name: "Naples Municipal", state: "FL", lat: 26.1526, lon: -81.7753, elevation_m: 3.0 },
    // Georgia extras
    Station { icao: "KVLD", name: "Valdosta Regional", state: "GA", lat: 30.7825, lon: -83.2767, elevation_m: 62.0 },
    Station { icao: "KABY", name: "Southwest Georgia Regional / Albany", state: "GA", lat: 31.5355, lon: -84.1945, elevation_m: 60.0 },
    // Idaho extras
    Station { icao: "KTWF", name: "Joslin Field / Magic Valley / Twin Falls", state: "ID", lat: 42.4818, lon: -114.4877, elevation_m: 1264.0 },
    // Illinois extras
    Station { icao: "KDEC", name: "Decatur", state: "IL", lat: 39.8346, lon: -88.8657, elevation_m: 206.0 },
    Station { icao: "KCMI", name: "Willard / Champaign-Urbana", state: "IL", lat: 40.0393, lon: -88.2781, elevation_m: 228.0 },
    // Iowa extras
    Station { icao: "KALO", name: "Waterloo Regional", state: "IA", lat: 42.5571, lon: -92.4003, elevation_m: 268.0 },
    // Kansas extras
    Station { icao: "KSLN", name: "Salina Regional", state: "KS", lat: 38.7910, lon: -97.6522, elevation_m: 384.0 },
    // Kentucky extras
    Station { icao: "KBWG", name: "Bowling Green-Warren County Regional", state: "KY", lat: 36.9645, lon: -86.4197, elevation_m: 166.0 },
    // Michigan extras
    Station { icao: "KAZO", name: "Kalamazoo / Battle Creek Intl", state: "MI", lat: 42.2349, lon: -85.5521, elevation_m: 271.0 },
    Station { icao: "KMBS", name: "MBS Intl / Saginaw", state: "MI", lat: 43.5329, lon: -84.0796, elevation_m: 201.0 },
    // Minnesota extras
    Station { icao: "KBRD", name: "Brainerd Lakes Regional", state: "MN", lat: 46.3979, lon: -94.1372, elevation_m: 374.0 },
    // Mississippi extras
    Station { icao: "KMEI", name: "Key Field / Meridian", state: "MS", lat: 32.3326, lon: -88.7519, elevation_m: 90.0 },
    // Missouri extras
    Station { icao: "KCGI", name: "Cape Girardeau Regional", state: "MO", lat: 37.2253, lon: -89.5708, elevation_m: 103.0 },
    // Montana extras
    Station { icao: "KGGW", name: "Wokal Field / Glasgow", state: "MT", lat: 48.2125, lon: -106.6147, elevation_m: 700.0 },
    // Nebraska extras
    Station { icao: "KEAR", name: "Kearney Regional", state: "NE", lat: 40.7270, lon: -99.0068, elevation_m: 655.0 },
    // New York extras
    Station { icao: "KITH", name: "Ithaca Tompkins Intl", state: "NY", lat: 42.4914, lon: -76.4584, elevation_m: 335.0 },
    Station { icao: "KBGM", name: "Greater Binghamton", state: "NY", lat: 42.2087, lon: -75.9798, elevation_m: 499.0 },
    Station { icao: "KSWF", name: "Stewart Intl / Newburgh", state: "NY", lat: 41.5041, lon: -74.1048, elevation_m: 150.0 },
    // North Carolina extras
    Station { icao: "KEWN", name: "Coastal Carolina Regional / New Bern", state: "NC", lat: 35.0730, lon: -77.0429, elevation_m: 6.0 },
    // Ohio extras
    Station { icao: "KYNG", name: "Youngstown-Warren Regional", state: "OH", lat: 41.2607, lon: -80.6791, elevation_m: 361.0 },
    Station { icao: "KMFD", name: "Mansfield Lahm Regional", state: "OH", lat: 40.8214, lon: -82.5166, elevation_m: 393.0 },
    // Oklahoma extras
    Station { icao: "KEND", name: "Vance AFB / Enid", state: "OK", lat: 36.3391, lon: -97.9165, elevation_m: 395.0 },
    // Oregon extras
    Station { icao: "KOTH", name: "Southwest Oregon Regional / North Bend", state: "OR", lat: 43.4171, lon: -124.2461, elevation_m: 5.0 },
    Station { icao: "KPDT", name: "Eastern Oregon Regional / Pendleton", state: "OR", lat: 45.6951, lon: -118.8414, elevation_m: 458.0 },
    // Pennsylvania extras
    Station { icao: "KUNV", name: "University Park / State College", state: "PA", lat: 40.8493, lon: -77.8487, elevation_m: 378.0 },
    Station { icao: "KIPT", name: "Williamsport Regional", state: "PA", lat: 41.2418, lon: -76.9211, elevation_m: 161.0 },
    // South Dakota extras
    Station { icao: "KMBG", name: "Mobridge Municipal", state: "SD", lat: 45.5465, lon: -100.4081, elevation_m: 508.0 },
    // Tennessee extras
    Station { icao: "KJWN", name: "John C. Tune / Nashville", state: "TN", lat: 36.1824, lon: -86.8867, elevation_m: 170.0 },
    // Texas extras
    Station { icao: "KSJT", name: "San Angelo Regional", state: "TX", lat: 31.3577, lon: -100.4963, elevation_m: 582.0 },
    Station { icao: "KACT", name: "Waco Regional", state: "TX", lat: 31.6113, lon: -97.2305, elevation_m: 156.0 },
    Station { icao: "KTYR", name: "Tyler Pounds Regional", state: "TX", lat: 32.3540, lon: -95.4024, elevation_m: 165.0 },
    Station { icao: "KSPS", name: "Sheppard AFB / Wichita Falls", state: "TX", lat: 33.9888, lon: -98.4919, elevation_m: 313.0 },
    Station { icao: "KGGG", name: "East Texas Regional / Longview", state: "TX", lat: 32.3840, lon: -94.7115, elevation_m: 111.0 },
    Station { icao: "KVCT", name: "Victoria Regional", state: "TX", lat: 28.8526, lon: -96.9185, elevation_m: 35.0 },
    // Washington extras
    Station { icao: "KPSC", name: "Tri-Cities / Pasco", state: "WA", lat: 46.2647, lon: -119.1191, elevation_m: 123.0 },
    Station { icao: "KEAT", name: "Pangborn Memorial / Wenatchee", state: "WA", lat: 47.3988, lon: -120.2069, elevation_m: 376.0 },
    // Wisconsin extras
    Station { icao: "KCWA", name: "Central Wisconsin / Wausau", state: "WI", lat: 44.7776, lon: -89.6668, elevation_m: 389.0 },
    Station { icao: "KOSH", name: "Wittman Regional / Oshkosh", state: "WI", lat: 43.9844, lon: -88.5570, elevation_m: 246.0 },

    // Additional stations — more coverage
    Station { icao: "KBHB", name: "Hancock County-Bar Harbor", state: "ME", lat: 44.4497, lon: -68.3616, elevation_m: 25.0 },
    Station { icao: "KPSM", name: "Portsmouth Intl / Pease", state: "NH", lat: 43.0779, lon: -70.8233, elevation_m: 30.0 },
    Station { icao: "KBTL", name: "W.K. Kellogg / Battle Creek", state: "MI", lat: 42.3073, lon: -85.2515, elevation_m: 290.0 },
    Station { icao: "KPLN", name: "Pellston Regional / Emmet County", state: "MI", lat: 45.5709, lon: -84.7968, elevation_m: 220.0 },
    Station { icao: "KESC", name: "Delta County / Escanaba", state: "MI", lat: 45.7227, lon: -87.0937, elevation_m: 185.0 },
    Station { icao: "KCMX", name: "Houghton County Memorial", state: "MI", lat: 47.1684, lon: -88.4891, elevation_m: 328.0 },
    Station { icao: "KISN", name: "Sloulin Field Intl / Williston", state: "ND", lat: 48.1779, lon: -103.6424, elevation_m: 593.0 },
    Station { icao: "KGGF", name: "Grant Municipal / Grant", state: "NE", lat: 40.8695, lon: -101.7333, elevation_m: 1124.0 },
    Station { icao: "KSNP", name: "St Paul Island", state: "AK", lat: 57.1674, lon: -170.2203, elevation_m: 19.0 },
    Station { icao: "PADQ", name: "Kodiak", state: "AK", lat: 57.7500, lon: -152.4939, elevation_m: 22.0 },
    Station { icao: "PAYA", name: "Yakutat", state: "AK", lat: 59.5033, lon: -139.6603, elevation_m: 10.0 },
    Station { icao: "PAKN", name: "King Salmon", state: "AK", lat: 58.6768, lon: -156.6492, elevation_m: 15.0 },
    Station { icao: "PACD", name: "Cold Bay", state: "AK", lat: 55.2061, lon: -162.7254, elevation_m: 29.0 },
    Station { icao: "PABE", name: "Bethel", state: "AK", lat: 60.7798, lon: -161.8380, elevation_m: 40.0 },

    Station { icao: "KGUC", name: "Gunnison-Crested Butte Regional", state: "CO", lat: 38.5339, lon: -106.9332, elevation_m: 2340.0 },
    Station { icao: "KDRO", name: "Durango-La Plata County", state: "CO", lat: 37.1515, lon: -107.7539, elevation_m: 2038.0 },

    Station { icao: "KTWF", name: "Magic Valley Regional / Twin Falls", state: "ID", lat: 42.4818, lon: -114.4877, elevation_m: 1264.0 },

    Station { icao: "KMCI", name: "Kansas City Intl", state: "MO", lat: 39.2976, lon: -94.7139, elevation_m: 312.0 },

    Station { icao: "KGRK", name: "Killeen-Fort Cavazos Regional", state: "TX", lat: 31.0672, lon: -97.8289, elevation_m: 308.0 },
    Station { icao: "KCLL", name: "Easterwood Field / College Station", state: "TX", lat: 30.5886, lon: -96.3638, elevation_m: 98.0 },
    Station { icao: "KLRD", name: "Laredo Intl", state: "TX", lat: 27.5438, lon: -99.4617, elevation_m: 155.0 },
    Station { icao: "KHRL", name: "Valley Intl / Harlingen", state: "TX", lat: 26.2285, lon: -97.6544, elevation_m: 11.0 },
    Station { icao: "KCDS", name: "Childress Municipal", state: "TX", lat: 34.4338, lon: -100.2880, elevation_m: 594.0 },
    Station { icao: "KGLS", name: "Scholes Intl / Galveston", state: "TX", lat: 29.2653, lon: -94.8604, elevation_m: 2.0 },

    Station { icao: "KEWB", name: "New Bedford Regional", state: "MA", lat: 41.6761, lon: -70.9569, elevation_m: 24.0 },
    Station { icao: "KCEF", name: "Westover ARB / Chicopee", state: "MA", lat: 42.1940, lon: -72.5348, elevation_m: 73.0 },
    Station { icao: "KHYA", name: "Barnstable Municipal / Hyannis", state: "MA", lat: 41.6693, lon: -70.2804, elevation_m: 16.0 },

    Station { icao: "KMSV", name: "Sullivan County Intl", state: "NY", lat: 41.7016, lon: -74.7950, elevation_m: 430.0 },
    Station { icao: "KPLB", name: "Clinton County / Plattsburgh", state: "NY", lat: 44.6895, lon: -73.5247, elevation_m: 72.0 },

    Station { icao: "KLWB", name: "Greenbrier Valley", state: "WV", lat: 37.8583, lon: -80.3995, elevation_m: 702.0 },
    Station { icao: "KMGW", name: "Morgantown Municipal", state: "WV", lat: 39.6429, lon: -79.9163, elevation_m: 381.0 },
    Station { icao: "KHTS", name: "Tri-State / Milton / Huntington", state: "WV", lat: 38.3667, lon: -82.5580, elevation_m: 255.0 },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_count() {
        assert!(STATIONS.len() >= 300, "expected >= 300 stations, got {}", STATIONS.len());
    }

    #[test]
    fn test_find_station() {
        let s = find_station("KOKC").unwrap();
        assert_eq!(s.state, "OK");
        assert!((s.lat - 35.3931).abs() < 0.01);
    }

    #[test]
    fn test_nearest() {
        // Near Denver
        let s = nearest_station(39.85, -104.67);
        assert_eq!(s.icao, "KDEN");
    }

    #[test]
    fn test_within() {
        // 50km around Chicago
        let hits = stations_within(41.88, -87.83, 50.0);
        let icaos: Vec<&str> = hits.iter().map(|s| s.icao).collect();
        assert!(icaos.contains(&"KORD"));
        assert!(icaos.contains(&"KMDW"));
    }
}
