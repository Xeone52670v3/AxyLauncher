use crate::{MainWindow, UiIcons};
use anyhow::{Context, Result};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use std::path::Path;
use std::sync::OnceLock;

pub const UI_ICON_SLOTS: [&str; 22] = [
    "add",
    "instances",
    "folders",
    "settings",
    "console",
    "news",
    "account",
    "play",
    "offline",
    "edit",
    "export",
    "copy",
    "delete",
    "check",
    "search",
    "refresh",
    "stop",
    "java",
    "language",
    "proxy",
    "tools",
    "api",
];

fn pixel_art_for_slot(slot: &str) -> [&'static str; 16] {
    match slot {
        "add" => [
            "................",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            ".##############.",
            ".##############.",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            ".......##.......",
            "................",
        ],
        "instances" => [
            "................",
            ".####.####.####.",
            ".####.####.####.",
            ".####.####.####.",
            ".####.####.####.",
            "................",
            ".####.####.####.",
            ".####.####.####.",
            ".####.####.####.",
            ".####.####.####.",
            "................",
            ".####.####.####.",
            ".####.####.####.",
            ".####.####.####.",
            ".####.####.####.",
            "................",
        ],
        "folders" => [
            "................",
            "................",
            ".######.........",
            ".#######........",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            "................",
            "................",
        ],
        "settings" => [
            "......####......",
            "......####......",
            "..##..####..##..",
            ".####.####.####.",
            "..############..",
            "...###....###...",
            "###..#....#..###",
            "###..........###",
            "###..........###",
            "###..#....#..###",
            "...###....###...",
            "..############..",
            ".####.####.####.",
            "..##..####..##..",
            "......####......",
            "......####......",
        ],
        "console" => [
            "................",
            ".##############.",
            ".##..........##.",
            ".##.##.......##.",
            ".##..##......##.",
            ".##...##.....##.",
            ".##....##....##.",
            ".##...##.....##.",
            ".##..##......##.",
            ".##.##.#####.##.",
            ".##..........##.",
            ".##..........##.",
            ".##############.",
            "................",
            "................",
            "................",
        ],
        "news" => [
            "................",
            ".##############.",
            ".##..........##.",
            ".##.####.###.##.",
            ".##.####.....##.",
            ".##.####.###.##.",
            ".##..........##.",
            ".##.########.##.",
            ".##..........##.",
            ".##.########.##.",
            ".##..........##.",
            ".##.########.##.",
            ".##..........##.",
            ".##############.",
            "................",
            "................",
        ],
        "account" => [
            "................",
            ".....######.....",
            "....########....",
            "....########....",
            "....########....",
            ".....######.....",
            "................",
            "...##########...",
            "..############..",
            ".##############.",
            ".##############.",
            ".##############.",
            ".##############.",
            "................",
            "................",
            "................",
        ],
        "play" => [
            "................",
            "...##...........",
            "...####.........",
            "...######.......",
            "...########.....",
            "...##########...",
            "...############.",
            "...############.",
            "...##########...",
            "...########.....",
            "...######.......",
            "...####.........",
            "...##...........",
            "................",
            "................",
            "................",
        ],
        "offline" => [
            "................",
            "....########....",
            "..############..",
            ".####......####.",
            ".###...##...###.",
            ".##...####...##.",
            ".##..######..##.",
            ".##..######..##.",
            ".##...####...##.",
            ".###...##...###.",
            ".####......####.",
            "..############..",
            "....########....",
            "................",
            "................",
            "................",
        ],
        "edit" => [
            "................",
            "...........###..",
            "..........#####.",
            ".........#####..",
            "........#####...",
            ".......#####....",
            "......#####.....",
            ".....#####......",
            "....#####.......",
            "...#####........",
            "..#####.........",
            ".#####..........",
            ".####...........",
            ".###............",
            "................",
            "................",
        ],
        "export" => [
            "......####......",
            "......####......",
            "......####......",
            "......####......",
            "......####......",
            "...##########...",
            "....########....",
            ".....######.....",
            "......####......",
            ".......##.......",
            "................",
            ".##############.",
            ".##############.",
            "................",
            "................",
            "................",
        ],
        "copy" => [
            "................",
            ".....#########..",
            ".....##.....##..",
            "..#########.##..",
            "..##.....##.##..",
            "..##.....##.##..",
            "..##.....##.##..",
            "..##.....##.##..",
            "..##.....#####..",
            "..##.....##.....",
            "..##.....##.....",
            "..##.....##.....",
            "..#########.....",
            "................",
            "................",
            "................",
        ],
        "delete" => [
            "................",
            ".....######.....",
            "..############..",
            "..############..",
            "...##..##..##...",
            "...##..##..##...",
            "...##..##..##...",
            "...##..##..##...",
            "...##..##..##...",
            "...##..##..##...",
            "...##..##..##...",
            "...##..##..##...",
            "....########....",
            "................",
            "................",
            "................",
        ],
        "check" => [
            "................",
            "................",
            ".............##.",
            "............###.",
            "...........###..",
            "..........###...",
            ".........###....",
            ".##.....###.....",
            ".###...###......",
            "..#######.......",
            "...#####........",
            "....###.........",
            "................",
            "................",
            "................",
            "................",
        ],
        "search" => [
            "................",
            "...######.......",
            "..##....##......",
            ".##......##.....",
            ".##......##.....",
            ".##......##.....",
            "..##....##......",
            "...######.......",
            "........###.....",
            ".........###....",
            "..........###...",
            "...........###..",
            "............##..",
            "................",
            "................",
            "................",
        ],
        "refresh" => [
            "................",
            "....########....",
            "..####....####..",
            ".###........###.",
            ".##..........###",
            ".##.........####",
            ".##.............",
            "................",
            ".............##.",
            "####.........##.",
            ".###........###.",
            "..####....####..",
            "....########....",
            "................",
            "................",
            "................",
        ],
        "stop" => [
            "................",
            "................",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "..############..",
            "................",
            "................",
            "................",
        ],
        "java" => [
            "....##....##....",
            ".....##....##...",
            "....##....##....",
            "................",
            "..##########....",
            "..##########.##.",
            "..##########.##.",
            "..##########.##.",
            "..#############.",
            "..##########....",
            "...########.....",
            "................",
            ".##############.",
            "................",
            "................",
            "................",
        ],
        "language" => [
            "....########....",
            "..##...##...##..",
            ".##....##....##.",
            ".##############.",
            "##.....##.....##",
            "##.....##.....##",
            ".##############.",
            "##.....##.....##",
            "##.....##.....##",
            ".##############.",
            ".##....##....##.",
            "..##...##...##..",
            "....########....",
            "................",
            "................",
            "................",
        ],
        "proxy" => [
            "......####......",
            "....##....##....",
            "..##........##..",
            ".##..........##.",
            ".##############.",
            "##.....##.....##",
            "##.....##.....##",
            ".##############.",
            ".##..........##.",
            "..##........##..",
            "....##....##....",
            "......####......",
            "................",
            "................",
            "................",
            "................",
        ],
        "tools" => [
            "................",
            "..........####..",
            ".........##..##.",
            ".........##.....",
            "........####....",
            ".......####.....",
            "......####......",
            ".....####.......",
            "....####........",
            "...####.........",
            "..####..........",
            ".####...........",
            ".###............",
            "................",
            "................",
            "................",
        ],
        _ => [
            "................",
            "..######........",
            ".##....##.......",
            ".##.##.##.......",
            ".##....##.......",
            "..############..",
            ".........##..##.",
            ".........##..##.",
            ".........##.....",
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
        ],
    }
}

