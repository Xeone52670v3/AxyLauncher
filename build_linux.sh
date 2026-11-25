#!/usr/bin/env bash
# ==============================================================================
# AxyLauncher — Linux Release Binary Builder
# Usage:
#   ./build_linux.sh                  # Build native Linux release binary
#   ./build_linux.sh --offline        # Build using cached Cargo crates offline
#   TARGET=aarch64-unknown-linux-gnu ./build_linux.sh
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Ошибка: Rust / Cargo не найден. Установите через https://rustup.rs" >&2
    exit 1
fi

CARGO_ARGS=(build --release)
if [[ "${1:-}" == "--offline" ]]; then
    CARGO_ARGS+=(--offline)
fi

if [[ -n "${TARGET:-}" ]]; then
    CARGO_ARGS+=(--target "${TARGET}")
    BIN_SRC="target/${TARGET}/release/axylauncher"
else
    BIN_SRC="target/release/axylauncher"
fi

echo "==> Сборка AxyLauncher для Linux (${TARGET:-native})..."
cargo "${CARGO_ARGS[@]}"

OUT_DIR="${SCRIPT_DIR}/dist/linux"
mkdir -p "${OUT_DIR}"
cp -f "${BIN_SRC}" "${OUT_DIR}/axylauncher"
chmod +x "${OUT_DIR}/axylauncher"

echo "==> Готово! Бинарник для Linux:"
ls -lh "${OUT_DIR}/axylauncher"
