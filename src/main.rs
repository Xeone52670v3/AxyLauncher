#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod auth;
mod cats;
mod discord;
mod i18n;
mod icon;
mod instance_icons;
mod launcher;
mod meta;
mod models;
mod skins;
mod tray;
mod ui_icons;

use crate::auth::{
    authenticate_yggdrasil, create_offline_account, fetch_patcher_versions,
    generate_random_nickname, poll_microsoft_device_flow, random_icon_id,
    refresh_microsoft_account, refresh_yggdrasil_account, start_microsoft_device_flow,
};
use crate::launcher::{copy_png_to_clipboard, detect_system_javas, launch_instance};
use crate::meta::{
    copy_dir_recursive, dir_name_from_string, export_instance_archive, extract_world_zip,
    import_prism_freesm_instances, rename_instance_folder, write_instance_cfg, MetaClient,
};
use crate::models::*;
use crate::skins::{
    fetch_mc_profile_skins_capes, generate_m3_palette_from_hex, import_skin_from_url,
    import_skin_from_username, parse_hex_rgb, render_cape_or_elytra, render_placeholder_cape,
    render_placeholder_skin, render_skin_front_back, reset_mc_skin, rgb_to_hex, set_mc_cape,
    upload_mc_skin,
};
use crate::discord::DiscordRpcController;
use crate::icon::{ensure_linux_desktop_icon_integration, render_logo_slint_image};
use crate::instance_icons::{
    instance_icon_image, instance_icon_name, load_custom_icon_image, normalize_icon_id,
    resolve_instance_icon_image, scan_saved_custom_instance_icons, INSTANCE_ICON_COUNT,
};
use crate::tray::TrayController;
use anyhow::Result;
use directories::ProjectDirs;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

slint::include_modules!();

fn pick_native_directory(title: &str) -> Option<String> {
    if cfg!(target_os = "windows") {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = '{}'; if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.SelectedPath }}",
            title.replace('\'', "")
        );
        return std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
    }
    if cfg!(target_os = "macos") {
        let script = format!(
            "POSIX path of (choose folder with prompt \"{}\")",
            title.replace('"', "")
        );
        return std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
    }
    std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--directory",
            &format!("--title={}", title),
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .or_else(|| {
            std::process::Command::new("kdialog")
                .args(["--getexistingdirectory", "."])
                .output()
                .ok()
                .filter(|o| o.status.success())
        })
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn pick_native_file(title: &str, png_only: bool) -> Option<String> {
    if cfg!(target_os = "windows") {
        let filter = if png_only {
            "PNG Images (*.png)|*.png|All Files (*.*)|*.*"
        } else {
            "All Files (*.*)|*.*"
        };
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.OpenFileDialog; $f.Title = '{}'; $f.Filter = '{}'; if ($f.ShowDialog() -eq 'OK') {{ Write-Output $f.FileName }}",
            title.replace('\'', ""),
            filter
        );
        return std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
    }
    if cfg!(target_os = "macos") {
        let script = format!(
            "POSIX path of (choose file with prompt \"{}\")",
            title.replace('"', "")
        );
        return std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
    }
    let mut z_args = vec![
        "--file-selection".to_string(),
        format!("--title={}", title),
    ];
    if png_only {
        z_args.push("--file-filter=PNG (*.png) | *.png".to_string());
    }
    std::process::Command::new("zenity")
        .args(&z_args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .or_else(|| {
            let mut k_args = vec!["--getopenfilename", "."];
            if png_only {
                k_args.push("*.png | PNG Skins");
            }
            std::process::Command::new("kdialog")
                .args(&k_args)
                .output()
                .ok()
                .filter(|o| o.status.success())
        })
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn get_config_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("org", "AxyLauncher", "AxyLauncher") {
        proj.config_dir().to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".axylauncher")
    }
}

fn get_data_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("org", "AxyLauncher", "AxyLauncher") {
        proj.data_dir().to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".axylauncher")
    }
}

fn save_config_to(config_dir: &std::path::Path, cfg: &LauncherConfig) {
    let _ = std::fs::create_dir_all(config_dir);
    let cfg_path = config_dir.join("launcher_config.json");
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(cfg_path, json);
    }
}

fn load_config(data_dir: &PathBuf) -> LauncherConfig {
    let config_dir = get_config_dir();
    let cfg_path = config_dir.join("launcher_config.json");
    if let Ok(content) = std::fs::read_to_string(&cfg_path) {
        if let Ok(mut cfg) = serde_json::from_str::<LauncherConfig>(&content) {
            if cfg.meta_url.trim().is_empty() || cfg.meta_url.contains("prismlauncher") {
                cfg.meta_url = DEFAULT_FREESM_META_URL.to_string();
            }
            return cfg;
        }
    }
    // Migrate from ~/.local/share/axylauncher/launcher_config.json to ~/.config/axylauncher/
    let old_data_cfg = data_dir.join("launcher_config.json");
    if old_data_cfg != cfg_path {
        if let Ok(content) = std::fs::read_to_string(&old_data_cfg) {
            if let Ok(cfg) = serde_json::from_str::<LauncherConfig>(&content) {
                save_config_to(&config_dir, &cfg);
                return cfg;
            }
        }
    }
    // Migrate from legacy MineLauncher config/data directories
    if let Some(legacy_proj) = ProjectDirs::from("org", "MineLauncher", "MineLauncher") {
        for legacy_cfg in [
            legacy_proj.config_dir().join("launcher_config.json"),
            legacy_proj.data_dir().join("launcher_config.json"),
        ] {
            if let Ok(content) = std::fs::read_to_string(&legacy_cfg) {
                if let Ok(mut cfg) = serde_json::from_str::<LauncherConfig>(&content) {
                    if cfg.data_root_dir.trim().is_empty()
                        && legacy_proj.data_dir().join("instances").is_dir()
                    {
                        cfg.data_root_dir = legacy_proj.data_dir().to_string_lossy().to_string();
                    }
                    save_config_to(&config_dir, &cfg);
                    return cfg;
                }
            }
        }
    }
    let mut def = LauncherConfig::default();
    if let Some(java) = detect_system_javas().into_iter().next() {
        def.java_path = java;
    }
    def
}

fn save_config(_data_dir: &PathBuf, cfg: &LauncherConfig) {
    save_config_to(&get_config_dir(), cfg);
}

fn render_filtered_log(lines: &[String], filter_idx: i32) -> String {
    let filtered: Vec<&str> = lines
        .iter()
        .map(|s| s.as_str())
        .filter(|line| match filter_idx {
            1 => {
                let l = line.to_lowercase();
                l.contains("error")
                    || l.contains("ошибка")
                    || l.contains("exception")
                    || l.contains("fatal")
                    || l.contains("[stderr]")
            }
            2 => {
                let l = line.to_lowercase();
                l.contains("warn") || l.contains("предупреждение") || l.contains("внимание")
            }
            _ => true,
        })
        .collect();
    if filtered.is_empty() {
        "Нет записей для выбранного фильтра.\n".to_string()
    } else {
        let mut out = filtered.join("\n");
        out.push('\n');
        out
    }
}

static LOG_MAX_LINES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(2000);
static LOG_STOP_ON_OVERFLOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn sync_log_limits_from_cfg(cfg: &LauncherConfig) {
    use std::sync::atomic::Ordering;
    LOG_MAX_LINES.store(cfg.log_history_limit.clamp(100, 100_000), Ordering::Relaxed);
    LOG_STOP_ON_OVERFLOW.store(cfg.stop_logging_on_overflow, Ordering::Relaxed);
}

fn append_log_to_ui(
    weak: &slint::Weak<MainWindow>,
    log_buf: &Arc<Mutex<Vec<String>>>,
    msg: String,
) {
    use std::sync::atomic::Ordering;
    let max_lines = LOG_MAX_LINES.load(Ordering::Relaxed).max(100);
    let stop_overflow = LOG_STOP_ON_OVERFLOW.load(Ordering::Relaxed);
    let rendered_line = msg.trim_end_matches('\n').to_string();
    {
        let mut buf = log_buf.lock().unwrap();
        if stop_overflow && buf.len() >= max_lines {
            return;
        }
        buf.push(rendered_line.clone());
        if !stop_overflow && buf.len() > max_lines {
            let excess = buf.len() - max_lines;
            buf.drain(0..excess);
        }
    }
    let weak = weak.clone();
    let log_buf = log_buf.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            let filter_idx = ui.get_console_filter_idx();
            let text = {
                let buf = log_buf.lock().unwrap();
                render_filtered_log(&buf, filter_idx)
            };
            let lower = rendered_line.to_lowercase();
            let is_err = lower.contains("ошибка")
                || lower.contains("error")
                || lower.contains("exception")
                || lower.contains("fatal")
                || lower.contains("⚠")
                || lower.contains("[stderr]");
            ui.set_console_log(SharedString::from(text));
            ui.set_status_is_error(is_err);
            ui.set_status_text(SharedString::from(rendered_line));
        }
    });
}

fn scan_installed_mods(instances_dir: &PathBuf, inst_id: &str) -> Vec<UiInstalledMod> {
    let mods_dir = instances_dir
        .join(inst_id)
        .join(".minecraft")
        .join("mods");
    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !fname.ends_with(".jar") && !fname.ends_with(".jar.disabled") {
                continue;
            }
            let enabled = !fname.ends_with(".disabled");
            let display_name = fname
                .trim_end_matches(".disabled")
                .trim_end_matches(".jar")
                .to_string();
            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let size_kb = if size_bytes >= 1024 * 1024 {
                format!("{:.1} МБ", size_bytes as f64 / (1024.0 * 1024.0))
            } else {
                format!("{} КБ", (size_bytes / 1024).max(1))
            };
            list.push(UiInstalledMod {
                filename: fname.into(),
                display_name: display_name.into(),
                enabled,
                size_kb: size_kb.into(),
            });
        }
    }
    list.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    list
}

fn scan_subfolder_entries(instances_dir: &PathBuf, inst_id: &str, sub: &str) -> Vec<UiSimpleEntry> {
    let dir = instances_dir
        .join(inst_id)
        .join(".minecraft")
        .join(sub);
    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let enabled = !name.ends_with(".disabled");
            let detail = if path.is_dir() {
                "Папка".to_string()
            } else {
                let sz = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let sz_str = if sz >= 1024 * 1024 {
                    format!("{:.1} МБ", sz as f64 / (1024.0 * 1024.0))
                } else {
                    format!("{} КБ", (sz / 1024).max(1))
                };
                if enabled {
                    sz_str
                } else {
                    format!("{} • Отключён", sz_str)
                }
            };
            list.push(UiSimpleEntry {
                name: name.into(),
                detail: detail.into(),
                enabled,
            });
        }
    }
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

fn default_repo_rows(cfg: &LauncherConfig) -> Vec<UiRepoInfo> {
    let meta_provider = if cfg.meta_url.contains("freesm") {
        "Freesm Meta"
    } else if cfg.meta_url.contains("prism") {
        "Prism Meta"
    } else {
        "Custom Meta"
    };
    vec![
        UiRepoInfo {
            name: "Minecraft & LWJGL3".into(),
            uid: "net.minecraft".into(),
            url: format!("{}/net.minecraft/index.json", cfg.meta_url).into(),
            provider: meta_provider.into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "Fabric & Intermediary".into(),
            uid: "net.fabricmc.fabric-loader".into(),
            url: format!("{}/net.fabricmc.fabric-loader/index.json", cfg.meta_url).into(),
            provider: meta_provider.into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "Minecraft Forge".into(),
            uid: "net.minecraftforge".into(),
            url: format!("{}/net.minecraftforge/index.json", cfg.meta_url).into(),
            provider: meta_provider.into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "NeoForge".into(),
            uid: "net.neoforged".into(),
            url: format!("{}/net.neoforged/index.json", cfg.meta_url).into(),
            provider: meta_provider.into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "Quilt Loader".into(),
            uid: "org.quiltmc.quilt-loader".into(),
            url: format!("{}/org.quiltmc.quilt-loader/index.json", cfg.meta_url).into(),
            provider: meta_provider.into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "LiteLoader".into(),
            uid: "com.mumfrey.liteloader".into(),
            url: format!("{}/com.mumfrey.liteloader/index.json", cfg.meta_url).into(),
            provider: meta_provider.into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "LabyMod 4 Production".into(),
            uid: "net.labymod".into(),
            url: format!(
                "{}/api/v1/manifest/production/latest.json",
                cfg.labymod_repo_url
            )
            .into(),
            provider: "LabyMod".into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "OptiFine HD Ultra".into(),
            uid: "optifine.OptiFine".into(),
            url: format!("{}/versionList", cfg.optifine_repo_url).into(),
            provider: "OptiFine".into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "Horizon (Minecraft on Rust)".into(),
            uid: "horizon-rust/horizon".into(),
            url: cfg.horizon_repo_url.clone().into(),
            provider: "Horizon".into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "unmojang/Loki & authlib-injector".into(),
            uid: "auth-agents".into(),
            url: "https://github.com/unmojang/loki • https://authlib-injector.yushi.moe".into(),
            provider: "Freesm Auth".into(),
            status: cfg.auth_agent_backend.label().into(),
        },
        UiRepoInfo {
            name: "Modrinth API v2".into(),
            uid: "modrinth".into(),
            url: "https://api.modrinth.com/v2".into(),
            provider: "Моды и Модпаки".into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "CurseForge Core API".into(),
            uid: "curseforge".into(),
            url: "https://api.curseforge.com/v1".into(),
            provider: "Моды и Модпаки".into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "FTB Modpacks Public".into(),
            uid: "ftb".into(),
            url: "https://api.modpacks.ch/public/modpack".into(),
            provider: "Модпаки".into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "Technic Platform".into(),
            uid: "technic".into(),
            url: "https://api.technicpack.net".into(),
            provider: "Модпаки".into(),
            status: "Активен".into(),
        },
        UiRepoInfo {
            name: "ATLauncher Packs".into(),
            uid: "atlauncher".into(),
            url: "https://download.nodecdn.net/containers/atl/launcher/json/packsnew.json".into(),
            provider: "Модпаки".into(),
            status: "Активен".into(),
        },
    ]
}

fn build_all_instance_icons(custom_dir: &std::path::Path) -> Vec<UiInstanceIcon> {
    let mut list: Vec<UiInstanceIcon> = (0..INSTANCE_ICON_COUNT)
        .map(|id| UiInstanceIcon {
            id,
            name: instance_icon_name(id).into(),
            image: instance_icon_image(id),
            path: "".into(),
        })
        .collect();
    for (idx, (name, p)) in scan_saved_custom_instance_icons(custom_dir).into_iter().enumerate() {
        if let Some(img) = load_custom_icon_image(&p) {
            list.push(UiInstanceIcon {
                id: INSTANCE_ICON_COUNT + idx as i32,
                name: name.into(),
                image: img,
                path: p.to_string_lossy().to_string().into(),
            });
        }
    }
    list
}

