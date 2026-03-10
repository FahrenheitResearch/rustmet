/// Look up the human-readable name of a GRIB2 parameter.
/// Returns "Unknown" for unrecognized combinations.
pub fn parameter_name(discipline: u8, category: u8, number: u8) -> &'static str {
    match (discipline, category, number) {
        // Discipline 0: Meteorological
        // Category 0: Temperature
        (0, 0, 0) => "Temperature",
        (0, 0, 1) => "Virtual Temperature",
        (0, 0, 2) => "Potential Temperature",
        (0, 0, 3) => "Pseudo-Adiabatic Potential Temperature",
        (0, 0, 4) => "Maximum Temperature",
        (0, 0, 5) => "Minimum Temperature",
        (0, 0, 6) => "Dewpoint Temperature",
        (0, 0, 7) => "Dewpoint Depression",
        (0, 0, 8) => "Lapse Rate",
        (0, 0, 9) => "Temperature Anomaly",
        (0, 0, 10) => "Latent Heat Net Flux",
        (0, 0, 11) => "Sensible Heat Net Flux",
        (0, 0, 12) => "Heat Index",
        (0, 0, 13) => "Wind Chill Factor",
        (0, 0, 15) => "Virtual Potential Temperature",
        (0, 0, 17) => "Skin Temperature",
        (0, 0, 192) => "Snow Phase Change Heat Flux",
        (0, 0, 193) => "Temperature Tendency by All Radiation",
        (0, 0, 21) => "Apparent Temperature",

        // Category 1: Moisture
        (0, 1, 0) => "Specific Humidity",
        (0, 1, 1) => "Relative Humidity",
        (0, 1, 2) => "Humidity Mixing Ratio",
        (0, 1, 3) => "Precipitable Water",
        (0, 1, 4) => "Vapor Pressure",
        (0, 1, 5) => "Saturation Deficit",
        (0, 1, 6) => "Evaporation",
        (0, 1, 7) => "Precipitation Rate",
        (0, 1, 8) => "Total Precipitation",
        (0, 1, 9) => "Large Scale Precipitation",
        (0, 1, 10) => "Convective Precipitation",
        (0, 1, 11) => "Snow Depth",
        (0, 1, 12) => "Snowfall Rate Water Equivalent",
        (0, 1, 13) => "Water Equivalent of Accumulated Snow Depth",
        (0, 1, 14) => "Convective Snow",
        (0, 1, 15) => "Large Scale Snow",
        (0, 1, 22) => "Cloud Mixing Ratio",
        (0, 1, 23) => "Ice Water Mixing Ratio",
        (0, 1, 24) => "Rain Mixing Ratio",
        (0, 1, 25) => "Snow Mixing Ratio",
        (0, 1, 32) => "Graupel",
        (0, 1, 192) => "Categorical Rain",
        (0, 1, 193) => "Categorical Freezing Rain",
        (0, 1, 194) => "Categorical Ice Pellets",
        (0, 1, 195) => "Categorical Snow",
        (0, 1, 196) => "Convective Precipitation Rate",
        (0, 1, 197) => "Horizontal Moisture Divergence",
        (0, 1, 199) => "Potential Evaporation",
        (0, 1, 200) => "Potential Evaporation Rate",
        (0, 1, 201) => "Snow Cover",
        (0, 1, 213) => "Frozen Rain",

        // Category 2: Momentum
        (0, 2, 0) => "Wind Direction",
        (0, 2, 1) => "Wind Speed",
        (0, 2, 2) => "U-Component of Wind",
        (0, 2, 3) => "V-Component of Wind",
        (0, 2, 4) => "Stream Function",
        (0, 2, 5) => "Velocity Potential",
        (0, 2, 8) => "Vertical Velocity (Pressure)",
        (0, 2, 9) => "Vertical Velocity (Geometric)",
        (0, 2, 10) => "Absolute Vorticity",
        (0, 2, 12) => "Relative Vorticity",
        (0, 2, 14) => "Potential Vorticity",
        (0, 2, 22) => "Storm Relative Helicity",
        (0, 2, 25) => "Vertical Speed Shear",
        (0, 2, 27) => "U-Component Storm Motion",
        (0, 2, 28) => "V-Component Storm Motion",
        (0, 2, 192) => "Vertical Speed Shear",
        (0, 2, 193) => "Horizontal Momentum Flux",
        (0, 2, 194) => "U-Component of Friction Velocity",
        (0, 2, 196) => "Wind Gust Speed",
        (0, 2, 197) => "U-Component of Wind (at 10m)",

        // Category 3: Mass
        (0, 3, 0) => "Pressure",
        (0, 3, 1) => "Pressure Reduced to MSL",
        (0, 3, 2) => "Pressure Tendency",
        (0, 3, 3) => "ICAO Standard Atmosphere Reference Height",
        (0, 3, 4) => "Geopotential",
        (0, 3, 5) => "Geopotential Height",
        (0, 3, 6) => "Geometric Height",
        (0, 3, 7) => "Standard Deviation of Height",
        (0, 3, 8) => "Pressure Anomaly",
        (0, 3, 9) => "Geopotential Height Anomaly",
        (0, 3, 192) => "MSLP (Eta Reduction)",
        (0, 3, 193) => "5-Wave Geopotential Height",
        (0, 3, 196) => "Planetary Boundary Layer Height",
        (0, 3, 198) => "MSLP (MAPS System Reduction)",
        (0, 3, 200) => "Pressure of Level from which Parcel was Lifted",

        // Category 4: Short-wave Radiation
        (0, 4, 0) => "Net Short-Wave Radiation Flux (Surface)",
        (0, 4, 1) => "Net Short-Wave Radiation Flux (TOA)",
        (0, 4, 7) => "Downward Short-Wave Radiation Flux",
        (0, 4, 8) => "Upward Short-Wave Radiation Flux",
        (0, 4, 192) => "Downward Short-Wave Radiation Flux",
        (0, 4, 193) => "Upward Short-Wave Radiation Flux",

        // Category 5: Long-wave Radiation
        (0, 5, 0) => "Net Long-Wave Radiation Flux (Surface)",
        (0, 5, 1) => "Net Long-Wave Radiation Flux (TOA)",
        (0, 5, 3) => "Downward Long-Wave Radiation Flux",
        (0, 5, 4) => "Upward Long-Wave Radiation Flux",
        (0, 5, 192) => "Downward Long-Wave Radiation Flux",
        (0, 5, 193) => "Upward Long-Wave Radiation Flux",

        // Category 6: Cloud
        (0, 6, 0) => "Cloud Ice",
        (0, 6, 1) => "Total Cloud Cover",
        (0, 6, 2) => "Convective Cloud Cover",
        (0, 6, 3) => "Low Cloud Cover",
        (0, 6, 4) => "Medium Cloud Cover",
        (0, 6, 5) => "High Cloud Cover",
        (0, 6, 6) => "Cloud Water",
        (0, 6, 11) => "Cloud Top",
        (0, 6, 12) => "Cloud Bottom",
        (0, 6, 192) => "Non-Convective Cloud Cover",
        (0, 6, 193) => "Cloud Work Function",
        (0, 6, 196) => "In-Flight Icing",

        // Category 7: Thermodynamic Stability
        (0, 7, 0) => "K Index",
        (0, 7, 1) => "Total Totals Index",
        (0, 7, 2) => "Sweat Index",
        (0, 7, 6) => "Convective Available Potential Energy",
        (0, 7, 7) => "Convective Inhibition",
        (0, 7, 8) => "Storm Relative Helicity",
        (0, 7, 10) => "Showalter Index",
        (0, 7, 192) => "Surface Lifted Index",
        (0, 7, 193) => "Best Lifted Index",
        (0, 7, 194) => "Richardson Number",
        (0, 7, 197) => "Updraft Helicity",

        // Category 13: Aerosols
        (0, 13, 193) => "Percent Frozen Precipitation",

        // Category 14: Trace Gases
        (0, 14, 0) => "Total Ozone",
        (0, 14, 192) => "Ozone Mixing Ratio",

        // Category 15: Radar
        (0, 15, 6) => "Radar Reflectivity",
        (0, 15, 7) => "Composite Reflectivity",
        (0, 15, 8) => "Echo Top",

        // Category 16: Forecast Radar
        (0, 16, 195) => "Maximum/Composite Radar Reflectivity",
        (0, 16, 196) => "Composite Reflectivity",

        // Category 19: Physical Atmospheric Properties
        (0, 19, 0) => "Visibility",
        (0, 19, 1) => "Albedo",
        (0, 19, 2) => "Thunderstorm Probability",
        (0, 19, 3) => "Mixed Layer Cape",
        (0, 19, 11) => "Turbulence",
        (0, 19, 192) => "Maximum Snow Albedo",
        (0, 19, 193) => "Snow-Free Albedo",
        (0, 19, 204) => "Icing Severity",
        (0, 19, 232) => "Derived Radar Reflectivity",
        (0, 19, 234) => "Composite Reflectivity (Max Hourly)",

        // Discipline 2: Land Surface
        (2, 0, 0) => "Land Cover",
        (2, 0, 1) => "Surface Roughness",
        (2, 0, 2) => "Soil Temperature",
        (2, 0, 3) => "Soil Moisture Content",
        (2, 0, 4) => "Vegetation",
        (2, 0, 5) => "Water Runoff",
        (2, 0, 7) => "Evapotranspiration",
        (2, 0, 192) => "Volumetric Soil Moisture Content",
        (2, 0, 193) => "Ground Heat Flux",
        (2, 0, 194) => "Moisture Availability",
        (2, 0, 196) => "Soil Type",
        (2, 0, 198) => "Vegetal Cover",
        (2, 0, 201) => "Ice-Free Water Surface",
        (2, 0, 207) => "Canopy Conductance",

        // Discipline 10: Oceanographic
        (10, 0, 3) => "Sea Surface Temperature",
        (10, 0, 4) => "Sea Surface Temperature Anomaly",
        (10, 2, 0) => "Ice Cover",

        _ => "Unknown",
    }
}

