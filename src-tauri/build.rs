use anyhow::Result;

fn main() -> Result<()> {
    tauri_build::build();

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Resources/libs/macos/");
    }

    // rerun if build.rs changes
    println!("cargo:rerun-if-changed=build.rs");
    
    Ok(())
}
