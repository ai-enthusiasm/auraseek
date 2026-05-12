use anyhow::Result;

/// preprocess image to chw f32 blob
pub fn preprocess_aura(path: &str) -> Result<Vec<f32>> {
    let img = image::io::Reader::open(path)?.decode()?;
    Ok(preprocess_aura_from_image(&img))
}

pub fn preprocess_aura_from_image(img: &image::DynamicImage) -> Vec<f32> {
    let resized = img.resize_exact(256, 256, image::imageops::FilterType::Triangle);
    let rgb     = resized.to_rgb8();

    let area: usize   = 256 * 256;
    let mut blob      = vec![0.0f32; 3 * area];

    for (x, y, pixel) in rgb.enumerate_pixels() {
        let idx = (y as usize * 256) + x as usize;
        // imagenet normalization
        blob[idx]            = (pixel[0] as f32 / 255.0 - 0.485) / 0.229; // r
        blob[idx + area]     = (pixel[1] as f32 / 255.0 - 0.456) / 0.224; // g
        blob[idx + 2 * area] = (pixel[2] as f32 / 255.0 - 0.406) / 0.225; // b
    }
    blob
}