fn build_pixel_icon_rgba(slot: &str) -> Vec<u8> {
    let art = pixel_art_for_slot(slot);
    let mut bytes = vec![0u8; 16 * 16 * 4];
    for (y, row) in art.iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            let idx = (y * 16 + x) * 4;
            if ch == b'#' {
                bytes[idx] = 255;
                bytes[idx + 1] = 255;
                bytes[idx + 2] = 255;
                bytes[idx + 3] = 255;
            }
        }
    }
    bytes
}

fn cached_pixel_icon(idx: usize) -> Image {
    static PIXEL_CACHE: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    let all = PIXEL_CACHE.get_or_init(|| {
        UI_ICON_SLOTS
            .iter()
            .map(|slot| build_pixel_icon_rgba(slot))
            .collect()
    });
    let mut pb = SharedPixelBuffer::<Rgba8Pixel>::new(16, 16);
    pb.make_mut_bytes().copy_from_slice(&all[idx]);
    Image::from_rgba8(pb)
}

pub fn ensure_packs_readme(packs_dir: &Path) {
    let _ = std::fs::create_dir_all(packs_dir);
    let readme = packs_dir.join("README.txt");
    if !readme.exists() {
        let content = format!(
            "AxyLauncher UI Icon Packs\n\n\
            Create a subfolder inside this directory (for example: my_pack/)\n\
            and place PNG/SVG/WebP icon files named after UI slots:\n\n\
            {}\n\n\
            Any missing icon automatically falls back to the built-in Material icon.\n",
            UI_ICON_SLOTS
                .iter()
                .map(|s| format!("{}.png", s))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = std::fs::write(readme, content);
    }
}

pub fn list_icon_theme_options(packs_dir: &Path) -> Vec<String> {
    let mut opts = vec![
        "Material".to_string(),
        "Контурные".to_string(),
        "Пиксельные".to_string(),
    ];
    if let Ok(entries) = std::fs::read_dir(packs_dir) {
        let mut custom_names = Vec::new();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if !name.starts_with('.') {
                        custom_names.push(name.to_string());
                    }
                }
            }
        }
        custom_names.sort_by_key(|a| a.to_lowercase());
        opts.extend(custom_names);
    }
    opts
}

