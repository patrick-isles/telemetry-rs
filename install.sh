#!/usr/bin/env bash
# ==============================================================================
# Script: install.sh
# Purpose: Build and install telemetry-rs into user environment
# Author: Patrick Isles (https://github.com/patrick-isles)
# License: MIT
# ==============================================================================

export PATH="${HOME}/.cargo/bin:${HOME}/.local/bin:${PATH}"

echo "================================================================="
echo "   Telemetry-RS - Rust Hardware DSP Engine Installation"
echo "================================================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"

mkdir -p "${BIN_DIR}"
mkdir -p "${SYSTEMD_USER_DIR}"

echo "[+] Compiling release binary with Link-Time Optimization (LTO)..."
cd "${SCRIPT_DIR}"
cargo build --release

echo "[+] Installing binary to ${BIN_DIR}/telemetry-rs..."
cp "${SCRIPT_DIR}/target/release/telemetry-rs" "${BIN_DIR}/telemetry-rs"
chmod +x "${BIN_DIR}/telemetry-rs"

echo "[+] Installing systemd user service..."
cp "${SCRIPT_DIR}/systemd/telemetry-rs.service" "${SYSTEMD_USER_DIR}/telemetry-rs.service"
systemctl --user daemon-reload || true

echo ""
echo "================================================================="
echo "   Installation Successful!"
echo "   Binary available: ${BIN_DIR}/telemetry-rs"
echo ""
echo "   Usage:"
echo "     telemetry-rs                  # Live interactive terminal oscilloscope"
echo "     telemetry-rs --once           # Snapshot current metrics to console"
echo "     telemetry-rs --bench          # Run zero-copy micro-benchmarks"
echo "     telemetry-rs --headless       # Run as silent Prometheus daemon (:9106)"
echo "================================================================="
