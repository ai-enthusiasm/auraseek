# 🛠️ AuraSeek — Environment Setup Guide

This document describes how to set up the development environment for AuraSeek from scratch on **macOS** and **Linux**.

---

## Table of Contents

- [Quick Start](#quick-start)
- [Prerequisites Overview](#prerequisites-overview)
- [macOS Setup](#macos-setup)
- [Linux Setup](#linux-setup)
- [Manual Setup (Advanced)](#manual-setup-advanced)
- [Environment Variables](#environment-variables)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

We provide automated setup scripts that detect your existing tools and only install what's missing.

### macOS

```bash
chmod +x setup-macos.sh
./setup-macos.sh
```

### Linux (Ubuntu/Debian, Fedora/RHEL, Arch)

```bash
chmod +x setup-linux.sh
./setup-linux.sh
```

After the script finishes, start the app:

```bash
yarn tauri dev
```

---

## Prerequisites Overview

| Dependency | Version | Purpose |
|---|---|---|
| **Rust** | stable (latest) | Backend language — compiles all `src-tauri/` code |
| **Node.js** | ≥ 22 | Frontend tooling (Vite, TypeScript, React) |
| **Yarn** | ≥ 1.22 | Package manager for frontend dependencies |
| **OpenCV** | 4.x | Computer vision — face detection alignment (opencv Rust crate) |
| **pkg-config** | any | Helps Rust build scripts locate C/C++ libraries |
| **CMake** | ≥ 3.16 | Build system for some native Rust dependencies |
| **Protobuf** | ≥ 3.0 | Required by `tonic`/`prost` crates (Qdrant gRPC client) |
| **LLVM/Clang** | ≥ 14 | Required by `bindgen` for FFI (OpenCV, ONNX Runtime) |
| **Tauri CLI** | 2.x | Desktop app framework — installed as npm devDependency |

> [!NOTE]
> **SurrealDB** is **not** needed. The project migrated to **SQLite** (bundled via `rusqlite`) + **Qdrant** (auto-downloaded as a sidecar binary on first launch). No external database setup is required.

> [!NOTE]
> **ONNX Runtime** is bundled automatically by the `ort` Rust crate during `cargo build`. No manual installation needed.

---

## macOS Setup

### What the script does

The `setup-macos.sh` script performs these steps in order:

| Step | Action | Skipped if... |
|---|---|---|
| 1 | Install Xcode Command Line Tools | `xcode-select -p` succeeds |
| 2 | Install Homebrew | `brew` is found |
| 3 | Install OpenCV, pkg-config, cmake, protobuf, llvm | each is already in `brew list` |
| 4 | Install Rust (via rustup) | `rustc` is found |
| 5 | Install Node.js 22 (via nvm) | `node --version` ≥ 22 |
| 6 | Install Yarn (via corepack) | `yarn` is found |
| 7 | Check Tauri CLI availability | — |
| 8 | Run `yarn install` | — |

### Post-setup: Shell Profile

If you use Apple Silicon, ensure LLVM is accessible for `bindgen`. Add this to your `~/.zshrc`:

```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

Then reload:

```bash
source ~/.zshrc
```

---

## Linux Setup

### Supported Distributions

| Family | Distributions | Package Manager |
|---|---|---|
| **Debian** | Ubuntu, Debian, Linux Mint, Pop!_OS, Zorin | `apt-get` |
| **Fedora** | Fedora, RHEL, CentOS Stream, Rocky, Alma | `dnf` |
| **Arch** | Arch Linux, Manjaro, EndeavourOS, Garuda | `pacman` |
| **openSUSE** | openSUSE Tumbleweed/Leap | `zypper` |

### What the script does

The `setup-linux.sh` script performs these steps:

| Step | Action | Skipped if... |
|---|---|---|
| 1 | Detect Linux distro family | — |
| 2 | Install system packages (Tauri deps + OpenCV + build tools) | — |
| 3 | Install Rust (via rustup) | `rustc` is found |
| 4 | Install Node.js 22 (via nvm) | `node --version` ≥ 22 |
| 5 | Install Yarn (via corepack) | `yarn` is found |
| 6 | Check Tauri CLI availability | — |
| 7 | Run `yarn install` | — |

### Tauri 2 Linux System Dependencies

Tauri 2 on Linux requires WebKit2GTK and related libraries. The setup script installs:

**Ubuntu/Debian:**
```bash
libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

**Fedora:**
```bash
webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel patchelf
```

**Arch:**
```bash
webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg patchelf
```

---

## Manual Setup (Advanced)

If you prefer not to use the automated scripts, follow these instructions.

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
```

### 2. Install Node.js 22

Using nvm (recommended):
```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
source ~/.bashrc  # or ~/.zshrc
nvm install 22
nvm use 22
```

### 3. Install Yarn

```bash
corepack enable
corepack prepare yarn@stable --activate
```

### 4. Install System Libraries

#### macOS (Homebrew)

```bash
brew install opencv pkg-config cmake protobuf llvm
```

#### Ubuntu/Debian

```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential curl wget file git \
    libwebkit2gtk-4.1-dev libgtk-3-dev \
    libayatana-appindicator3-dev librsvg2-dev patchelf \
    libopencv-dev pkg-config cmake \
    protobuf-compiler libprotobuf-dev \
    llvm-dev libclang-dev clang libssl-dev
```

#### Fedora

```bash
sudo dnf install -y \
    gcc gcc-c++ curl wget file git \
    webkit2gtk4.1-devel gtk3-devel \
    libappindicator-gtk3-devel librsvg2-devel patchelf \
    opencv-devel pkgconf-pkg-config cmake \
    protobuf-compiler protobuf-devel \
    llvm-devel clang-devel clang openssl-devel
```

#### Arch Linux

```bash
sudo pacman -S --needed \
    base-devel curl wget file git \
    webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg patchelf \
    opencv vtk hdf5 pkgconf cmake protobuf \
    llvm clang openssl
```

### 5. Install Frontend Dependencies

```bash
yarn install
```

### 6. Run

```bash
yarn tauri dev
```

---

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `LIBCLANG_PATH` | macOS (Homebrew) | — | Path to LLVM `lib/` directory. Needed for `bindgen`. Set to `$(brew --prefix llvm)/lib`. |
| `OPENCV_LINK_LIBS` | Rarely | auto-detected | Override OpenCV link libraries if `pkg-config` detection fails. |
| `OPENCV_INCLUDE_PATHS` | Rarely | auto-detected | Override OpenCV include paths. |
| `ORT_DYLIB_PATH` | Never | bundled | ONNX Runtime is bundled by the `ort` crate. |

---

## Troubleshooting

### `error: failed to run custom build command for 'opencv'`

**Cause:** OpenCV is not installed or `pkg-config` cannot find it.

**Fix:**
```bash
# macOS
brew install opencv pkg-config

# Ubuntu/Debian
sudo apt-get install libopencv-dev pkg-config

# Verify
pkg-config --modversion opencv4
```

### `fatal error: 'clang-c/Index.h' file not found` or `bindgen` errors

**Cause:** LLVM/Clang development libraries are missing.

**Fix (macOS):**
```bash
brew install llvm
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

**Fix (Ubuntu):**
```bash
sudo apt-get install llvm-dev libclang-dev clang
```

### `error: linker 'cc' not found`

**Cause:** C/C++ compiler is not installed.

**Fix:**
```bash
# macOS
xcode-select --install

# Ubuntu/Debian
sudo apt-get install build-essential

# Fedora
sudo dnf install gcc gcc-c++

# Arch
sudo pacman -S base-devel
```

### `yarn tauri dev` fails with `webkit2gtk-4.1 not found` (Linux only)

**Cause:** Tauri 2 requires WebKit2GTK 4.1.

**Fix:**
```bash
# Ubuntu/Debian (22.04+)
sudo apt-get install libwebkit2gtk-4.1-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel

# Arch
sudo pacman -S webkit2gtk-4.1
```

### Node.js version conflict

**Cause:** An older Node.js is installed system-wide (e.g., from distro package manager).

**Fix:** Use nvm to manage Node.js versions independently:
```bash
nvm install 22
nvm use 22
nvm alias default 22
```

The `.nvmrc` file in the project root specifies `22`, so `nvm use` in the project directory will automatically select the correct version.

### `protoc: command not found`

**Cause:** The Protobuf compiler is required by the `prost` / `tonic` crates (Qdrant gRPC client).

**Fix:**
```bash
# macOS
brew install protobuf

# Ubuntu/Debian
sudo apt-get install protobuf-compiler

# Fedora
sudo dnf install protobuf-compiler

# Arch
sudo pacman -S protobuf
```

### Qdrant fails to start

Qdrant is auto-downloaded on first launch. If it fails:

1. Check your internet connection
2. Look at the Qdrant log: `~/Library/Application Support/com.aienthusiasm.auraseek/qdrant/qdrant.log` (macOS) or `~/.local/share/com.aienthusiasm.auraseek/qdrant/qdrant.log` (Linux)
3. Kill any stale Qdrant processes: `pkill -x qdrant`

### AI Models not found

Models are downloaded automatically during `cargo build`. If they're missing:

1. Run `cargo build` in the `src-tauri/` directory once to trigger the download
2. Check your internet connection — models are fetched from GitHub Releases

---

## Verification

After setup, verify your environment:

```bash
# Rust
rustc --version      # Should be ≥ 1.75
cargo --version

# Node.js
node --version       # Should be ≥ 22.x

# Yarn
yarn --version

# OpenCV
pkg-config --modversion opencv4   # Should be 4.x

# Build check
cd src-tauri && cargo check       # Should compile without errors
```

If all commands succeed, you're ready to develop:

```bash
yarn tauri dev
```
