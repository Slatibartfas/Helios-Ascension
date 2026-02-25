use helios_ascension::fleets::types::PropulsionType;

/// Verifies that the thrust calculation produces reasonable, unit-correct values.
///
/// This test guards against the previous bug where the tonne-to-kilogram
/// conversion was applied incorrectly, resulting in thrust figures that were
/// three orders of magnitude too small.
#[test]
fn thrust_calculation_units() {
    // a 2 000‑tonne frigate with a chemical engine (TWR = 10) should produce
    // roughly 2 000 × 10 × 9.81 = 196_200 kN of thrust.
    let dry = 2_000.0_f32;
    let expected = dry * 10.0 * 9.81;
    let chemical = PropulsionType::Chemical.thrust_kn(dry);
    assert!(
        (chemical - expected).abs() < 1.0,
        "chemical thrust was {} kN, expected {} kN",
        chemical,
        expected
    );

    // an ion drive on a 3 000‑tonne ship should be very low thrust but still
    // correctly scaled (TWR = 0.001 → ≈ 29.43 kN).
    let dry2 = 3_000.0_f32;
    let expected2 = dry2 * 0.001 * 9.81;
    let ion = PropulsionType::IonDrive.thrust_kn(dry2);
    assert!(
        (ion - expected2).abs() < 1.0,
        "ion drive thrust was {} kN, expected {} kN",
        ion,
        expected2
    );

    // Check that every propulsion type returns a non-negative value for a
    // nominal mass; this serves as a basic sanity check.
    for prop in [
        PropulsionType::Chemical,
        PropulsionType::NuclearThermal,
        PropulsionType::IonDrive,
        PropulsionType::NuclearPulse,
        PropulsionType::FusionTorch,
    ] {
        let thrust = prop.thrust_kn(1_000.0);
        assert!(thrust >= 0.0, "negative thrust for {:?}", prop);
    }
}
