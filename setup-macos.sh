#!/usr/bin/env bash
#
# setup-macos.sh — One-shot dev environment setup for AuraSeek on macOS
#
# Usage:  chmod +x setup-macos.sh && ./setup-macos.sh
#
# What it does:
#   1. Installs Xcode Command Line Tools (if missing)
#   2. Installs Homebrew (if missing)
#   3. Installs system dependencies: OpenCV 4, pkg-config, cmake, protobuf, llvm
#   4. Installs Rust via rustup (if missing), updates to stable
#   5. Installs Node.js 22 via nvm (if missing, respects existing installations)
#   6. Installs Yarn (if missing)
#   7. Installs Tauri CLI system dependencies
#   8. Runs yarn install
#   9. Validates the full tool chain
#
set -euo pipefail

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[✓]${NC}     $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
fail()  { echo -e "${RED}[✗]${NC}     $*"; exit 1; }
step()  { echo -e "\n${BOLD}${CYAN}━━━ $* ━━━${NC}"; }

REQUIRED_NODE_MAJOR=22

# ── 0. macOS check ──────────────────────────────────────────────────────────
if [[ "$(uname)" != "Darwin" ]]; then
    fail "This script is for macOS only. Use setup-linux.sh instead."
fi

# ── 1. Xcode Command Line Tools ────────────────────────────────────────────
step "1/8  Xcode Command Line Tools"
if xcode-select -p &>/dev/null; then
    ok "Xcode CLT already installed at $(xcode-select -p)"
else
    info "Installing Xcode Command Line Tools..."
    xcode-select --install 2>/dev/null || true
    # Wait for installation to complete
    until xcode-select -p &>/dev/null; do
        sleep 5
    done
    ok "Xcode CLT installed"
fi

# ── 2. Homebrew ─────────────────────────────────────────────────────────────
step "2/8  Homebrew"
if command -v brew &>/dev/null; then
    ok "Homebrew already installed: $(brew --version | head -1)"
else
    info "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    # Add brew to PATH for Apple Silicon
    if [[ -f /opt/homebrew/bin/brew ]]; then
        eval "$(/opt/homebrew/bin/brew shellenv)"
    fi
    ok "Homebrew installed"
fi

# ── 3. System Dependencies (Homebrew) ──────────────────────────────────────
step "3/8  System Dependencies"

BREW_PACKAGES=(
    "opencv"        # OpenCV 4.x — required by opencv Rust crate
    "pkg-config"    # Needed for Rust build scripts to find OpenCV and other C libs
    "cmake"         # Build tool often required by native Rust dependencies
    "protobuf"      # gRPC/protobuf — needed by tonic/prost (Qdrant client)
    "llvm"          # Clang/LLVM — needed for bindgen (OpenCV, ort FFI)
)

for pkg in "${BREW_PACKAGES[@]}"; do
    if brew list "$pkg" &>/dev/null; then
        ok "$pkg already installed"
    else
        info "Installing $pkg..."
        brew install "$pkg"
        ok "$pkg installed"
    fi
done

# Ensure LLVM is accessible for bindgen (clang-sys)
LLVM_PREFIX="$(brew --prefix llvm 2>/dev/null || echo "")"
if [[ -n "$LLVM_PREFIX" ]]; then
    export LIBCLANG_PATH="$LLVM_PREFIX/lib"
    info "LIBCLANG_PATH=$LIBCLANG_PATH"
fi

# ── 4. Rust ─────────────────────────────────────────────────────────────────
step "4/8  Rust Toolchain"
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

# Ensure cargo is on PATH for this session
export PATH="$HOME/.cargo/bin:$PATH"

# ── 5. Node.js ──────────────────────────────────────────────────────────────
step "5/8  Node.js"

# Helper: check if current Node.js version satisfies the requirement
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
        # Install nvm first
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

# ── 6. Yarn ─────────────────────────────────────────────────────────────────
step "6/8  Yarn"
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

# ── 7. Tauri CLI Dependencies ──────────────────────────────────────────────
step "7/8  Tauri CLI"
if npx tauri --version &>/dev/null 2>&1; then
    ok "Tauri CLI available: $(npx tauri --version 2>/dev/null)"
else
    info "Tauri CLI will be installed when you run 'yarn install' (it's a devDependency)"
fi

# ── 8. yarn install ────────────────────────────────────────────────────────
step "8/8  Frontend Dependencies"
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
echo -e "  ${GREEN}Rust${NC}      $(rustc --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}Cargo${NC}     $(cargo --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}Node.js${NC}   $(node --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}Yarn${NC}      $(yarn --version 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}OpenCV${NC}    $(pkg-config --modversion opencv4 2>/dev/null || echo 'not found')"
echo -e "  ${GREEN}CMake${NC}     $(cmake --version 2>/dev/null | head -1 || echo 'not found')"
echo -e "  ${GREEN}Protobuf${NC}  $(protoc --version 2>/dev/null || echo 'not found')"
echo ""

# Check if LIBCLANG_PATH needs to be exported permanently
if [[ -n "${LLVM_PREFIX:-}" ]]; then
    echo -e "  ${YELLOW}NOTE:${NC} Add the following to your shell profile (~/.zshrc or ~/.bashrc):"
    echo ""
    echo -e "    export LIBCLANG_PATH=\"$LLVM_PREFIX/lib\""
    echo ""
fi

echo -e "  Run ${BOLD}yarn tauri dev${NC} to start the development server."
echo ""