pub fn import_icon_pack_dir(src_dir: &Path, packs_dir: &Path) -> Result<String> {
    if !src_dir.is_dir() {
        anyhow::bail!("Выбранный путь не является папкой");
    }
    let raw_name = src_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("custom_pack")
        .trim();
    let pack_name = if raw_name.is_empty() {
        "custom_pack".to_string()
    } else {
        raw_name.to_string()
    };
    let dest_dir = packs_dir.join(&pack_name);
    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("Не удалось создать папку пака {}", dest_dir.display()))?;
    let mut copied = 0usize;
    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if matches!(ext.as_str(), "png" | "svg" | "webp" | "jpg" | "jpeg" | "gif") {
            if let Some(fname) = p.file_name() {
                let _ = std::fs::copy(&p, dest_dir.join(fname));
                copied += 1;
            }
        }
    }
    if copied == 0 {
        anyhow::bail!("В выбранной папке не найдено изображений иконок (PNG/SVG/WebP)");
    }
    Ok(pack_name)
}

fn load_slot_from_pack(pack_dir: &Path, slot: &str) -> Option<Image> {
    for ext in ["png", "svg", "webp", "jpg", "jpeg", "gif"] {
        let p = pack_dir.join(format!("{}.{}", slot, ext));
        if p.is_file() {
            if let Ok(img) = Image::load_from_path(&p) {
                return Some(img);
            }
        }
    }
    None
}

pub fn apply_ui_icon_theme(ui: &MainWindow, theme_name: &str, packs_dir: &Path) {
    let t = theme_name.trim();
    let lower = t.to_lowercase();
    let ui_icons = ui.global::<UiIcons>();

    if lower == "контурные" || lower == "outlined" {
        ui_icons.set_style_idx(1);
        clear_all_custom_imgs(&ui_icons);
    } else if lower == "пиксельные" || lower == "pixel" {
        ui_icons.set_style_idx(2);
        set_slot_imgs(&ui_icons, |idx| Some(cached_pixel_icon(idx)));
    } else if lower != "material" && !t.is_empty() && packs_dir.join(t).is_dir() {
        ui_icons.set_style_idx(3);
        let pack_dir = packs_dir.join(t);
        set_slot_imgs(&ui_icons, |idx| load_slot_from_pack(&pack_dir, UI_ICON_SLOTS[idx]));
    } else {
        ui_icons.set_style_idx(0);
        clear_all_custom_imgs(&ui_icons);
    }
}

fn clear_all_custom_imgs(ui_icons: &UiIcons<'_>) {
    set_slot_imgs(ui_icons, |_| None);
}

fn set_slot_imgs<F>(ui_icons: &UiIcons<'_>, mut loader: F)
where
    F: FnMut(usize) -> Option<Image>,
{
    let empty = Image::default();
    macro_rules! set_slot {
        ($idx:expr, $set_has:ident, $set_img:ident) => {
            if let Some(img) = loader($idx) {
                ui_icons.$set_has(true);
                ui_icons.$set_img(img);
            } else {
                ui_icons.$set_has(false);
                ui_icons.$set_img(empty.clone());
            }
        };
    }
    set_slot!(0, set_has_add, set_img_add);
    set_slot!(1, set_has_instances, set_img_instances);
    set_slot!(2, set_has_folders, set_img_folders);
    set_slot!(3, set_has_settings, set_img_settings);
    set_slot!(4, set_has_console, set_img_console);
    set_slot!(5, set_has_news, set_img_news);
    set_slot!(6, set_has_account, set_img_account);
    set_slot!(7, set_has_play, set_img_play);
    set_slot!(8, set_has_offline, set_img_offline);
    set_slot!(9, set_has_edit, set_img_edit);
    set_slot!(10, set_has_export, set_img_export);
    set_slot!(11, set_has_copy, set_img_copy);
    set_slot!(12, set_has_delete, set_img_delete);
    set_slot!(13, set_has_check, set_img_check);
    set_slot!(14, set_has_search, set_img_search);
    set_slot!(15, set_has_refresh, set_img_refresh);
    set_slot!(16, set_has_stop, set_img_stop);
    set_slot!(17, set_has_java, set_img_java);
    set_slot!(18, set_has_language, set_img_language);
    set_slot!(19, set_has_proxy, set_img_proxy);
    set_slot!(20, set_has_tools, set_img_tools);
    set_slot!(21, set_has_api, set_img_api);
}
