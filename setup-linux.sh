#!/usr/bin/env bash
#
# setup-linux.sh — One-shot dev environment setup for AuraSeek on Linux
#
# Usage:  chmod +x setup-linux.sh && ./setup-linux.sh
#
# Supports: Ubuntu/Debian, Fedora/RHEL, Arch Linux
#
# What it does:
#   1. Detects Linux distribution
#   2. Installs system dependencies (OpenCV, build-essential, webkit2gtk, etc.)
#   3. Installs Rust via rustup (if missing)
#   4. Installs Node.js 22 via nvm (if missing, respects existing installations)
#   5. Installs Yarn (if missing)
#   6. Runs yarn install
#   7. Validates the full tool chain
#
set -euo pipefail

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[✓]${NC}     $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
fail()  { echo -e "${RED}[✗]${NC}     $*"; exit 1; }
step()  { echo -e "\n${BOLD}${CYAN}━━━ $* ━━━${NC}"; }

REQUIRED_NODE_MAJOR=22

# ── 0. Linux check ──────────────────────────────────────────────────────────
if [[ "$(uname)" != "Linux" ]]; then
    fail "This script is for Linux only. Use setup-macos.sh instead."
fi

# ── 1. Detect distribution ─────────────────────────────────────────────────
step "1/7  Detecting Linux Distribution"

DISTRO="unknown"
if [[ -f /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    case "$ID" in
        ubuntu|debian|linuxmint|pop|elementary|zorin)
            DISTRO="debian"
            ;;
        fedora|rhel|centos|rocky|alma)
            DISTRO="fedora"
            ;;
        arch|manjaro|endeavouros|garuda)
            DISTRO="arch"
            ;;
        opensuse*|suse*)
            DISTRO="suse"
            ;;
    esac
fi

if [[ "$DISTRO" == "unknown" ]]; then
    # Fallback detection
    if command -v apt-get &>/dev/null; then
        DISTRO="debian"
    elif command -v dnf &>/dev/null; then
        DISTRO="fedora"
    elif command -v pacman &>/dev/null; then
        DISTRO="arch"
    elif command -v zypper &>/dev/null; then
        DISTRO="suse"
    else
        fail "Unsupported Linux distribution. Please install dependencies manually."
    fi
fi

ok "Detected distribution family: $DISTRO (${PRETTY_NAME:-$ID})"

# ── 2. System Dependencies ─────────────────────────────────────────────────
step "2/7  System Dependencies"

# Determine if we need sudo
SUDO=""
if [[ $EUID -ne 0 ]]; then
    if command -v sudo &>/dev/null; then
        SUDO="sudo"
    else
        warn "Not running as root and 'sudo' not found. Package installation may fail."
    fi
fi

case "$DISTRO" in
    debian)
        info "Updating package index..."
        $SUDO apt-get update -y

        # Tauri 2 system dependencies for Linux
        # https://v2.tauri.app/start/prerequisites/#linux
        PACKAGES=(
            # Build essentials
            build-essential
            curl
            wget
            file
            git
            # Tauri/GTK/WebKit
            libwebkit2gtk-4.1-dev
            libgtk-3-dev
            libayatana-appindicator3-dev
            librsvg2-dev
            patchelf
            # OpenCV
            libopencv-dev
            # Misc build tools
            pkg-config
            cmake
            protobuf-compiler
            libprotobuf-dev
            # Clang/LLVM for bindgen
            llvm-dev
            libclang-dev
            clang
            # SSL
            libssl-dev
        )

        info "Installing packages: ${PACKAGES[*]}"
        $SUDO apt-get install -y "${PACKAGES[@]}"
        ok "Debian/Ubuntu packages installed"
        ;;

    fedora)
        PACKAGES=(
            # Build essentials
            gcc
            gcc-c++
            curl
            wget
            file
            git
            # Tauri/GTK/WebKit
            webkit2gtk4.1-devel
            gtk3-devel
            libappindicator-gtk3-devel
            librsvg2-devel
            patchelf
            # OpenCV
            opencv-devel
            # Misc build tools
            pkgconf-pkg-config
            cmake
            protobuf-compiler
            protobuf-devel
            # Clang/LLVM
            llvm-devel
            clang-devel
            clang
            # SSL
            openssl-devel
        )

        info "Installing packages: ${PACKAGES[*]}"
        $SUDO dnf install -y "${PACKAGES[@]}"
        ok "Fedora/RHEL packages installed"
        ;;

    arch)
        PACKAGES=(
            # Build essentials
            base-devel
            curl
            wget
            file
            git
            # Tauri/GTK/WebKit
            webkit2gtk-4.1
            gtk3
            libappindicator-gtk3
            librsvg
            patchelf
            # OpenCV
            opencv
            vtk
            hdf5
            # Misc build tools
            pkgconf
            cmake
            protobuf
            # Clang/LLVM
            llvm
            clang
            # SSL
            openssl
        )

        info "Installing packages: ${PACKAGES[*]}"
        $SUDO pacman -S --needed --noconfirm "${PACKAGES[@]}"
        ok "Arch Linux packages installed"
        ;;

    suse)
        PACKAGES=(
            gcc
            gcc-c++
            curl
            wget
            file
            git
            webkit2gtk3-devel
            gtk3-devel
            libappindicator3-devel
            librsvg-devel
            patchelf
            opencv-devel
            pkg-config
            cmake
            protobuf-devel
            llvm-devel
            clang
            libopenssl-devel
        )

        info "Installing packages: ${PACKAGES[*]}"
        $SUDO zypper install -y "${PACKAGES[@]}"
        ok "openSUSE packages installed"
        ;;
