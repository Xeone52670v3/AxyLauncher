Язык: Русский | [English](README.md)

# AxyLauncher

Лаунчер для Minecraft на Rust с интерфейсом на Slint в стиле Material Design 3. Проект вырос из идеи сделать лёгкую альтернативу в духе FreesmLauncher без тяжёлых веб-движков.

## Что умеет лаунчер

Лаунчер запускает ванильный Minecraft и сборки с загрузчиками Fabric, Forge, NeoForge, LiteLoader, OptiFine, LabyMod и Horizon. Свободного места и времени на настройку уходит меньше, потому что системную Java программа находит сама, а если нужной версии нет - скачивает подходящую сборку под выбранную версию игры.

Встроенный каталог ищет моды, модпаки, ресурспаки, шейдеры, датапаки и миры на Modrinth, CurseForge, FTB, ATLauncher, Technic и Prism. Готовые архивы `.mrpack` можно импортировать напрямую, а свои сборки - выгрузить обратно в форматы Modrinth, CurseForge или Prism.

Для входа работают оффлайн-профили, Microsoft Device Flow и Yggdrasil. Если нужен сервер со своими скинами, подключаются агенты `authlib-injector` или `loki`, а сами скины и плащи можно подтянуть по нику или ссылке.

Из приятных мелочей - интерфейс сам перекрашивает палитру Material 3 под выбранный акцентный цвет, для сборок есть набор встроенных пиксельных иконок и поддержка своих картинок, а поверх окна можно посадить процедурно нарисованного котика. Скриншоты из игры автоматически копируются в буфер обмена, работает Discord Rich Presence, сворачивание в трей на Linux и девять языков интерфейса, включая русский, английский и украинский.

## Установка

Готовые сборки (`.exe`, `.msi`, `.AppImage`, `.deb`, `.rpm`, `.pacman`, `.flatpak`, `.dmg`) лежат на странице [GitHub Releases](https://github.com/Xeone52670v3/AxyLauncher/releases).

На Linux лаунчер можно поставить и обновлять через собственный Flatpak-репозиторий:

```bash
flatpak remote-add --user --if-not-exists axylauncher https://xeone52670v3.github.io/AxyLauncher/axylauncher.flatpakrepo
flatpak install --user -y axylauncher io.github.xeone52670v3.AxyLauncher
```

## Сборка из исходников

Для сборки понадобится установленный Rust с сайта [rustup.rs](https://rustup.rs). На Windows дополнительно нужны Desktop C++ Build Tools.

Быстрый запуск через Cargo:

```bash
cargo run --release
```

В корне репозитория лежат готовые скрипты под каждую платформу.

На Linux скрипт соберёт бинарник в `dist/linux/axylauncher` (поддерживает флаг `--offline` и переменную `TARGET`):

```bash
./build_linux.sh
```

Отдельный скрипт упаковывает проект в `AxyLauncher-x86_64.AppImage`:

```bash
./build_appimage.sh
```

На macOS сборка создаёт бинарник и пакет `AxyLauncher.app` в каталоге `dist/macos/` (есть флаги `--universal` и `--offline`):

```bash
./build_macos.sh
```

На Windows батник собирает `dist\windows\AxyLauncher.exe` (также понимает `--offline`):

```bat
build_windows.bat
```

## Лицензия

Проект распространяется под лицензией `GPL-3.0-only`, как указано в [Cargo.toml](Cargo.toml).