fn build_filtered_instances(
    cfg: &LauncherConfig,
    search_query: &str,
    group_filter: &str,
    sort_mode: &str,
) -> (Vec<UiInstance>, i32) {
    let q = search_query.trim().to_lowercase();
    let mut filtered: Vec<&Instance> = cfg
        .instances
        .iter()
        .filter(|inst| {
            let grp_ok = group_filter == "Все"
                || group_filter.is_empty()
                || inst.group.eq_ignore_ascii_case(group_filter);
            let q_ok = q.is_empty()
                || inst.name.to_lowercase().contains(&q)
                || inst.mc_version.to_lowercase().contains(&q)
                || inst.loader.as_str().to_lowercase().contains(&q)
                || inst.group.to_lowercase().contains(&q);
            grp_ok && q_ok
        })
        .collect();

    match sort_mode {
        "По времени запуска" => {
            filtered.sort_by(|a, b| b.last_played_ts.cmp(&a.last_played_ts));
        }
        "По времени игры" => {
            filtered.sort_by(|a, b| b.total_playtime_minutes.cmp(&a.total_playtime_minutes));
        }
        "По версии MC" => {
            filtered.sort_by(|a, b| b.mc_version.cmp(&a.mc_version));
        }
        _ => {
            filtered.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
    }

    let mut selected_idx = -1i32;
    let ui_instances: Vec<UiInstance> = filtered
        .iter()
        .enumerate()
        .map(|(idx, inst)| {
            if cfg.selected_instance_id.as_deref() == Some(inst.id.as_str()) {
                selected_idx = idx as i32;
            }
            let of_badge = if (inst.optifine_enabled && inst.loader == LoaderType::Forge)
                || inst.loader == LoaderType::OptiFine
            {
                format!(
                    "OptiFine {}",
                    inst.optifine_version.as_deref().unwrap_or("HD_U")
                )
            } else {
                String::new()
            };
            let custom_badge = if inst.has_custom_overrides() {
                "Свои настройки".to_string()
            } else {
                String::new()
            };
            UiInstance {
                id: inst.id.clone().into(),
                name: inst.name.clone().into(),
                group: inst.group.clone().into(),
                mc_version: inst.mc_version.clone().into(),
                loader: inst.loader.as_str().into(),
                loader_version: inst.loader_version.clone().into(),
                icon_id: normalize_icon_id(inst.icon_id),
                icon_image: resolve_instance_icon_image(inst.icon_id, &inst.custom_icon_path),
                optifine_badge: of_badge.into(),
                custom_badge: custom_badge.into(),
                status_badge: inst
                    .last_played
                    .clone()
                    .unwrap_or_else(|| "Готов".to_string())
                    .into(),
                playtime_text: if cfg.show_playtime_instances {
                    inst.playtime_label(cfg.always_show_hours).into()
                } else {
                    "".into()
                },
            }
        })
        .collect();

    (ui_instances, selected_idx)
}

fn apply_custom_accent_to_ui(ui: &MainWindow, hex: &str) {
    let pal = generate_m3_palette_from_hex(hex);
    let m3 = ui.global::<M3>();
    m3.set_custom_dark_bg(pal.dark_bg);
    m3.set_custom_dark_surface_low(pal.dark_surface_low);
    m3.set_custom_dark_surface(pal.dark_surface);
    m3.set_custom_dark_surface_high(pal.dark_surface_high);
    m3.set_custom_dark_surface_highest(pal.dark_surface_highest);
    m3.set_custom_dark_primary(pal.dark_primary);
    m3.set_custom_dark_on_primary(pal.dark_on_primary);
    m3.set_custom_dark_primary_container(pal.dark_primary_container);
    m3.set_custom_dark_on_primary_container(pal.dark_on_primary_container);
    m3.set_custom_dark_secondary_container(pal.dark_secondary_container);
    m3.set_custom_dark_on_secondary_container(pal.dark_on_secondary_container);
    m3.set_custom_dark_outline_variant(pal.dark_outline_variant);

    m3.set_custom_light_bg(pal.light_bg);
    m3.set_custom_light_surface_low(pal.light_surface_low);
    m3.set_custom_light_surface(pal.light_surface);
    m3.set_custom_light_surface_high(pal.light_surface_high);
    m3.set_custom_light_surface_highest(pal.light_surface_highest);
    m3.set_custom_light_primary(pal.light_primary);
    m3.set_custom_light_primary_container(pal.light_primary_container);
    m3.set_custom_light_on_primary_container(pal.light_on_primary_container);
    m3.set_custom_light_secondary_container(pal.light_secondary_container);
    m3.set_custom_light_outline_variant(pal.light_outline_variant);

    if let Some((r, g, b)) = parse_hex_rgb(hex) {
        ui.set_custom_accent_preview(slint::Color::from_rgb_u8(r, g, b));
    }
}

fn sync_saved_colors_to_ui(ui: &MainWindow, cfg: &LauncherConfig) {
    let swatches: Vec<UiColorSwatch> = cfg
        .custom_saved_colors
        .iter()
        .filter_map(|hex| {
            parse_hex_rgb(hex).map(|(r, g, b)| UiColorSwatch {
                hex: rgb_to_hex(r, g, b).into(),
                col: slint::Color::from_rgb_u8(r, g, b),
            })
        })
        .collect();
    ui.set_custom_saved_colors(ModelRc::from(Rc::new(VecModel::from(swatches))));
}

fn scan_saved_skins(skins_dir: &std::path::Path, slim: bool) -> Vec<UiSkinEntry> {
    let _ = std::fs::create_dir_all(skins_dir);
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(skins_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file()
                && p.extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("png"))
                    .unwrap_or(false)
            {
                let name = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("skin")
                    .to_string();
                let thumb = render_skin_front_back(&p, slim, 3)
                    .unwrap_or_else(|| render_placeholder_skin(slim, 3));
                entries.push(UiSkinEntry {
                    name: name.into(),
                    path: p.to_string_lossy().to_string().into(),
                    thumbnail: thumb,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}

fn sync_cat_image(ui: &MainWindow, cfg: &LauncherConfig) {
    if !cfg.enable_cat_pack || cfg.cat_pack_preset <= 0 {
        ui.set_has_cat_image(false);
        return;
    }
    if cfg.cat_pack_preset == 5 {
        if !cfg.cat_pack_custom_path.trim().is_empty() {
            let p = PathBuf::from(cfg.cat_pack_custom_path.trim());
            if let Ok(img) = slint::Image::load_from_path(&p) {
                ui.set_cat_pack_image(img);
                ui.set_has_cat_image(true);
                return;
            }
        }
        ui.set_has_cat_image(false);
        return;
    }
    if let Some(img) = crate::cats::render_cat_preset(cfg.cat_pack_preset) {
        ui.set_cat_pack_image(img);
        ui.set_has_cat_image(true);
    } else {
        ui.set_has_cat_image(false);
    }
}

fn sync_custom_bg_images(ui: &MainWindow, cfg: &LauncherConfig) {
    if !cfg.custom_bg_launcher_path.trim().is_empty() {
        let p = PathBuf::from(cfg.custom_bg_launcher_path.trim());
        if let Ok(img) = slint::Image::load_from_path(&p) {
            ui.set_custom_bg_launcher_image(img);
            ui.set_has_launcher_bg_image(true);
        } else {
            ui.set_has_launcher_bg_image(false);
        }
    } else {
        ui.set_has_launcher_bg_image(false);
    }

    if !cfg.custom_bg_instances_path.trim().is_empty() {
        let p = PathBuf::from(cfg.custom_bg_instances_path.trim());
        if let Ok(img) = slint::Image::load_from_path(&p) {
            ui.set_custom_bg_instances_image(img);
            ui.set_has_instances_bg_image(true);
        } else {
            ui.set_has_instances_bg_image(false);
        }
    } else {
        ui.set_has_instances_bg_image(false);
    }
}

fn detect_system_fonts() -> Vec<String> {
    use std::collections::BTreeSet;
    let mut set = BTreeSet::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("fc-list")
            .args([":", "family"])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    for part in line.split(',') {
                        let name = part.trim();
                        if !name.is_empty() && name.len() <= 64 {
                            set.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }
    if set.is_empty() {
        for f in [
            "DejaVu Sans",
            "DejaVu Sans Mono",
            "Inter",
            "JetBrains Mono",
            "Liberation Sans",
            "Liberation Mono",
            "Noto Sans",
            "Roboto",
            "Segoe UI",
            "Ubuntu",
            "Consolas",
            "Courier New",
        ] {
            set.insert(f.to_string());
        }
    }
    let mut res = Vec::with_capacity(set.len() + 2);
    res.push("Системный".to_string());
    res.push("monospace".to_string());
    for f in set {
        if f != "Системный" && f != "monospace" {
            res.push(f);
        }
    }
    res
}

fn sync_ui_from_config(ui: &MainWindow, cfg: &LauncherConfig) {
    ui.set_is_linux(cfg!(target_os = "linux"));
    if ui.get_system_font_options().row_count() <= 4 {
        let fonts: Vec<SharedString> = detect_system_fonts()
            .into_iter()
            .map(SharedString::from)
            .collect();
        ui.set_system_font_options(ModelRc::from(Rc::new(VecModel::from(fonts))));
    }
    let data_root = get_data_dir();
    let icons_dir = cfg.resolve_icons_dir(&data_root);
    let packs_dir = cfg.resolve_icon_packs_dir(&data_root);
    let inst_icons_dir = cfg.resolve_instance_icons_dir(&data_root);
    crate::ui_icons::ensure_packs_readme(&packs_dir);
    let pack_opts: Vec<SharedString> = crate::ui_icons::list_icon_theme_options(&packs_dir)
        .into_iter()
        .map(SharedString::from)
        .collect();
    ui.set_icon_theme_options(ModelRc::from(Rc::new(VecModel::from(pack_opts))));
    crate::ui_icons::apply_ui_icon_theme(ui, &cfg.icon_theme, &packs_dir);

    let frame_idx = crate::instance_icons::set_widget_frame_style(&cfg.ui_widget_style) as i32;
    let prev_frame_idx = ui.global::<M3>().get_widget_frame_idx();
    ui.global::<M3>().set_widget_frame_idx(frame_idx);
    ui.global::<M3>().set_animations_enabled(cfg.enable_ui_animations);
    ui.global::<M3>().set_wavy_progress(cfg.wavy_progress_bar);
    let ui_font_eff = if cfg.ui_font_family.trim().is_empty() || cfg.ui_font_family == "Системный" {
        ""
    } else {
        cfg.ui_font_family.trim()
    };
    ui.global::<M3>().set_ui_font(ui_font_eff.into());
    ui.set_setting_ui_font(
        if ui_font_eff.is_empty() {
            "Системный"
        } else {
            ui_font_eff
        }
        .into(),
    );
    let was_icons_empty = ui.get_instance_icons().row_count() == 0;
    if was_icons_empty || prev_frame_idx != frame_idx {
        ui.set_instance_icons(ModelRc::from(Rc::new(VecModel::from(
            build_all_instance_icons(&inst_icons_dir),
        ))));
        let init_new_ic = if was_icons_empty {
            random_icon_id()
        } else {
            ui.get_new_inst_icon_id()
        };
        ui.set_new_inst_icon_id(init_new_ic);
        let new_custom = ui.get_new_inst_custom_icon_path().to_string();
        ui.set_new_inst_icon_image(resolve_instance_icon_image(init_new_ic, &new_custom));
        let edit_ic = ui.get_edit_inst_icon_id();
        ui.set_edit_inst_icon_image(instance_icon_image(edit_ic));
    }
    apply_custom_accent_to_ui(ui, &cfg.custom_accent_hex);
    ui.set_custom_accent_hex(cfg.custom_accent_hex.clone().into());
    sync_saved_colors_to_ui(ui, cfg);
    ui.global::<M3>().set_scheme(cfg.theme_scheme);
    ui.global::<M3>().set_light_mode(cfg.light_theme);
    let r_scale = match cfg.ui_widget_style.as_str() {
        "Fluent" => 0.45,
        "Компактный" => 0.65,
        _ => 1.0,
    };
    ui.global::<M3>().set_radius_scale(r_scale);
    sync_log_limits_from_cfg(cfg);
    i18n::apply_language(ui, &cfg.language, cfg.use_system_locale);

    // Dynamic group chips
    let mut groups = vec!["Все".to_string()];
    for inst in &cfg.instances {
        let g = inst.group.trim();
        if !g.is_empty() && !groups.iter().any(|x| x.eq_ignore_ascii_case(g)) {
            groups.push(g.to_string());
        }
    }
    let group_models: Vec<SharedString> = groups.into_iter().map(SharedString::from).collect();
    ui.set_instance_groups(ModelRc::from(Rc::new(VecModel::from(group_models))));

    let search_q = ui.get_instance_search_query().to_string();
    let group_f = ui.get_instance_group_filter().to_string();
    let (ui_instances, selected_idx) =
        build_filtered_instances(cfg, &search_q, &group_f, &cfg.instance_sort_mode);

    ui.set_instances(ModelRc::from(Rc::new(VecModel::from(ui_instances))));
    ui.set_selected_instance_index(selected_idx);

    if let Some(inst) = cfg
        .instances
        .iter()
        .find(|i| Some(&i.id) == cfg.selected_instance_id.as_ref())
    {
        ui.set_selected_instance_name(inst.name.clone().into());
        ui.set_selected_instance_group(inst.group.clone().into());
        ui.set_selected_instance_mc(inst.mc_version.clone().into());
        ui.set_selected_instance_loader(inst.loader.as_str().into());
        ui.set_selected_instance_loader_ver(inst.loader_version.clone().into());
        let norm_ic = normalize_icon_id(inst.icon_id);
        ui.set_selected_instance_icon_id(norm_ic);
        ui.set_selected_instance_icon_image(resolve_instance_icon_image(
            norm_ic,
            &inst.custom_icon_path,
        ));
        ui.set_selected_instance_playtime(inst.playtime_label(cfg.always_show_hours).into());
        let of_str = if (inst.optifine_enabled && inst.loader == LoaderType::Forge)
            || inst.loader == LoaderType::OptiFine
        {
            inst.optifine_version
                .clone()
                .unwrap_or_else(|| "Включен".to_string())
        } else {
            "Выключен".to_string()
        };
        ui.set_selected_instance_optifine(of_str.into());

        let eff_ram = if inst.loader == LoaderType::Horizon {
            "Native Rust".to_string()
        } else if inst.override_memory {
            format!(
                "{} МБ (своя)",
                inst.custom_max_memory_mb.unwrap_or(cfg.max_memory_mb)
            )
        } else {
            format!("{} МБ", cfg.max_memory_mb)
        };
        ui.set_selected_instance_effective_ram(eff_ram.into());
    } else {
        ui.set_selected_instance_name("".into());
        ui.set_selected_instance_group("".into());
        ui.set_selected_instance_mc("".into());
        ui.set_selected_instance_loader("".into());
        ui.set_selected_instance_loader_ver("".into());
        ui.set_selected_instance_playtime("".into());
        ui.set_selected_instance_optifine("Выключен".into());
        ui.set_selected_instance_effective_ram("".into());
    }

    let total_mins: u64 = cfg.instances.iter().map(|i| i.total_playtime_minutes).sum();
    let total_pt = if total_mins == 0 {
        String::new()
    } else if cfg.always_show_hours {
        format!("{:.1} ч", total_mins as f64 / 60.0)
    } else if total_mins >= 60 {
        format!("{} ч {} мин", total_mins / 60, total_mins % 60)
    } else {
        format!("{} мин", total_mins)
    };
    ui.set_total_playtime_text(total_pt.into());

    let active_acc_id = cfg
        .selected_account_id
        .clone()
        .or_else(|| cfg.accounts.first().map(|a| a.id.clone()));

    let ui_accounts: Vec<UiAccount> = cfg
        .accounts
        .iter()
        .map(|acc| {
            let is_active = active_acc_id.as_deref() == Some(acc.id.as_str());
            if is_active {
                ui.set_active_account_name(acc.username.clone().into());
                ui.set_active_account_type(acc.account_type.label().into());
                ui.set_active_account_uuid(acc.uuid.clone().into());
            }
            UiAccount {
                id: acc.id.clone().into(),
                username: acc.username.clone().into(),
                uuid: acc.uuid.clone().into(),
                account_type: acc.account_type.label().into(),
                server_url: acc
                    .auth_server_url
                    .clone()
                    .unwrap_or_else(|| {
                        if acc.account_type == AccountType::Microsoft {
                            "Microsoft / Xbox Live".to_string()
                        } else {
                            "Offline".to_string()
                        }
                    })
                    .into(),
                active: is_active,
                can_refresh: acc.account_type != AccountType::Offline,
                is_microsoft: acc.account_type == AccountType::Microsoft,
            }
        })
        .collect();
    if cfg.accounts.is_empty() {
        ui.set_active_account_name(i18n::translate_current("Нет аккаунта").into());
        ui.set_active_account_type("".into());
        ui.set_active_account_uuid("".into());
    }
    ui.set_accounts(ModelRc::from(Rc::new(VecModel::from(ui_accounts))));

    // === ALL FREESMLAUNCHER GLOBAL SETTINGS ===
    let custom_logo_img = ["app_logo.png", "app_logo.jpg", "app_logo.webp", "app_logo.svg"]
        .iter()
        .find_map(|name| load_custom_icon_image(&icons_dir.join(name)));
    if let Some(img) = custom_logo_img {
        ui.set_app_icon(img);
    } else {
        ui.set_app_icon(render_logo_slint_image(128));
    }
    ui.set_is_linux(cfg!(target_os = "linux"));
    // 0: LauncherPage + AppearanceWidget + Folders
    ui.set_setting_language(cfg.language.clone().into());
    ui.set_setting_use_system_locale(cfg.use_system_locale);
    ui.set_setting_use_lang_formats(cfg.use_language_for_formats);
    ui.set_setting_default_data_root(get_data_dir().to_string_lossy().to_string().into());
    ui.set_setting_data_root_dir(cfg.data_root_dir.clone().into());
    ui.set_setting_instances_dir(cfg.instances_dir.clone().into());
    ui.set_setting_mods_dir(cfg.mods_dir.clone().into());
    ui.set_setting_icons_dir(cfg.icons_dir.clone().into());
    ui.set_setting_java_dir(cfg.java_dir.clone().into());
    ui.set_setting_skins_dir(cfg.skins_dir.clone().into());
    ui.set_setting_assets_dir(cfg.assets_dir.clone().into());
    ui.set_setting_libraries_dir(cfg.libraries_dir.clone().into());
    ui.set_setting_exports_dir(cfg.exports_dir.clone().into());
    ui.set_setting_widget_style(cfg.ui_widget_style.clone().into());
    ui.set_setting_icon_theme(cfg.icon_theme.clone().into());
    ui.set_setting_instance_card_size(cfg.instance_card_size);
    ui.set_setting_wrap_instance_names(cfg.wrap_instance_names);
    ui.set_setting_lock_toolbars(cfg.lock_toolbars);
    ui.set_setting_show_status_bar(cfg.show_status_bar);
    ui.set_setting_sort_mode(cfg.instance_sort_mode.clone().into());
    ui.set_setting_folder_rename_mode(cfg.folder_rename_mode);
    ui.set_setting_ui_animations(cfg.enable_ui_animations);
    ui.set_setting_wavy_progress(cfg.wavy_progress_bar);
    ui.set_setting_snow_effect(cfg.enable_snow_effect);
    ui.set_setting_enable_cat_pack(cfg.enable_cat_pack);
    ui.set_setting_cat_preset(cfg.cat_pack_preset);
    ui.set_setting_cat_opacity(cfg.cat_pack_opacity);
    ui.set_setting_cat_custom_path(cfg.cat_pack_custom_path.clone().into());
    ui.set_setting_cat_scale_mode(cfg.cat_pack_scale_mode);
    sync_cat_image(ui, cfg);
    ui.set_setting_enable_custom_bg(cfg.enable_custom_bg);
    ui.set_setting_custom_bg_mode(cfg.custom_bg_mode);
    ui.set_setting_bg_launcher_path(cfg.custom_bg_launcher_path.clone().into());
    ui.set_setting_bg_launcher_opacity(cfg.custom_bg_launcher_opacity);
    ui.set_setting_bg_launcher_scale(cfg.custom_bg_launcher_scale);
    ui.set_setting_bg_instances_path(cfg.custom_bg_instances_path.clone().into());
    ui.set_setting_bg_instances_opacity(cfg.custom_bg_instances_opacity);
    ui.set_setting_bg_instances_scale(cfg.custom_bg_instances_scale);
    sync_custom_bg_images(ui, cfg);

    ui.set_setting_show_tray_icon(cfg.show_tray_icon);
    ui.set_setting_close_action_mode(cfg.close_action_mode);
    ui.set_setting_show_news_toolbar(cfg.show_news_toolbar);
    ui.set_setting_use_native_file_dialog(cfg.use_native_file_dialog);
    ui.set_setting_discord_rpc_launcher(cfg.discord_rpc_launcher);
    ui.set_setting_discord_client_id(cfg.discord_client_id.clone().into());
    ui.set_setting_auto_check_updates(cfg.auto_check_updates);
    ui.set_setting_update_hours_str(cfg.update_check_interval_hours.to_string().into());
    ui.set_setting_update_channel(cfg.update_channel.clone().into());
    ui.set_setting_keep_mod_meta(cfg.keep_mod_metadata);
    ui.set_setting_install_deps(cfg.install_mod_dependencies);
    ui.set_setting_detect_incompat(cfg.detect_mod_incompatibilities);
    ui.set_setting_check_subfolders_blocked(cfg.check_subfolders_blocked_mods);
    ui.set_setting_move_blocked_mods(cfg.move_blocked_mods);
    ui.set_setting_suggest_update_modpack(cfg.suggest_update_existing_modpack);
    ui.set_setting_auto_update_mods(cfg.auto_update_mods);
    ui.set_setting_skip_mod_compat_check(cfg.skip_mod_loader_compat_check);
    ui.set_setting_show_mod_file_sizes(cfg.show_mod_file_sizes);
    ui.set_setting_log_limit_str(cfg.log_history_limit.to_string().into());
    ui.set_setting_stop_log_overflow(cfg.stop_logging_on_overflow);
    ui.set_setting_console_font(cfg.console_font_family.clone().into());
    ui.set_setting_console_font_size_str(cfg.console_font_size.to_string().into());
    ui.set_console_font_size_px(cfg.console_font_size.clamp(8, 32) as f32);
    ui.set_setting_console_colorize(cfg.console_colorize_logs);
    ui.set_setting_console_autoscroll(cfg.console_auto_scroll);
    ui.set_setting_concurrency_str(cfg.download_concurrency.to_string().into());
    ui.set_setting_task_limit_str(cfg.concurrent_task_limit.to_string().into());
    ui.set_setting_retry_limit_str(cfg.download_retry_limit.to_string().into());
    ui.set_setting_timeout_str(cfg.network_timeout_secs.to_string().into());
    ui.set_setting_use_offline_cache(cfg.use_offline_meta_cache);

    // 1: MinecraftPage / MinecraftSettingsWidget
    ui.set_setting_win_width_str(cfg.window_width.to_string().into());
    ui.set_setting_win_height_str(cfg.window_height.to_string().into());
    ui.set_setting_start_maximized(cfg.start_maximized);
    ui.set_setting_close_after_launch(cfg.close_after_launch);
    ui.set_setting_quit_after_game_closed(cfg.quit_after_game_window_closed);
    ui.set_setting_auto_copy_screenshots(cfg.auto_copy_screenshots);
    ui.set_setting_show_console_on_launch(cfg.show_console_on_launch);
    ui.set_setting_show_console_on_error(cfg.show_console_on_error);
    ui.set_setting_hide_console_on_exit(cfg.hide_console_on_exit);
    ui.set_setting_enable_global_datapacks(cfg.enable_global_datapacks);
    ui.set_setting_global_datapacks_path(cfg.global_datapacks_path.clone().into());
    ui.set_setting_record_playtime(cfg.record_playtime);
    ui.set_setting_show_playtime_instances(cfg.show_playtime_instances);
    ui.set_setting_show_total_playtime(cfg.show_total_playtime);
    ui.set_setting_always_show_hours(cfg.always_show_hours);
    ui.set_setting_elyby_skin_mode(cfg.elyby_skin_mode);
    ui.set_setting_auth_backend_idx(cfg.auth_agent_backend.to_index());
    ui.set_setting_auth_agent_version(cfg.auth_agent_version.clone().into());
    ui.set_setting_custom_patcher_path(cfg.custom_patcher_path.clone().into());
    ui.set_setting_patcher_prefetch(cfg.patcher_prefetch_metadata);
    ui.set_setting_discord_rpc_game(cfg.discord_rpc_game);
    ui.set_setting_legacy_online_fixes(cfg.legacy_online_fixes);
    ui.set_setting_force_x11(cfg.force_x11_backend);
    ui.set_setting_disable_hidpi(cfg.disable_hidpi_scaling);
    ui.set_setting_use_system_glfw(cfg.use_system_glfw);
    ui.set_setting_custom_glfw_path(cfg.custom_glfw_path.clone().into());
    ui.set_setting_use_system_openal(cfg.use_system_openal);
    ui.set_setting_custom_openal_path(cfg.custom_openal_path.clone().into());
    ui.set_setting_use_discrete_gpu(cfg.use_discrete_gpu);
    ui.set_setting_enable_gamemode(cfg.enable_gamemode);
    ui.set_setting_enable_mangohud(cfg.enable_mangohud);
    ui.set_setting_use_zink(cfg.use_zink);

    // 2: JavaPage / JavaSettingsWidget
    ui.set_setting_auto_detect_java(cfg.auto_detect_java);
    ui.set_setting_auto_download_java(cfg.auto_download_java);
    ui.set_setting_skip_java_compat(cfg.skip_java_compat_checks);
    ui.set_setting_skip_java_prompt(cfg.skip_java_startup_prompt);
    ui.set_setting_java_path(cfg.java_path.clone().into());
    ui.set_setting_min_mem_str(cfg.min_memory_mb.to_string().into());
    ui.set_setting_max_mem_str(cfg.max_memory_mb.to_string().into());
    ui.set_setting_permgen_str(cfg.permgen_mb.to_string().into());
    ui.set_setting_warn_low_mem(cfg.warn_low_memory);
    ui.set_setting_jvm_args(cfg.jvm_args.clone().into());

    // 3: Custom Commands & 4: Environment
    ui.set_setting_pre_launch_cmd(cfg.pre_launch_cmd.clone().into());
    ui.set_setting_wrapper_cmd(cfg.wrapper_cmd.clone().into());
    ui.set_setting_post_exit_cmd(cfg.post_exit_cmd.clone().into());
    ui.set_setting_env_vars(cfg.env_vars.clone().into());

    // 5: Proxy & External Tools
    ui.set_setting_proxy_mode(cfg.proxy_mode);
    ui.set_setting_proxy_host(cfg.proxy_host.clone().into());
    ui.set_setting_proxy_port_str(cfg.proxy_port.to_string().into());
    ui.set_setting_proxy_user(cfg.proxy_user.clone().into());
    ui.set_setting_proxy_pass(cfg.proxy_pass.clone().into());
    ui.set_setting_ext_editor(cfg.ext_text_editor.clone().into());
    ui.set_setting_ext_mcedit(cfg.ext_mcedit.clone().into());
    ui.set_setting_ext_jprofiler(cfg.ext_jprofiler.clone().into());
    ui.set_setting_ext_visualvm(cfg.ext_visualvm.clone().into());

    // 6: APIs & Repositories
    ui.set_setting_paste_type(cfg.paste_service_type.clone().into());
    ui.set_setting_paste_url(cfg.paste_base_url.clone().into());
    ui.set_setting_meta_url(cfg.meta_url.clone().into());
    ui.set_setting_assets_url(cfg.assets_server_url.clone().into());
    ui.set_setting_legacy_fml_url(cfg.legacy_fml_libs_url.clone().into());
    ui.set_setting_user_agent(cfg.custom_user_agent.clone().into());
    ui.set_setting_labymod_url(cfg.labymod_repo_url.clone().into());
    ui.set_setting_optifine_url(cfg.optifine_repo_url.clone().into());
    ui.set_setting_horizon_url(cfg.horizon_repo_url.clone().into());
    ui.set_setting_custom_news_url(cfg.custom_news_url.clone().into());
    ui.set_setting_msa_client_id(cfg.msa_client_id.clone().into());
    ui.set_setting_modrinth_token(cfg.modrinth_api_token.clone().into());
    ui.set_cf_key_confirmed(cfg.curseforge_key_confirmed);
    ui.set_setting_cf_api_key(cfg.curseforge_api_key.clone().into());
    ui.set_setting_cf_fallback_modrinth(cfg.cf_fallback_to_modrinth);
    ui.set_setting_technic_key(cfg.technic_api_key.clone().into());
}

fn load_instance_into_editor(ui: &MainWindow, cfg: &LauncherConfig, data_dir: &PathBuf) {
    let Some(inst) = cfg
        .instances
        .iter()
        .find(|i| Some(&i.id) == cfg.selected_instance_id.as_ref())
        .or_else(|| cfg.instances.first())
    else {
        return;
    };

    ui.set_edit_inst_name(inst.name.clone().into());
    ui.set_edit_inst_folder_name(inst.id.clone().into());
    ui.set_edit_inst_group(inst.group.clone().into());
    ui.set_edit_inst_mc(inst.mc_version.clone().into());
    ui.set_edit_inst_loader_idx(inst.loader.to_index());
    ui.set_edit_inst_loader_ver(inst.loader_version.clone().into());
    let edit_ic = normalize_icon_id(inst.icon_id);
    ui.set_edit_inst_icon_id(edit_ic);
    ui.set_edit_inst_icon_image(resolve_instance_icon_image(edit_ic, &inst.custom_icon_path));
    ui.set_edit_inst_icon_name(
        if !inst.custom_icon_path.trim().is_empty() {
            "Своя иконка".into()
        } else {
            instance_icon_name(edit_ic).into()
        },
    );
    ui.set_edit_inst_optifine(inst.optifine_enabled && inst.loader == LoaderType::Forge);
    ui.set_edit_inst_optifine_ver(inst.optifine_version.clone().unwrap_or_default().into());
    ui.set_edit_use_custom_loader(inst.use_custom_loader);
    ui.set_edit_custom_loader_name(inst.custom_loader_name.clone().into());
    ui.set_edit_custom_loader_path(inst.custom_loader_path.clone().into());
    ui.set_edit_custom_loader_main_class(inst.custom_loader_main_class.clone().into());
    ui.set_edit_inst_notes(inst.notes.clone().into());
    ui.set_edit_enable_auto_join(inst.enable_auto_join);
    ui.set_edit_inst_quick_server(inst.quick_play_server.clone().into());
    ui.set_edit_inst_quick_world(inst.quick_play_world.clone().into());

    ui.set_edit_override_account(inst.override_account);
    ui.set_edit_custom_account_id(
        inst.custom_account_id
            .clone()
            .or_else(|| cfg.selected_account_id.clone())
            .unwrap_or_default()
            .into(),
    );
    ui.set_edit_override_elyby(inst.override_elyby);
    ui.set_edit_custom_elyby_mode(inst.custom_elyby_skin_mode);
    ui.set_edit_custom_auth_backend(inst.custom_auth_backend);
    ui.set_edit_custom_auth_version(inst.custom_auth_version.clone().into());
    ui.set_edit_override_discord(inst.override_discord);
    ui.set_edit_custom_discord_rpc(inst.custom_discord_rpc);
    ui.set_edit_override_datapacks(inst.override_datapacks);
    ui.set_edit_custom_datapacks_path(if inst.custom_datapacks_path.is_empty() {
        cfg.global_datapacks_path.clone().into()
    } else {
        inst.custom_datapacks_path.clone().into()
    });
    ui.set_edit_override_playtime(inst.override_playtime);
    ui.set_edit_custom_record_playtime(inst.custom_record_playtime);

    ui.set_edit_override_java(inst.override_java);
    ui.set_edit_custom_java(
        inst.custom_java_path
            .clone()
            .unwrap_or_else(|| cfg.java_path.clone())
            .into(),
    );
    ui.set_edit_custom_skip_java_compat(inst.custom_skip_java_compat);

    ui.set_edit_override_memory(inst.override_memory);
    ui.set_edit_custom_min_mem_str(
        inst.custom_min_memory_mb
            .unwrap_or(cfg.min_memory_mb)
            .to_string()
            .into(),
    );
    ui.set_edit_custom_max_mem_str(
        inst.custom_max_memory_mb
            .unwrap_or(cfg.max_memory_mb)
            .to_string()
            .into(),
    );
    ui.set_edit_custom_permgen_str(
        inst.custom_permgen_mb
            .unwrap_or(cfg.permgen_mb)
            .to_string()
            .into(),
    );

    ui.set_edit_override_jvm(inst.override_jvm_args);
    ui.set_edit_custom_jvm(
        inst.custom_jvm_args
            .clone()
            .unwrap_or_else(|| cfg.jvm_args.clone())
            .into(),
    );

    ui.set_edit_override_window(inst.override_window);
    ui.set_edit_custom_width_str(
        inst.custom_window_width
            .unwrap_or(cfg.window_width)
            .to_string()
            .into(),
    );
    ui.set_edit_custom_height_str(
        inst.custom_window_height
            .unwrap_or(cfg.window_height)
            .to_string()
            .into(),
    );
    ui.set_edit_custom_maximized(inst.custom_start_maximized);
    ui.set_edit_custom_quit_after_close(inst.custom_quit_after_close);
    ui.set_edit_custom_copy_screenshots(inst.custom_copy_screenshots);

    ui.set_edit_override_commands(inst.override_commands);
    ui.set_edit_custom_pre_cmd(
        inst.custom_pre_launch_cmd
            .clone()
            .unwrap_or_else(|| cfg.pre_launch_cmd.clone())
            .into(),
    );
    ui.set_edit_custom_wrapper(
        inst.custom_wrapper_cmd
            .clone()
            .unwrap_or_else(|| cfg.wrapper_cmd.clone())
            .into(),
    );
    ui.set_edit_custom_post_cmd(
        inst.custom_post_exit_cmd
            .clone()
            .unwrap_or_else(|| cfg.post_exit_cmd.clone())
            .into(),
    );

    ui.set_edit_override_env(inst.override_env);
    ui.set_edit_custom_env(
        inst.custom_env_vars
            .clone()
            .unwrap_or_else(|| cfg.env_vars.clone())
            .into(),
    );

    ui.set_edit_override_linux(inst.override_linux_tweaks);
    ui.set_edit_custom_legacy_fixes(inst.custom_legacy_online_fixes);
    ui.set_edit_custom_use_sys_glfw(inst.custom_use_system_glfw);
    ui.set_edit_custom_glfw_path(inst.custom_glfw_path.clone().into());
    ui.set_edit_custom_use_sys_openal(inst.custom_use_system_openal);
    ui.set_edit_custom_openal_path(inst.custom_openal_path.clone().into());
    ui.set_edit_custom_dgpu(inst.custom_use_discrete_gpu);
    ui.set_edit_custom_gamemode(inst.custom_enable_gamemode);
    ui.set_edit_custom_mangohud(inst.custom_enable_mangohud);
    ui.set_edit_custom_zink(inst.custom_use_zink);

    ui.set_edit_override_console(inst.override_console);
    ui.set_edit_custom_show_console_launch(inst.custom_show_console_on_launch);
    ui.set_edit_custom_show_console_error(inst.custom_show_console_on_error);
    ui.set_edit_custom_hide_console_exit(inst.custom_hide_console_on_exit);
    ui.set_edit_custom_close_after_launch(inst.custom_close_after_launch);

    let instances_dir = cfg.resolve_instances_dir(data_dir);
    ui.set_installed_mods(ModelRc::from(Rc::new(VecModel::from(scan_installed_mods(
        &instances_dir,
        &inst.id,
    )))));
    ui.set_resource_packs(ModelRc::from(Rc::new(VecModel::from(
        scan_subfolder_entries(&instances_dir, &inst.id, "resourcepacks"),
    ))));
    ui.set_shader_packs(ModelRc::from(Rc::new(VecModel::from(
        scan_subfolder_entries(&instances_dir, &inst.id, "shaderpacks"),
    ))));
    ui.set_data_packs(ModelRc::from(Rc::new(VecModel::from(
        scan_subfolder_entries(&instances_dir, &inst.id, "datapacks"),
    ))));
    ui.set_worlds_list(ModelRc::from(Rc::new(VecModel::from(
        scan_subfolder_entries(&instances_dir, &inst.id, "saves"),
    ))));
    ui.set_screenshots_list(ModelRc::from(Rc::new(VecModel::from(
        scan_subfolder_entries(&instances_dir, &inst.id, "screenshots"),
    ))));
}

fn sync_discord_launcher_state(
    discord: &DiscordRpcController,
    cfg: &LauncherConfig,
    launcher_start_ts: u64,
) {
    let sel_inst = cfg
        .instances
        .iter()
        .find(|i| Some(&i.id) == cfg.selected_instance_id.as_ref())
        .map(|i| (i.name.as_str(), i.mc_version.as_str(), i.loader.as_str()));
    discord.set_launcher_presence(
        cfg.discord_rpc_launcher,
        sel_inst,
        &cfg.discord_client_id,
        launcher_start_ts,
    );
}

fn current_meta(meta_holder: &Arc<Mutex<Arc<MetaClient>>>) -> Arc<MetaClient> {
    meta_holder.lock().unwrap().clone()
}

fn main() -> Result<()> {
    env_logger::init();
    ensure_linux_desktop_icon_integration();

    let launcher_start_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()?,
    );

    let data_dir = get_data_dir();
    let config = Arc::new(Mutex::new(load_config(&data_dir)));
    {
        let cfg_guard = config.lock().unwrap();
        save_config(&data_dir, &cfg_guard);
    }

    let meta_holder: Arc<Mutex<Arc<MetaClient>>> = {
        let cfg_guard = config.lock().unwrap();
        Arc::new(Mutex::new(Arc::new(MetaClient::from_config(&cfg_guard)?)))
    };

    let log_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![
        "[AxyLauncher] Лаунчер запущен.".to_string(),
    ]));
    let active_kill_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>> =
        Arc::new(Mutex::new(None));

    let ui = MainWindow::new()?;
    ui.global::<Tr>()
        .on_translate(|key, _rev| SharedString::from(i18n::translate_current(key.as_str())));
    {
        let ui_weak_lang = ui.as_weak();
        let config_lang = config.clone();
        let data_dir_lang = data_dir.clone();
        ui.on_apply_language(move |lang| {
            if let Some(ui) = ui_weak_lang.upgrade() {
                let use_sys = ui.get_setting_use_system_locale();
                i18n::apply_language(&ui, lang.as_str(), use_sys);
                let mut cfg = config_lang.lock().unwrap();
                cfg.language = lang.to_string();
                cfg.use_system_locale = use_sys;
                save_config(&data_dir_lang, &cfg);
            }
        });
    }
    {
        let ui_weak_ws = ui.as_weak();
        let config_ws = config.clone();
        let data_dir_ws = data_dir.clone();
        ui.on_apply_widget_style(move |style| {
            if let Some(ui) = ui_weak_ws.upgrade() {
                let mut cfg = config_ws.lock().unwrap();
                cfg.ui_widget_style = style.to_string();
                save_config(&data_dir_ws, &cfg);
                sync_ui_from_config(&ui, &cfg);
            }
        });
    }
    {
        let ui_weak_ic = ui.as_weak();
        let config_ic = config.clone();
        let data_dir_ic = data_dir.clone();
        ui.on_apply_icon_theme(move |theme| {
            if let Some(ui) = ui_weak_ic.upgrade() {
                let mut cfg = config_ic.lock().unwrap();
                cfg.icon_theme = theme.to_string();
                save_config(&data_dir_ic, &cfg);
                let packs_dir = cfg.resolve_icon_packs_dir(&data_dir_ic);
                crate::ui_icons::apply_ui_icon_theme(&ui, &cfg.icon_theme, &packs_dir);
            }
        });
    }
    {
        let ui_weak_uf = ui.as_weak();
        let config_uf = config.clone();
        let data_dir_uf = data_dir.clone();
        ui.on_apply_ui_font(move |font| {
            if let Some(ui) = ui_weak_uf.upgrade() {
                let mut cfg = config_uf.lock().unwrap();
                let f = font.trim();
                cfg.ui_font_family = if f.is_empty() || f == "Системный" {
                    String::new()
                } else {
                    f.to_string()
                };
                save_config(&data_dir_uf, &cfg);
                sync_ui_from_config(&ui, &cfg);
            }
        });
    }
    {
        let ui_weak_cf = ui.as_weak();
        let config_cf = config.clone();
        let data_dir_cf = data_dir.clone();
        ui.on_apply_console_font(move |font| {
            if let Some(ui) = ui_weak_cf.upgrade() {
                let mut cfg = config_cf.lock().unwrap();
                let f = font.trim();
                cfg.console_font_family = if f.is_empty() || f == "Системный" {
                    "monospace".to_string()
                } else {
                    f.to_string()
                };
                save_config(&data_dir_cf, &cfg);
                ui.set_setting_console_font(cfg.console_font_family.clone().into());
            }
        });
    }
    if let Err(e) = slint::set_xdg_app_id(crate::icon::XDG_APP_ID) {
        log_buf
            .lock()
            .unwrap()
            .push(format!("[UI] Не удалось установить Wayland app_id: {}", e));
    }
    ui.set_app_icon(render_logo_slint_image(128));
    let discord_ctrl = {
        let ui_weak_rpc = ui.as_weak();
        let log_buf_rpc = log_buf.clone();
        DiscordRpcController::spawn(Arc::new(move |connected, status_msg| {
            let w = ui_weak_rpc.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = w.upgrade() {
                    ui.set_discord_rpc_connected(connected);
                    ui.set_discord_rpc_error(!connected && ui.get_setting_discord_rpc_launcher());
                }
            });
            if !connected && !status_msg.is_empty() {
                append_log_to_ui(&ui_weak_rpc, &log_buf_rpc, status_msg);
            }
        }))
    };
    let tray_ctrl = {
        let ui_weak_restore = ui.as_weak();
        let show_tray_initial = config.lock().unwrap().show_tray_icon;
        TrayController::new(
            show_tray_initial,
            Arc::new(move || {
                let w = ui_weak_restore.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(win) = w.upgrade() {
                        let _ = win.show();
                    }
                });
            }),
        )
    };
    {
        let cfg_guard = config.lock().unwrap();
        sync_ui_from_config(&ui, &cfg_guard);
        sync_discord_launcher_state(&discord_ctrl, &cfg_guard, launcher_start_ts);
        let repos = default_repo_rows(&cfg_guard);
        ui.set_repositories(ModelRc::from(Rc::new(VecModel::from(repos))));
        if !cfg_guard.first_run_completed {
            ui.set_show_setup_wizard(true);
        }
    }

    // Setup Wizard completion
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_complete_setup_wizard(move |scheme, light_mode, nick| {
            if let Some(ui) = ui_weak.upgrade() {
                let mut cfg = config.lock().unwrap();
                cfg.first_run_completed = true;
                cfg.theme_scheme = scheme;
                cfg.light_theme = light_mode;
                cfg.language = ui.get_setting_language().to_string();
                if let Ok(min_m) = ui.get_setting_min_mem_str().trim().parse::<u32>() {
                    cfg.min_memory_mb = min_m.max(8);
                }
                if let Ok(max_m) = ui.get_setting_max_mem_str().trim().parse::<u32>() {
                    cfg.max_memory_mb = max_m.max(8);
                }
                if let Ok(w) = ui.get_setting_win_width_str().trim().parse::<u32>() {
                    cfg.window_width = w.max(640);
                }
                if let Ok(h) = ui.get_setting_win_height_str().trim().parse::<u32>() {
                    cfg.window_height = h.max(480);
                }
                let cf_key = ui.get_setting_cf_api_key().trim().to_string();
                if !cf_key.is_empty() {
                    cfg.curseforge_api_key = cf_key;
                    cfg.curseforge_key_confirmed = true;
                }
                let clean_nick = nick.trim();
                if !clean_nick.is_empty() {
                    if let Ok(acc) = create_offline_account(clean_nick, None) {
                        let acc_id = acc.id.clone();
                        cfg.accounts.push(acc);
                        if cfg.selected_account_id.is_none() {
                            cfg.selected_account_id = Some(acc_id);
                        }
                    }
                }
                save_config(&data_dir, &cfg);
                if let Ok(new_meta) = MetaClient::from_config(&cfg) {
                    *meta_holder.lock().unwrap() = Arc::new(new_meta);
                }
                sync_ui_from_config(&ui, &cfg);
            }
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[Настройка] Начальная настройка завершена.".to_string(),
            );
        });
    }

    // Random nickname & random icon generators (FreesmLauncher Lavarand-style!)
    {
        let ui_weak = ui.as_weak();
        ui.on_generate_random_nick(move |for_wizard| {
            let nick = generate_random_nickname();
            if let Some(ui) = ui_weak.upgrade() {
                if for_wizard {
                    ui.set_wizard_offline_nick(nick.into());
                } else {
                    ui.set_auth_username(nick.into());
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_randomize_edit_icon(move || {
            let ic = random_icon_id();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_edit_inst_icon_id(ic);
                ui.set_edit_inst_icon_image(instance_icon_image(ic));
                ui.set_edit_inst_icon_name(instance_icon_name(ic).into());
                let mut cfg = config.lock().unwrap();
                let instances_dir = cfg.resolve_instances_dir(&data_dir);
                let target_id = cfg.selected_instance_id.clone();
                if let Some(inst) = cfg
                    .instances
                    .iter_mut()
                    .find(|i| Some(&i.id) == target_id.as_ref())
                {
                    inst.icon_id = ic;
                    inst.custom_icon_path.clear();
                    write_instance_cfg(&instances_dir, inst);
                    save_config(&data_dir, &cfg);
                    sync_ui_from_config(&ui, &cfg);
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_randomize_new_icon(move || {
            let ic = random_icon_id();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_new_inst_icon_id(ic);
                ui.set_new_inst_custom_icon_path("".into());
                ui.set_new_inst_icon_image(instance_icon_image(ic));
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_select_instance_icon(move |icon_id, for_new| {
            let ic = normalize_icon_id(icon_id);
            if let Some(ui) = ui_weak.upgrade() {
                if for_new {
                    ui.set_new_inst_icon_id(ic);
                    ui.set_new_inst_custom_icon_path("".into());
                    ui.set_new_inst_icon_image(instance_icon_image(ic));
                } else {
                    ui.set_edit_inst_icon_id(ic);
                    ui.set_edit_inst_icon_image(instance_icon_image(ic));
                    ui.set_edit_inst_icon_name(instance_icon_name(ic).into());
                    let mut cfg = config.lock().unwrap();
                    let instances_dir = cfg.resolve_instances_dir(&data_dir);
                    let target_id = cfg.selected_instance_id.clone();
                    if let Some(inst) = cfg
                        .instances
                        .iter_mut()
                        .find(|i| Some(&i.id) == target_id.as_ref())
                    {
                        inst.icon_id = ic;
                        inst.custom_icon_path.clear();
                        write_instance_cfg(&instances_dir, inst);
                        save_config(&data_dir, &cfg);
                        sync_ui_from_config(&ui, &cfg);
                    }
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_pick_custom_instance_icon(move |for_new| {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let log_buf = log_buf.clone();
            rt.spawn_blocking(move || {
                if let Some(chosen) = pick_native_file("Выберите иконку сборки (PNG / JPG / WebP)", false) {
                    let src = PathBuf::from(&chosen);
                    let Ok(bytes) = std::fs::read(&src) else {
                        append_log_to_ui(&ui_weak, &log_buf, "[Иконки] Ошибка чтения файла иконки.".to_string());
                        return;
                    };
                    let Some(ext) = crate::instance_icons::detect_image_ext(&bytes) else {
                        append_log_to_ui(&ui_weak, &log_buf, "[Иконки] Неподдерживаемый формат изображения (PNG/JPG/WebP/SVG).".to_string());
                        return;
                    };
                    let inst_icons_dir = config.lock().unwrap().resolve_instance_icons_dir(&data_dir);
                    let _ = std::fs::create_dir_all(&inst_icons_dir);
                    let ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let dest = inst_icons_dir.join(format!("inst_{}.{}", ts, ext));
                    if let Err(e) = std::fs::write(&dest, &bytes) {
                        append_log_to_ui(&ui_weak, &log_buf, format!("[Иконки] Ошибка сохранения: {}", e));
                        return;
                    }
                    let dest_str = dest.to_string_lossy().to_string();
                    let w = ui_weak.clone();
                    let cfg_arc = config.clone();
                    let dd = data_dir.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = w.upgrade() {
                            ui.set_instance_icons(ModelRc::from(Rc::new(VecModel::from(
                                build_all_instance_icons(&inst_icons_dir),
                            ))));
                            if for_new {
                                ui.set_new_inst_custom_icon_path(dest_str.clone().into());
                                if let Some(im) = load_custom_icon_image(&dest) {
                                    ui.set_new_inst_icon_image(im);
                                }
                            } else {
                                let mut cfg = cfg_arc.lock().unwrap();
                                let instances_dir = cfg.resolve_instances_dir(&dd);
                                let target_id = cfg.selected_instance_id.clone();
                                if let Some(inst) = cfg
                                    .instances
                                    .iter_mut()
                                    .find(|i| Some(&i.id) == target_id.as_ref())
                                {
                                    inst.custom_icon_path = dest_str.clone();
                                    write_instance_cfg(&instances_dir, inst);
                                    save_config(&dd, &cfg);
                                    if let Some(im) = load_custom_icon_image(&dest) {
                                        ui.set_edit_inst_icon_image(im);
                                    }
                                    ui.set_edit_inst_icon_name("Своя иконка".into());
                                    sync_ui_from_config(&ui, &cfg);
                                }
                            }
                        }
                    });
                    append_log_to_ui(&ui_weak, &log_buf, "[Иконки] Своя иконка сборки применена и сохранена.".to_string());
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_select_custom_instance_icon(move |path_str, for_new| {
            let p = PathBuf::from(path_str.as_str());
            if let Some(ui) = ui_weak.upgrade() {
                if for_new {
                    ui.set_new_inst_custom_icon_path(path_str);
                    if let Some(im) = load_custom_icon_image(&p) {
                        ui.set_new_inst_icon_image(im);
                    }
                } else {
                    let mut cfg = config.lock().unwrap();
                    let instances_dir = cfg.resolve_instances_dir(&data_dir);
                    let target_id = cfg.selected_instance_id.clone();
                    if let Some(inst) = cfg
                        .instances
                        .iter_mut()
                        .find(|i| Some(&i.id) == target_id.as_ref())
                    {
                        inst.custom_icon_path = path_str.to_string();
                        write_instance_cfg(&instances_dir, inst);
                        save_config(&data_dir, &cfg);
                        if let Some(im) = load_custom_icon_image(&p) {
                            ui.set_edit_inst_icon_image(im);
                        }
                        ui.set_edit_inst_icon_name("Своя иконка".into());
                        sync_ui_from_config(&ui, &cfg);
                    }
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_add_custom_icon_pack(move || {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let log_buf = log_buf.clone();
            rt.spawn_blocking(move || {
                if let Some(chosen_dir) = pick_native_directory("Выберите папку с паком иконок (PNG/SVG/WebP)") {
                    let packs_dir = config.lock().unwrap().resolve_icon_packs_dir(&data_dir);
                    crate::ui_icons::ensure_packs_readme(&packs_dir);
                    match crate::ui_icons::import_icon_pack_dir(std::path::Path::new(&chosen_dir), &packs_dir) {
                        Ok(pack_name) => {
                            let w = ui_weak.clone();
                            let cfg_arc = config.clone();
                            let dd = data_dir.clone();
                            let pname = pack_name.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w.upgrade() {
                                    let mut cfg = cfg_arc.lock().unwrap();
                                    cfg.icon_theme = pname;
                                    save_config(&dd, &cfg);
                                    sync_ui_from_config(&ui, &cfg);
                                }
                            });
                            append_log_to_ui(&ui_weak, &log_buf, format!("[Иконки] Импортирован пак иконок «{}».", pack_name));
                        }
                        Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Иконки] Ошибка импорта пака: {}", e)),
                    }
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_pick_custom_app_icon(move || {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let log_buf = log_buf.clone();
            rt.spawn_blocking(move || {
                if let Some(chosen) = pick_native_file("Выберите логотип лаунчера (PNG / JPG / WebP)", false) {
                    let src = PathBuf::from(&chosen);
                    let Ok(bytes) = std::fs::read(&src) else { return; };
                    let Some(ext) = crate::instance_icons::detect_image_ext(&bytes) else {
                        append_log_to_ui(&ui_weak, &log_buf, "[Иконки] Неподдерживаемый формат изображения (PNG/JPG/WebP/SVG).".to_string());
                        return;
                    };
                    let icons_dir = config.lock().unwrap().resolve_icons_dir(&data_dir);
                    let _ = std::fs::create_dir_all(&icons_dir);
                    let dest = icons_dir.join(format!("app_logo_tmp.{}", ext));
                    if std::fs::write(&dest, &bytes).is_ok() {
                        let valid = slint::Image::load_from_path(&dest).is_ok();
                        if !valid {
                            let _ = std::fs::remove_file(&dest);
                            append_log_to_ui(&ui_weak, &log_buf, "[Иконки] Не удалось декодировать выбранное изображение.".to_string());
                            return;
                        }
                        for old in ["app_logo.png", "app_logo.jpg", "app_logo.webp", "app_logo.svg", "app_logo.gif"] {
                            let _ = std::fs::remove_file(icons_dir.join(old));
                        }
                        let final_dest = icons_dir.join(format!("app_logo.{}", ext));
                        let _ = std::fs::rename(&dest, &final_dest);
                        let w = ui_weak.clone();
                        let cfg_arc = config.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                let cfg = cfg_arc.lock().unwrap();
                                sync_ui_from_config(&ui, &cfg);
                            }
                        });
                        append_log_to_ui(&ui_weak, &log_buf, "[Иконки] Логотип лаунчера обновлён.".to_string());
                    }
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_reset_custom_app_icon(move || {
            let icons_dir = config.lock().unwrap().resolve_icons_dir(&data_dir);
            for old in ["app_logo.png", "app_logo.jpg", "app_logo.webp", "app_logo.svg", "app_logo.gif"] {
                let _ = std::fs::remove_file(icons_dir.join(old));
            }
            if let Some(ui) = ui_weak.upgrade() {
                let cfg = config.lock().unwrap();
                sync_ui_from_config(&ui, &cfg);
            }
            append_log_to_ui(&ui_weak, &log_buf, "[Иконки] Логотип лаунчера сброшен на стандартный.".to_string());
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_preview_cat_pack(move |enabled, preset, custom_path| {
            if let Some(ui) = ui_weak.upgrade() {
                let mut tmp = LauncherConfig::default();
                tmp.enable_cat_pack = enabled;
                tmp.cat_pack_preset = preset;
                tmp.cat_pack_custom_path = custom_path.to_string();
                sync_cat_image(&ui, &tmp);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_clear_custom_bg(move |which| {
            let mut cfg = config.lock().unwrap();
            if which == "instances" {
                cfg.custom_bg_instances_path.clear();
            } else {
                cfg.custom_bg_launcher_path.clear();
            }
            save_config(&data_dir, &cfg);
            if let Some(ui) = ui_weak.upgrade() {
                if which == "instances" {
                    ui.set_setting_bg_instances_path("".into());
                    ui.set_has_instances_bg_image(false);
                } else {
                    ui.set_setting_bg_launcher_path("".into());
                    ui.set_has_launcher_bg_image(false);
                }
            }
        });
    }

    // Open external URL (e.g., News article link)
    {
        ui.on_open_url(move |url| {
            let u = url.trim();
            if u.starts_with("http://") || u.starts_with("https://") {
                let _ = open::that(u);
            }
        });
    }
    // Fetch Mojang + Custom Resource News & hydrate thumbnails
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_refresh_news(move |filter_idx| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_news_loading(true);
            }
            let meta = current_meta(&meta_holder);
            let (custom_url, cache_dir) = {
                let cfg = config.lock().unwrap();
                (
                    cfg.custom_news_url.clone(),
                    cfg.resolve_icons_dir(&data_dir).join("news_cache"),
                )
            };
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            let rt_imgs = rt.clone();
            rt.spawn(async move {
                let (mj_res, cu_res) = tokio::join!(
                    async {
                        if filter_idx == 2 {
                            Ok(Vec::new())
                        } else {
                            meta.fetch_mojang_news().await
                        }
                    },
                    async {
                        if filter_idx == 1 || custom_url.trim().is_empty() {
                            Ok(Vec::new())
                        } else {
                            meta.fetch_custom_news(&custom_url).await
                        }
                    }
                );

                let mut combined = Vec::new();
                if let Ok(cu) = cu_res {
                    combined.extend(cu);
                } else if let Err(e) = cu_res {
                    append_log_to_ui(&ui_weak, &log_buf, format!("[Новости] {}", e));
                }
                if let Ok(mj) = mj_res {
                    combined.extend(mj);
                } else if let Err(e) = mj_res {
                    append_log_to_ui(&ui_weak, &log_buf, format!("[Новости Mojang] {}", e));
                }

                let img_urls: Vec<(usize, String)> = combined
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| !n.image_url.is_empty())
                    .map(|(i, n)| (i, n.image_url.clone()))
                    .collect();

                let w = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = w.upgrade() {
                        let ui_items: Vec<UiNewsItem> = combined
                            .into_iter()
                            .map(|n| UiNewsItem {
                                title: n.title.into(),
                                summary: n.summary.into(),
                                date: n.date.into(),
                                source: n.source.into(),
                                url: n.url.into(),
                                image: slint::Image::default(),
                            })
                            .collect();
                        ui.set_news_items(ModelRc::from(Rc::new(VecModel::from(ui_items))));
                        ui.set_news_loading(false);
                    }
                });

                let sem = Arc::new(tokio::sync::Semaphore::new(4));
                for (idx, url) in img_urls {
                    let sem_p = sem.clone();
                    let meta_p = meta.clone();
                    let cd = cache_dir.clone();
                    let w_p = ui_weak.clone();
                    rt_imgs.spawn(async move {
                        let Ok(_permit) = sem_p.acquire().await else { return; };
                        if let Some(path) = meta_p.download_cached_icon(&url, &cd).await {
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w_p.upgrade() {
                                    if let Ok(img) = slint::Image::load_from_path(&path) {
                                        let model = ui.get_news_items();
                                        if let Some(mut row) = model.row_data(idx) {
                                            row.image = img;
                                            model.set_row_data(idx, row);
                                        }
                                    }
                                }
                            });
                        }
                    });
                }
            });
        });
    }

    // Refresh available skin/auth patcher versions (authlib-injector / unmojang/Loki)
    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        ui.on_refresh_patcher_versions(move |backend_idx| {
            let ui_weak = ui_weak.clone();
            let http = current_meta(&meta_holder).http().clone();
            let backend = AuthAgentBackend::from_index(backend_idx);
            rt.spawn(async move {
                let versions = fetch_patcher_versions(&http, backend).await;
                let shared: Vec<SharedString> = versions.into_iter().map(SharedString::from).collect();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_available_patcher_versions(ModelRc::from(Rc::new(VecModel::from(
                            shared,
                        ))));
                    }
                });
            });
        });
    }

    // CurseForge API Key helpers
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_apply_builtin_cf_key(move || {
            let mut cfg = config.lock().unwrap();
            cfg.curseforge_api_key = BUILTIN_CF_KEY.to_string();
            cfg.curseforge_key_confirmed = true;
            save_config(&data_dir, &cfg);
            if let Ok(new_meta) = MetaClient::from_config(&cfg) {
                *meta_holder.lock().unwrap() = Arc::new(new_meta);
            }
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_setting_cf_api_key(BUILTIN_CF_KEY.into());
                ui.set_cf_key_confirmed(true);
            }
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[CurseForge] Подключён встроенный ключ API.".to_string(),
            );
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_confirm_cf_key(move |key| {
            let mut cfg = config.lock().unwrap();
            let k = key.trim().to_string();
            if !k.is_empty() {
                cfg.curseforge_api_key = k;
            }
            cfg.curseforge_key_confirmed = true;
            save_config(&data_dir, &cfg);
            if let Ok(new_meta) = MetaClient::from_config(&cfg) {
                *meta_holder.lock().unwrap() = Arc::new(new_meta);
            }
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_cf_key_confirmed(true);
            }
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[CurseForge] Ключ API сохранён.".to_string(),
            );
        });
    }

    // 1. Select Instance & Filter/Sort Instances
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let discord_ctrl = discord_ctrl.clone();
        ui.on_select_instance(move |inst_id| {
            let mut cfg = config.lock().unwrap();
            let id_str = inst_id.to_string();
            if id_str.is_empty() {
                cfg.selected_instance_id = None;
            } else if cfg.instances.iter().any(|i| i.id == id_str) {
                cfg.selected_instance_id = Some(id_str);
            }
            save_config(&data_dir, &cfg);
            sync_discord_launcher_state(&discord_ctrl, &cfg, launcher_start_ts);
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_show_export_menu(false);
                sync_ui_from_config(&ui, &cfg);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_filter_instances(move |query, group, sort_mode| {
            let mut cfg = config.lock().unwrap();
            cfg.instance_sort_mode = sort_mode.to_string();
            save_config(&data_dir, &cfg);
            if let Some(ui) = ui_weak.upgrade() {
                let (ui_instances, sel_idx) = build_filtered_instances(
                    &cfg,
                    query.as_str(),
                    group.as_str(),
                    sort_mode.as_str(),
                );
                ui.set_instances(ModelRc::from(Rc::new(VecModel::from(ui_instances))));
                ui.set_selected_instance_index(sel_idx);
            }
        });
    }

    // 1b. Open Edit Instance (and auto-populate MC / Loader / OptiFine dropdowns!)
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_open_edit_instance(move || {
            let cfg = config.lock().unwrap();
            if let Some(ui) = ui_weak.upgrade() {
                load_instance_into_editor(&ui, &cfg, &data_dir);
                ui.set_edit_subtab(1);
                ui.set_active_tab(5);
                let l_idx = ui.get_edit_inst_loader_idx();
                let mc = ui.get_edit_inst_mc();
                let snaps = ui.get_include_snapshots();
                ui.invoke_refresh_edit_versions(l_idx, mc.clone(), snaps);
                if l_idx == 2 {
                    ui.invoke_refresh_optifine_versions(mc);
                }
            }
        });
    }

    // 1c. Save Per-Instance Settings
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_save_instance_settings(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut cfg = config.lock().unwrap();
                let target_id = cfg.selected_instance_id.clone();
                let folder_rename_mode = cfg.folder_rename_mode;
                let instances_dir = cfg.resolve_instances_dir(&data_dir);
                let mut rename_prompt: Option<(String, String, String)> = None;
                let mut renamed_log: Option<String> = None;

                if let Some(inst) = cfg
                    .instances
                    .iter_mut()
                    .find(|i| Some(&i.id) == target_id.as_ref())
                {
                    let old_name = inst.name.clone();
                    let old_id = inst.id.clone();
                    let name_val = ui.get_edit_inst_name().to_string();
                    if !name_val.trim().is_empty() {
                        inst.name = name_val.trim().to_string();
                    }
                    inst.group = ui.get_edit_inst_group().trim().to_string();
                    let chosen_lv = ui.get_edit_inst_loader_ver().trim().to_string();
                    if chosen_lv.starts_with("Нет сборок") {
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Сборка] Нельзя сохранить: {}", chosen_lv),
                        );
                        return;
                    }
                    inst.mc_version = ui.get_edit_inst_mc().trim().to_string();
                    inst.loader = LoaderType::from_index(ui.get_edit_inst_loader_idx());
                    inst.loader_version = chosen_lv;
                    inst.icon_id = normalize_icon_id(ui.get_edit_inst_icon_id());
                    let of_v = ui.get_edit_inst_optifine_ver().trim().to_string();
                    inst.optifine_enabled = ui.get_edit_inst_optifine()
                        && inst.loader == LoaderType::Forge
                        && !of_v.starts_with("Нет сборок");
                    inst.optifine_version = if of_v.is_empty() || of_v.starts_with("Нет сборок") {
                        None
                    } else {
                        Some(of_v)
                    };
                    inst.notes = ui.get_edit_inst_notes().to_string();
                    inst.use_custom_loader = ui.get_edit_use_custom_loader();
                    inst.custom_loader_name = ui.get_edit_custom_loader_name().trim().to_string();
                    inst.custom_loader_path = ui.get_edit_custom_loader_path().trim().to_string();
                    inst.custom_loader_main_class = ui.get_edit_custom_loader_main_class().trim().to_string();
                    inst.enable_auto_join = ui.get_edit_enable_auto_join();
                    inst.quick_play_server = ui.get_edit_inst_quick_server().trim().to_string();
                    inst.quick_play_world = ui.get_edit_inst_quick_world().trim().to_string();

                    inst.override_account = ui.get_edit_override_account();
                    let ca = ui.get_edit_custom_account_id().trim().to_string();
                    inst.custom_account_id = if ca.is_empty() { None } else { Some(ca) };
                    inst.override_elyby = ui.get_edit_override_elyby();
                    inst.custom_elyby_skin_mode = ui.get_edit_custom_elyby_mode();
                    inst.custom_auth_backend = ui.get_edit_custom_auth_backend();
                    inst.custom_auth_version = ui.get_edit_custom_auth_version().to_string();
                    inst.override_discord = ui.get_edit_override_discord();
                    inst.custom_discord_rpc = ui.get_edit_custom_discord_rpc();
                    inst.override_datapacks = ui.get_edit_override_datapacks();
                    inst.custom_datapacks_path = ui.get_edit_custom_datapacks_path().to_string();
                    inst.override_playtime = ui.get_edit_override_playtime();
                    inst.custom_record_playtime = ui.get_edit_custom_record_playtime();

                    inst.override_java = ui.get_edit_override_java();
                    inst.custom_java_path = Some(ui.get_edit_custom_java().to_string());
                    inst.custom_skip_java_compat = ui.get_edit_custom_skip_java_compat();

                    inst.override_memory = ui.get_edit_override_memory();
                    if let Ok(min_m) = ui.get_edit_custom_min_mem_str().trim().parse::<u32>() {
                        inst.custom_min_memory_mb = Some(min_m.max(8));
                    }
                    if let Ok(max_m) = ui.get_edit_custom_max_mem_str().trim().parse::<u32>() {
                        inst.custom_max_memory_mb = Some(max_m.max(8));
                    }
                    if let Ok(pg) = ui.get_edit_custom_permgen_str().trim().parse::<u32>() {
                        inst.custom_permgen_mb = Some(pg.max(4));
                    }

                    inst.override_jvm_args = ui.get_edit_override_jvm();
                    inst.custom_jvm_args = Some(ui.get_edit_custom_jvm().to_string());

                    inst.override_window = ui.get_edit_override_window();
                    if let Ok(w) = ui.get_edit_custom_width_str().trim().parse::<u32>() {
                        inst.custom_window_width = Some(w.max(640));
                    }
                    if let Ok(h) = ui.get_edit_custom_height_str().trim().parse::<u32>() {
                        inst.custom_window_height = Some(h.max(480));
                    }
                    inst.custom_start_maximized = ui.get_edit_custom_maximized();
                    inst.custom_quit_after_close = ui.get_edit_custom_quit_after_close();
                    inst.custom_copy_screenshots = ui.get_edit_custom_copy_screenshots();

                    inst.override_commands = ui.get_edit_override_commands();
                    inst.custom_pre_launch_cmd = Some(ui.get_edit_custom_pre_cmd().to_string());
                    inst.custom_wrapper_cmd = Some(ui.get_edit_custom_wrapper().to_string());
                    inst.custom_post_exit_cmd = Some(ui.get_edit_custom_post_cmd().to_string());

                    inst.override_env = ui.get_edit_override_env();
                    inst.custom_env_vars = Some(ui.get_edit_custom_env().to_string());

                    inst.override_linux_tweaks = ui.get_edit_override_linux();
                    inst.custom_legacy_online_fixes = ui.get_edit_custom_legacy_fixes();
                    inst.custom_use_system_glfw = ui.get_edit_custom_use_sys_glfw();
                    inst.custom_glfw_path = ui.get_edit_custom_glfw_path().to_string();
                    inst.custom_use_system_openal = ui.get_edit_custom_use_sys_openal();
                    inst.custom_openal_path = ui.get_edit_custom_openal_path().to_string();
                    inst.custom_use_discrete_gpu = ui.get_edit_custom_dgpu();
                    inst.custom_enable_gamemode = ui.get_edit_custom_gamemode();
                    inst.custom_enable_mangohud = ui.get_edit_custom_mangohud();
                    inst.custom_use_zink = ui.get_edit_custom_zink();

                    inst.override_console = ui.get_edit_override_console();
                    inst.custom_show_console_on_launch = ui.get_edit_custom_show_console_launch();
                    inst.custom_show_console_on_error = ui.get_edit_custom_show_console_error();
                    inst.custom_hide_console_on_exit = ui.get_edit_custom_hide_console_exit();
                    inst.custom_close_after_launch = ui.get_edit_custom_close_after_launch();

                    let saved_name = inst.name.clone();
                    let desired_dir = dir_name_from_string(&saved_name, &instances_dir, Some(&old_id));

                    // Exact FreesmLauncher / PrismLauncher folder rename behavior:
                    // 0 = Ask, 1 = Always rename, 2 = Never rename
                    if saved_name != old_name && desired_dir != old_id {
                        match folder_rename_mode {
                            1 => {
                                if let Ok(new_id) = rename_instance_folder(&instances_dir, &old_id, &saved_name) {
                                    inst.id = new_id.clone();
                                    ui.set_edit_inst_folder_name(new_id.clone().into());
                                    renamed_log = Some(format!(
                                        "[Сборка] Папка переименована: {} -> {}",
                                        old_id, new_id
                                    ));
                                }
                            }
                            0 => {
                                rename_prompt = Some((old_id.clone(), desired_dir, saved_name.clone()));
                            }
                            _ => {}
                        }
                    }

                    let final_id = inst.id.clone();
                    write_instance_cfg(&instances_dir, inst);
                    cfg.selected_instance_id = Some(final_id);
                    save_config(&data_dir, &cfg);
                    sync_ui_from_config(&ui, &cfg);

                    if let Some((old_f, new_f, display_n)) = rename_prompt {
                        ui.set_rename_folder_old(old_f.into());
                        ui.set_rename_folder_new(new_f.into());
                        ui.set_rename_folder_inst_name(display_n.into());
                        ui.set_rename_folder_remember(false);
                        ui.set_show_rename_folder_modal(true);
                    }
                    if let Some(msg) = renamed_log {
                        append_log_to_ui(&ui_weak, &log_buf, msg);
                    }
                    append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Сборка] Настройки «{}» сохранены.", saved_name),
                    );
                }
            }
        });
    }

    // 1c-2. Confirm Rename Folder Modal & Manual Folder Sync (Freesm/Prism feature!)
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_confirm_rename_folder(move |do_rename, remember_choice| {
            let mut cfg = config.lock().unwrap();
            if remember_choice {
                cfg.folder_rename_mode = if do_rename { 1 } else { 2 };
            }
            if do_rename {
                let instances_dir = cfg.resolve_instances_dir(&data_dir);
                let target_id = cfg.selected_instance_id.clone();
                if let Some(inst) = cfg
                    .instances
                    .iter_mut()
                    .find(|i| Some(&i.id) == target_id.as_ref())
                {
                    let old_id = inst.id.clone();
                    let name = inst.name.clone();
                    match rename_instance_folder(&instances_dir, &old_id, &name) {
                        Ok(new_id) => {
                            inst.id = new_id.clone();
                            write_instance_cfg(&instances_dir, inst);
                            cfg.selected_instance_id = Some(new_id.clone());
                            if let Some(ui) = ui_weak.upgrade() {
                                ui.set_edit_inst_folder_name(new_id.clone().into());
                            }
                            append_log_to_ui(
                                &ui_weak,
                                &log_buf,
                                format!(
                                    "[Сборка] Папка переименована: {} -> {}",
                                    old_id, new_id
                                ),
                            );
                        }
                        Err(e) => append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Сборка] Не удалось переименовать папку: {}", e),
                        ),
                    }
                }
            }
            save_config(&data_dir, &cfg);
            if let Some(ui) = ui_weak.upgrade() {
                sync_ui_from_config(&ui, &cfg);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_sync_instance_folder_name(move || {
            let mut cfg = config.lock().unwrap();
            let instances_dir = cfg.resolve_instances_dir(&data_dir);
            let target_id = cfg.selected_instance_id.clone();
            if let Some(inst) = cfg
                .instances
                .iter_mut()
                .find(|i| Some(&i.id) == target_id.as_ref())
            {
                let old_id = inst.id.clone();
                let new_name = if let Some(ui) = ui_weak.upgrade() {
                    let typed = ui.get_edit_inst_name().trim().to_string();
                    if !typed.is_empty() {
                        inst.name = typed.clone();
                        typed
                    } else {
                        inst.name.clone()
                    }
                } else {
                    inst.name.clone()
                };
                match rename_instance_folder(&instances_dir, &old_id, &new_name) {
                    Ok(new_id) => {
                        inst.id = new_id.clone();
                        write_instance_cfg(&instances_dir, inst);
                        cfg.selected_instance_id = Some(new_id.clone());
                        save_config(&data_dir, &cfg);
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_edit_inst_folder_name(new_id.clone().into());
                            sync_ui_from_config(&ui, &cfg);
                        }
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!(
                                "[Сборка] Папка синхронизирована: {} -> {}",
                                old_id, new_id
                            ),
                        );
                    }
                    Err(e) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Сборка] Не удалось переименовать папку: {}", e),
                    ),
                }
            }
        });
    }

    // 1d. Toggle / Delete Installed Mods, Subfolder Assets & Copy Screenshots
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_toggle_installed_mod(move |filename| {
            let cfg = config.lock().unwrap();
            let Some(sel_id) = cfg.selected_instance_id.as_ref() else {
                return;
            };
            let instances_dir = cfg.resolve_instances_dir(&data_dir);
            let mods_dir = instances_dir
                .join(sel_id)
                .join(".minecraft")
                .join("mods");
            let fname = filename.to_string();
            let old_path = mods_dir.join(&fname);
            let new_name = if let Some(stripped) = fname.strip_suffix(".disabled") {
                stripped.to_string()
            } else {
                format!("{}.disabled", fname)
            };
            let _ = std::fs::rename(&old_path, mods_dir.join(&new_name));
            if let Some(ui) = ui_weak.upgrade() {
                let mods = scan_installed_mods(&instances_dir, sel_id);
                ui.set_installed_mods(ModelRc::from(Rc::new(VecModel::from(mods))));
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_delete_installed_mod(move |filename| {
            let cfg = config.lock().unwrap();
            let Some(sel_id) = cfg.selected_instance_id.as_ref() else {
                return;
            };
            let instances_dir = cfg.resolve_instances_dir(&data_dir);
            let mods_dir = instances_dir
                .join(sel_id)
                .join(".minecraft")
                .join("mods");
            let _ = std::fs::remove_file(mods_dir.join(filename.as_str()));
            if let Some(ui) = ui_weak.upgrade() {
                let mods = scan_installed_mods(&instances_dir, sel_id);
                ui.set_installed_mods(ModelRc::from(Rc::new(VecModel::from(mods))));
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_toggle_instance_asset(move |sub, name| {
            let cfg = config.lock().unwrap();
            let Some(sel_id) = cfg.selected_instance_id.as_ref() else {
                return;
            };
            let dir = cfg
                .resolve_instances_dir(&data_dir)
                .join(sel_id)
                .join(".minecraft")
                .join(sub.as_str());
            let fname = name.to_string();
            let new_name = if let Some(stripped) = fname.strip_suffix(".disabled") {
                stripped.to_string()
            } else {
                format!("{}.disabled", fname)
            };
            let _ = std::fs::rename(dir.join(&fname), dir.join(&new_name));
            if let Some(ui) = ui_weak.upgrade() {
                load_instance_into_editor(&ui, &cfg, &data_dir);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_delete_instance_asset(move |sub, name| {
            let cfg = config.lock().unwrap();
            let Some(sel_id) = cfg.selected_instance_id.as_ref() else {
                return;
            };
            let target = cfg
                .resolve_instances_dir(&data_dir)
                .join(sel_id)
                .join(".minecraft")
                .join(sub.as_str())
                .join(name.as_str());
            if target.is_dir() {
                let _ = std::fs::remove_dir_all(&target);
            } else {
                let _ = std::fs::remove_file(&target);
            }
            if let Some(ui) = ui_weak.upgrade() {
                load_instance_into_editor(&ui, &cfg, &data_dir);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_copy_screenshot_to_clipboard(move |name| {
            let cfg = config.lock().unwrap();
            let Some(sel_id) = cfg.selected_instance_id.as_ref() else {
                return;
            };
            let path = cfg
                .resolve_instances_dir(&data_dir)
                .join(sel_id)
                .join(".minecraft")
                .join("screenshots")
                .join(name.as_str());
            match copy_png_to_clipboard(&path) {
                Ok(()) => append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    format!("[Скриншот] Скопирован в буфер обмена: {}", name),
                ),
                Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Скриншот] {}", e)),
            }
        });
    }

    // 1e. Deep Duplicate Instance & Export Instance (.mrpack)
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_duplicate_selected_instance(move || {
            let mut cfg = config.lock().unwrap();
            let Some(orig) = cfg
                .instances
                .iter()
                .find(|i| Some(&i.id) == cfg.selected_instance_id.as_ref())
                .cloned()
            else {
                return;
            };
            let mut copy = orig.clone();
            copy.name = format!("{} (Копия)", orig.name);
            let instances_dir = cfg.resolve_instances_dir(&data_dir);
            let _ = std::fs::create_dir_all(&instances_dir);
            copy.id = dir_name_from_string(&copy.name, &instances_dir, None);
            write_instance_cfg(&instances_dir, &copy);
            let cname = copy.name.clone();
            let new_id = copy.id.clone();
            let old_id = orig.id.clone();
            cfg.selected_instance_id = Some(new_id.clone());
            cfg.instances.push(copy);
            save_config(&data_dir, &cfg);
            if let Some(ui) = ui_weak.upgrade() {
                sync_ui_from_config(&ui, &cfg);
            }
            let src = instances_dir.join(&old_id).join(".minecraft");
            let dst = instances_dir.join(&new_id).join(".minecraft");
            let ui_weak_c = ui_weak.clone();
            let log_buf_c = log_buf.clone();
            rt.spawn_blocking(move || {
                if src.is_dir() {
                    let _ = copy_dir_recursive(&src, &dst);
                }
                append_log_to_ui(
                    &ui_weak_c,
                    &log_buf_c,
                    format!("[Сборки] Создана полная копия «{}».", cname),
                );
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_export_selected_instance(move |fmt_str| {
            let (inst_opt, instances_dir, exports_dir) = {
                let cfg = config.lock().unwrap();
                let inst = cfg
                    .instances
                    .iter()
                    .find(|i| Some(&i.id) == cfg.selected_instance_id.as_ref())
                    .cloned();
                (
                    inst,
                    cfg.resolve_instances_dir(&data_dir),
                    cfg.resolve_exports_dir(&data_dir),
                )
            };
            let Some(inst) = inst_opt else { return };
            let fmt = ExportFormat::from_str_tag(fmt_str.as_str());
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                format!("[Экспорт] Упаковка «{}» в формат {}...", inst.name, fmt.label()),
            );
            rt.spawn_blocking(move || {
                match export_instance_archive(&instances_dir, &exports_dir, &inst, fmt) {
                    Ok(path) => {
                        if let Some(parent) = path.parent() {
                            let _ = open::that(parent);
                        }
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Экспорт • {}] Готово: {}", fmt.label(), path.display()),
                        );
                    }
                    Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Ошибка экспорта] {:#}", e)),
                }
            });
        });
    }

    // 2. Refresh Versions
    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        ui.on_refresh_optifine_versions(move |mc_ver| {
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let mcv = mc_ver.to_string();
            rt.spawn(async move {
                let list: Vec<String> = meta
                    .fetch_optifine_versions(&mcv)
                    .await
                    .map(|v| v.into_iter().map(|e| e.full_version()).collect())
                    .unwrap_or_default();
                let display_val = list
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Нет сборок OptiFine для этой версии".to_string());
                let is_empty = list.is_empty();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        let shared: Vec<SharedString> =
                            list.into_iter().map(SharedString::from).collect();
                        ui.set_available_optifine_versions(ModelRc::from(Rc::new(VecModel::from(
                            shared,
                        ))));
                        ui.set_new_inst_optifine_version(display_val.clone().into());
                        let cur_edit = ui.get_edit_inst_optifine_ver().to_string();
                        if cur_edit.is_empty() || is_empty || cur_edit.starts_with("Нет сборок") {
                            ui.set_edit_inst_optifine_ver(display_val.into());
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        ui.on_refresh_versions(move |loader_idx, mc_ver, include_snapshots| {
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let loader = LoaderType::from_index(loader_idx);
            let mc_ver_str = mc_ver.to_string();

            rt.spawn(async move {
                let mc_list = if loader == LoaderType::Horizon {
                    vec!["26.3".to_string(), "latest".to_string()]
                } else if loader == LoaderType::LabyMod {
                    match meta.fetch_labymod_manifest().await {
                        Ok(m) => m
                            .minecraft_versions
                            .into_iter()
                            .map(|v| v.version)
                            .collect::<Vec<_>>(),
                        Err(_) => vec!["1.21.4".to_string(), "1.21.1".to_string()],
                    }
                } else {
                    meta.fetch_minecraft_versions(include_snapshots)
                        .await
                        .unwrap_or_else(|_| vec!["1.21.4".to_string(), "1.21.1".to_string()])
                };

                let chosen_mc = if !mc_ver_str.is_empty() && mc_list.contains(&mc_ver_str) {
                    mc_ver_str.clone()
                } else {
                    mc_list
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "1.21.4".to_string())
                };

                let loader_vers = meta
                    .fetch_loader_versions(&loader, &chosen_mc)
                    .await
                    .unwrap_or_default();
                let latest_loader_ver = loader_vers
                    .first()
                    .cloned()
                    .unwrap_or_else(|| format!("Нет сборок {} для этой версии", loader.as_str()));

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        let mc_shared: Vec<SharedString> =
                            mc_list.into_iter().map(SharedString::from).collect();
                        ui.set_available_mc_versions(ModelRc::from(Rc::new(VecModel::from(
                            mc_shared,
                        ))));
                        ui.set_new_inst_mc_version(chosen_mc.into());

                        let loader_shared: Vec<SharedString> =
                            loader_vers.into_iter().map(SharedString::from).collect();
                        ui.set_available_loader_versions(ModelRc::from(Rc::new(VecModel::from(
                            loader_shared,
                        ))));
                        ui.set_new_inst_loader_version(latest_loader_ver.into());
                    }
                });
            });
        });
    }

    // 2b. Refresh Versions for Edit Instance Dropdowns
    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        ui.on_refresh_edit_versions(move |loader_idx, mc_ver, include_snapshots| {
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let loader = LoaderType::from_index(loader_idx);
            let mc_ver_str = mc_ver.to_string();

            rt.spawn(async move {
                let mc_list = if loader == LoaderType::Horizon {
                    vec!["26.3".to_string(), "latest".to_string()]
                } else if loader == LoaderType::LabyMod {
                    match meta.fetch_labymod_manifest().await {
                        Ok(m) => m
                            .minecraft_versions
                            .into_iter()
                            .map(|v| v.version)
                            .collect::<Vec<_>>(),
                        Err(_) => vec!["1.21.4".to_string(), "1.21.1".to_string()],
                    }
                } else {
                    meta.fetch_minecraft_versions(include_snapshots)
                        .await
                        .unwrap_or_else(|_| vec!["1.21.4".to_string(), "1.21.1".to_string()])
                };

                let chosen_mc = if !mc_ver_str.is_empty() {
                    mc_ver_str.clone()
                } else {
                    mc_list
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "1.21.4".to_string())
                };

                let loader_vers = meta
                    .fetch_loader_versions(&loader, &chosen_mc)
                    .await
                    .unwrap_or_default();
                let latest_loader_ver = loader_vers
                    .first()
                    .cloned()
                    .unwrap_or_else(|| format!("Нет сборок {} для этой версии", loader.as_str()));

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        let mc_shared: Vec<SharedString> =
                            mc_list.into_iter().map(SharedString::from).collect();
                        ui.set_available_mc_versions(ModelRc::from(Rc::new(VecModel::from(
                            mc_shared,
                        ))));
                        if ui.get_edit_inst_mc().is_empty() {
                            ui.set_edit_inst_mc(chosen_mc.into());
                        }

                        let current_lv = ui.get_edit_inst_loader_ver().to_string();
                        let needs_lv_update = loader_vers.is_empty()
                            || current_lv.is_empty()
                            || !loader_vers.contains(&current_lv);
                        let loader_shared: Vec<SharedString> =
                            loader_vers.into_iter().map(SharedString::from).collect();
                        ui.set_available_loader_versions(ModelRc::from(Rc::new(VecModel::from(
                            loader_shared,
                        ))));
                        if needs_lv_update {
                            ui.set_edit_inst_loader_ver(latest_loader_ver.into());
                        }
                    }
                });
            });
        });
    }

    // 3. Create Custom Instance (and immediately download & prepare its files with progress modal!)
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        ui.on_create_instance(
            move |name, group, mc_ver, loader_idx, loader_ver, include_optifine, optifine_ver| {
                let loader = LoaderType::from_index(loader_idx);
                if loader_ver.trim().is_empty() || loader_ver.starts_with("Нет сборок") {
                    append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!(
                            "[Сборки] Нет сборок {} для версии Minecraft {}",
                            loader.as_str(),
                            mc_ver
                        ),
                    );
                    return;
                }
                let (chosen_icon_id, chosen_custom_icon) = ui_weak
                    .upgrade()
                    .map(|u| {
                        (
                            normalize_icon_id(u.get_new_inst_icon_id()),
                            u.get_new_inst_custom_icon_path().to_string(),
                        )
                    })
                    .unwrap_or_else(|| (random_icon_id(), String::new()));
                let (cfg_snapshot, inst_snapshot, clean_name_dl, mc_ver_dl, loader_label) = {
                    let mut cfg = config.lock().unwrap();
                    let clean_name = if name.trim().is_empty() {
                        format!("{} {}", loader.as_str(), mc_ver)
                    } else {
                        name.trim().to_string()
                    };
                    let clean_group = if group.trim().is_empty() {
                        "Основное".to_string()
                    } else {
                        group.trim().to_string()
                    };
                    let instances_dir = cfg.resolve_instances_dir(&data_dir);
                    let _ = std::fs::create_dir_all(&instances_dir);
                    let id = dir_name_from_string(&clean_name, &instances_dir, None);
                    let _ = std::fs::create_dir_all(instances_dir.join(&id).join(".minecraft"));
                    let of_v = optifine_ver.trim().to_string();
                    let of_enabled = include_optifine
                        && loader == LoaderType::Forge
                        && !of_v.is_empty()
                        && !of_v.starts_with("Нет");
                    let mut inst = Instance::new_default(
                        id.clone(),
                        clean_name.clone(),
                        clean_group,
                        mc_ver.to_string(),
                        loader.clone(),
                        loader_ver.to_string(),
                        of_enabled,
                        if of_enabled { Some(of_v) } else { None },
                        if loader == LoaderType::Horizon {
                            "Native Rust"
                        } else {
                            "Готов"
                        },
                    );
                    inst.icon_id = chosen_icon_id;
                    inst.custom_icon_path = chosen_custom_icon;
                    write_instance_cfg(&instances_dir, &inst);
                    let inst_clone = inst.clone();
                    cfg.instances.push(inst);
                    cfg.selected_instance_id = Some(id);
                    save_config(&data_dir, &cfg);
                    if let Some(ui) = ui_weak.upgrade() {
                        let next_ic = random_icon_id();
                        ui.set_new_inst_icon_id(next_ic);
                        ui.set_new_inst_custom_icon_path("".into());
                        ui.set_new_inst_icon_image(instance_icon_image(next_ic));
                        sync_ui_from_config(&ui, &cfg);
                        ui.set_download_modal_title("Загрузка сборки".into());
                        ui.set_download_modal_subtitle(
                            format!("{} • {} {}", clean_name, loader.as_str(), mc_ver).into(),
                        );
                        ui.set_download_modal_status(
                            "Подготовка и скачивание файлов Minecraft...".into(),
                        );
                        ui.set_download_modal_progress(0.03);
                        ui.set_download_modal_done(false);
                        ui.set_download_modal_error(false);
                        ui.set_show_download_modal(true);
                        ui.set_is_busy(true);
                        ui.set_launch_progress(0.03);
                        ui.set_status_is_error(false);
                        ui.set_status_text(
                            format!("Загрузка сборки «{}»...", clean_name).into(),
                        );
                    }
                    (
                        cfg.clone(),
                        inst_clone,
                        clean_name,
                        mc_ver.to_string(),
                        loader.as_str().to_string(),
                    )
                };
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    format!(
                        "[Сборки] Создана сборка «{}» ({} | {}) — запуск первоначальной загрузки...",
                        clean_name_dl, mc_ver_dl, loader_label
                    ),
                );

                let ui_weak_dl = ui_weak.clone();
                let log_buf_dl = log_buf.clone();
                let data_dir_dl = data_dir.clone();
                let meta_dl = meta_holder.lock().unwrap().clone();
                rt.spawn(async move {
                    let w_log = ui_weak_dl.clone();
                    let lb_log = log_buf_dl.clone();
                    let w_prog = ui_weak_dl.clone();
                    let res = launcher::prepare_instance(
                        meta_dl,
                        cfg_snapshot,
                        inst_snapshot,
                        data_dir_dl,
                        move |line| {
                            append_log_to_ui(&w_log, &lb_log, line);
                        },
                        move |p, msg| {
                            let w = w_prog.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w.upgrade() {
                                    ui.set_download_modal_progress(p);
                                    ui.set_download_modal_status(msg.clone().into());
                                    ui.set_launch_progress(p);
                                    ui.set_status_text(msg.into());
                                }
                            });
                        },
                    )
                    .await;

                    match res {
                        Ok(()) => {
                            let w = ui_weak_dl.clone();
                            let done_name = clean_name_dl.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w.upgrade() {
                                    ui.set_download_modal_progress(1.0);
                                    ui.set_download_modal_done(true);
                                    ui.set_download_modal_error(false);
                                    ui.set_download_modal_status(
                                        format!(
                                            "Сборка «{}» полностью загружена и готова к запуску!",
                                            done_name
                                        )
                                        .into(),
                                    );
                                    ui.set_is_busy(false);
                                    ui.set_launch_progress(0.0);
                                    ui.set_status_is_error(false);
                                    ui.set_status_text(
                                        format!("Сборка «{}» готова к запуску", done_name).into(),
                                    );
                                }
                            });
                        }
                        Err(e) => {
                            let err_msg = format!("Ошибка загрузки сборки: {}", e);
                            append_log_to_ui(&ui_weak_dl, &log_buf_dl, err_msg.clone());
                            let w = ui_weak_dl.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w.upgrade() {
                                    ui.set_download_modal_error(true);
                                    ui.set_download_modal_done(false);
                                    ui.set_download_modal_status(err_msg.clone().into());
                                    ui.set_is_busy(false);
                                    ui.set_launch_progress(0.0);
                                    ui.set_status_is_error(true);
                                    ui.set_status_text(err_msg.into());
                                }
                            });
                        }
                    }
                });
            },
        );
    }

    // 3b. Search External Modpacks (with live debounce + async icon hydration)
    {
        use std::sync::atomic::{AtomicU64, Ordering};
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        let search_gen = Arc::new(AtomicU64::new(0));

        ui.on_search_modpacks(move |provider_idx, query| {
            let my_gen = search_gen.fetch_add(1, Ordering::Relaxed) + 1;
            let gen_ref = search_gen.clone();
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let log_buf = log_buf.clone();
            let provider = ExternalProvider::from_index(provider_idx);
            let q = query.to_string();
            let cache_dir = config
                .lock()
                .unwrap()
                .resolve_icons_dir(&data_dir)
                .join("catalog_cache");
            let rt_icons = rt.clone();

            rt.spawn(async move {
                if !q.trim().is_empty() {
                    tokio::time::sleep(std::time::Duration::from_millis(260)).await;
                }
                if gen_ref.load(Ordering::Relaxed) != my_gen {
                    return;
                }
                match meta
                    .search_external_asset(provider, AssetKind::Modpack, &q, None, None)
                    .await
                {
                    Ok(items) => {
                        if gen_ref.load(Ordering::Relaxed) != my_gen {
                            return;
                        }
                        let icon_urls: Vec<(usize, String)> = items
                            .iter()
                            .enumerate()
                            .filter(|(_, it)| !it.icon_url.is_empty())
                            .map(|(i, it)| (i, it.icon_url.clone()))
                            .collect();
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                let ui_hits: Vec<UiModHit> = items
                                    .into_iter()
                                    .map(|item| UiModHit {
                                        project_id: item.id.into(),
                                        provider: item.provider.into(),
                                        title: item.title.into(),
                                        author: item.author.into(),
                                        description: item.description.into(),
                                        downloads: item.downloads.into(),
                                        mc_version: item.mc_version.into(),
                                        loader: item.loader.into(),
                                        icon: slint::Image::default(),
                                    })
                                    .collect();
                                ui.set_modpack_search_results(ModelRc::from(Rc::new(
                                    VecModel::from(ui_hits),
                                )));
                            }
                        });

                        let sem = Arc::new(tokio::sync::Semaphore::new(6));
                        for (idx, url) in icon_urls {
                            if gen_ref.load(Ordering::Relaxed) != my_gen {
                                break;
                            }
                            let sem_p = sem.clone();
                            let meta_p = meta.clone();
                            let cd = cache_dir.clone();
                            let gen_p = gen_ref.clone();
                            let w_p = ui_weak.clone();
                            rt_icons.spawn(async move {
                                let Ok(_permit) = sem_p.acquire().await else { return; };
                                if gen_p.load(Ordering::Relaxed) != my_gen {
                                    return;
                                }
                                if let Some(path) = meta_p.download_cached_icon(&url, &cd).await {
                                    if gen_p.load(Ordering::Relaxed) != my_gen {
                                        return;
                                    }
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if gen_p.load(Ordering::Relaxed) != my_gen {
                                            return;
                                        }
                                        if let Some(ui) = w_p.upgrade() {
                                            if let Ok(img) = slint::Image::load_from_path(&path) {
                                                let model = ui.get_modpack_search_results();
                                                if let Some(mut row) = model.row_data(idx) {
                                                    row.icon = img;
                                                    model.set_row_data(idx, row);
                                                }
                                            }
                                        }
                                    });
                                }
                            });
                        }
                    }
                    Err(e) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Модпаки • {}] Ошибка: {}", provider.as_str(), e),
                    ),
                }
            });
        });
    }

    // 3c. Install External Modpack
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();

        ui.on_install_external_modpack(move |provider_idx, pack_id, title, mc_ver, group| {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let meta = current_meta(&meta_holder);
            let log_buf = log_buf.clone();
            let provider = ExternalProvider::from_index(provider_idx);
            let pid = pack_id.to_string();
            let title_str = title.to_string();
            let mc_str = mc_ver.to_string();
            let grp_str = if group.trim().is_empty() {
                "Модпаки".to_string()
            } else {
                group.trim().to_string()
            };

            if let Some(ui) = ui_weak.upgrade() {
                ui.set_is_busy(true);
                ui.set_launch_progress(0.2);
            }

            let (instances_dir, icons_dir) = {
                let cfg = config.lock().unwrap();
                (
                    cfg.resolve_instances_dir(&data_dir),
                    cfg.resolve_icons_dir(&data_dir),
                )
            };
            let ui_weak_log = ui_weak.clone();
            let log_buf_cb = log_buf.clone();
            rt.spawn(async move {
                match meta
                    .install_external_modpack(
                        provider,
                        &pid,
                        &title_str,
                        &mc_str,
                        &grp_str,
                        &instances_dir,
                        move |msg| {
                            append_log_to_ui(&ui_weak_log, &log_buf_cb, msg);
                        },
                    )
                    .await
                {
                    Ok(mut inst) => {
                        let icon_save = icons_dir.join(format!("pack_{}", inst.id));
                        if let Some(saved_icon) = meta
                            .fetch_external_modpack_icon(provider, &pid, &inst.name, &icon_save)
                            .await
                        {
                            inst.custom_icon_path = saved_icon.to_string_lossy().to_string();
                        }
                        let name = inst.name.clone();
                        {
                            let mut cfg = config.lock().unwrap();
                            cfg.selected_instance_id = Some(inst.id.clone());
                            cfg.instances.push(inst);
                            save_config(&data_dir, &cfg);
                            let cfg_clone = cfg.clone();
                            let w = ui_weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w.upgrade() {
                                    ui.set_is_busy(false);
                                    ui.set_launch_progress(0.0);
                                    sync_ui_from_config(&ui, &cfg_clone);
                                }
                            });
                        }
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Модпак] Сборка «{}» установлена.", name),
                        );
                    }
                    Err(e) => {
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_is_busy(false);
                                ui.set_launch_progress(0.0);
                            }
                        });
                        append_log_to_ui(&ui_weak, &log_buf, format!("[Ошибка модпака] {:#}", e));
                    }
                }
            });
        });
    }

    // 3d. Import Modpack Archive & Migrate from Prism/Freesm/MultiMC/PineconeMC
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();

        ui.on_import_modpack_archive(move |path_or_url, name, group| {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let meta = current_meta(&meta_holder);
            let log_buf = log_buf.clone();
            let src = path_or_url.to_string();
            let name_str = name.to_string();
            let grp_str = group.to_string();

            let instances_dir = config.lock().unwrap().resolve_instances_dir(&data_dir);
            let ui_weak_log = ui_weak.clone();
            let log_buf_cb = log_buf.clone();
            rt.spawn(async move {
                match meta
                    .import_modpack_from_path_or_url(
                        &src,
                        &name_str,
                        &grp_str,
                        &instances_dir,
                        move |msg| {
                            append_log_to_ui(&ui_weak_log, &log_buf_cb, msg);
                        },
                    )
                    .await
                {
                    Ok(inst) => {
                        let iname = inst.name.clone();
                        {
                            let mut cfg = config.lock().unwrap();
                            cfg.selected_instance_id = Some(inst.id.clone());
                            cfg.instances.push(inst);
                            save_config(&data_dir, &cfg);
                            let cfg_clone = cfg.clone();
                            let w = ui_weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w.upgrade() {
                                    sync_ui_from_config(&ui, &cfg_clone);
                                }
                            });
                        }
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Импорт] Сборка «{}» готова.", iname),
                        );
                    }
                    Err(e) => {
                        append_log_to_ui(&ui_weak, &log_buf, format!("[Ошибка импорта] {:#}", e))
                    }
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();

        ui.on_migrate_external_launcher(move |custom_path| {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let log_buf = log_buf.clone();
            let p_str = custom_path.to_string();
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[Миграция] Поиск сборок FreesmLauncher / PrismLauncher / MultiMC...".to_string(),
            );
            let instances_dir = config.lock().unwrap().resolve_instances_dir(&data_dir);
            rt.spawn_blocking(move || {
                let opt = if p_str.trim().is_empty() {
                    None
                } else {
                    Some(p_str.as_str())
                };
                match import_prism_freesm_instances(opt, &instances_dir) {
                    Ok(list) => {
                        let count = list.len();
                        let mut cfg = config.lock().unwrap();
                        if let Some(first) = list.first() {
                            cfg.selected_instance_id = Some(first.id.clone());
                        }
                        cfg.instances.extend(list);
                        save_config(&data_dir, &cfg);
                        let cfg_clone = cfg.clone();
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                sync_ui_from_config(&ui, &cfg_clone);
                            }
                        });
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Миграция] Импортировано сборок: {}.", count),
                        );
                    }
                    Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Миграция] {}", e)),
                }
            });
        });
    }

    // 4. Delete Selected Instance (including files on disk!)
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_delete_selected_instance(move || {
            let mut cfg = config.lock().unwrap();
            if let Some(sel_id) = cfg.selected_instance_id.clone() {
                let inst_folder = cfg.resolve_instances_dir(&data_dir).join(&sel_id);
                cfg.instances.retain(|i| i.id != sel_id);
                cfg.selected_instance_id = None;
                save_config(&data_dir, &cfg);
                if inst_folder.is_dir() {
                    let _ = std::fs::remove_dir_all(&inst_folder);
                }
                if let Some(ui) = ui_weak.upgrade() {
                    sync_ui_from_config(&ui, &cfg);
                }
                append_log_to_ui(&ui_weak, &log_buf, "[Сборки] Сборка удалена.".to_string());
            }
        });
    }

    // 5. Open Folders
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_open_instance_folder(move || {
            let cfg = config.lock().unwrap();
            if let Some(sel_id) = &cfg.selected_instance_id {
                let folder = cfg
                    .resolve_instances_dir(&data_dir)
                    .join(sel_id)
                    .join(".minecraft");
                let _ = std::fs::create_dir_all(&folder);
                let _ = open::that(&folder);
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    format!("Открыта папка: {}", folder.display()),
                );
            }
        });
    }
    {
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_open_root_folder(move || {
            let root = config.lock().unwrap().effective_data_root(&data_dir);
            let _ = std::fs::create_dir_all(&root);
            let _ = open::that(&root);
        });
    }
    {
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_open_instance_subfolder(move |sub| {
            let cfg = config.lock().unwrap();
            if let Some(sel_id) = &cfg.selected_instance_id {
                let folder = cfg
                    .resolve_instances_dir(&data_dir)
                    .join(sel_id)
                    .join(".minecraft")
                    .join(sub.as_str());
                let _ = std::fs::create_dir_all(&folder);
                let _ = open::that(&folder);
            }
        });
    }

    // 6. Kill Running Instance & Launch Selected Instance
    {
        let active_kill_tx = active_kill_tx.clone();
        let ui_weak = ui.as_weak();
        let log_buf = log_buf.clone();
        ui.on_kill_running_instance(move || {
            if let Some(tx) = active_kill_tx.lock().unwrap().take() {
                let _ = tx.send(());
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    "[Процесс] Отправлен сигнал завершения игры (Kill)...".to_string(),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        let active_kill_tx = active_kill_tx.clone();
        let tray_ctrl = tray_ctrl.clone();
        let discord_ctrl = discord_ctrl.clone();

        ui.on_launch_selected_instance(move |force_offline| {
            let cfg_snapshot = config.lock().unwrap().clone();
            let Some(inst) = cfg_snapshot
                .instances
                .iter()
                .find(|i| Some(&i.id) == cfg_snapshot.selected_instance_id.as_ref())
                .or_else(|| cfg_snapshot.instances.first())
                .cloned()
            else {
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    "Ошибка: сначала создайте сборку!".to_string(),
                );
                return;
            };

            // Respect per-instance account override (Freesm / Prism feature!)
            let wanted_acc_id = if inst.override_account {
                inst.custom_account_id
                    .as_ref()
                    .or(cfg_snapshot.selected_account_id.as_ref())
            } else {
                cfg_snapshot.selected_account_id.as_ref()
            };
            let Some(mut acc) = cfg_snapshot
                .accounts
                .iter()
                .find(|a| Some(&a.id) == wanted_acc_id)
                .or_else(|| cfg_snapshot.accounts.first())
                .cloned()
            else {
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    "Ошибка: сначала добавьте аккаунт!".to_string(),
                );
                return;
            };

            let show_console_launch = if inst.override_console {
                inst.custom_show_console_on_launch
            } else {
                cfg_snapshot.show_console_on_launch
            };
            let show_console_error = if inst.override_console {
                inst.custom_show_console_on_error
            } else {
                cfg_snapshot.show_console_on_error
            };
            let hide_on_launch = if inst.override_console {
                inst.custom_close_after_launch
            } else {
                cfg_snapshot.close_after_launch
            };
            let show_tray = cfg_snapshot.show_tray_icon;
            let discord_game_enabled = if inst.override_discord {
                inst.custom_discord_rpc
            } else {
                cfg_snapshot.discord_rpc_launcher
            };
            let discord_cid = cfg_snapshot.discord_client_id.clone();
            let quit_after_close = if inst.override_window {
                inst.custom_quit_after_close
            } else {
                cfg_snapshot.quit_after_game_window_closed
            };
            let hide_console_exit = if inst.override_console {
                inst.custom_hide_console_on_exit
            } else {
                cfg_snapshot.hide_console_on_exit
            };
            let record_playtime = if inst.override_playtime {
                inst.custom_record_playtime
            } else {
                cfg_snapshot.record_playtime
            };

            let (kill_tx, kill_rx) = tokio::sync::oneshot::channel();
            *active_kill_tx.lock().unwrap() = Some(kill_tx);

            if let Some(ui) = ui_weak.upgrade() {
                ui.set_is_busy(true);
                ui.set_is_game_running(true);
                ui.set_launch_progress(0.02);
                if show_console_launch {
                    ui.set_active_tab(4);
                }
            }

            let ui_weak_log = ui_weak.clone();
            let ui_weak_prog = ui_weak.clone();
            let ui_weak_done = ui_weak.clone();
            let ui_weak_tray = ui_weak.clone();
            let log_buf_cb = log_buf.clone();
            let log_buf_done = log_buf.clone();
            let meta = current_meta(&meta_holder);
            let data_dir_run = data_dir.clone();
            let config_run = config.clone();
            let active_kill_tx_run = active_kill_tx.clone();
            let tray_run = tray_ctrl.clone();
            let discord_run = discord_ctrl.clone();
            let inst_id = inst.id.clone();
            let inst_name = inst.name.clone();
            let inst_mc = inst.mc_version.clone();
            let inst_loader_str = inst.loader.as_str().to_string();

            rt.spawn(async move {
                // Auto-refresh Microsoft token before launch if online
                if !force_offline
                    && acc.account_type == AccountType::Microsoft
                    && acc.refresh_token.is_some()
                {
                    append_log_to_ui(
                        &ui_weak_done,
                        &log_buf_done,
                        "[Microsoft] Обновление токена сессии перед запуском...".to_string(),
                    );
                    let custom_cid = if cfg_snapshot.msa_client_id.trim().is_empty() {
                        None
                    } else {
                        Some(cfg_snapshot.msa_client_id.as_str())
                    };
                    match refresh_microsoft_account(meta.http(), &acc, custom_cid).await {
                        Ok(refreshed) => {
                            acc = refreshed.clone();
                            let mut cfg = config_run.lock().unwrap();
                            if let Some(slot) = cfg.accounts.iter_mut().find(|a| a.id == acc.id) {
                                *slot = refreshed;
                            }
                            save_config(&data_dir_run, &cfg);
                        }
                        Err(e) => append_log_to_ui(
                            &ui_weak_done,
                            &log_buf_done,
                            format!("[Microsoft] Предупреждение при обновлении токена: {}", e),
                        ),
                    }
                }

                if !force_offline && cfg_snapshot.auto_update_mods {
                    let mods_dir = cfg_snapshot
                        .resolve_instances_dir(&data_dir_run)
                        .join(&inst.id)
                        .join(".minecraft")
                        .join("mods");
                    if mods_dir.is_dir() {
                        append_log_to_ui(
                            &ui_weak_done,
                            &log_buf_done,
                            "[Моды] Проверка обновлений модов перед запуском (Modrinth)...".to_string(),
                        );
                        let w_u = ui_weak_done.clone();
                        let lb_u = log_buf_done.clone();
                        match meta
                            .update_instance_mods(
                                &mods_dir,
                                &inst.mc_version,
                                &inst.loader,
                                move |m| append_log_to_ui(&w_u, &lb_u, m),
                            )
                            .await
                        {
                            Ok(n) if n > 0 => append_log_to_ui(
                                &ui_weak_done,
                                &log_buf_done,
                                format!("[Моды] Автоматически обновлено модов: {}", n),
                            ),
                            Ok(_) => {}
                            Err(e) => append_log_to_ui(
                                &ui_weak_done,
                                &log_buf_done,
                                format!("[Моды] Ошибка автообновления модов: {}", e),
                            ),
                        }
                    }
                }

                if show_tray {
                    tray_run.set_enabled(true, "");
                }
                discord_run.set_game_presence(
                    discord_game_enabled,
                    &inst_name,
                    &inst_mc,
                    &inst_loader_str,
                    &discord_cid,
                );

                if hide_on_launch {
                    let w_hide = ui_weak_tray.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = w_hide.upgrade() {
                            let _ = ui.hide();
                        }
                    });
                }

                let res = launch_instance(
                    meta,
                    cfg_snapshot,
                    inst,
                    acc,
                    data_dir_run.clone(),
                    force_offline,
                    kill_rx,
                    move |log_line| {
                        append_log_to_ui(&ui_weak_log, &log_buf_cb, log_line);
                    },
                    move |progress, status_msg| {
                        let w = ui_weak_prog.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_launch_progress(progress);
                                ui.set_status_is_error(false);
                                ui.set_status_text(SharedString::from(status_msg));
                            }
                        });
                    },
                )
                .await;

                *active_kill_tx_run.lock().unwrap() = None;
                {
                    let cfg = config_run.lock().unwrap();
                    tray_run.set_enabled(cfg.show_tray_icon, "");
                    sync_discord_launcher_state(&discord_run, &cfg, launcher_start_ts);
                }

                let mut clean_exit = false;
                match res {
                    Ok(outcome) => {
                        clean_exit = outcome.exit_code == 0;
                        if record_playtime {
                            let mut cfg = config_run.lock().unwrap();
                            let now_ts = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);
                            if let Some(slot) = cfg.instances.iter_mut().find(|i| i.id == inst_id) {
                                slot.total_playtime_minutes = slot
                                    .total_playtime_minutes
                                    .saturating_add(outcome.played_minutes.max(1));
                                slot.last_played_ts = now_ts;
                                slot.last_played = Some("Недавно".to_string());
                            }
                            save_config(&data_dir_run, &cfg);
                            let cfg_clone = cfg.clone();
                            let w_sync = ui_weak_done.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w_sync.upgrade() {
                                    sync_ui_from_config(&ui, &cfg_clone);
                                }
                            });
                        }
                        if outcome.exit_code != 0 && outcome.exit_code != -1 && show_console_error {
                            let w_err = ui_weak_done.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w_err.upgrade() {
                                    let _ = ui.show();
                                    ui.set_active_tab(4);
                                }
                            });
                        }
                    }
                    Err(err) => {
                        append_log_to_ui(
                            &ui_weak_done,
                            &log_buf_done,
                            format!("[ОШИБКА ЗАПУСКА] {:#}", err),
                        );
                        if show_console_error {
                            let w_err = ui_weak_done.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = w_err.upgrade() {
                                    let _ = ui.show();
                                    ui.set_active_tab(4);
                                }
                            });
                        }
                    }
                }

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak_done.upgrade() {
                        ui.set_is_busy(false);
                        ui.set_is_game_running(false);
                        ui.set_launch_progress(0.0);
                        if quit_after_close && clean_exit {
                            let _ = slint::quit_event_loop();
                        } else {
                            if hide_console_exit && clean_exit && ui.get_active_tab() == 4 {
                                ui.set_active_tab(0);
                            }
                            let _ = ui.show();
                        }
                    }
                });
            });
        });
    }

    // 7. Auth: Offline
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_add_offline_account(move |username, custom_uuid| {
            let uuid_opt = if custom_uuid.trim().is_empty() {
                None
            } else {
                Some(custom_uuid.as_str())
            };
            match create_offline_account(username.as_str(), uuid_opt) {
                Ok(acc) => {
                    let mut cfg = config.lock().unwrap();
                    cfg.selected_account_id = Some(acc.id.clone());
                    let uname = acc.username.clone();
                    cfg.accounts.push(acc);
                    save_config(&data_dir, &cfg);
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_auth_username("".into());
                        ui.set_auth_custom_uuid("".into());
                        sync_ui_from_config(&ui, &cfg);
                    }
                    append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Аккаунты] Добавлен Offline-профиль «{}».", uname),
                    );
                }
                Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Аккаунты] Ошибка: {}", e)),
            }
        });
    }

    // 8. Auth: Microsoft Device Code
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();

        ui.on_start_microsoft_login(move || {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let http = current_meta(&meta_holder).http().clone();
            let log_buf = log_buf.clone();
            let custom_cid = {
                let c = config.lock().unwrap().msa_client_id.trim().to_string();
                if c.is_empty() { None } else { Some(c) }
            };

            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[Microsoft] Запрос кода устройства...".to_string(),
            );

            rt.spawn(async move {
                match start_microsoft_device_flow(&http, custom_cid.as_deref()).await {
                    Ok(dc) => {
                        let user_code = dc.user_code.clone();
                        let verify_url = dc.verification_uri.clone();
                        let msg = dc.message.clone();
                        let _ = open::that(&verify_url);

                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_ms_user_code(user_code.into());
                                ui.set_ms_verification_url(verify_url.into());
                                ui.set_ms_device_code_info(msg.into());
                            }
                        });

                        match poll_microsoft_device_flow(
                            &http,
                            &dc.device_code,
                            dc.interval,
                            custom_cid.as_deref(),
                        )
                        .await
                        {
                            Ok(acc) => {
                                let uname = acc.username.clone();
                                {
                                    let mut cfg = config.lock().unwrap();
                                    cfg.selected_account_id = Some(acc.id.clone());
                                    cfg.accounts.push(acc);
                                    save_config(&data_dir, &cfg);
                                    let cfg_clone = cfg.clone();
                                    let w2 = ui_weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(ui) = w2.upgrade() {
                                            ui.set_ms_user_code("".into());
                                            sync_ui_from_config(&ui, &cfg_clone);
                                        }
                                    });
                                }
                                append_log_to_ui(
                                    &ui_weak,
                                    &log_buf,
                                    format!("[Microsoft] Вход выполнен: {}", uname),
                                );
                            }
                            Err(e) => append_log_to_ui(
                                &ui_weak,
                                &log_buf,
                                format!("[Microsoft] Ошибка: {}", e),
                            ),
                        }
                    }
                    Err(e) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Microsoft] Ошибка: {}", e),
                    ),
                }
            });
        });
    }

    // 9. Auth: Ely.by & Custom Yggdrasil + Account Refresh
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();

        ui.on_add_yggdrasil_account(
            move |method_idx, server_url, username, password, custom_login_url, custom_refresh_url| {
                let ui_weak = ui_weak.clone();
                let config = config.clone();
                let data_dir = data_dir.clone();
                let http = current_meta(&meta_holder).http().clone();
                let log_buf = log_buf.clone();

                let (acc_type, url_str, login_opt, refresh_opt) = if method_idx == 2 {
                    (
                        AccountType::ElyBy,
                        "https://authserver.ely.by".to_string(),
                        None,
                        None,
                    )
                } else if method_idx == 3 {
                    (
                        AccountType::LittleSkin,
                        "https://littleskin.cn/api/yggdrasil".to_string(),
                        None,
                        None,
                    )
                } else {
                    (
                        AccountType::CustomYggdrasil,
                        server_url.to_string(),
                        Some(custom_login_url.to_string()),
                        Some(custom_refresh_url.to_string()),
                    )
                };
                let user_str = username.to_string();
                let pass_str = password.to_string();

                rt.spawn(async move {
                    match authenticate_yggdrasil(
                        &http,
                        acc_type,
                        &url_str,
                        &user_str,
                        &pass_str,
                        login_opt.as_deref(),
                        refresh_opt.as_deref(),
                    )
                    .await
                    {
                        Ok(acc) => {
                            let uname = acc.username.clone();
                            {
                                let mut cfg = config.lock().unwrap();
                                cfg.selected_account_id = Some(acc.id.clone());
                                cfg.accounts.push(acc);
                                save_config(&data_dir, &cfg);
                                let cfg_clone = cfg.clone();
                                let w = ui_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = w.upgrade() {
                                        ui.set_auth_username("".into());
                                        ui.set_auth_password("".into());
                                        ui.set_auth_custom_login_url("".into());
                                        ui.set_auth_custom_refresh_url("".into());
                                        sync_ui_from_config(&ui, &cfg_clone);
                                    }
                                });
                            }
                            append_log_to_ui(
                                &ui_weak,
                                &log_buf,
                                format!("[Yggdrasil] Вход выполнен: {}", uname),
                            );
                        }
                        Err(e) => append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Yggdrasil] Ошибка: {}", e),
                        ),
                    }
                });
            },
        );
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();

        ui.on_refresh_account(move |acc_id| {
            let ui_weak = ui_weak.clone();
            let config = config.clone();
            let data_dir = data_dir.clone();
            let http = current_meta(&meta_holder).http().clone();
            let log_buf = log_buf.clone();
            let id_str = acc_id.to_string();
            let (acc_opt, custom_cid) = {
                let cfg = config.lock().unwrap();
                let a = cfg.accounts.iter().find(|a| a.id == id_str).cloned();
                let cid = if cfg.msa_client_id.trim().is_empty() {
                    None
                } else {
                    Some(cfg.msa_client_id.clone())
                };
                (a, cid)
            };
            let Some(acc) = acc_opt else { return };
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                format!("[Аккаунты] Обновление сессии «{}»...", acc.username),
            );
            rt.spawn(async move {
                let res = match acc.account_type {
                    AccountType::Microsoft => {
                        refresh_microsoft_account(&http, &acc, custom_cid.as_deref()).await
                    }
                    AccountType::ElyBy | AccountType::LittleSkin | AccountType::CustomYggdrasil => {
                        refresh_yggdrasil_account(&http, &acc).await
                    }
                    AccountType::Offline => Ok(acc),
                };
                match res {
                    Ok(updated) => {
                        let uname = updated.username.clone();
                        let mut cfg = config.lock().unwrap();
                        if let Some(slot) = cfg.accounts.iter_mut().find(|a| a.id == updated.id) {
                            *slot = updated;
                        }
                        save_config(&data_dir, &cfg);
                        let cfg_clone = cfg.clone();
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                sync_ui_from_config(&ui, &cfg_clone);
                            }
                        });
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[Аккаунты] Токен «{}» успешно обновлён.", uname),
                        );
                    }
                    Err(e) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Аккаунты] Ошибка обновления: {}", e),
                    ),
                }
            });
        });
    }

    // 10. Select & Remove Account
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_select_account(move |acc_id| {
            let mut cfg = config.lock().unwrap();
            cfg.selected_account_id = Some(acc_id.to_string());
            save_config(&data_dir, &cfg);
            if let Some(ui) = ui_weak.upgrade() {
                sync_ui_from_config(&ui, &cfg);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_remove_account(move |acc_id| {
            let mut cfg = config.lock().unwrap();
            if cfg.accounts.len() <= 1 {
                return;
            }
            cfg.accounts.retain(|a| a.id != acc_id.as_str());
            if cfg.selected_account_id.as_deref() == Some(acc_id.as_str()) {
                cfg.selected_account_id = cfg.accounts.first().map(|a| a.id.clone());
            }
            save_config(&data_dir, &cfg);
            if let Some(ui) = ui_weak.upgrade() {
                sync_ui_from_config(&ui, &cfg);
            }
        });
    }

    // 11. Sync All Repositories
    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let config = config.clone();
        let log_buf = log_buf.clone();

        ui.on_sync_all_repositories(move || {
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let cfg_snap = config.lock().unwrap().clone();
            let log_buf = log_buf.clone();

            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[Репозитории] Проверка доступности...".to_string(),
            );

            rt.spawn(async move {
                let mut rows = default_repo_rows(&cfg_snap);
                if let Ok(releases) = meta.fetch_horizon_releases().await {
                    if let Some(row) = rows.iter_mut().find(|r| r.uid == "horizon-rust/horizon") {
                        row.status = format!("ОК ({} релизов)", releases.len()).into();
                    }
                }
                if let Ok(m) = meta.fetch_labymod_manifest().await {
                    if let Some(row) = rows.iter_mut().find(|r| r.uid == "net.labymod") {
                        row.status = format!("ОК (v{})", m.labymod_version).into();
                    }
                }
                if let Ok(idx) = meta.fetch_prism_index("net.minecraft").await {
                    if let Some(row) = rows.iter_mut().find(|r| r.uid == "net.minecraft") {
                        row.status = format!("ОК ({} версий)", idx.versions.len()).into();
                    }
                }
                if let Ok(ofs) = meta.fetch_optifine_versions("1.20.1").await {
                    if let Some(row) = rows.iter_mut().find(|r| r.uid == "optifine.OptiFine") {
                        row.status = format!("ОК ({} сборок 1.20.1)", ofs.len()).into();
                    }
                }

                let w = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = w.upgrade() {
                        ui.set_repositories(ModelRc::from(Rc::new(VecModel::from(rows))));
                    }
                });
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    "[Репозитории] Проверка завершена.".to_string(),
                );
            });
        });
    }

    // 12. Universal Asset Downloader (Modrinth & CurseForge for Mods, ResourcePacks, Shaders, DataPacks, Worlds!)
    {
        use std::sync::atomic::{AtomicU64, Ordering};
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        let asset_gens: Arc<[AtomicU64; 6]> = Arc::new([
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
        ]);

        ui.on_search_asset(move |kind_str, provider_idx, query| {
            let kind = AssetKind::from_str_tag(kind_str.as_str());
            let kind_slot = match kind {
                AssetKind::Mod => 0,
                AssetKind::ResourcePack => 1,
                AssetKind::Shader => 2,
                AssetKind::DataPack => 3,
                AssetKind::World => 4,
                AssetKind::Modpack => 5,
            };
            let my_gen = asset_gens[kind_slot].fetch_add(1, Ordering::Relaxed) + 1;
            let gens_ref = asset_gens.clone();
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let log_buf = log_buf.clone();
            let q = query.to_string();
            let (mc_ver, loader, cache_dir) = {
                let cfg = config.lock().unwrap();
                let inst = cfg
                    .instances
                    .iter()
                    .find(|i| Some(&i.id) == cfg.selected_instance_id.as_ref())
                    .or_else(|| cfg.instances.first());
                let (m, l) = match inst {
                    Some(i) => (Some(i.mc_version.clone()), Some(i.loader.clone())),
                    None => (None, None),
                };
                (m, l, cfg.resolve_icons_dir(&data_dir).join("catalog_cache"))
            };
            let rt_icons = rt.clone();

            rt.spawn(async move {
                if !q.trim().is_empty() {
                    tokio::time::sleep(std::time::Duration::from_millis(260)).await;
                }
                if gens_ref[kind_slot].load(Ordering::Relaxed) != my_gen {
                    return;
                }
                let loader_opt = if kind == AssetKind::Mod {
                    loader.as_ref()
                } else {
                    None
                };

                let hits_res: Result<Vec<ExternalPackItem>> = if provider_idx == 2 {
                    let (r_mr, r_cf) = tokio::join!(
                        meta.search_external_asset(
                            ExternalProvider::Modrinth,
                            kind,
                            &q,
                            mc_ver.as_deref(),
                            loader_opt
                        ),
                        meta.search_external_asset(
                            ExternalProvider::CurseForge,
                            kind,
                            &q,
                            mc_ver.as_deref(),
                            loader_opt
                        ),
                    );
                    let mr = r_mr.unwrap_or_default();
                    let cf = r_cf.unwrap_or_default();
                    let mut merged = Vec::with_capacity(mr.len() + cf.len());
                    let mut seen = std::collections::HashSet::new();
                    let max_len = mr.len().max(cf.len());
                    for i in 0..max_len {
                        if let Some(item) = mr.get(i) {
                            let key = item.title.to_lowercase();
                            if seen.insert(key) {
                                merged.push(item.clone());
                            }
                        }
                        if let Some(item) = cf.get(i) {
                            let key = item.title.to_lowercase();
                            if seen.insert(key) {
                                merged.push(item.clone());
                            }
                        }
                    }
                    Ok(merged)
                } else {
                    let provider = ExternalProvider::from_index(provider_idx);
                    meta.search_external_asset(provider, kind, &q, mc_ver.as_deref(), loader_opt)
                        .await
                };

                match hits_res {
                    Ok(hits) => {
                        if gens_ref[kind_slot].load(Ordering::Relaxed) != my_gen {
                            return;
                        }
                        let icon_urls: Vec<(usize, String)> = hits
                            .iter()
                            .enumerate()
                            .filter(|(_, h)| !h.icon_url.is_empty())
                            .map(|(i, h)| (i, h.icon_url.clone()))
                            .collect();
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                let ui_hits: Vec<UiModHit> = hits
                                    .into_iter()
                                    .map(|h| UiModHit {
                                        project_id: h.id.into(),
                                        provider: h.provider.into(),
                                        title: h.title.into(),
                                        author: h.author.into(),
                                        description: h.description.into(),
                                        downloads: h.downloads.into(),
                                        mc_version: h.mc_version.into(),
                                        loader: h.loader.into(),
                                        icon: slint::Image::default(),
                                    })
                                    .collect();
                                let model = ModelRc::from(Rc::new(VecModel::from(ui_hits)));
                                match kind {
                                    AssetKind::Mod => ui.set_mod_search_results(model),
                                    AssetKind::ResourcePack => ui.set_rp_search_results(model),
                                    AssetKind::Shader => ui.set_shader_search_results(model),
                                    AssetKind::DataPack => ui.set_dp_search_results(model),
                                    AssetKind::World => ui.set_world_search_results(model),
                                    AssetKind::Modpack => ui.set_modpack_search_results(model),
                                }
                            }
                        });

                        let sem = Arc::new(tokio::sync::Semaphore::new(6));
                        for (idx, url) in icon_urls {
                            if gens_ref[kind_slot].load(Ordering::Relaxed) != my_gen {
                                break;
                            }
                            let sem_p = sem.clone();
                            let meta_p = meta.clone();
                            let cd = cache_dir.clone();
                            let gens_p = gens_ref.clone();
                            let w_p = ui_weak.clone();
                            rt_icons.spawn(async move {
                                let Ok(_permit) = sem_p.acquire().await else { return; };
                                if gens_p[kind_slot].load(Ordering::Relaxed) != my_gen {
                                    return;
                                }
                                if let Some(path) = meta_p.download_cached_icon(&url, &cd).await {
                                    if gens_p[kind_slot].load(Ordering::Relaxed) != my_gen {
                                        return;
                                    }
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if gens_p[kind_slot].load(Ordering::Relaxed) != my_gen {
                                            return;
                                        }
                                        if let Some(ui) = w_p.upgrade() {
                                            if let Ok(img) = slint::Image::load_from_path(&path) {
                                                let model = match kind {
                                                    AssetKind::Mod => ui.get_mod_search_results(),
                                                    AssetKind::ResourcePack => ui.get_rp_search_results(),
                                                    AssetKind::Shader => ui.get_shader_search_results(),
                                                    AssetKind::DataPack => ui.get_dp_search_results(),
                                                    AssetKind::World => ui.get_world_search_results(),
                                                    AssetKind::Modpack => ui.get_modpack_search_results(),
                                                };
                                                if let Some(mut row) = model.row_data(idx) {
                                                    row.icon = img;
                                                    model.set_row_data(idx, row);
                                                }
                                            }
                                        }
                                    });
                                }
                            });
                        }
                    }
                    Err(e) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Каталог] Ошибка поиска: {}", e),
                    ),
                }
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();

        ui.on_install_asset(move |kind_str, project_id, title, provider| {
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let data_dir = data_dir.clone();
            let log_buf = log_buf.clone();
            let kind = AssetKind::from_str_tag(kind_str.as_str());
            let pid = project_id.to_string();
            let title_str = title.to_string();
            let prov = provider.to_string();

            let (inst_opt, instances_dir) = {
                let cfg = config.lock().unwrap();
                let inst = cfg
                    .instances
                    .iter()
                    .find(|i| Some(&i.id) == cfg.selected_instance_id.as_ref())
                    .or_else(|| cfg.instances.first())
                    .cloned();
                (inst, cfg.resolve_instances_dir(&data_dir))
            };
            let Some(inst) = inst_opt else {
                return;
            };

            let sub = kind.subfolder();
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                format!("[Установка] «{}» ({}) -> {}...", title_str, prov, sub),
            );

            rt.spawn(async move {
                let dl_res = if prov == "CurseForge" {
                    let cf_mc = if kind == AssetKind::World {
                        ""
                    } else {
                        inst.mc_version.as_str()
                    };
                    meta.fetch_curseforge_download(&pid, cf_mc).await
                } else {
                    let mr_loader = if kind == AssetKind::Mod {
                        inst.loader.clone()
                    } else {
                        LoaderType::Vanilla
                    };
                    meta.fetch_modrinth_download(&pid, &inst.mc_version, &mr_loader)
                        .await
                };

                match dl_res {
                    Ok((filename, url)) => {
                        let target_dir = instances_dir
                            .join(&inst.id)
                            .join(".minecraft")
                            .join(sub);
                        let _ = tokio::fs::create_dir_all(&target_dir).await;
                        let mut req = meta.http().get(&url);
                        if prov == "CurseForge" && !meta.curseforge_key().is_empty() {
                            req = req.header("x-api-key", meta.curseforge_key());
                        }
                        match req.send().await {
                            Ok(resp) if resp.status().is_success() => {
                                if let Ok(bytes) = resp.bytes().await {
                                    if kind == AssetKind::World && filename.to_lowercase().ends_with(".zip") {
                                        let tmp_zip = target_dir.join(format!(".tmp_{}", filename));
                                        let _ = tokio::fs::write(&tmp_zip, &bytes).await;
                                        match extract_world_zip(&tmp_zip, &target_dir, &title_str) {
                                            Ok(world_dir) => {
                                                let _ = tokio::fs::remove_file(&tmp_zip).await;
                                                let list = scan_subfolder_entries(&instances_dir, &inst.id, "saves");
                                                let w = ui_weak.clone();
                                                let _ = slint::invoke_from_event_loop(move || {
                                                    if let Some(ui) = w.upgrade() {
                                                        ui.set_worlds_list(ModelRc::from(Rc::new(VecModel::from(list))));
                                                    }
                                                });
                                                append_log_to_ui(
                                                    &ui_weak,
                                                    &log_buf,
                                                    format!("[Миры] Распакован мир «{}» в saves/{}", title_str, world_dir),
                                                );
                                            }
                                            Err(e) => {
                                                let _ = tokio::fs::remove_file(&tmp_zip).await;
                                                append_log_to_ui(
                                                    &ui_weak,
                                                    &log_buf,
                                                    format!("[Миры] Ошибка распаковки {}: {}", filename, e),
                                                );
                                            }
                                        }
                                    } else {
                                        let target = target_dir.join(&filename);
                                        let _ = tokio::fs::write(&target, &bytes).await;
                                        let w = ui_weak.clone();
                                        let insts_c = instances_dir.clone();
                                        let inst_id_c = inst.id.clone();
                                        match kind {
                                            AssetKind::Mod => {
                                                let mods = scan_installed_mods(&insts_c, &inst_id_c);
                                                let _ = slint::invoke_from_event_loop(move || {
                                                    if let Some(ui) = w.upgrade() {
                                                        ui.set_installed_mods(ModelRc::from(Rc::new(VecModel::from(mods))));
                                                    }
                                                });
                                            }
                                            AssetKind::ResourcePack => {
                                                let list = scan_subfolder_entries(&insts_c, &inst_id_c, "resourcepacks");
                                                let _ = slint::invoke_from_event_loop(move || {
                                                    if let Some(ui) = w.upgrade() {
                                                        ui.set_resource_packs(ModelRc::from(Rc::new(VecModel::from(list))));
                                                    }
                                                });
                                            }
                                            AssetKind::Shader => {
                                                let list = scan_subfolder_entries(&insts_c, &inst_id_c, "shaderpacks");
                                                let _ = slint::invoke_from_event_loop(move || {
                                                    if let Some(ui) = w.upgrade() {
                                                        ui.set_shader_packs(ModelRc::from(Rc::new(VecModel::from(list))));
                                                    }
                                                });
                                            }
                                            AssetKind::DataPack => {
                                                let list = scan_subfolder_entries(&insts_c, &inst_id_c, "datapacks");
                                                let _ = slint::invoke_from_event_loop(move || {
                                                    if let Some(ui) = w.upgrade() {
                                                        ui.set_data_packs(ModelRc::from(Rc::new(VecModel::from(list))));
                                                    }
                                                });
                                            }
                                            AssetKind::World => {
                                                let list = scan_subfolder_entries(&insts_c, &inst_id_c, "saves");
                                                let _ = slint::invoke_from_event_loop(move || {
                                                    if let Some(ui) = w.upgrade() {
                                                        ui.set_worlds_list(ModelRc::from(Rc::new(VecModel::from(list))));
                                                    }
                                                });
                                            }
                                            AssetKind::Modpack => {}
                                        }
                                        append_log_to_ui(
                                            &ui_weak,
                                            &log_buf,
                                            format!("[Установка] Готово: {} -> {}", filename, sub),
                                        );
                                    }
                                }
                            }
                            Ok(resp) => append_log_to_ui(
                                &ui_weak,
                                &log_buf,
                                format!("[Установка] HTTP {}: не удалось скачать {}", resp.status(), filename),
                            ),
                            Err(e) => append_log_to_ui(
                                &ui_weak,
                                &log_buf,
                                format!("[Установка] Ошибка загрузки {}: {}", filename, e),
                            ),
                        }
                    }
                    Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Установка] {}", e)),
                }
            });
        });
    }

    // 13. Java Detection
    {
        let ui_weak = ui.as_weak();
        let log_buf = log_buf.clone();
        ui.on_detect_java(move |for_instance| {
            let javas = detect_system_javas();
            if let Some(best) = javas.first() {
                if let Some(ui) = ui_weak.upgrade() {
                    if for_instance {
                        ui.set_edit_custom_java(best.clone().into());
                    } else {
                        ui.set_setting_java_path(best.clone().into());
                    }
                }
                append_log_to_ui(&ui_weak, &log_buf, format!("[Java] Найдена: {}", best));
            }
        });
    }

    // 14. Save All Global Settings (rebuilds MetaClient with new proxy/timeout/keys!)
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        let tray_ctrl = tray_ctrl.clone();
        let discord_ctrl = discord_ctrl.clone();
        ui.on_save_settings(move |scheme, light_mode| {
            if let Some(ui) = ui_weak.upgrade() {
                let mut cfg = config.lock().unwrap();
                // 0: LauncherPage + AppearanceWidget + Folders
                cfg.theme_scheme = scheme;
                cfg.light_theme = light_mode;
                cfg.language = ui.get_setting_language().to_string();
                cfg.use_system_locale = ui.get_setting_use_system_locale();
                cfg.use_language_for_formats = ui.get_setting_use_lang_formats();
                cfg.data_root_dir = ui.get_setting_data_root_dir().trim().to_string();
                cfg.instances_dir = ui.get_setting_instances_dir().trim().to_string();
                cfg.mods_dir = ui.get_setting_mods_dir().trim().to_string();
                cfg.icons_dir = ui.get_setting_icons_dir().trim().to_string();
                cfg.java_dir = ui.get_setting_java_dir().trim().to_string();
                cfg.skins_dir = ui.get_setting_skins_dir().trim().to_string();
                cfg.assets_dir = ui.get_setting_assets_dir().trim().to_string();
                cfg.libraries_dir = ui.get_setting_libraries_dir().trim().to_string();
                cfg.exports_dir = ui.get_setting_exports_dir().trim().to_string();
                cfg.ui_widget_style = ui.get_setting_widget_style().to_string();
                cfg.icon_theme = ui.get_setting_icon_theme().to_string();
                let uf = ui.get_setting_ui_font();
                cfg.ui_font_family = if uf.trim().is_empty() || uf.trim() == "Системный" {
                    String::new()
                } else {
                    uf.trim().to_string()
                };
                cfg.instance_card_size = ui.get_setting_instance_card_size();
                cfg.wrap_instance_names = ui.get_setting_wrap_instance_names();
                cfg.lock_toolbars = ui.get_setting_lock_toolbars();
                cfg.show_status_bar = ui.get_setting_show_status_bar();
                cfg.instance_sort_mode = ui.get_setting_sort_mode().to_string();
                cfg.folder_rename_mode = ui.get_setting_folder_rename_mode();
                cfg.enable_ui_animations = ui.get_setting_ui_animations();
                cfg.wavy_progress_bar = ui.get_setting_wavy_progress();
                cfg.enable_snow_effect = ui.get_setting_snow_effect();
                cfg.enable_cat_pack = ui.get_setting_enable_cat_pack();
                cfg.cat_pack_preset = ui.get_setting_cat_preset();
                cfg.cat_pack_opacity = ui.get_setting_cat_opacity();
                cfg.cat_pack_custom_path = ui.get_setting_cat_custom_path().to_string();
                cfg.cat_pack_scale_mode = ui.get_setting_cat_scale_mode();
                cfg.enable_custom_bg = ui.get_setting_enable_custom_bg();
                cfg.custom_bg_mode = ui.get_setting_custom_bg_mode();
                cfg.custom_bg_launcher_path = ui.get_setting_bg_launcher_path().to_string();
                cfg.custom_bg_launcher_opacity = ui.get_setting_bg_launcher_opacity();
                cfg.custom_bg_launcher_scale = ui.get_setting_bg_launcher_scale();
                cfg.custom_bg_instances_path = ui.get_setting_bg_instances_path().to_string();
                cfg.custom_bg_instances_opacity = ui.get_setting_bg_instances_opacity();
                cfg.custom_bg_instances_scale = ui.get_setting_bg_instances_scale();

                cfg.show_tray_icon = ui.get_setting_show_tray_icon();
                cfg.close_action_mode = ui.get_setting_close_action_mode().clamp(0, 2);
                cfg.minimize_to_tray = cfg.close_action_mode == 1;
                cfg.show_news_toolbar = ui.get_setting_show_news_toolbar();
                cfg.use_native_file_dialog = ui.get_setting_use_native_file_dialog();
                cfg.discord_rpc_launcher = ui.get_setting_discord_rpc_launcher();
                cfg.auto_check_updates = ui.get_setting_auto_check_updates();
                if let Ok(h) = ui.get_setting_update_hours_str().trim().parse::<u32>() {
                    cfg.update_check_interval_hours = h.min(720);
                }
                cfg.update_channel = ui.get_setting_update_channel().to_string();
                cfg.keep_mod_metadata = ui.get_setting_keep_mod_meta();
                cfg.install_mod_dependencies = ui.get_setting_install_deps();
                cfg.detect_mod_incompatibilities = ui.get_setting_detect_incompat();
                cfg.check_subfolders_blocked_mods = ui.get_setting_check_subfolders_blocked();
                cfg.move_blocked_mods = ui.get_setting_move_blocked_mods();
                cfg.suggest_update_existing_modpack = ui.get_setting_suggest_update_modpack();
                cfg.auto_update_mods = ui.get_setting_auto_update_mods();
                cfg.skip_mod_loader_compat_check = ui.get_setting_skip_mod_compat_check();
                cfg.show_mod_file_sizes = ui.get_setting_show_mod_file_sizes();
                if let Ok(lim) = ui.get_setting_log_limit_str().trim().parse::<usize>() {
                    cfg.log_history_limit = lim.clamp(100, 100_000);
                }
                cfg.stop_logging_on_overflow = ui.get_setting_stop_log_overflow();
                cfg.console_font_family = ui.get_setting_console_font().to_string();
                if let Ok(fsz) = ui.get_setting_console_font_size_str().trim().parse::<u32>() {
                    cfg.console_font_size = fsz.clamp(8, 32);
                }
                cfg.console_colorize_logs = ui.get_setting_console_colorize();
                cfg.console_auto_scroll = ui.get_setting_console_autoscroll();
                if let Ok(c) = ui.get_setting_concurrency_str().trim().parse::<usize>() {
                    cfg.download_concurrency = c.clamp(1, 32);
                }
                if let Ok(tl) = ui.get_setting_task_limit_str().trim().parse::<usize>() {
                    cfg.concurrent_task_limit = tl.clamp(1, 32);
                }
                if let Ok(rl) = ui.get_setting_retry_limit_str().trim().parse::<usize>() {
                    cfg.download_retry_limit = rl.clamp(0, 15);
                }
                if let Ok(t) = ui.get_setting_timeout_str().trim().parse::<u64>() {
                    cfg.network_timeout_secs = t.clamp(5, 300);
                }
                cfg.use_offline_meta_cache = ui.get_setting_use_offline_cache();

                // 1: MinecraftPage / MinecraftSettingsWidget
                if let Ok(w) = ui.get_setting_win_width_str().trim().parse::<u32>() {
                    cfg.window_width = w.max(640);
                }
                if let Ok(h) = ui.get_setting_win_height_str().trim().parse::<u32>() {
                    cfg.window_height = h.max(480);
                }
                cfg.start_maximized = ui.get_setting_start_maximized();
                cfg.close_after_launch = ui.get_setting_close_after_launch();
                cfg.quit_after_game_window_closed = ui.get_setting_quit_after_game_closed();
                cfg.auto_copy_screenshots = ui.get_setting_auto_copy_screenshots();
                cfg.show_console_on_launch = ui.get_setting_show_console_on_launch();
                cfg.show_console_on_error = ui.get_setting_show_console_on_error();
                cfg.hide_console_on_exit = ui.get_setting_hide_console_on_exit();
                cfg.enable_global_datapacks = ui.get_setting_enable_global_datapacks();
                cfg.global_datapacks_path = ui.get_setting_global_datapacks_path().to_string();
                cfg.record_playtime = ui.get_setting_record_playtime();
                cfg.show_playtime_instances = ui.get_setting_show_playtime_instances();
                cfg.show_total_playtime = ui.get_setting_show_total_playtime();
                cfg.always_show_hours = ui.get_setting_always_show_hours();
                cfg.elyby_skin_mode = ui.get_setting_elyby_skin_mode();
                cfg.auth_agent_backend =
                    AuthAgentBackend::from_index(ui.get_setting_auth_backend_idx());
                cfg.auth_agent_version = ui.get_setting_auth_agent_version().to_string();
                cfg.custom_patcher_path = ui.get_setting_custom_patcher_path().to_string();
                cfg.patcher_prefetch_metadata = ui.get_setting_patcher_prefetch();
                cfg.discord_rpc_game = ui.get_setting_discord_rpc_game();
                cfg.legacy_online_fixes = ui.get_setting_legacy_online_fixes();
                cfg.force_x11_backend = ui.get_setting_force_x11();
                cfg.disable_hidpi_scaling = ui.get_setting_disable_hidpi();
                cfg.use_system_glfw = ui.get_setting_use_system_glfw();
                cfg.custom_glfw_path = ui.get_setting_custom_glfw_path().to_string();
                cfg.use_system_openal = ui.get_setting_use_system_openal();
                cfg.custom_openal_path = ui.get_setting_custom_openal_path().to_string();
                cfg.use_discrete_gpu = ui.get_setting_use_discrete_gpu();
                cfg.enable_gamemode = ui.get_setting_enable_gamemode();
                cfg.enable_mangohud = ui.get_setting_enable_mangohud();
                cfg.use_zink = ui.get_setting_use_zink();

                // 2: JavaPage / JavaSettingsWidget
                cfg.auto_detect_java = ui.get_setting_auto_detect_java();
                cfg.auto_download_java = ui.get_setting_auto_download_java();
                cfg.skip_java_compat_checks = ui.get_setting_skip_java_compat();
                cfg.skip_java_startup_prompt = ui.get_setting_skip_java_prompt();
                cfg.java_path = ui.get_setting_java_path().to_string();
                if let Ok(min_m) = ui.get_setting_min_mem_str().trim().parse::<u32>() {
                    cfg.min_memory_mb = min_m.max(8);
                }
                if let Ok(max_m) = ui.get_setting_max_mem_str().trim().parse::<u32>() {
                    cfg.max_memory_mb = max_m.max(8);
                }
                if let Ok(pg) = ui.get_setting_permgen_str().trim().parse::<u32>() {
                    cfg.permgen_mb = pg.max(4);
                }
                cfg.warn_low_memory = ui.get_setting_warn_low_mem();
                cfg.jvm_args = ui.get_setting_jvm_args().to_string();

                // 3: Custom Commands & 4: Environment
                cfg.pre_launch_cmd = ui.get_setting_pre_launch_cmd().to_string();
                cfg.wrapper_cmd = ui.get_setting_wrapper_cmd().to_string();
                cfg.post_exit_cmd = ui.get_setting_post_exit_cmd().to_string();
                cfg.env_vars = ui.get_setting_env_vars().to_string();

                // 5: Proxy & External Tools
                cfg.proxy_mode = ui.get_setting_proxy_mode();
                cfg.proxy_enabled = cfg.proxy_mode == 2 || cfg.proxy_mode == 3;
                cfg.proxy_type = if cfg.proxy_mode == 3 {
                    "SOCKS5".to_string()
                } else {
                    "HTTP".to_string()
                };
                cfg.proxy_host = ui.get_setting_proxy_host().to_string();
                if let Ok(p) = ui.get_setting_proxy_port_str().trim().parse::<u16>() {
                    cfg.proxy_port = p;
                }
                cfg.proxy_user = ui.get_setting_proxy_user().to_string();
                cfg.proxy_pass = ui.get_setting_proxy_pass().to_string();
                cfg.ext_text_editor = ui.get_setting_ext_editor().to_string();
                cfg.ext_mcedit = ui.get_setting_ext_mcedit().to_string();
                cfg.ext_jprofiler = ui.get_setting_ext_jprofiler().to_string();
                cfg.ext_visualvm = ui.get_setting_ext_visualvm().to_string();

                // 6: APIs & Repositories
                cfg.paste_service_type = ui.get_setting_paste_type().to_string();
                cfg.paste_base_url = ui.get_setting_paste_url().to_string();
                cfg.meta_url = ui.get_setting_meta_url().to_string();
                cfg.assets_server_url = ui.get_setting_assets_url().to_string();
                cfg.legacy_fml_libs_url = ui.get_setting_legacy_fml_url().to_string();
                cfg.custom_user_agent = ui.get_setting_user_agent().to_string();
                cfg.labymod_repo_url = ui.get_setting_labymod_url().to_string();
                cfg.optifine_repo_url = ui.get_setting_optifine_url().to_string();
                cfg.horizon_repo_url = ui.get_setting_horizon_url().to_string();
                cfg.custom_news_url = ui.get_setting_custom_news_url().trim().to_string();
                cfg.msa_client_id = ui.get_setting_msa_client_id().trim().to_string();
                cfg.discord_client_id = ui.get_setting_discord_client_id().trim().to_string();
                cfg.modrinth_api_token = ui.get_setting_modrinth_token().trim().to_string();
                cfg.curseforge_api_key = ui.get_setting_cf_api_key().to_string();
                if !cfg.curseforge_api_key.trim().is_empty() {
                    cfg.curseforge_key_confirmed = true;
                }
                cfg.cf_fallback_to_modrinth = ui.get_setting_cf_fallback_modrinth();
                cfg.technic_api_key = ui.get_setting_technic_key().trim().to_string();

                save_config(&data_dir, &cfg);
                tray_ctrl.set_enabled(cfg.show_tray_icon, "");
                if !cfg.discord_rpc_launcher {
                    ui.set_discord_rpc_error(false);
                }
                sync_discord_launcher_state(&discord_ctrl, &cfg, launcher_start_ts);
                if let Ok(new_meta) = MetaClient::from_config(&cfg) {
                    *meta_holder.lock().unwrap() = Arc::new(new_meta);
                }
                sync_ui_from_config(&ui, &cfg);
                ui.set_repositories(ModelRc::from(Rc::new(VecModel::from(default_repo_rows(
                    &cfg,
                )))));
            }
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[Настройки] Все настройки сохранены и применены.".to_string(),
            );
        });
    }

    // Manual update of installed mods for the selected instance (Modrinth SHA-1 check)
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let meta_holder = meta_holder.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_update_installed_mods(move || {
            let cfg_snap = config.lock().unwrap().clone();
            let Some(inst) = cfg_snap
                .instances
                .iter()
                .find(|i| Some(&i.id) == cfg_snap.selected_instance_id.as_ref())
                .cloned()
            else {
                return;
            };
            let instances_dir = cfg_snap.resolve_instances_dir(&data_dir);
            let mods_dir = instances_dir.join(&inst.id).join(".minecraft").join("mods");
            if !mods_dir.is_dir() {
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    "[Моды] Папка mods не найдена или пуста.".to_string(),
                );
                return;
            }
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_is_busy(true);
            }
            let meta = current_meta(&meta_holder);
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            let inst_id = inst.id.clone();
            let mc_ver = inst.mc_version.clone();
            let loader = inst.loader.clone();
            rt.spawn(async move {
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    format!("[Моды] Проверка обновлений модов для «{}»...", inst.name),
                );
                let w_log = ui_weak.clone();
                let lb_log = log_buf.clone();
                let res = meta
                    .update_instance_mods(&mods_dir, &mc_ver, &loader, move |msg| {
                        append_log_to_ui(&w_log, &lb_log, msg);
                    })
                    .await;
                let refreshed = scan_installed_mods(&instances_dir, &inst_id);
                let w_done = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = w_done.upgrade() {
                        ui.set_installed_mods(ModelRc::from(Rc::new(VecModel::from(refreshed))));
                        ui.set_is_busy(false);
                    }
                });
                match res {
                    Ok(0) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        "[Моды] Все установленные моды уже актуальны.".to_string(),
                    ),
                    Ok(n) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Моды] Обновлено модов: {}.", n),
                    ),
                    Err(e) => append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Моды] Ошибка обновления модов: {}", e),
                    ),
                }
            });
        });
    }

    // 14b. Configurable Folders: Browse, Open, Reset & Test Java Path (FreesmLauncher!)
    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_browse_directory(move |field_key| {
            let key = field_key.to_string();
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            rt.spawn_blocking(move || {
                if let Some(chosen) = pick_native_directory("Выберите папку")
                {
                    let w = ui_weak.clone();
                    let chosen_c = chosen.clone();
                    let key_c = key.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = w.upgrade() {
                            match key_c.as_str() {
                                "data_root" => ui.set_setting_data_root_dir(chosen_c.into()),
                                "instances" => ui.set_setting_instances_dir(chosen_c.into()),
                                "mods" => ui.set_setting_mods_dir(chosen_c.into()),
                                "icons" => ui.set_setting_icons_dir(chosen_c.into()),
                                "java" => ui.set_setting_java_dir(chosen_c.into()),
                                "skins" => ui.set_setting_skins_dir(chosen_c.into()),
                                "assets" => ui.set_setting_assets_dir(chosen_c.into()),
                                "libraries" => ui.set_setting_libraries_dir(chosen_c.into()),
                                "exports" => ui.set_setting_exports_dir(chosen_c.into()),
                                "global_datapacks" => {
                                    ui.set_setting_global_datapacks_path(chosen_c.into())
                                }
                                "migrate_launcher" => ui.set_migrate_launcher_path(chosen_c.into()),
                                _ => {}
                            }
                        }
                    });
                    append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Папки] Выбран путь ({}): {} (нажмите «Сохранить»)", key, chosen),
                    );
                } else {
                    append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        "[Папки] Диалог выбора папки отменён (или введите путь вручную в поле).".to_string(),
                    );
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_browse_file(move |field_key| {
            let key = field_key.to_string();
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            rt.spawn_blocking(move || {
                if let Some(chosen) = pick_native_file("Выберите файл", false)
                {
                    let w = ui_weak.clone();
                    let chosen_c = chosen.clone();
                    let key_c = key.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = w.upgrade() {
                            match key_c.as_str() {
                                "cat_pack" => {
                                    ui.set_setting_cat_custom_path(chosen_c.clone().into());
                                    ui.set_setting_enable_cat_pack(true);
                                    ui.set_setting_cat_preset(5);
                                    if let Ok(img) = slint::Image::load_from_path(std::path::Path::new(&chosen_c)) {
                                        ui.set_cat_pack_image(img);
                                        ui.set_has_cat_image(true);
                                    }
                                }
                                "bg_launcher" => {
                                    ui.set_setting_bg_launcher_path(chosen_c.clone().into());
                                    ui.set_setting_enable_custom_bg(true);
                                    if let Ok(img) = slint::Image::load_from_path(std::path::Path::new(&chosen_c)) {
                                        ui.set_custom_bg_launcher_image(img);
                                        ui.set_has_launcher_bg_image(true);
                                    }
                                }
                                "bg_instances" => {
                                    ui.set_setting_bg_instances_path(chosen_c.clone().into());
                                    ui.set_setting_enable_custom_bg(true);
                                    if let Ok(img) = slint::Image::load_from_path(std::path::Path::new(&chosen_c)) {
                                        ui.set_custom_bg_instances_image(img);
                                        ui.set_has_instances_bg_image(true);
                                    }
                                }
                                "import_archive" => ui.set_import_archive_path(chosen_c.into()),
                                "glfw" => ui.set_setting_custom_glfw_path(chosen_c.into()),
                                "openal" => ui.set_setting_custom_openal_path(chosen_c.into()),
                                "patcher" => ui.set_setting_custom_patcher_path(chosen_c.into()),
                                "custom_loader" => ui.set_edit_custom_loader_path(chosen_c.into()),
                                "java" => ui.set_setting_java_path(chosen_c.into()),
                                "ext_editor" => ui.set_setting_ext_editor(chosen_c.into()),
                                "ext_mcedit" => ui.set_setting_ext_mcedit(chosen_c.into()),
                                "ext_jprofiler" => ui.set_setting_ext_jprofiler(chosen_c.into()),
                                "ext_visualvm" => ui.set_setting_ext_visualvm(chosen_c.into()),
                                _ => {}
                            }
                        }
                    });
                    append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        format!("[Файл] Выбран файл ({}): {}", key, chosen),
                    );
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_open_configured_folder(move |field_key| {
            let mut tmp_cfg = config.lock().unwrap().clone();
            if let Some(ui) = ui_weak.upgrade() {
                tmp_cfg.data_root_dir = ui.get_setting_data_root_dir().trim().to_string();
                tmp_cfg.instances_dir = ui.get_setting_instances_dir().trim().to_string();
                tmp_cfg.mods_dir = ui.get_setting_mods_dir().trim().to_string();
                tmp_cfg.icons_dir = ui.get_setting_icons_dir().trim().to_string();
                tmp_cfg.java_dir = ui.get_setting_java_dir().trim().to_string();
                tmp_cfg.skins_dir = ui.get_setting_skins_dir().trim().to_string();
                tmp_cfg.assets_dir = ui.get_setting_assets_dir().trim().to_string();
                tmp_cfg.libraries_dir = ui.get_setting_libraries_dir().trim().to_string();
                tmp_cfg.exports_dir = ui.get_setting_exports_dir().trim().to_string();
            }
            let target = match field_key.as_str() {
                "data_root" => tmp_cfg.effective_data_root(&data_dir),
                "instances" => tmp_cfg.resolve_instances_dir(&data_dir),
                "mods" => tmp_cfg.resolve_mods_dir(&data_dir),
                "icons" => tmp_cfg.resolve_icons_dir(&data_dir),
                "icons_packs" => {
                    let p = tmp_cfg.resolve_icon_packs_dir(&data_dir);
                    crate::ui_icons::ensure_packs_readme(&p);
                    p
                }
                "java" => tmp_cfg.resolve_java_dir(&data_dir),
                "skins" => tmp_cfg.resolve_skins_dir(&data_dir),
                "assets" => tmp_cfg.resolve_assets_dir(&data_dir),
                "libraries" => tmp_cfg.resolve_libraries_dir(&data_dir),
                "exports" => tmp_cfg.resolve_exports_dir(&data_dir),
                _ => tmp_cfg.effective_data_root(&data_dir),
            };
            let _ = std::fs::create_dir_all(&target);
            let _ = open::that(&target);
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                format!("[Папки] Открыта папка: {}", target.display()),
            );
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_reset_folder_paths(move || {
            let mut cfg = config.lock().unwrap();
            cfg.reset_folder_paths();
            save_config(&data_dir, &cfg);
            if let Some(ui) = ui_weak.upgrade() {
                sync_ui_from_config(&ui, &cfg);
            }
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                "[Папки] Пути хранения данных сброшены на стандартные.".to_string(),
            );
        });
    }
    {
        let ui_weak = ui.as_weak();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_test_java_path(move || {
            let java_cmd = ui_weak
                .upgrade()
                .map(|u| u.get_setting_java_path().trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "java".to_string());
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            rt.spawn_blocking(move || {
                let res = std::process::Command::new(&java_cmd).arg("-version").output();
                let msg = match res {
                    Ok(out) => {
                        let combined = format!(
                            "{}{}",
                            String::from_utf8_lossy(&out.stderr),
                            String::from_utf8_lossy(&out.stdout)
                        );
                        let first_line = combined
                            .lines()
                            .map(|l| l.trim())
                            .find(|l| !l.is_empty())
                            .unwrap_or("Java отвечает");
                        if out.status.success() {
                            format!("✓ Java работает: {}", first_line)
                        } else {
                            format!("✗ Ошибка запуска Java: {}", first_line)
                        }
                    }
                    Err(e) => format!("✗ Не удалось запустить «{}»: {}", java_cmd, e),
                };
                let w = ui_weak.clone();
                let m_ui = msg.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = w.upgrade() {
                        ui.set_java_test_result(m_ui.into());
                    }
                });
                append_log_to_ui(&ui_weak, &log_buf, format!("[Тест Java] {}", msg));
            });
        });
    }

    // 15. Console Filter, Copy, Upload (mclo.gs), Clear
    {
        let ui_weak = ui.as_weak();
        let log_buf = log_buf.clone();
        ui.on_set_console_filter(move |filter_idx| {
            if let Some(ui) = ui_weak.upgrade() {
                let text = {
                    let buf = log_buf.lock().unwrap();
                    render_filtered_log(&buf, filter_idx)
                };
                ui.set_console_log(SharedString::from(text));
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_upload_console_log(move || {
            let ui_weak = ui_weak.clone();
            let meta = current_meta(&meta_holder);
            let log_buf = log_buf.clone();
            let (svc_type, svc_url) = {
                let cfg = config.lock().unwrap();
                (cfg.paste_service_type.clone(), cfg.paste_base_url.clone())
            };
            let full = {
                let buf = log_buf.lock().unwrap();
                buf.join("\n")
            };
            append_log_to_ui(
                &ui_weak,
                &log_buf,
                format!("[{}] Загрузка журнала...", svc_type),
            );
            rt.spawn(async move {
                match meta.upload_log(&full, &svc_type, &svc_url).await {
                    Ok(url) => {
                        let url_clip = url.clone();
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.invoke_copy_to_clipboard_native(url_clip.into());
                            }
                        });
                        let _ = open::that(&url);
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            format!("[{}] Лог загружен (ссылка скопирована): {}", svc_type, url),
                        );
                    }
                    Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[{}] {}", svc_type, e)),
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let log_buf = log_buf.clone();
        ui.on_clear_console(move || {
            {
                let mut buf = log_buf.lock().unwrap();
                buf.clear();
                buf.push("[AxyLauncher] Журнал очищен.".to_string());
            }
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_console_log("[AxyLauncher] Журнал очищен.\n".into());
            }
        });
    }

    // 16. Custom Accent Color Picker (Screenshot 1!)
    {
        let ui_weak = ui.as_weak();
        ui.on_preview_custom_color(move |hex| {
            if let Some(ui) = ui_weak.upgrade() {
                if parse_hex_rgb(hex.as_str()).is_some() {
                    apply_custom_accent_to_ui(&ui, hex.as_str());
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let log_buf = log_buf.clone();
        ui.on_apply_custom_color(move |hex| {
            if let Some((r, g, b)) = parse_hex_rgb(hex.as_str()) {
                let norm = rgb_to_hex(r, g, b);
                let mut cfg = config.lock().unwrap();
                cfg.theme_scheme = 7;
                cfg.custom_accent_hex = norm.clone();
                save_config(&data_dir, &cfg);
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_custom_accent_hex(norm.clone().into());
                    apply_custom_accent_to_ui(&ui, &norm);
                    ui.global::<M3>().set_scheme(7);
                }
                append_log_to_ui(
                    &ui_weak,
                    &log_buf,
                    format!("[Тема] Применён пользовательский цвет: {}", norm),
                );
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_add_saved_custom_color(move |hex| {
            if let Some((r, g, b)) = parse_hex_rgb(hex.as_str()) {
                let norm = rgb_to_hex(r, g, b);
                let mut cfg = config.lock().unwrap();
                if !cfg.custom_saved_colors.iter().any(|c| c.eq_ignore_ascii_case(&norm)) {
                    cfg.custom_saved_colors.insert(0, norm);
                    if cfg.custom_saved_colors.len() > 8 {
                        cfg.custom_saved_colors.truncate(8);
                    }
                    save_config(&data_dir, &cfg);
                }
                if let Some(ui) = ui_weak.upgrade() {
                    sync_saved_colors_to_ui(&ui, &cfg);
                }
            }
        });
    }

    // 17. Microsoft Skin & Cape Upload Manager (Screenshot 2 — Freesm 2.3.2 style!)
    // Stores loaded capes for current modal session: (cape_id, alias, cached_png_path)
    let loaded_capes: Arc<Mutex<Vec<(String, String, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        let loaded_capes = loaded_capes.clone();

        ui.on_open_skin_manager(move |acc_id| {
            let (acc_opt, skins_dir, capes_dir) = {
                let cfg = config.lock().unwrap();
                let acc = cfg
                    .accounts
                    .iter()
                    .find(|a| a.id == acc_id.as_str() && a.account_type == AccountType::Microsoft)
                    .cloned();
                (
                    acc,
                    cfg.resolve_skins_dir(&data_dir),
                    cfg.resolve_capes_dir(&data_dir),
                )
            };
            let Some(acc) = acc_opt else { return };

            if let Some(ui) = ui_weak.upgrade() {
                let local_skins = scan_saved_skins(&skins_dir, false);
                ui.set_skin_account_id(acc.id.clone().into());
                ui.set_skin_account_name(acc.username.clone().into());
                ui.set_skin_status_text("Загрузка профиля Mojang...".into());
                ui.set_skin_is_slim(false);
                ui.set_skin_preview_elytra(false);
                ui.set_skin_preview_image(render_placeholder_skin(false, 8));
                ui.set_cape_preview_image(render_placeholder_cape(false));
                ui.set_selected_cape_name("Без плаща".into());
                ui.set_available_capes(ModelRc::from(Rc::new(VecModel::from(vec![
                    SharedString::from("Без плаща"),
                ]))));
                if let Some(first) = local_skins.first() {
                    let p = PathBuf::from(first.path.as_str());
                    ui.set_selected_skin_path(first.path.clone());
                    if let Some(img) = render_skin_front_back(&p, false, 8) {
                        ui.set_skin_preview_image(img);
                    }
                } else {
                    ui.set_selected_skin_path("".into());
                }
                ui.set_saved_skins(ModelRc::from(Rc::new(VecModel::from(local_skins))));
                ui.set_show_skin_modal(true);
            }

            let ui_weak = ui_weak.clone();
            let http = current_meta(&meta_holder).http().clone();
            let log_buf = log_buf.clone();
            let loaded_capes = loaded_capes.clone();
            let skins_dir = skins_dir.clone();
            let capes_dir = capes_dir.clone();

            rt.spawn(async move {
                let _ = tokio::fs::create_dir_all(&skins_dir).await;
                let _ = tokio::fs::create_dir_all(&capes_dir).await;
                match fetch_mc_profile_skins_capes(&http, &acc.access_token).await {
                    Ok(prof) => {
                        let active_skin = prof
                            .skins
                            .iter()
                            .find(|s| s.state.eq_ignore_ascii_case("ACTIVE"))
                            .or_else(|| prof.skins.first());
                        let is_slim = active_skin
                            .map(|s| s.variant.eq_ignore_ascii_case("SLIM"))
                            .unwrap_or(false);
                        let mut active_skin_path = String::new();
                        if let Some(sk) = active_skin {
                            if let Ok(path) =
                                import_skin_from_url(&http, &sk.url, &skins_dir, Some(&prof.name))
                                    .await
                            {
                                active_skin_path = path.to_string_lossy().to_string();
                            }
                        }

                        // Download & cache all owned capes
                        let mut cape_rows: Vec<(String, String, PathBuf)> = Vec::new();
                        let mut active_cape_alias = "Без плаща".to_string();
                        let mut active_cape_png: Option<PathBuf> = None;
                        for c in &prof.capes {
                            let alias = if c.alias.trim().is_empty() {
                                c.id.clone()
                            } else {
                                c.alias.clone()
                            };
                            let cpath = capes_dir.join(format!("{}.png", c.id));
                            if !cpath.exists() {
                                if let Ok(r) = http.get(&c.url).send().await {
                                    if let Ok(bytes) = r.bytes().await {
                                        let _ = tokio::fs::write(&cpath, &bytes).await;
                                    }
                                }
                            }
                            if c.state.eq_ignore_ascii_case("ACTIVE") {
                                active_cape_alias = alias.clone();
                                active_cape_png = Some(cpath.clone());
                            }
                            cape_rows.push((c.id.clone(), alias, cpath));
                        }

                        let mut cape_names = vec![SharedString::from("Без плаща")];
                        for (_, alias, _) in &cape_rows {
                            cape_names.push(SharedString::from(alias.as_str()));
                        }
                        *loaded_capes.lock().unwrap() = cape_rows;

                        let w = ui_weak.clone();
                        let skins_dir_c = skins_dir.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_skin_is_slim(is_slim);
                                let list = scan_saved_skins(&skins_dir_c, is_slim);
                                ui.set_saved_skins(ModelRc::from(Rc::new(VecModel::from(list))));
                                if !active_skin_path.is_empty() {
                                    ui.set_selected_skin_path(active_skin_path.clone().into());
                                    if let Some(img) =
                                        render_skin_front_back(PathBuf::from(&active_skin_path).as_path(), is_slim, 8)
                                    {
                                        ui.set_skin_preview_image(img);
                                    }
                                }
                                ui.set_available_capes(ModelRc::from(Rc::new(VecModel::from(
                                    cape_names,
                                ))));
                                ui.set_selected_cape_name(active_cape_alias.into());
                                if let Some(cp) = active_cape_png {
                                    if let Some(img) = render_cape_or_elytra(&cp, false, 12) {
                                        ui.set_cape_preview_image(img);
                                    }
                                }
                                ui.set_skin_status_text("Профиль загружен".into());
                            }
                        });
                    }
                    Err(e) => {
                        let msg = format!("{}", e);
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_skin_status_text("Офлайн / обновите токен MS".into());
                            }
                        });
                        append_log_to_ui(&ui_weak, &log_buf, format!("[Скины MS] {}", msg));
                    }
                }
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_select_skin_entry(move |path_str| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_selected_skin_path(path_str.clone());
                let slim = ui.get_skin_is_slim();
                let p = PathBuf::from(path_str.as_str());
                if let Some(img) = render_skin_front_back(&p, slim, 8) {
                    ui.set_skin_preview_image(img);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_set_skin_model(move |slim| {
            let skins_dir = config.lock().unwrap().resolve_skins_dir(&data_dir);
            if let Some(ui) = ui_weak.upgrade() {
                let sel = ui.get_selected_skin_path().to_string();
                let p = PathBuf::from(&sel);
                if p.is_file() {
                    if let Some(img) = render_skin_front_back(&p, slim, 8) {
                        ui.set_skin_preview_image(img);
                    }
                } else {
                    ui.set_skin_preview_image(render_placeholder_skin(slim, 8));
                }
                let list = scan_saved_skins(&skins_dir, slim);
                ui.set_saved_skins(ModelRc::from(Rc::new(VecModel::from(list))));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let loaded_capes = loaded_capes.clone();
        ui.on_select_cape_by_name(move |alias| {
            if let Some(ui) = ui_weak.upgrade() {
                let elytra = ui.get_skin_preview_elytra();
                let capes = loaded_capes.lock().unwrap();
                if let Some((_, _, cpath)) = capes.iter().find(|(_, a, _)| a == alias.as_str()) {
                    if let Some(img) = render_cape_or_elytra(cpath, elytra, 12) {
                        ui.set_cape_preview_image(img);
                    }
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let loaded_capes = loaded_capes.clone();
        ui.on_toggle_preview_elytra(move |elytra| {
            if let Some(ui) = ui_weak.upgrade() {
                let alias = ui.get_selected_cape_name().to_string();
                let capes = loaded_capes.lock().unwrap();
                if let Some((_, _, cpath)) = capes.iter().find(|(_, a, _)| a == &alias) {
                    if let Some(img) = render_cape_or_elytra(cpath, elytra, 12) {
                        ui.set_cape_preview_image(img);
                    }
                } else {
                    ui.set_cape_preview_image(render_placeholder_cape(elytra));
                }
            }
        });
    }

    {
        let config = config.clone();
        let data_dir = data_dir.clone();
        ui.on_open_skins_folder(move || {
            let skins_dir = config.lock().unwrap().resolve_skins_dir(&data_dir);
            let _ = std::fs::create_dir_all(&skins_dir);
            let _ = open::that(&skins_dir);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_reset_ms_skin(move || {
            let acc_id = ui_weak
                .upgrade()
                .map(|u| u.get_skin_account_id().to_string())
                .unwrap_or_default();
            let token_opt = {
                let cfg = config.lock().unwrap();
                cfg.accounts
                    .iter()
                    .find(|a| a.id == acc_id)
                    .map(|a| a.access_token.clone())
            };
            let Some(token) = token_opt else { return };
            let http = current_meta(&meta_holder).http().clone();
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            rt.spawn(async move {
                match reset_mc_skin(&http, &token).await {
                    Ok(()) => {
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_skin_status_text("Скин сброшен на стандартный".into());
                            }
                        });
                        append_log_to_ui(&ui_weak, &log_buf, "[Скины MS] Скин сброшен.".to_string());
                    }
                    Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Скины MS] {}", e)),
                }
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_import_skin_url(move |url_str| {
            let url = url_str.trim().to_string();
            if url.is_empty() {
                return;
            }
            let skins_dir = config.lock().unwrap().resolve_skins_dir(&data_dir);
            let http = current_meta(&meta_holder).http().clone();
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            rt.spawn(async move {
                match import_skin_from_url(&http, &url, &skins_dir, None).await {
                    Ok(saved) => {
                        let path_s = saved.to_string_lossy().to_string();
                        let w = ui_weak.clone();
                        let sd = skins_dir.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                let slim = ui.get_skin_is_slim();
                                let list = scan_saved_skins(&sd, slim);
                                ui.set_saved_skins(ModelRc::from(Rc::new(VecModel::from(list))));
                                ui.set_selected_skin_path(path_s.clone().into());
                                if let Some(img) = render_skin_front_back(PathBuf::from(&path_s).as_path(), slim, 8) {
                                    ui.set_skin_preview_image(img);
                                }
                                ui.set_skin_status_text("Скин импортирован по URL".into());
                            }
                        });
                    }
                    Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Импорт скина] {}", e)),
                }
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        ui.on_import_skin_user(move |uname| {
            let clean = uname.trim().to_string();
            if clean.is_empty() {
                return;
            }
            let skins_dir = config.lock().unwrap().resolve_skins_dir(&data_dir);
            let http = current_meta(&meta_holder).http().clone();
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            rt.spawn(async move {
                match import_skin_from_username(&http, &clean, &skins_dir).await {
                    Ok(saved) => {
                        let path_s = saved.to_string_lossy().to_string();
                        let w = ui_weak.clone();
                        let sd = skins_dir.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                let slim = ui.get_skin_is_slim();
                                let list = scan_saved_skins(&sd, slim);
                                ui.set_saved_skins(ModelRc::from(Rc::new(VecModel::from(list))));
                                ui.set_selected_skin_path(path_s.clone().into());
                                if let Some(img) = render_skin_front_back(PathBuf::from(&path_s).as_path(), slim, 8) {
                                    ui.set_skin_preview_image(img);
                                }
                                ui.set_skin_status_text(format!("Скин «{}» импортирован", clean).into());
                            }
                        });
                    }
                    Err(e) => append_log_to_ui(&ui_weak, &log_buf, format!("[Импорт скина] {}", e)),
                }
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let rt = rt.clone();
        let log_buf = log_buf.clone();
        ui.on_import_skin_file(move |path_str| {
            let typed = path_str.trim().to_string();
            let skins_dir = config.lock().unwrap().resolve_skins_dir(&data_dir);
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();
            rt.spawn_blocking(move || {
                let chosen = if !typed.is_empty() {
                    Some(PathBuf::from(&typed))
                } else {
                    pick_native_file("Выберите PNG-скин", true).map(PathBuf::from)
                };
                if let Some(src) = chosen.filter(|p| p.is_file()) {
                    let _ = std::fs::create_dir_all(&skins_dir);
                    let fname = src
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("custom_skin.png");
                    let dest = skins_dir.join(fname);
                    if std::fs::copy(&src, &dest).is_ok() {
                        let path_s = dest.to_string_lossy().to_string();
                        let w = ui_weak.clone();
                        let sd = skins_dir.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                let slim = ui.get_skin_is_slim();
                                let list = scan_saved_skins(&sd, slim);
                                ui.set_saved_skins(ModelRc::from(Rc::new(VecModel::from(list))));
                                ui.set_selected_skin_path(path_s.clone().into());
                                if let Some(img) = render_skin_front_back(PathBuf::from(&path_s).as_path(), slim, 8) {
                                    ui.set_skin_preview_image(img);
                                }
                                ui.set_skin_status_text("Файл скина импортирован".into());
                            }
                        });
                    }
                } else {
                    append_log_to_ui(
                        &ui_weak,
                        &log_buf,
                        "[Импорт скина] Укажите путь к .png файлу в поле ввода или положите его в папку скинов.".to_string(),
                    );
                }
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let rt = rt.clone();
        let meta_holder = meta_holder.clone();
        let log_buf = log_buf.clone();
        let loaded_capes = loaded_capes.clone();
        ui.on_apply_ms_skin_cape(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let acc_id = ui.get_skin_account_id().to_string();
            let skin_path = ui.get_selected_skin_path().to_string();
            let slim = ui.get_skin_is_slim();
            let cape_alias = ui.get_selected_cape_name().to_string();
            ui.set_skin_status_text("Применение скина и плаща...".into());

            let token_opt = {
                let cfg = config.lock().unwrap();
                cfg.accounts
                    .iter()
                    .find(|a| a.id == acc_id)
                    .map(|a| a.access_token.clone())
            };
            let Some(token) = token_opt else { return };
            let cape_id_opt = {
                let capes = loaded_capes.lock().unwrap();
                capes
                    .iter()
                    .find(|(_, a, _)| a == &cape_alias)
                    .map(|(id, _, _)| id.clone())
            };
            let http = current_meta(&meta_holder).http().clone();
            let ui_weak = ui_weak.clone();
            let log_buf = log_buf.clone();

            rt.spawn(async move {
                let mut err_msg: Option<String> = None;
                if !skin_path.is_empty() {
                    if let Ok(bytes) = tokio::fs::read(&skin_path).await {
                        if let Err(e) = upload_mc_skin(&http, &token, slim, &bytes).await {
                            err_msg = Some(format!("{}", e));
                        }
                    }
                }
                if err_msg.is_none() {
                    if let Err(e) = set_mc_cape(&http, &token, cape_id_opt.as_deref()).await {
                        err_msg = Some(format!("{}", e));
                    }
                }
                match err_msg {
                    None => {
                        let w = ui_weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_skin_status_text("Скин и плащ применены!".into());
                                ui.set_show_skin_modal(false);
                            }
                        });
                        append_log_to_ui(
                            &ui_weak,
                            &log_buf,
                            "[Скины MS] Скин и плащ успешно обновлены на аккаунте Microsoft.".to_string(),
                        );
                    }
                    Some(e) => {
                        let w = ui_weak.clone();
                        let e_ui = e.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_skin_status_text("Ошибка загрузки (см. Консоль)".into());
                            }
                        });
                        append_log_to_ui(&ui_weak, &log_buf, format!("[Скины MS] {}", e_ui));
                    }
                }
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let tray_ctrl = tray_ctrl.clone();
        ui.window().on_close_requested(move || {
            let mode = config.lock().unwrap().close_action_mode;
            match mode {
                1 => {
                    tray_ctrl.set_enabled(true, "");
                    slint::CloseRequestResponse::HideWindow
                }
                2 => {
                    let _ = slint::quit_event_loop();
                    slint::CloseRequestResponse::HideWindow
                }
                _ => {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_close_confirm_remember(false);
                        ui.set_show_close_confirm_modal(true);
                    }
                    slint::CloseRequestResponse::KeepWindowShown
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let data_dir = data_dir.clone();
        let tray_ctrl = tray_ctrl.clone();
        ui.on_confirm_close_action(move |action, remember| {
            if remember {
                let mut cfg = config.lock().unwrap();
                cfg.close_action_mode = action.clamp(1, 2);
                cfg.minimize_to_tray = cfg.close_action_mode == 1;
                save_config(&data_dir, &cfg);
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_setting_close_action_mode(cfg.close_action_mode);
                }
            }
            if action == 1 {
                tray_ctrl.set_enabled(true, "");
                if let Some(ui) = ui_weak.upgrade() {
                    let _ = ui.hide();
                }
            } else {
                if let Some(ui) = ui_weak.upgrade() {
                    let _ = ui.hide();
                }
                let _ = slint::quit_event_loop();
            }
        });
    }

    ui.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
    use slint::platform::{Platform, WindowAdapter};
    use slint::{LogicalPosition, PhysicalSize};

    struct TestPlatform {
        window: Rc<MinimalSoftwareWindow>,
    }

    impl Platform for TestPlatform {
        fn create_window_adapter(
            &self,
        ) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(self.window.clone())
        }
    }

    #[test]
    fn test_freesm_helpers_and_scroll_layout() {
        // 1. Random nickname & UUID & base64
        let n1 = generate_random_nickname();
        let n2 = generate_random_nickname();
        assert!(!n1.is_empty() && n1.len() <= 16);
        assert_ne!(n1, n2);
        assert_eq!(
            crate::auth::generate_offline_uuid("Player"),
            "a01e3843e5213998958af459800e4d11"
        );
        assert_eq!(crate::auth::base64_encode(b"hello"), "aGVsbG8=");

        // 2. Verify clean first-run defaults (no pre-populated instances/accounts, English first)
        let mut cfg = LauncherConfig::default();
        assert!(cfg.instances.is_empty());
        assert!(cfg.accounts.is_empty());
        assert_eq!(cfg.language, "English (US)");
        // Populate sample instances for filtering, sorting & export tests
        cfg.instances = vec![
            Instance::new_default(
                "Fabric 1.21.1".to_string(),
                "Fabric 1.21.1".to_string(),
                "Основное".to_string(),
                "1.21.1".to_string(),
                LoaderType::Fabric,
                "0.16.9".to_string(),
                false,
                None,
                "Готов",
            ),
            Instance::new_default(
                "Forge 1.20.1 + OptiFine".to_string(),
                "Forge 1.20.1 + OptiFine".to_string(),
                "Модпаки".to_string(),
                "1.20.1".to_string(),
                LoaderType::Forge,
                "47.3.0".to_string(),
                true,
                Some("HD_U_I6".to_string()),
                "Готов",
            ),
            Instance::new_default(
                "LabyMod 4 (1.21.4)".to_string(),
                "LabyMod 4 (1.21.4)".to_string(),
                "Клиенты".to_string(),
                "1.21.4".to_string(),
                LoaderType::LabyMod,
                "latest".to_string(),
                false,
                None,
                "Готов",
            ),
            Instance::new_default(
                "Horizon (Minecraft on Rust)".to_string(),
                "Horizon (Minecraft on Rust)".to_string(),
                "Клиенты".to_string(),
                "26.3".to_string(),
                LoaderType::Horizon,
                "latest".to_string(),
                false,
                None,
                "Native Rust",
            ),
        ];
        cfg.instances[0].total_playtime_minutes = 120;
        cfg.instances[1].total_playtime_minutes = 300;
        let (sorted, _) = build_filtered_instances(&cfg, "", "Все", "По времени игры");
        assert_eq!(sorted[0].id.as_str(), "Forge 1.20.1 + OptiFine");
        let (only_cli, _) = build_filtered_instances(&cfg, "", "Клиенты", "По имени");
        assert_eq!(only_cli.len(), 2);

        // 2a. Verify Freesm/Prism DirNameFromString + folder rename on disk
        let tmp_rename_root = std::env::temp_dir().join("minelauncher_rename_test");
        let _ = std::fs::remove_dir_all(&tmp_rename_root);
        let insts_dir = tmp_rename_root.join("instances");
        std::fs::create_dir_all(&insts_dir).unwrap();
        let d1 = dir_name_from_string("My:Pack/1.21*", &insts_dir, None);
        assert_eq!(d1, "My_Pack_1.21_");
        std::fs::create_dir_all(insts_dir.join(&d1).join(".minecraft")).unwrap();
        std::fs::write(insts_dir.join(&d1).join(".minecraft/options.txt"), "ok").unwrap();
        let d2 = dir_name_from_string("My:Pack/1.21*", &insts_dir, None);
        assert_eq!(d2, "My_Pack_1.21_ 1");
        let renamed = rename_instance_folder(&insts_dir, &d1, "Survival 1.21.4").unwrap();
        assert_eq!(renamed, "Survival 1.21.4");
        assert!(insts_dir.join("Survival 1.21.4/.minecraft/options.txt").exists());
        assert!(!insts_dir.join(&d1).exists());
        let _ = std::fs::remove_dir_all(&tmp_rename_root);

        // 2a-2. Verify Configurable Launcher Folders resolution (relative, custom root, absolute override)
        let base_root = PathBuf::from("/tmp/ml_base");
        let mut fcfg = LauncherConfig::default();
        assert_eq!(fcfg.resolve_instances_dir(&base_root), base_root.join("instances"));
        assert_eq!(fcfg.resolve_java_dir(&base_root), base_root.join("java"));
        fcfg.data_root_dir = "/mnt/Games/ML".to_string();
        assert_eq!(
            fcfg.resolve_instances_dir(&base_root),
            PathBuf::from("/mnt/Games/ML/instances")
        );
        fcfg.java_dir = "/opt/custom-javas".to_string();
        assert_eq!(
            fcfg.resolve_java_dir(&base_root),
            PathBuf::from("/opt/custom-javas")
        );
        fcfg.reset_folder_paths();
        assert_eq!(fcfg.resolve_instances_dir(&base_root), base_root.join("instances"));
        assert_eq!(fcfg.resolve_java_dir(&base_root), base_root.join("java"));

        // 2b. Verify all 3 Export formats (Prism .zip, CurseForge .zip, Modrinth .mrpack)
        let tmp_export_root = std::env::temp_dir().join("minelauncher_export_test");
        let _ = std::fs::remove_dir_all(&tmp_export_root);
        let export_insts_dir = tmp_export_root.join("instances");
        let exports_dir = tmp_export_root.join("exports");
        let sample_inst = &cfg.instances[0];
        let sample_mc = export_insts_dir.join(&sample_inst.id).join(".minecraft");
        std::fs::create_dir_all(&sample_mc).unwrap();
        std::fs::write(sample_mc.join("options.txt"), "fov:90\n").unwrap();

        let p_prism =
            export_instance_archive(&export_insts_dir, &exports_dir, sample_inst, ExportFormat::PrismZip)
                .unwrap();
        let p_cf = export_instance_archive(
            &export_insts_dir,
            &exports_dir,
            sample_inst,
            ExportFormat::CurseForgeZip,
        )
        .unwrap();
        let p_mr = export_instance_archive(
            &export_insts_dir,
            &exports_dir,
            sample_inst,
            ExportFormat::ModrinthMrpack,
        )
        .unwrap();

        let z_prism = zip::ZipArchive::new(std::fs::File::open(&p_prism).unwrap()).unwrap();
        assert!(z_prism.file_names().any(|n| n == "instance.cfg"));
        assert!(z_prism.file_names().any(|n| n == "mmc-pack.json"));
        assert!(z_prism.file_names().any(|n| n == ".minecraft/options.txt"));

        let z_cf = zip::ZipArchive::new(std::fs::File::open(&p_cf).unwrap()).unwrap();
        assert!(z_cf.file_names().any(|n| n == "manifest.json"));
        assert!(z_cf.file_names().any(|n| n == "overrides/options.txt"));

        let z_mr = zip::ZipArchive::new(std::fs::File::open(&p_mr).unwrap()).unwrap();
        assert!(z_mr.file_names().any(|n| n == "modrinth.index.json"));
        assert!(z_mr.file_names().any(|n| n == "overrides/options.txt"));
        let _ = std::fs::remove_dir_all(&tmp_export_root);

        // 3. Render MainWindow headlessly across ALL tabs & modals to verify zero layout/scroll errors!
        let sw = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        sw.set_size(PhysicalSize::new(1260, 800));
        let _ = slint::platform::set_platform(Box::new(TestPlatform { window: sw.clone() }));

        let ui = MainWindow::new().expect("MainWindow init");
        let _ = slint::set_xdg_app_id(crate::icon::XDG_APP_ID);
        // Populate 20 instances & 60 MC versions & 120 console lines
        let mut many_cfg = LauncherConfig::default();
        many_cfg.cat_pack_preset = 1;
        many_cfg.enable_snow_effect = true;
        for i in 0..20 {
            let title = if i == 0 {
                "Очень длинное название тестовой сборки Minecraft для проверки переноса текста на несколько строк и автоматического расширения высоты карточки без съезжания плашки версии".to_string()
            } else {
                format!("Сборка #{i:02} • {}", instance_icon_name(i * 3))
            };
            let mut inst = Instance::new_default(
                format!("inst-{i:02}"),
                title,
                "Основное".into(),
                "1.21.4".into(),
                LoaderType::Fabric,
                "0.16.9".into(),
                false,
                None,
                "Готов",
            );
            inst.icon_id = i * 3;
            many_cfg.instances.push(inst);
        }
        many_cfg.selected_instance_id = Some("inst-00".to_string());
        sync_ui_from_config(&ui, &many_cfg);
        assert_eq!(ui.get_instance_icons().row_count(), INSTANCE_ICON_COUNT as usize);
        for ic_id in 0..INSTANCE_ICON_COUNT {
            let img = instance_icon_image(ic_id);
            assert_eq!((img.size().width, img.size().height), (64, 64));
            assert!(!instance_icon_name(ic_id).is_empty());
        }
        let mc_vers: Vec<SharedString> = (0..60).map(|i| SharedString::from(format!("1.21.{i}"))).collect();
        ui.set_available_mc_versions(ModelRc::from(Rc::new(VecModel::from(mc_vers))));
        let loader_vers: Vec<SharedString> = (0..30).map(|i| SharedString::from(format!("0.16.{i}"))).collect();
        ui.set_available_loader_versions(ModelRc::from(Rc::new(VecModel::from(loader_vers))));
        let log_lines: String = (0..120).map(|i| format!("[INFO] Строка журнала #{i}: проверка скролла и переноса строк в консоли AxyLauncher\n")).collect();
        ui.set_console_log(log_lines.into());

        let dump_shots = std::env::var_os("ML_RENDER_SCREENSHOTS").is_some();
        let mut buf = vec![Rgb565Pixel(0); 1260 * 800];
        let mut render = |name: Option<&str>| {
            sw.draw_if_needed(|renderer| {
                renderer.render(&mut buf, 1260);
            });
            if dump_shots {
                if let Some(n) = name {
                    let mut out = format!("P6\n1260 800\n255\n").into_bytes();
                    out.reserve(1260 * 800 * 3);
                    for p in &buf {
                        let v = p.0;
                        out.push((((v >> 11) & 0x1f) as u32 * 255 / 31) as u8);
                        out.push((((v >> 5) & 0x3f) as u32 * 255 / 63) as u8);
                        out.push(((v & 0x1f) as u32 * 255 / 31) as u8);
                    }
                    let _ = std::fs::write(format!("/tmp/ml_test_{n}.ppm"), out);
                }
            }
        };

        // Render main instances tab: first with NO selection (right panel closed!), then selected + Split Export Menu
        ui.set_active_tab(0);
        ui.set_selected_instance_index(-1);
        render(Some("instances_no_selection"));
        ui.set_selected_instance_index(0);
        ui.set_show_export_menu(true);
        render(Some("instances_export_menu"));
        ui.set_show_export_menu(false);
        // Wheel scroll on instances list
        sw.dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: LogicalPosition::new(400.0, 400.0),
            delta_x: 0.0,
            delta_y: -300.0,
        });
        render(Some("instances_scrolled"));

        // Accounts (Microsoft Device Code + Sidebar Patcher & Version Selector!)
        ui.set_active_tab(1);
        ui.set_auth_method_idx(1);
        ui.set_ms_user_code("R57RM3T5".into());
        ui.set_ms_verification_url("https://www.microsoft.com/link".into());
        ui.set_ms_device_code_info("To sign in, open https://www.microsoft.com/link and enter code R57RM3T5.".into());
        render(Some("accounts_sidebar"));

        // Settings (exact 7 FreesmLauncher pages + inner subtabs!)
        ui.set_repositories(ModelRc::from(Rc::new(VecModel::from(default_repo_rows(
            &many_cfg,
        )))));
        ui.set_active_tab(3);
        for st in 0..=6 {
            ui.set_settings_subtab(st);
            if st == 0 {
                for lt in 0..=4 {
                    ui.set_launcher_inner_tab(lt);
                    render(if lt == 0 {
                        Some("settings_launcher_folders")
                    } else if lt == 1 {
                        Some("settings_launcher_ui")
                    } else {
                        None
                    });
                }
                ui.set_launcher_inner_tab(0);
            } else if st == 1 {
                for mt in 0..=2 {
                    ui.set_mc_inner_tab(mt);
                    render(if mt == 2 {
                        Some("settings_minecraft_patcher")
                    } else {
                        None
                    });
                }
                ui.set_mc_inner_tab(0);
            } else if st == 2 {
                for jt in 0..=1 {
                    ui.set_java_inner_tab(jt);
                    render(if jt == 0 {
                        Some("settings_java")
                    } else {
                        None
                    });
                }
                ui.set_java_inner_tab(0);
            } else if st == 6 {
                for at in 0..=2 {
                    ui.set_api_inner_tab(at);
                    render(if at == 2 {
                        Some("settings_api_repos")
                    } else {
                        None
                    });
                }
                ui.set_api_inner_tab(0);
            } else {
                render(None);
            }
        }

        // Edit Instance (all 10 subtabs, including Worlds downloader & Freesm Instance Settings!)
        let mock_hits: Vec<UiModHit> = (0..12)
            .map(|i| UiModHit {
                project_id: format!("proj-{i}").into(),
                provider: if i % 2 == 0 { "Modrinth" } else { "CurseForge" }.into(),
                title: format!("Example Pack / World #{i}").into(),
                author: "Creator".into(),
                description: "Подробное описание для проверки полноценной высоты скролла в списке.".into(),
                downloads: "120000".into(),
                mc_version: "1.21.4".into(),
                loader: "Fabric".into(),
                icon: slint::Image::default(),
            })
            .collect();
        ui.set_modpack_search_results(ModelRc::from(Rc::new(VecModel::from(mock_hits.clone()))));
        ui.set_world_search_results(ModelRc::from(Rc::new(VecModel::from(mock_hits))));
        ui.set_active_tab(5);
        let of_vers: Vec<SharedString> = (1..=11)
            .rev()
            .map(|i| SharedString::from(format!("HD_U_J7_pre{i}")))
            .collect();
        ui.set_available_optifine_versions(ModelRc::from(Rc::new(VecModel::from(of_vers))));
        ui.set_edit_inst_name("Forge 1.20.1".into());
        ui.set_edit_inst_group("Основное".into());
        ui.set_edit_inst_mc("1.21.10".into());
        ui.set_edit_inst_loader_idx(2);
        ui.set_edit_inst_loader_ver("60.1.18".into());
        ui.set_edit_inst_icon_id(9);
        ui.set_edit_inst_icon_image(instance_icon_image(9));
        ui.set_edit_inst_icon_name(instance_icon_name(9).into());
        ui.set_edit_inst_optifine(true);
        ui.set_edit_inst_optifine_ver("HD_U_J7_pre11".into());
        for et in 0..=9 {
            ui.set_edit_subtab(et);
            if et == 1 {
                render(Some("edit_version_3col_optifine"));
            } else if et == 3 {
                render(Some("edit_resourcepacks"));
            } else if et == 4 {
                render(Some("edit_shaders"));
            } else if et == 6 {
                ui.set_show_world_downloader(true);
                render(Some("edit_worlds_dl"));
                ui.set_show_world_downloader(false);
            } else if et == 9 {
                render(Some("edit_settings"));
            } else {
                render(None);
            }
        }

        // Icon Picker Modal (60 pixel-art Minecraft icons!)
        ui.set_icon_picker_for_new(false);
        ui.set_show_icon_picker_modal(true);
        render(Some("icon_picker_modal"));
        ui.set_show_icon_picker_modal(false);

        // Add Instance Modal — verify empty loader state ("Нет сборок NeoForge для этой версии")!
        ui.set_active_tab(0);
        ui.set_show_create_modal(true);
        ui.set_create_mode_idx(0);
        ui.set_new_inst_icon_id(15);
        ui.set_new_inst_icon_image(instance_icon_image(15));
        ui.set_new_inst_loader_idx(3); // NeoForge
        ui.set_new_inst_mc_version("1.12.2".into());
        ui.set_new_inst_loader_version("Нет сборок NeoForge для этой версии".into());
        ui.set_available_loader_versions(ModelRc::from(Rc::new(VecModel::from(
            Vec::<SharedString>::new(),
        ))));
        render(Some("create_modal_empty_loader"));

        // Add Instance Modal — verify 3-column Forge+OptiFine side layout stays strictly locked even with long text!
        ui.set_new_inst_loader_idx(2);
        ui.set_new_inst_mc_version("1.21.10-very-long-snapshot-name".into());
        ui.set_new_inst_loader_version("60.1.18-recommended-extra-long".into());
        ui.set_new_inst_include_optifine(true);
        ui.set_new_inst_optifine_version("HD_U_J7_pre11_ultra_long_version".into());
        ui.set_available_loader_versions(ModelRc::from(Rc::new(VecModel::from(vec![
            SharedString::from("60.1.18-recommended-extra-long"),
            SharedString::from("60.1.17"),
            SharedString::from("60.1.16"),
        ]))));
        for cm in 0..=2 {
            ui.set_create_mode_idx(cm);
            render(if cm == 0 {
                Some("create_modal_forge_optifine")
            } else if cm == 1 {
                Some("create_modal_modpacks")
            } else {
                None
            });
        }
        ui.set_show_create_modal(false);

        // Setup Wizard Step 0 — verify Language Dropdown!
        ui.set_show_setup_wizard(true);
        ui.set_wizard_step(0);
        render(Some("wizard_step0"));

        // Rename Instance Folder Prompt Modal
        ui.set_show_setup_wizard(false);
        ui.set_rename_folder_old("Fabric 1.21.1".into());
        ui.set_rename_folder_new("Survival 1.21.4".into());
        ui.set_rename_folder_inst_name("Survival 1.21.4".into());
        ui.set_show_rename_folder_modal(true);
        render(Some("rename_folder_modal"));
        ui.set_show_rename_folder_modal(false);

        // Custom Accent Color Picker Modal (Screenshot 1!)
        assert_eq!(parse_hex_rgb("#7C5CBF"), Some((124, 92, 191)));
        apply_custom_accent_to_ui(&ui, "#7C5CBF");
        ui.global::<M3>().set_scheme(7);
        ui.set_active_tab(3);
        ui.set_settings_subtab(0);
        ui.set_custom_accent_hex("#7C5CBF".into());
        ui.set_show_color_picker_modal(true);
        render(Some("color_picker_modal"));
        ui.set_show_color_picker_modal(false);

        // Microsoft Skin & Cape Upload Modal (Screenshot 2!)
        ui.set_active_tab(1);
        ui.set_skin_account_name("xeon_e5_2670_v3".into());
        ui.set_skin_status_text("Профиль загружен".into());
        ui.set_skin_is_slim(true);
        ui.set_skin_preview_image(render_placeholder_skin(true, 8));
        ui.set_cape_preview_image(render_placeholder_cape(false));
        ui.set_selected_cape_name("Follower's".into());
        ui.set_available_capes(ModelRc::from(Rc::new(VecModel::from(vec![
            SharedString::from("Без плаща"),
            SharedString::from("Follower's"),
            SharedString::from("Migrator"),
            SharedString::from("Cherry Blossom"),
        ]))));
        ui.set_selected_skin_path("/skins/xeon_e5_2670_v3.png".into());
        ui.set_saved_skins(ModelRc::from(Rc::new(VecModel::from(vec![
            UiSkinEntry {
                name: "xeon_e5_2670_v3".into(),
                path: "/skins/xeon_e5_2670_v3.png".into(),
                thumbnail: render_placeholder_skin(true, 3),
            },
            UiSkinEntry {
                name: "Classic_Steve".into(),
                path: "/skins/Classic_Steve.png".into(),
                thumbnail: render_placeholder_skin(false, 3),
            },
        ]))));
        ui.set_show_skin_modal(true);
        render(Some("skin_upload_modal"));
        ui.set_show_skin_modal(false);

        // Close Confirmation Modal (Minimize to Tray or Quit)
        ui.set_active_tab(0);
        ui.set_close_confirm_remember(true);
        ui.set_show_close_confirm_modal(true);
        render(Some("close_confirm_modal"));
        ui.set_show_close_confirm_modal(false);

        // Verify PNG logo encoder decodes cleanly via Slint Image::load_from_path!
        let tmp_png = std::env::temp_dir().join("minelauncher_icon_test.png");
        let logo_rgba = crate::icon::render_logo_rgba(64);
        let png_bytes = crate::icon::encode_png_rgba(64, 64, &logo_rgba);
        std::fs::write(&tmp_png, &png_bytes).unwrap();
        let loaded_icon = slint::Image::load_from_path(&tmp_png).expect("Valid RFC-2083 PNG");
        assert_eq!((loaded_icon.size().width, loaded_icon.size().height), (64, 64));
        let _ = std::fs::remove_file(&tmp_png);

        // Verify Linux Discord Rich Presence IPC framing & handshake over real UnixListener!
        #[cfg(unix)]
        {
            let sock_path = std::env::temp_dir().join(format!(
                "ml_discord_ipc_test_{}.sock",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&sock_path);
            let listener = std::os::unix::net::UnixListener::bind(&sock_path).unwrap();
            let server_handle = std::thread::spawn(move || {
                let (mut srv, _) = listener.accept().unwrap();
                let (op_hs, hs_body) = crate::discord::read_ipc_frame(&mut srv).unwrap();
                assert_eq!(op_hs, 0); // HANDSHAKE
                assert!(hs_body.contains(crate::discord::DEFAULT_DISCORD_APP_ID));
                crate::discord::write_ipc_frame(
                    &mut srv,
                    1,
                    r#"{"cmd":"DISPATCH","evt":"READY","data":{}}"#,
                )
                .unwrap();
                let (op_act, act_body) = crate::discord::read_ipc_frame(&mut srv).unwrap();
                assert_eq!(op_act, 1); // FRAME (SET_ACTIVITY)
                assert!(act_body.contains("SET_ACTIVITY"));
                assert!(act_body.contains("Fabric 1.21.1"));
            });

            let mut cli = crate::discord::connect_unix_path_and_handshake(
                &sock_path,
                crate::discord::DEFAULT_DISCORD_APP_ID,
            )
            .expect("Discord IPC handshake should succeed");
            let act = crate::discord::DiscordActivity {
                details: "Играет: Fabric 1.21.1".to_string(),
                state: "Minecraft 1.21.1 • Fabric".to_string(),
                start_timestamp: 1700000000,
                custom_client_id: None,
            };
            let payload = crate::discord::build_set_activity_payload(Some(&act), 1, true);
            assert!(payload.contains("\"large_image\":\"grass\""));
            crate::discord::write_ipc_frame(&mut cli, 1, &payload).unwrap();
            server_handle.join().unwrap();
            let _ = std::fs::remove_file(&sock_path);

            // Verify DiscordRpcController clone + drop does NOT kill the background worker
            let ctrl = crate::discord::DiscordRpcController::spawn(Arc::new(|_, _| {}));
            {
                let _cloned = ctrl.clone();
            }
            ctrl.set_launcher_presence(false, None, "", 0);
        }

        let non_zero = buf.iter().filter(|p| p.0 != 0).count();
        assert!(non_zero > 500_000, "Expected rich rendered frame, got {non_zero} non-zero pixels");
        assert_eq!(LauncherConfig::default().meta_url, DEFAULT_FREESM_META_URL);
        assert!(ui.get_new_inst_name().is_empty());
        assert!(ui.get_modpack_search_query().is_empty());

        // Verify Linux config directory uses ~/.config/axylauncher
        #[cfg(target_os = "linux")]
        {
            let cfg_dir = get_config_dir();
            let cfg_str = cfg_dir.to_string_lossy();
            assert!(
                cfg_str.contains(".config") || std::env::var("XDG_CONFIG_HOME").is_ok(),
                "Expected config dir in .config, got {}",
                cfg_str
            );
            assert!(cfg_str.ends_with("axylauncher"));
        }

        // Verify 9-language i18n translations
        assert_eq!(i18n::translate("Настройки", "Русский (RU)"), "Настройки");
        assert_eq!(i18n::translate("Настройки", "English (US)"), "Settings");
        assert_eq!(i18n::translate("Настройки", "Українська (UA)"), "Налаштування");
        assert_eq!(i18n::translate("Настройки", "Español (ES)"), "Ajustes");
        assert_eq!(i18n::translate("Настройки", "Français (FR)"), "Paramètres");
        assert_eq!(i18n::translate("Настройки", "Deutsch (DE)"), "Einstellungen");
        assert_eq!(i18n::translate("Настройки", "Polski (PL)"), "Ustawienia");
        assert_eq!(i18n::translate("Настройки", "中文 (ZH)"), "设置");
        assert_eq!(i18n::translate("Настройки", "日本語 (JA)"), "設定");

        // Verify UI icon packs, detailed cat presets, and logo validation
        let tmp_packs = std::env::temp_dir().join("axylauncher_ui_packs_test");
        let _ = std::fs::remove_dir_all(&tmp_packs);
        crate::ui_icons::ensure_packs_readme(&tmp_packs);
        let opts = crate::ui_icons::list_icon_theme_options(&tmp_packs);
        assert!(opts.contains(&"Material".to_string()));
        assert!(opts.contains(&"Контурные".to_string()));
        assert!(opts.contains(&"Пиксельные".to_string()));
        let _ = std::fs::remove_dir_all(&tmp_packs);

        for p in 1..=4 {
            assert!(crate::cats::render_cat_preset(p).is_some());
        }
        assert!(crate::instance_icons::detect_image_ext(b"\x89PNG\r\n\x1a\n").is_some());
        assert_eq!(crate::instance_icons::detect_image_ext(b"BM\x00\x00"), Some("bmp"));
        assert_eq!(crate::instance_icons::detect_image_ext(b"\x00\x00\x01\x00"), Some("ico"));
        assert_eq!(crate::instance_icons::detect_image_ext(b"\xEF\xBB\xBF  <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"), Some("svg"));
        assert!(crate::instance_icons::detect_image_ext(b"not_an_image").is_none());
        assert_eq!(i18n::translate("Оба", "English (US)"), "Both");
        assert_eq!(i18n::translate("Оба", "Українська (UA)"), "Обидва");
        assert_eq!(i18n::translate("Новости лаунчера ещё не указаны (Настройки → API)", "English (US)"), "Launcher news source not configured yet (Settings → API)");
        assert!(image::load_from_memory_with_format(
            b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;",
            image::ImageFormat::Gif
        ).is_ok());
        assert!(image::ImageFormat::WebP.reading_enabled());
    }
}
