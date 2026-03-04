use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

fn main() -> Result<()> {

    tauri_build::build();

    // define model URLs and paths
    let models = vec![
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/text_tower_aura.onnx",
            "assets/models/text_tower_aura.onnx"
        ),
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/vision_tower_aura.onnx",
            "assets/models/vision_tower_aura.onnx"
        ),
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/face_recognition_sface_2021dec.onnx",
            "assets/models/face_recognition_sface_2021dec.onnx"
        ),
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/face_detection_yunet_2022mar.onnx",
            "assets/models/face_detection_yunet_2022mar.onnx"
        ),
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/yolo26n-seg.onnx",
            "assets/models/yolo26n-seg.onnx"
        ),
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/bpe.codes",
            "assets/tokenizer/bpe.codes"
        ),
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/vocab.txt",
            "assets/tokenizer/vocab.txt"
        ),
        (
            "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0/DejaVuSans.ttf",
            "assets/fonts/DejaVuSans.ttf"
        ),
    ];

    // ensure directories exist and download if missing
    for (url, path_str) in models {
        let path = Path::new(path_str);
        if !path.exists() {
            println!("cargo:warning=Downloading model from {} to {}", url, path_str);
            
            // Create parent directory
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {:?}", parent))?;
            }

            // Download file
            download_file(url, path_str)?;
        }
    }

    // create empty directories if missing
    let empty_dirs = vec![
        "assets/face_db",
    ];

    for dir in empty_dirs {
        if !Path::new(dir).exists() {
            fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create directory: {}", dir))?;
            println!("cargo:warning=Created directory: {}", dir);
        }
    }

    // rerun if build.rs changes or models are missing (though path.exists check handles the latter)
    println!("cargo:rerun-if-changed=build.rs");
    
    Ok(())
}

fn download_file(url: &str, path: &str) -> Result<()> {
    // use ureq to download
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("Failed to download from {}", url))?;

    if response.status() != 200 {
        return Err(anyhow::anyhow!("Download failed with status: {}", response.status()));
    }

    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed to create file: {}", path))?;
    
    let mut reader = response.into_reader();
    std::io::copy(&mut reader, &mut file)
        .with_context(|| format!("Failed to write to file: {}", path))?;

    Ok(())
}
