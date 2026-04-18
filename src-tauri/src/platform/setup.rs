use std::path::Path;

/// Platform-specific initialization that must run before the Tauri app starts.
pub fn pre_init() {
    #[cfg(target_os = "linux")]
    {
        // Workaround for WebKitGTK DRI2/hardware-acceleration crash on
        // NVIDIA proprietary drivers (DMABUF → DRI2Connect X11 errors).
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Future macOS-specific init (e.g. Sparkle updater, entitlements checks)
    }
}

/// Ensure native shared libraries are present next to the executable.
///
/// On **Windows** this copies bundled OpenCV / MSVC DLLs from the Tauri
/// resource directory into the exe directory so the app works on machines
/// without a global install.  On other platforms this is a no-op.
pub fn ensure_native_libs(resource_dir: &Path) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get exe parent dir"))?;

    #[cfg(windows)]
    {
        let windows_libs_dir = resource_dir.join("libs").join("windows");
        if windows_libs_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&windows_libs_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(filename) = path.file_name() {
                            let dst = exe_dir.join(filename);
                            if !dst.exists() {
                                crate::log_info!("📦 Deploying system DLL to exe dir: {:?}", filename);
                                if let Err(e) = std::fs::copy(&path, &dst) {
                                    crate::log_warn!("⚠️ Failed to copy {:?}: {}", filename, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let macos_libs_dir = resource_dir.join("libs").join("macos");
        if macos_libs_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&macos_libs_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() || path.is_symlink() {
                        if let Some(filename) = path.file_name() {
                            let dst = exe_dir.join(filename);
                            if !dst.exists() {
                                crate::log_info!("📦 Deploying system library to exe dir: {:?}", filename);
                                if let Err(e) = std::fs::copy(&path, &dst) {
                                    crate::log_warn!("⚠️ Failed to copy {:?}: {}", filename, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
