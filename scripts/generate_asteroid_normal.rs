use image::{ImageBuffer, Rgba};
use std::path::Path;

fn main() {
    let path = Path::new("assets/textures/celestial/asteroids/generic_rock_normal_2k.png");
    let size = 1024u32;
    let mut image = ImageBuffer::from_pixel(size, size, Rgba([128, 128, 255, 255]));
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let nx = x as f32 / size as f32 * std::f32::consts::TAU;
        let ny = y as f32 / size as f32 * std::f32::consts::TAU;
        let relief = (nx.sin() * (ny * 3.0).cos() + (nx * 3.0).cos() * ny.sin()) * 7.0;
        let detail = ((nx * 17.0).sin() * (ny * 13.0).cos()) * 3.0;
        let z = (255.0 - relief.abs() - detail.abs()).clamp(220.0, 255.0) as u8;
        *pixel = Rgba([
            (128.0 + relief + detail).clamp(0.0, 255.0) as u8,
            (128.0 - relief * 0.5).clamp(0.0, 255.0) as u8,
            z,
            255,
        ]);
    }
    image.save(path).expect("write asteroid normal map");
}
