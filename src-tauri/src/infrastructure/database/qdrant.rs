use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, VectorParamsBuilder,
    ScalarQuantizationBuilder, HnswConfigDiffBuilder,
    OptimizersConfigDiffBuilder,
};

use crate::platform::process::hidden_command;

const QDRANT_VERSION: &str = "v1.18.0";
const QDRANT_WEB_UI_VERSION: &str = "v0.2.11";

pub struct QdrantService;

pub struct QdrantStartResult {
    pub child: Option<Child>,
    pub grpc_port: u16,
    pub http_port: u16,
}

impl QdrantService {
    pub fn qdrant_binary_name() -> &'static str {
        if cfg!(windows) { "qdrant.exe" } else { "qdrant" }
    }

    pub fn find_binary(data_dir: &Path) -> Option<PathBuf> {
        let bin = Self::qdrant_binary_name();
        let candidate = data_dir.join("qdrant").join(bin);
        if candidate.exists() {
            crate::log_info!("🗄️  Found Qdrant binary: {}", candidate.display());
            return Some(candidate);
        }

        let which_cmd = crate::platform::paths::which_command();
        if let Ok(out) = hidden_command(which_cmd).arg("qdrant").output() {
            let s = String::from_utf8_lossy(&out.stdout);
            let first_line = s.lines().next().unwrap_or("").trim().to_string();
            if !first_line.is_empty() {
                let p = PathBuf::from(&first_line);
                crate::log_info!("🗄️  Found Qdrant binary (PATH): {}", p.display());
                return Some(p);
            }
        }

        crate::log_warn!("⚠️  Qdrant binary not found. It will be downloaded on first launch.");
        None
    }

    pub fn web_ui_static_dir(data_dir: &Path) -> PathBuf {
        data_dir.join("qdrant").join("static")
    }

    pub fn web_ui_present(data_dir: &Path) -> bool {
        let static_dir = Self::web_ui_static_dir(data_dir);
        static_dir.join("index.html").exists() && static_dir.join("assets").is_dir()
    }

    pub fn assets_present(data_dir: &Path, dashboard_enabled: bool) -> bool {
        Self::find_binary(data_dir).is_some()
            && (!dashboard_enabled || Self::web_ui_present(data_dir))
    }

    pub fn start(
        binary: &Path,
        storage_dir: &Path,
        static_dir: &Path,
        grpc_port: u16,
        http_port: u16,
        dashboard_enabled: bool,
    ) -> Result<Child> {
        std::fs::create_dir_all(storage_dir)
            .context("Failed to create Qdrant storage directory")?;
        let snapshots_dir = storage_dir.join("snapshots");
        std::fs::create_dir_all(&snapshots_dir)
            .context("Failed to create Qdrant snapshots directory")?;

        let log_path = storage_dir.join("qdrant.log");

        crate::log_info!(
            "🗄️  Starting Qdrant | binary={} grpc={} http={} dashboard={} storage={} static={}",
            binary.display(),
            grpc_port,
            http_port,
            if dashboard_enabled { "enabled" } else { "disabled" },
            storage_dir.display(),
            static_dir.display()
        );

        let log_file = std::fs::OpenOptions::new()
            .create(true).append(true).open(&log_path)
            .with_context(|| format!("Failed to open Qdrant log: {}", log_path.display()))?;
        let log_err = log_file.try_clone()?;

        let mut command = hidden_command(binary);
        command
            .current_dir(storage_dir)
            .env("QDRANT__STORAGE__STORAGE_PATH", storage_dir.to_string_lossy().as_ref())
            .env("QDRANT__STORAGE__SNAPSHOTS_PATH", snapshots_dir.to_string_lossy().as_ref())
            .env("QDRANT__SERVICE__GRPC_PORT", grpc_port.to_string())
            .env("QDRANT__SERVICE__HTTP_PORT", http_port.to_string())
            .env("QDRANT__SERVICE__ENABLE_STATIC_CONTENT", dashboard_enabled.to_string())
            .env("QDRANT__STORAGE__ON_DISK_PAYLOAD", "true");

        if dashboard_enabled {
            command.env("QDRANT__SERVICE__STATIC_CONTENT_DIR", static_dir.to_string_lossy().as_ref());
        }

        let child = command
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_err))
            .spawn()
            .with_context(|| format!("Failed to spawn Qdrant from {}", binary.display()))?;

        crate::log_info!("✅ Qdrant spawned (pid={})", child.id());
        Ok(child)
    }

    fn log_tail(storage_dir: &Path, lines: usize) -> String {
        let log_path = storage_dir.join("qdrant.log");
        let snippet = std::fs::read_to_string(&log_path).unwrap_or_default();
        snippet
            .lines()
            .rev()
            .take(lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn http_status(port: u16, path: &str) -> Option<u16> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().ok()?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            path, port
        );
        stream.write_all(req.as_bytes()).ok()?;

        let mut buf = [0u8; 512];
        let n = stream.read(&mut buf).ok()?;
        let head = std::str::from_utf8(&buf[..n]).ok()?;
        let status = head.lines().next()?.split_whitespace().nth(1)?;
        status.parse().ok()
    }

    fn http_is_qdrant(port: u16) -> bool {
        let addr: SocketAddr = match format!("127.0.0.1:{}", port).parse() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(300)) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let req = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            port
        );
        if stream.write_all(req.as_bytes()).is_err() {
            return false;
        }

        let mut buf = [0u8; 1024];
        let n = match stream.read(&mut buf) {
            Ok(size) => size,
            Err(_) => return false,
        };
        let response = match std::str::from_utf8(&buf[..n]) {
            Ok(s) => s,
            Err(_) => return false,
        };
        response.to_lowercase().contains("qdrant")
    }

    pub fn wait_ready(
        child: &mut Child,
        storage_dir: &Path,
        grpc_port: u16,
        http_port: u16,
        dashboard_enabled: bool,
        max_secs: u64,
    ) -> Result<()> {
        let addr: SocketAddr = format!("127.0.0.1:{}", grpc_port).parse()?;
        let deadline = Instant::now() + Duration::from_secs(max_secs);
        let mut last_dashboard_status = None;

        while Instant::now() < deadline {
            if let Ok(Some(status)) = child.try_wait() {
                anyhow::bail!(
                    "Qdrant exited before becoming ready (status={}). See {}.\n{}",
                    status,
                    storage_dir.join("qdrant.log").display(),
                    Self::log_tail(storage_dir, 30)
                );
            }

            let grpc_ready = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok();
            let dashboard_ready = if dashboard_enabled {
                last_dashboard_status = Self::http_status(http_port, "/dashboard")
                    .or_else(|| Self::http_status(http_port, "/dashboard/"));
                matches!(last_dashboard_status, Some(200..=399))
            } else {
                true
            };
            if grpc_ready && dashboard_ready {
                if dashboard_enabled {
                    crate::log_info!(
                        "✅ Qdrant ready | grpc={} dashboard=http://127.0.0.1:{}/dashboard",
                        grpc_port, http_port
                    );
                } else {
                    crate::log_info!("✅ Qdrant ready | grpc={} dashboard=disabled", grpc_port);
                }
                return Ok(());
            }

            std::thread::sleep(Duration::from_millis(400));
        }

        Err(anyhow::anyhow!(
            "Qdrant did not become ready within {}s (grpc_port={}, dashboard_enabled={}, dashboard_port={}, dashboard_status={:?}). \
             If this keeps happening, stop any stale qdrant process using these ports.\n{}",
            max_secs,
            grpc_port,
            dashboard_enabled,
            http_port,
            last_dashboard_status,
            Self::log_tail(storage_dir, 30)
        ))
    }

    pub fn ensure(
        data_dir: &Path,
        storage_dir: &Path,
        grpc_port: u16,
        http_port: u16,
        dashboard_enabled: bool,
    ) -> Result<QdrantStartResult> {
        // 1. Check if Qdrant is already running and healthy on the default ports
        if Self::http_is_qdrant(http_port) {
            crate::log_info!("🗄️  Qdrant is already running and healthy on grpc={}, http={}. Reusing it!", grpc_port, http_port);
            return Ok(QdrantStartResult {
                child: None,
                grpc_port,
                http_port,
            });
        }

        // 2. If not healthy or not running, clean up any stuck/zombie Qdrant instances to release database locks & ports
        crate::log_info!("🗄️  Cleaning up any stale/zombie Qdrant instances...");
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("pkill")
                .arg("-x")
                .arg("qdrant")
                .output();
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .arg("/F")
                .arg("/IM")
                .arg("qdrant.exe")
                .output();
        }
        // Give the OS a moment to release file locks and sockets
        std::thread::sleep(Duration::from_millis(500));

        let binary = Self::find_binary(data_dir)
            .ok_or_else(|| anyhow::anyhow!(
                "Qdrant binary not found. It should have been downloaded on first launch."
            ))?;

        let static_dir = Self::web_ui_static_dir(data_dir);

        let max_attempts = 20u16;
        let mut errors = Vec::new();
        for attempt in 0..max_attempts {
            let candidate_grpc = grpc_port.saturating_add(attempt);
            let candidate_http = http_port.saturating_add(attempt);

            let grpc_free = std::net::TcpListener::bind(("127.0.0.1", candidate_grpc)).is_ok();
            let http_free = std::net::TcpListener::bind(("127.0.0.1", candidate_http)).is_ok();

            if !grpc_free || !http_free {
                errors.push(format!(
                    "ports in use: grpc_free={} (port={}), http_free={} (port={})",
                    grpc_free, candidate_grpc, http_free, candidate_http
                ));
                continue;
            }

            crate::log_info!(
                "🗄️  Qdrant port attempt {}/{} | grpc={} http={}",
                attempt + 1,
                max_attempts,
                candidate_grpc,
                candidate_http
            );

            let mut child = match Self::start(
                &binary,
                storage_dir,
                &static_dir,
                candidate_grpc,
                candidate_http,
                dashboard_enabled,
            ) {
                Ok(child) => child,
                Err(e) => {
                    errors.push(format!("grpc={} http={}: spawn failed: {:#}", candidate_grpc, candidate_http, e));
                    continue;
                }
            };

            match Self::wait_ready(&mut child, storage_dir, candidate_grpc, candidate_http, dashboard_enabled, 10) {
                Ok(()) => {
                    if dashboard_enabled {
                        crate::log_info!("🧭 Qdrant dashboard: http://127.0.0.1:{}/dashboard", candidate_http);
                    }
                    return Ok(QdrantStartResult {
                        child: Some(child),
                        grpc_port: candidate_grpc,
                        http_port: candidate_http,
                    });
                }
                Err(e) => {
                    errors.push(format!("grpc={} http={}: {:#}", candidate_grpc, candidate_http, e));
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }

        anyhow::bail!(
            "Qdrant could not start after {} port attempts from grpc={} http={}. Last errors:\n{}",
            max_attempts,
            grpc_port,
            http_port,
            errors.join("\n---\n")
        )
    }

    pub async fn connect_client(grpc_port: u16) -> Result<Qdrant> {
        let url = format!("http://127.0.0.1:{}", grpc_port);
        // We set a short default timeout to avoid hanging the init process
        let mut config = Qdrant::from_url(&url);
        config.timeout = Duration::from_secs(10);
        config.check_compatibility = false;
        let client = config.build()
            .with_context(|| format!("Failed to construct Qdrant client at {}", url))?;

        let mut attempts = 0;
        let max_attempts = 15;
        loop {
            // Ping Qdrant with a very short 1-second timeout so we don't hang if the gRPC port is open but unresponsive
            let ping = tokio::time::timeout(Duration::from_secs(1), async {
                client.health_check().await.is_ok() || client.list_collections().await.is_ok()
            }).await;

            if let Ok(true) = ping {
                break;
            }
            attempts += 1;
            if attempts >= max_attempts {
                anyhow::bail!("Qdrant gRPC layer not ready after {} attempts at {}", max_attempts, url);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        crate::log_info!("✅ Qdrant client connected and fully ready: {}", url);
        Ok(client)
    }

    pub async fn ensure_collection(client: &Qdrant, name: &str, dim: u32) -> Result<()> {
        let collections = client.list_collections().await
            .context("Failed to list Qdrant collections")?;

        let exists = collections.collections.iter().any(|c| c.name == name);
        if exists {
            crate::log_info!("📋 Qdrant collection '{}' already exists", name);
            return Ok(());
        }

        crate::log_info!("📋 Creating Qdrant collection '{}' (dim={}, cosine, SQ int8, on-disk)", name, dim);

        let vectors_config = VectorParamsBuilder::new(dim as u64, Distance::Cosine)
            .on_disk(true);

        client.create_collection(
            CreateCollectionBuilder::new(name)
                .vectors_config(vectors_config)
                .quantization_config(
                    ScalarQuantizationBuilder::default()
                        .r#type(1) // 1 = Int8
                        .always_ram(true)
                )
                .hnsw_config(
                    HnswConfigDiffBuilder::default()
                        .m(16)
                        .ef_construct(100)
                )
                .optimizers_config(
                    OptimizersConfigDiffBuilder::default()
                        .memmap_threshold(20000)
                )
        ).await.context("Failed to create Qdrant collection")?;

        crate::log_info!("✅ Qdrant collection '{}' created", name);
        Ok(())
    }

    pub async fn download_if_missing(data_dir: &Path, dashboard_enabled: bool) -> Result<PathBuf> {
        let bin_dir = data_dir.join("qdrant");
        let bin_path = bin_dir.join(Self::qdrant_binary_name());
        if bin_path.exists() {
            if dashboard_enabled {
                Self::download_web_ui_if_missing(data_dir).await?;
            }
            return Ok(bin_path);
        }

        std::fs::create_dir_all(&bin_dir)?;

        let target_os = std::env::consts::OS;
        let target_arch = std::env::consts::ARCH;

        let (asset_name, is_zip) = match (target_os, target_arch) {
            ("macos", "aarch64") => ("qdrant-aarch64-apple-darwin.tar.gz", false),
            ("macos", "x86_64")  => ("qdrant-x86_64-apple-darwin.tar.gz", false),
            ("linux", "x86_64")  => ("qdrant-x86_64-unknown-linux-musl.tar.gz", false),
            ("windows", "x86_64") => ("qdrant-x86_64-pc-windows-msvc.zip", true),
            _ => anyhow::bail!("Qdrant: unsupported platform {}/{}", target_os, target_arch),
        };

        let url = format!(
            "https://github.com/qdrant/qdrant/releases/download/{}/{}",
            QDRANT_VERSION, asset_name
        );
        crate::log_info!("📥 Downloading Qdrant {} from {}", QDRANT_VERSION, url);

        let client = reqwest::Client::new();
        let res = client.get(&url).send().await
            .with_context(|| format!("connect to {}", url))?;
        if !res.status().is_success() {
            anyhow::bail!("HTTP {} for Qdrant download", res.status());
        }

        let bytes = res.bytes().await.context("download Qdrant binary")?;
        let tmp = bin_dir.join("qdrant_download.tmp");
        std::fs::write(&tmp, &bytes)?;

        if is_zip {
            let tmp_clone = tmp.clone();
            let bin_dir_clone = bin_dir.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let file = std::fs::File::open(&tmp_clone)?;
                let mut archive = zip::ZipArchive::new(file)?;
                for i in 0..archive.len() {
                    let mut entry = archive.by_index(i)?;
                    let name = entry.name().to_string();
                    if name.ends_with("qdrant.exe") || name.ends_with("qdrant") {
                        let out_path = bin_dir_clone.join(
                            std::path::Path::new(&name).file_name().unwrap_or(std::ffi::OsStr::new("qdrant"))
                        );
                        let mut out_file = std::fs::File::create(&out_path)?;
                        std::io::copy(&mut entry, &mut out_file)?;
                        break;
                    }
                }
                Ok(())
            }).await??;
        } else {
            let tmp_clone = tmp.clone();
            let bin_dir_clone = bin_dir.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                use flate2::read::GzDecoder;
                use tar::Archive;
                let f = std::fs::File::open(&tmp_clone)?;
                let gz = GzDecoder::new(f);
                let mut archive = Archive::new(gz);
                for entry in archive.entries()? {
                    let mut entry = entry?;
                    let path = entry.path()?;
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name == "qdrant" || name == "qdrant.exe" {
                        entry.unpack(bin_dir_clone.join(name))?;
                        break;
                    }
                }
                Ok(())
            }).await??;
        }

        let _ = std::fs::remove_file(&tmp);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = std::fs::metadata(&bin_path).map(|m| m.permissions()) {
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&bin_path, perms);
            }
        }

        crate::log_info!("✅ Qdrant downloaded: {}", bin_path.display());
        if dashboard_enabled {
            Self::download_web_ui_if_missing(data_dir).await?;
        }
        Ok(bin_path)
    }

    async fn download_web_ui_if_missing(data_dir: &Path) -> Result<()> {
        if Self::web_ui_present(data_dir) {
            return Ok(());
        }

        let static_dir = Self::web_ui_static_dir(data_dir);
        if static_dir.exists() {
            let _ = std::fs::remove_dir_all(&static_dir);
        }
        std::fs::create_dir_all(&static_dir)?;

        let url = format!(
            "https://github.com/qdrant/qdrant-web-ui/releases/download/{}/dist-qdrant.zip",
            QDRANT_WEB_UI_VERSION
        );
        crate::log_info!("📥 Downloading Qdrant Web UI {} from {}", QDRANT_WEB_UI_VERSION, url);

        let client = reqwest::Client::new();
        let res = client.get(&url).send().await
            .with_context(|| format!("connect to {}", url))?;
        if !res.status().is_success() {
            anyhow::bail!("HTTP {} for Qdrant Web UI download", res.status());
        }

        let bytes = res.bytes().await.context("download Qdrant Web UI")?;
        let static_dir_clone = static_dir.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let cursor = std::io::Cursor::new(bytes);
            let mut archive = zip::ZipArchive::new(cursor)?;

            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)?;
                if !entry.is_file() {
                    continue;
                }

                let Some(enclosed) = entry.enclosed_name() else {
                    continue;
                };
                let relative = enclosed
                    .strip_prefix("dist")
                    .unwrap_or(enclosed.as_path());
                if relative.as_os_str().is_empty() {
                    continue;
                }

                let out_path = static_dir_clone.join(relative);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut out_file = std::fs::File::create(&out_path)?;
                std::io::copy(&mut entry, &mut out_file)?;
            }

            Ok(())
        }).await??;

        if !Self::web_ui_present(data_dir) {
            anyhow::bail!(
                "Qdrant Web UI download did not create expected files in {}",
                static_dir.display()
            );
        }

        crate::log_info!("✅ Qdrant Web UI ready: {}", static_dir.display());
        Ok(())
    }
}
