#!/usr/bin/env bash
# ==============================================================================
# AxyLauncher — macOS Release Binary & .app Bundle Builder
# Usage (run on macOS):
#   ./build_macos.sh                  # Build native macOS binary + AxyLauncher.app
#   ./build_macos.sh --universal      # Build Universal (Apple Silicon + Intel) binary + .app
#   ./build_macos.sh --offline        # Build offline using local Cargo cache
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Ошибка: Rust / Cargo не найден. Установите через https://rustup.rs" >&2
    exit 1
fi

OUT_DIR="${SCRIPT_DIR}/dist/macos"
APP_BUNDLE="${OUT_DIR}/AxyLauncher.app"
mkdir -p "${OUT_DIR}"

if [[ "${1:-}" == "--universal" ]]; then
    echo "==> Сборка Universal-бинарника для macOS (aarch64 + x86_64)..."
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    lipo -create \
        "target/aarch64-apple-darwin/release/axylauncher" \
        "target/x86_64-apple-darwin/release/axylauncher" \
        -output "${OUT_DIR}/axylauncher"
else
    EXTRA_ARGS=()
    if [[ "${1:-}" == "--offline" ]]; then
        EXTRA_ARGS+=(--offline)
    fi
    if [[ -n "${TARGET:-}" ]]; then
        EXTRA_ARGS+=(--target "${TARGET}")
        BIN_SRC="target/${TARGET}/release/axylauncher"
    else
        BIN_SRC="target/release/axylauncher"
    fi
    echo "==> Сборка AxyLauncher для macOS (${TARGET:-native})..."
    cargo build --release ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}
    cp -f "${BIN_SRC}" "${OUT_DIR}/axylauncher"
fi

chmod +x "${OUT_DIR}/axylauncher"

# Package into a native macOS .app bundle as well
rm -rf "${APP_BUNDLE}"
mkdir -p "${APP_BUNDLE}/Contents/MacOS"
mkdir -p "${APP_BUNDLE}/Contents/Resources"
cp -f "${OUT_DIR}/axylauncher" "${APP_BUNDLE}/Contents/MacOS/axylauncher"

cat > "${APP_BUNDLE}/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>AxyLauncher</string>
    <key>CFBundleDisplayName</key>
    <string>AxyLauncher</string>
    <key>CFBundleIdentifier</key>
    <string>org.axylauncher.AxyLauncher</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>axylauncher</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
</dict>
</plist>
EOF

echo "==> Готово! Бинарник и .app бандл для macOS:"
ls -lh "${OUT_DIR}/axylauncher"
echo "    App Bundle: ${APP_BUNDLE}"
