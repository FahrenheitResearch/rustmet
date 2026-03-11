// Meteorological calculations demo
//
// Demonstrates rustmet-core's thermodynamic functions:
// - Potential temperature, equivalent potential temperature, mixing ratio
// - LCL (Lifting Condensation Level)
// - CAPE/CIN from a synthetic sounding
//
// No network access or external files needed — all data is synthetic.
//
// Run with: cargo run --example metcalc

use rustmet_core::metfuncs;

fn main() {
    println!("=== rustmet-core: Meteorological Calculations ===\n");

    // --- Basic thermodynamic quantities ---
    let t_c = 25.0;     // Temperature in Celsius
    let td_c = 18.0;    // Dewpoint in Celsius
    let p_hpa = 1000.0;  // Pressure in hPa

    println!("Surface conditions:");
    println!("  Temperature:  {:.1} C", t_c);
    println!("  Dewpoint:     {:.1} C", td_c);
    println!("  Pressure:     {:.0} hPa\n", p_hpa);

    // Saturation vapor pressure
    let es = metfuncs::vappres(t_c);
    println!("Saturation vapor pressure at {:.1} C: {:.2} hPa", t_c, es);

    // Mixing ratio
    let w = metfuncs::mixratio(p_hpa, td_c);
    println!("Mixing ratio at Td={:.1} C, P={:.0} hPa: {:.2} g/kg", td_c, p_hpa, w);

    // Virtual temperature
    let tv = metfuncs::virtual_temp(t_c, p_hpa, td_c);
    println!("Virtual temperature: {:.2} C", tv);

    // Potential temperature (dry adiabatic)
    let theta_k = (t_c + metfuncs::ZEROCNK) * (1000.0 / p_hpa as f64).powf(metfuncs::ROCP);
    println!("Potential temperature: {:.2} K", theta_k);

    // Equivalent potential temperature
    let theta_e = metfuncs::thetae(p_hpa, t_c, td_c);
    println!("Equivalent potential temperature: {:.2} C ({:.2} K)", theta_e, theta_e + metfuncs::ZEROCNK);

    // --- LCL calculation ---
    println!("\n--- Lifting Condensation Level ---");
    let (p_lcl, t_lcl) = metfuncs::drylift(p_hpa, t_c, td_c);
    println!("LCL pressure:    {:.1} hPa", p_lcl);
    println!("LCL temperature: {:.1} C", t_lcl);

    // --- CAPE/CIN from a synthetic sounding ---
    println!("\n--- CAPE/CIN from Synthetic Sounding ---");

    // Construct a simple atmospheric profile (surface to ~200 hPa)
    // Typical warm-season severe weather environment
    let pressures = vec![
        1000.0, 975.0, 950.0, 925.0, 900.0, 875.0, 850.0,
        800.0, 750.0, 700.0, 650.0, 600.0, 550.0, 500.0,
        450.0, 400.0, 350.0, 300.0, 250.0, 200.0,
    ];
    let temperatures = vec![
        30.0, 28.0, 26.0, 24.0, 22.0, 20.0, 18.0,
        14.0, 10.0, 5.0, 0.0, -5.0, -10.0, -17.0,
        -25.0, -33.0, -42.0, -52.0, -58.0, -60.0,
    ];
    let dewpoints = vec![
        22.0, 21.0, 20.0, 18.0, 15.0, 12.0, 10.0,
        5.0, 0.0, -5.0, -10.0, -18.0, -25.0, -30.0,
        -35.0, -42.0, -50.0, -55.0, -60.0, -65.0,
    ];

    // Heights AGL (approximate, in meters)
    let heights_agl = vec![
        0.0, 250.0, 500.0, 750.0, 1000.0, 1250.0, 1500.0,
        2000.0, 2500.0, 3000.0, 3500.0, 4200.0, 5000.0, 5600.0,
        6500.0, 7500.0, 8500.0, 9500.0, 10500.0, 12000.0,
    ];

    println!("\nSounding profile ({} levels, {:.0}-{:.0} hPa):",
        pressures.len(), pressures[0], pressures[pressures.len() - 1]);
    println!("  {:>8}  {:>8}  {:>8}  {:>8}", "P (hPa)", "T (C)", "Td (C)", "H (m)");
    for i in (0..pressures.len()).step_by(3) {
        println!("  {:>8.0}  {:>8.1}  {:>8.1}  {:>8.0}",
            pressures[i], temperatures[i], dewpoints[i], heights_agl[i]);
    }

    // Compute CAPE/CIN using surface-based parcel
    let (cape, cin, h_lcl, h_lfc) = metfuncs::cape_cin_core(
        &pressures[1..],   // Model levels (skip surface for profile)
        &temperatures[1..],
        &dewpoints[1..],
        &heights_agl[1..],
        pressures[0],      // Surface pressure
        temperatures[0],   // 2m temperature
        dewpoints[0],      // 2m dewpoint
        "sb",              // Surface-based parcel
        100.0,             // ML depth (not used for SB)
        300.0,             // MU depth (not used for SB)
        None,              // No height cap
    );

    println!("\nSurface-Based Parcel Results:");
    println!("  CAPE:       {:.0} J/kg", cape);
    println!("  CIN:        {:.0} J/kg", cin);
    println!("  LCL height: {:.0} m AGL", h_lcl);
    println!("  LFC height: {:.0} m AGL", h_lfc);

    // Classify the environment
    let severity = if cape > 2500.0 {
        "Extreme instability"
    } else if cape > 1500.0 {
        "Strong instability"
    } else if cape > 500.0 {
        "Moderate instability"
    } else if cape > 0.0 {
        "Weak instability"
    } else {
        "Stable"
    };
    println!("  Assessment: {}", severity);

    // Mixed-layer parcel for comparison
    let (ml_cape, ml_cin, ml_lcl, ml_lfc) = metfuncs::cape_cin_core(
        &pressures[1..],
        &temperatures[1..],
        &dewpoints[1..],
        &heights_agl[1..],
        pressures[0],
        temperatures[0],
        dewpoints[0],
        "ml",
        100.0,
        300.0,
        None,
    );

    println!("\nMixed-Layer Parcel Results (100 hPa depth):");
    println!("  CAPE:       {:.0} J/kg", ml_cape);
    println!("  CIN:        {:.0} J/kg", ml_cin);
    println!("  LCL height: {:.0} m AGL", ml_lcl);
    println!("  LFC height: {:.0} m AGL", ml_lfc);

    println!("\nDone.");
}
