**Language:** English | [Русский](README.ru.md)

# AxyLauncher

A Minecraft launcher written in Rust with a Slint UI styled after Material Design 3, inspired by FreesmLauncher.

## Features

- **Mod loaders and clients:** Vanilla, Fabric, Forge, NeoForge, LiteLoader, OptiFine, LabyMod, and Horizon.
- **Content catalog:** Search and install mods, modpacks, resource packs, shaders, data packs, and worlds from Modrinth, CurseForge, FTB, ATLauncher, Technic, and Prism.
- **Import and export:** Install `.mrpack` and generic archives, and export instances as Modrinth `.mrpack`, CurseForge zip, or Prism zip.
- **Accounts and skins:** Offline mode, Microsoft Device Flow, and Yggdrasil login, with `authlib-injector` and `loki` agent backends plus skin and cape import by username or URL.
- **Java and launch:** Automatic system Java detection, managed Java downloads, native library extraction, live game logs, and automatic clipboard copy for new screenshots.
- **Customization and extras:** Dynamic Material 3 color palettes from a custom accent color, built-in pixel-art and custom instance icons, an optional procedural cat overlay, Discord Rich Presence, Linux system tray support, and 9 UI languages (`ru`, `en`, `uk`, `es`, `fr`, `de`, `pl`, `zh`, `ja`).

## Building from source

Install the Rust toolchain from [rustup.rs](https://rustup.rs). On Windows, you also need Desktop C++ Build Tools.

Build and run directly with Cargo:

```bash
cargo run --release
```

Platform build scripts are included in the repository root:

- **Linux binary** (outputs to `dist/linux/axylauncher`, supports `--offline` and `TARGET=...`):
  ```bash
  ./build_linux.sh
  ```
- **Linux AppImage** (outputs `AxyLauncher-x86_64.AppImage`):
  ```bash
  ./build_appimage.sh
  ```
- **macOS binary and `.app` bundle** (outputs to `dist/macos/`, supports `--universal` and `--offline`):
  ```bash
  ./build_macos.sh
  ```
- **Windows executable** (outputs `dist\windows\AxyLauncher.exe`, supports `--offline`):
  ```bat
  build_windows.bat
  ```

## License

Licensed under [GPL-3.0-only](Cargo.toml).
