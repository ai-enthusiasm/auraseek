use crate::log_info;
use crate::log_warn;

pub fn build_session(model_path: &str, num_threads: usize) -> anyhow::Result<ort::session::Session> {
    use ort::session::Session;
    use ort::execution_providers::ExecutionProviderDispatch;

    // Define platform-specific provider preferences
    #[cfg(target_os = "macos")]
    let ep_builders: Vec<(&str, Box<dyn Fn() -> ExecutionProviderDispatch>)> = vec![
        ("CoreML",   Box::new(|| ort::execution_providers::CoreMLExecutionProvider::default().build())),
    ];

    #[cfg(not(target_os = "macos"))]
    let ep_builders: Vec<(&str, Box<dyn Fn() -> ExecutionProviderDispatch>)> = vec![
        ("TensorRT", Box::new(|| ort::execution_providers::TensorRTExecutionProvider::default().build())),
        ("CUDA",     Box::new(|| ort::execution_providers::CUDAExecutionProvider::default().build())),
        ("DirectML", Box::new(|| ort::execution_providers::DirectMLExecutionProvider::default().build())),
        ("OpenVINO", Box::new(|| ort::execution_providers::OpenVINOExecutionProvider::default().build())),
        ("CoreML",   Box::new(|| ort::execution_providers::CoreMLExecutionProvider::default().build())),
    ];

    for (name, builder_fn) in ep_builders {
        let ep = builder_fn();
        
        // Attempt to create builder with EP and thread limits
        let builder = match Session::builder().map_err(|e| anyhow::anyhow!(e.to_string())) {
            Ok(b) => b,
            Err(e) => {
                log_warn!("Failed to create SessionBuilder: {}", e);
                continue;
            }
        };

        // Apply thread limiting for resource optimization
        let builder = builder
            .with_intra_threads(num_threads)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_inter_threads(num_threads)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        match builder.with_execution_providers([ep]) {
            Ok(mut builder) => {
                match builder.commit_from_file(model_path) {
                    Ok(session) => {
                        log_info!("✅ Model initialized: {:<30} | provider: {} | threads: {}", 
                            std::path::Path::new(model_path).file_name().unwrap_or_default().to_string_lossy(), 
                            name, num_threads);
                        return Ok(session);
                    }
                    Err(e) => {
                        log_warn!("⚠️ Failed to commit {} with {}: {}", model_path, name, e);
                    }
                }
            }
            Err(e) => {
                log_warn!("❌ Provider {} not available for {}: {}", name, model_path, e);
            }
        }
    }

    // fallback to cpu
    log_info!("🐢 Falling back to CPU for model: {} | threads: {}", model_path, num_threads);
    let cpu_ep = ort::execution_providers::CPUExecutionProvider::default().build();

    let builder = Session::builder()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .with_intra_threads(num_threads)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .with_inter_threads(num_threads)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let mut builder = builder
        .with_execution_providers([cpu_ep])
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        
    let session = builder
        .commit_from_file(model_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(session)
}