esac

# ── 3. Rust ─────────────────────────────────────────────────────────────────
step "3/7  Rust Toolchain"
if command -v rustc &>/dev/null; then
    ok "Rust already installed: $(rustc --version)"
    info "Updating Rust to latest stable..."
    rustup update stable 2>/dev/null || true
else
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
    ok "Rust installed: $(rustc --version)"
fi

export PATH="$HOME/.cargo/bin:$PATH"

# ── 4. Node.js ──────────────────────────────────────────────────────────────
step "4/7  Node.js"

check_node_version() {
    if ! command -v node &>/dev/null; then
        return 1
    fi
    local ver
    ver="$(node --version 2>/dev/null | sed 's/^v//')"
    local major
    major="$(echo "$ver" | cut -d. -f1)"
    if [[ "$major" -ge "$REQUIRED_NODE_MAJOR" ]]; then
        return 0
    fi
    return 1
}

if check_node_version; then
    ok "Node.js already installed: $(node --version)"
else
    # Try to use nvm if available
    export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
    # shellcheck disable=SC1091
    [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"

    if command -v nvm &>/dev/null; then
        info "nvm detected. Installing Node.js $REQUIRED_NODE_MAJOR via nvm..."
        nvm install "$REQUIRED_NODE_MAJOR"
        nvm use "$REQUIRED_NODE_MAJOR"
    else
        info "Installing nvm..."
        curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
        export NVM_DIR="$HOME/.nvm"
        # shellcheck disable=SC1091
        [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"

        info "Installing Node.js $REQUIRED_NODE_MAJOR via nvm..."
        nvm install "$REQUIRED_NODE_MAJOR"
        nvm use "$REQUIRED_NODE_MAJOR"
    fi

    if check_node_version; then
        ok "Node.js installed: $(node --version)"
    else
        fail "Failed to install Node.js >= $REQUIRED_NODE_MAJOR"
    fi
fi

# ── 5. Yarn ─────────────────────────────────────────────────────────────────
step "5/7  Yarn"
if command -v yarn &>/dev/null; then
    ok "Yarn already installed: $(yarn --version)"
else
    info "Installing Yarn via corepack..."
    if command -v corepack &>/dev/null; then
        corepack enable
        corepack prepare yarn@stable --activate
    else
        info "corepack not found, using npm to install yarn..."
        npm install -g yarn
    fi
    ok "Yarn installed: $(yarn --version)"
fi

# ── 6. Tauri CLI ───────────────────────────────────────────────────────────
step "6/7  Tauri CLI"
if npx tauri --version &>/dev/null 2>&1; then
    ok "Tauri CLI available: $(npx tauri --version 2>/dev/null)"
else
    info "Tauri CLI will be installed when you run 'yarn install' (it's a devDependency)"
fi

# ── 7. yarn install ────────────────────────────────────────────────────────
step "7/7  Frontend Dependencies"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "$SCRIPT_DIR/package.json" ]]; then
    info "Running yarn install..."
    cd "$SCRIPT_DIR"
    yarn install
    ok "Frontend dependencies installed"
else
    warn "package.json not found in script directory. Please run 'yarn install' manually in the project root."
fi

# ── Summary ────────────────────────────────────────────────────────────────
echo ""
step "Setup Complete!"
echo ""
echo -e "  ${GREEN}Distro${NC}    ${PRETTY_NAME:-$DISTRO}"
echo -e "  ${GREEN}Rust${NC}      $(rustc --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}Cargo${NC}     $(cargo --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}Node.js${NC}   $(node --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}Yarn${NC}      $(yarn --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}OpenCV${NC}    $(pkg-config --modversion opencv4 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}CMake${NC}     $(cmake --version 2>/dev/null | head -1 || echo 'not found')"
echo -e "  ${GREEN}Protobuf${NC}  $(protoc --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}Clang${NC}     $(clang --version 2>/dev/null | head -1 || echo 'not found')"
echo ""
echo -e "  Run ${BOLD}yarn tauri dev${NC} to start the development server."
echo ""
