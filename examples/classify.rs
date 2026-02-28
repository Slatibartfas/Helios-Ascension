use helios_ascension::plugins::starmap::classify_exoplanet;
use helios_ascension::plugins::solar_system_data::BodyType;

fn main() {
    for temp in &[-20.0_f32, -10.0, -5.0, 0.0, 15.0, 50.0, 60.0, 60.1, 200.0] {
        let cat = classify_exoplanet(BodyType::Planet, None, *temp, 0);
        println!("{} -> {}", temp, cat);
    }
}