/// Look up the units of a GRIB2 parameter.
pub fn parameter_units(discipline: u8, category: u8, number: u8) -> &'static str {
    match (discipline, category, number) {
        // Temperature
        (0, 0, 0) | (0, 0, 1) | (0, 0, 2) | (0, 0, 3) | (0, 0, 4) | (0, 0, 5) => "K",
        (0, 0, 6) | (0, 0, 7) => "K",
        (0, 0, 8) => "K/m",
        (0, 0, 9) => "K",
        (0, 0, 10) | (0, 0, 11) => "W/m²",
        (0, 0, 12) | (0, 0, 13) => "K",
        (0, 0, 15) | (0, 0, 17) | (0, 0, 21) => "K",
        (0, 0, 192) => "W/m²",
        (0, 0, 193) => "K/s",

        // Moisture
        (0, 1, 0) => "kg/kg",
        (0, 1, 1) => "%",
        (0, 1, 2) => "kg/kg",
        (0, 1, 3) => "kg/m²",
        (0, 1, 4) => "Pa",
        (0, 1, 5) => "kg/kg",
        (0, 1, 6) => "kg/m²",
        (0, 1, 7) => "kg/m²/s",
        (0, 1, 8) | (0, 1, 9) | (0, 1, 10) => "kg/m²",
        (0, 1, 11) => "m",
        (0, 1, 12) => "kg/m²/s",
        (0, 1, 13) | (0, 1, 14) | (0, 1, 15) => "kg/m²",
        (0, 1, 22) | (0, 1, 23) | (0, 1, 24) | (0, 1, 25) | (0, 1, 32) => "kg/kg",
        (0, 1, 192) | (0, 1, 193) | (0, 1, 194) | (0, 1, 195) => "Code table",
        (0, 1, 196) => "kg/m²/s",
        (0, 1, 197) => "kg/kg/s",
        (0, 1, 199) => "kg/m²",
        (0, 1, 200) => "W/m²",
        (0, 1, 201) => "%",

        // Momentum
        (0, 2, 0) => "degrees",
        (0, 2, 1) => "m/s",
        (0, 2, 2) | (0, 2, 3) => "m/s",
        (0, 2, 4) => "m²/s",
        (0, 2, 5) => "m²/s",
        (0, 2, 8) => "Pa/s",
        (0, 2, 9) => "m/s",
        (0, 2, 10) | (0, 2, 12) => "1/s",
        (0, 2, 14) => "K m²/kg/s",
        (0, 2, 22) => "m²/s²",
        (0, 2, 25) => "1/s",
        (0, 2, 27) | (0, 2, 28) => "m/s",
        (0, 2, 196) => "m/s",

        // Mass
        (0, 3, 0) | (0, 3, 1) | (0, 3, 2) => "Pa",
        (0, 3, 3) | (0, 3, 5) | (0, 3, 6) | (0, 3, 7) => "m",
        (0, 3, 4) => "m²/s²",
        (0, 3, 8) => "Pa",
        (0, 3, 9) => "gpm",
        (0, 3, 192) | (0, 3, 198) => "Pa",
        (0, 3, 196) => "m",

        // Radiation
        (0, 4, 0) | (0, 4, 1) | (0, 4, 7) | (0, 4, 8) => "W/m²",
        (0, 4, 192) | (0, 4, 193) => "W/m²",
        (0, 5, 0) | (0, 5, 1) | (0, 5, 3) | (0, 5, 4) => "W/m²",
        (0, 5, 192) | (0, 5, 193) => "W/m²",

        // Cloud
        (0, 6, 0) => "kg/m²",
        (0, 6, 1) | (0, 6, 2) | (0, 6, 3) | (0, 6, 4) | (0, 6, 5) => "%",
        (0, 6, 6) => "kg/m²",
        (0, 6, 11) | (0, 6, 12) => "m",
        (0, 6, 192) => "%",

        // Stability
        (0, 7, 0) | (0, 7, 1) | (0, 7, 2) | (0, 7, 10) => "K",
        (0, 7, 6) | (0, 7, 7) => "J/kg",
        (0, 7, 8) => "m²/s²",
        (0, 7, 192) | (0, 7, 193) => "K",
        (0, 7, 197) => "m²/s²",

        // Radar
        (0, 15, 6) | (0, 15, 7) => "dBZ",
        (0, 15, 8) => "m",
        (0, 16, 195) | (0, 16, 196) => "dBZ",

        // Visibility / Albedo
        (0, 19, 0) => "m",
        (0, 19, 1) => "%",
        (0, 19, 2) => "%",
        (0, 19, 3) => "J/kg",
        (0, 19, 192) | (0, 19, 193) => "%",
        (0, 19, 232) | (0, 19, 234) => "dBZ",

        // Land
        (2, 0, 0) => "Proportion",
        (2, 0, 1) => "m",
        (2, 0, 2) => "K",
        (2, 0, 3) => "kg/m³",
        (2, 0, 4) | (2, 0, 198) => "%",
        (2, 0, 5) => "kg/m²",
        (2, 0, 7) => "kg/m²/s",
        (2, 0, 192) => "fraction",
        (2, 0, 193) => "W/m²",

        // Ocean
        (10, 0, 3) | (10, 0, 4) => "K",
        (10, 2, 0) => "Proportion",

        _ => "Unknown",
    }
}

