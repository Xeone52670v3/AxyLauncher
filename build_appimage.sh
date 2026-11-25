#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPDIR="${PROJECT_DIR}/target/AppDir"
OUTPUT_APPIMAGE="${PROJECT_DIR}/AxyLauncher-x86_64.AppImage"
BINARY="${PROJECT_DIR}/target/release/axylauncher"

rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/lib"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"

cp "${BINARY}" "${APPDIR}/usr/bin/axylauncher"
chmod +x "${APPDIR}/usr/bin/axylauncher"

cat > "${APPDIR}/AppRun" << 'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "${0}")")"
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH:-}"
exec "${HERE}/usr/bin/axylauncher" "$@"
EOF
chmod +x "${APPDIR}/AppRun"

cat > "${APPDIR}/axylauncher.desktop" << 'EOF'
[Desktop Entry]
Type=Application
Name=AxyLauncher
GenericName=Minecraft Launcher (Freesm Rust Edition)
Comment=Material Design 3 Minecraft Launcher in Rust with Prism, LabyMod 4, OptiFine & Freesm Auth
Exec=axylauncher
Icon=axylauncher
Categories=Game;
Terminal=false
StartupWMClass=axylauncher
EOF
cp "${APPDIR}/axylauncher.desktop" "${APPDIR}/usr/share/applications/axylauncher.desktop"

cat > "${APPDIR}/axylauncher.svg" << 'EOF'
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <rect width="256" height="256" rx="56" fill="#1C448E"/>
  <rect x="36" y="36" width="184" height="184" rx="36" fill="#181B26" stroke="#A8C7FA" stroke-width="8"/>
  <path d="M68 84 h44 v44 h-44 z M144 84 h44 v44 h-44 z M106 128 h44 v36 h-44 z M84 148 h22 v40 h-22 z M150 148 h22 v40 h-22 z" fill="#A8C7FA"/>
</svg>
EOF
cp "${APPDIR}/axylauncher.svg" "${APPDIR}/usr/share/icons/hicolor/scalable/apps/axylauncher.svg"
ln -sf axylauncher.svg "${APPDIR}/.DirIcon"

RUNTIME_FILE="${PROJECT_DIR}/target/runtime-x86_64"
if [ -f "/tmp/s/laby.AppImage" ]; then
    OFFSET=$(/tmp/s/laby.AppImage --appimage-offset)
    head -c "${OFFSET}" /tmp/s/laby.AppImage > "${RUNTIME_FILE}"
    chmod +x "${RUNTIME_FILE}"
fi

SQUASHFS_FILE="${PROJECT_DIR}/target/axylauncher.squashfs"
rm -f "${SQUASHFS_FILE}"
mksquashfs "${APPDIR}" "${SQUASHFS_FILE}" -root-owned -noappend -no-xattrs -comp gzip >/dev/null

rm -f "${OUTPUT_APPIMAGE}"
cat "${RUNTIME_FILE}" "${SQUASHFS_FILE}" > "${OUTPUT_APPIMAGE}"
chmod +x "${OUTPUT_APPIMAGE}"
rm -f "${SQUASHFS_FILE}"

echo "AppImage built successfully: ${OUTPUT_APPIMAGE}"
ls -lh "${OUTPUT_APPIMAGE}"