/// Look up the human-readable name of a level type.
pub fn level_name(level_type: u8) -> &'static str {
    match level_type {
        1 => "Ground or Water Surface",
        2 => "Cloud Base Level",
        3 => "Cloud Top Level",
        4 => "Level of 0°C Isotherm",
        5 => "Level of Adiabatic Condensation Lifted from Surface",
        6 => "Maximum Wind Level",
        7 => "Tropopause",
        8 => "Nominal Top of Atmosphere",
        9 => "Sea Bottom",
        10 => "Entire Atmosphere",
        20 => "Isothermal Level",
        100 => "Isobaric Surface",
        101 => "Mean Sea Level",
        102 => "Specific Altitude Above Mean Sea Level",
        103 => "Specified Height Level Above Ground",
        104 => "Sigma Level",
        105 => "Hybrid Level",
        106 => "Depth Below Land Surface",
        107 => "Isentropic (Theta) Level",
        108 => "Level at Specified Pressure Difference from Ground to Level",
        109 => "Potential Vorticity Surface",
        200 => "Entire Atmosphere (as single layer)",
        204 => "Highest Tropospheric Freezing Level",
        206 => "Grid Scale Cloud Bottom Level",
        207 => "Grid Scale Cloud Top Level",
        211 => "Boundary Layer Cloud Bottom Level",
        212 => "Boundary Layer Cloud Top Level",
        213 => "Low Cloud Bottom Level",
        214 => "Low Cloud Top Level",
        215 => "Cloud Ceiling",
        220 => "Planetary Boundary Layer",
        222 => "Middle Cloud Bottom Level",
        223 => "Middle Cloud Top Level",
        224 => "High Cloud Bottom Level",
        232 => "High Cloud Top Level",
        233 => "Highest Level where Temperature Exceeds Value",
        234 => "Highest Level where Wet Bulb Temperature Exceeds Value",
        235 => "Highest Level where Equivalent Potential Temperature Exceeds Value",
        242 => "Convective Cloud Bottom Level",
        243 => "Convective Cloud Top Level",
        244 => "Deep Convective Cloud Bottom Level",
        245 => "Deep Convective Cloud Top Level",
        251 => "Shallow Convective Cloud Bottom Level",
        252 => "Shallow Convective Cloud Top Level",
        253 => "Supercooled Liquid Water Bottom",
        254 => "Supercooled Liquid Water Top",
        _ => "Unknown Level Type",
    }
}
