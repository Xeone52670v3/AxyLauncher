use crate::auth::{
    ensure_authlib_injector, ensure_loki_agent, fetch_yggdrasil_prefetched_base64,
};
use crate::meta::MetaClient;
use crate::models::*;
use anyhow::{anyhow, Context, Result};
use futures::stream::{self, StreamExt};
use sha1::{Digest as Sha1Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Copy a PNG file to the system clipboard (Linux Wayland/X11, Windows PowerShell, macOS AppleScript) — FreesmLauncher feature!
pub fn copy_png_to_clipboard(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(anyhow!("Не удалось найти скриншот {}", path.display()));
    }
    if cfg!(target_os = "windows") {
        let escaped = path.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; $img = [System.Drawing.Image]::FromFile('{}'); [System.Windows.Forms.Clipboard]::SetImage($img); $img.Dispose()",
            escaped
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Не удалось скопировать скриншот через PowerShell")?;
        return if status.success() {
            Ok(())
        } else {
            Err(anyhow!("PowerShell завершился с кодом {}", status))
        };
    }
    if cfg!(target_os = "macos") {
        let escaped = path.to_string_lossy().replace('"', "\\\"");
        let script = format!(
            "set the clipboard to (read (POSIX file \"{}\") as «class PNGf»)",
            escaped
        );
        let status = std::process::Command::new("osascript")
            .args(["-e", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Не удалось скопировать скриншот через osascript")?;
        return if status.success() {
            Ok(())
        } else {
            Err(anyhow!("osascript завершился с кодом {}", status))
        };
    }

    let bytes = std::fs::read(path)
        .with_context(|| format!("Не удалось прочитать скриншот {}", path.display()))?;
    // Try wl-copy (Wayland) first if WAYLAND_DISPLAY is set
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Ok(mut child) = std::process::Command::new("wl-copy")
            .args(["--type", "image/png"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(&bytes);
            }
            if let Ok(status) = child.wait() {
                if status.success() {
                    return Ok(());
                }
            }
        }
    }
    // Fallback to xclip (X11 / XWayland)
    let mut child = std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-t", "image/png", "-i"])
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Для копирования скриншотов установите wl-copy (Wayland) или xclip (X11)")?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("xclip завершился с кодом {}", status))
    }
}

/// Fallback CLI clipboard copy (primary copy uses Slint's built-in native clipboard)
#[allow(dead_code)]
pub fn copy_text_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write;
    let try_cmd = |bin: &str, args: &[&str]| -> bool {
        if let Ok(mut child) = std::process::Command::new(bin)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().map(|s| s.success()).unwrap_or(false);
        }
        false
    };
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && try_cmd("wl-copy", &[]) {
        return Ok(());
    }
    if try_cmd("xclip", &["-selection", "clipboard"]) {
        return Ok(());
    }
    if try_cmd("xsel", &["--clipboard", "--input"]) {
        return Ok(());
    }
    Err(anyhow!("Установите wl-copy, xclip или xsel для буфера обмена"))
}

/// Check if a PNG file is completely written (ends with PNG IEND chunk, just like FreesmLauncher 2.3.2!)
fn is_complete_png(path: &Path) -> bool {
    const PNG_IEND: [u8; 8] = [0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.len() > 32 && bytes.ends_with(&PNG_IEND)
}

/// Watch `.minecraft/screenshots/` while the game runs and copy every new PNG screenshot to clipboard!
fn spawn_screenshot_watcher<FLog>(
    screenshots_dir: PathBuf,
    log_cb: Arc<FLog>,
) -> tokio::sync::watch::Sender<bool>
where
    FLog: Fn(String) + Send + Sync + 'static,
{
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let mut seen: HashSet<PathBuf> = HashSet::new();
        if let Ok(entries) = std::fs::read_dir(&screenshots_dir) {
            for e in entries.flatten() {
                seen.insert(e.path());
            }
        }
        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                    if let Ok(entries) = std::fs::read_dir(&screenshots_dir) {
                        for e in entries.flatten() {
                            let p = e.path();
                            if p.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("png")).unwrap_or(false)
                                && !seen.contains(&p)
                            {
                                // Wait up to 2s for complete PNG IEND chunk (FreesmLauncher 2.3.2 PR #240)
                                let mut ready = false;
                                for _ in 0..10 {
                                    if is_complete_png(&p) {
                                        ready = true;
                                        break;
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                }
                                seen.insert(p.clone());
                                if ready {
                                    match copy_png_to_clipboard(&p) {
                                        Ok(()) => log_cb(format!(
                                            "[Screenshot] Copied to clipboard: {}",
                                            p.file_name().and_then(|n| n.to_str()).unwrap_or("screenshot.png")
                                        )),
                                        Err(err) => log_cb(format!("[Screenshot] Failed to copy to clipboard: {}", err)),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    stop_tx
}

fn find_system_library(names: &[&str]) -> Option<String> {
    let dirs = [
        "/usr/lib",
        "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/local/lib",
    ];
    for dir in dirs {
        for name in names {
            let candidate = Path::new(dir).join(name);
            if candidate.exists() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct LaunchOutcome {
    pub exit_code: i32,
    pub played_minutes: u64,
}

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub dest: PathBuf,
    pub expected_sha1: Option<String>,
    pub extract_natives_to: Option<PathBuf>,
}

/// Convert Maven coordinate `group:artifact:version[:classifier][@ext]` to relative path
pub fn maven_coord_to_path(coord: &str) -> Option<String> {
    let (coord_main, ext) = match coord.split_once('@') {
        Some((c, e)) => (c, e),
        None => (coord, "jar"),
    };
    let parts: Vec<&str> = coord_main.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group_path = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier_suffix = if parts.len() >= 4 && !parts[3].is_empty() {
        format!("-{}", parts[3])
    } else {
        String::new()
    };
    Some(format!(
        "{}/{}/{}/{}-{}{}.{}",
        group_path, artifact, version, artifact, version, classifier_suffix, ext
    ))
}

/// Extract `group:artifact` key so higher-priority components (like Fabric/Forge) override lower ones
fn library_ga_key(coord: &str) -> String {
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() >= 4 {
        format!("{}:{}:{}", parts[0], parts[1], parts[3])
    } else if parts.len() >= 2 {
        format!("{}:{}", parts[0], parts[1])
    } else {
        coord.to_string()
    }
}

/// Evaluate Mojang/Prism OS rules for current platform (Linux x86_64)
pub fn check_os_rules(rules: &[OsRule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let current_os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };

    let mut allowed = false;
    for rule in rules {
        let matches = match &rule.os {
            None => true,
            Some(os) => {
                let name_ok = os
                    .name
                    .as_deref()
                    .map(|n| n == current_os)
                    .unwrap_or(true);
                let arch_ok = os
                    .arch
                    .as_deref()
                    .map(|a| a == "x86_64" || a == "amd64" || a == "64")
                    .unwrap_or(true);
                name_ok && arch_ok
            }
        };
        if matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

/// Verify SHA-1 of an existing file
pub fn verify_file_sha1(path: &Path, expected_sha1: Option<&str>) -> bool {
    if !path.exists() {
        return false;
    }
    let Some(expected) = expected_sha1.filter(|s| !s.is_empty()) else {
        return path.metadata().map(|m| m.len() > 0).unwrap_or(false);
    };
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let mut hasher = Sha1::new();
    hasher.update(&bytes);
    let actual = hex::encode(hasher.finalize());
    actual.eq_ignore_ascii_case(expected)
}

/// Extract `.so` / `.dll` / `.dylib` native libraries from a native jar into `natives_dir`
pub fn extract_native_jar(jar_path: &Path, natives_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    std::fs::create_dir_all(natives_dir)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name.starts_with("META-INF/") || entry.is_dir() {
            continue;
        }
        if name.ends_with(".so")
            || name.ends_with(".dll")
            || name.ends_with(".dylib")
            || name.contains(".so.")
        {
            let file_name = Path::new(&name)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(&name));
            let out_path = natives_dir.join(file_name);
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }
    Ok(())
}

/// Auto-detect installed Java runtimes on Linux, Windows, and macOS
pub fn detect_system_javas() -> Vec<String> {
    let mut found = Vec::new();
    let locate_cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(output) = std::process::Command::new(locate_cmd).arg("java").output() {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let p = line.trim().to_string();
                if !p.is_empty() && !found.contains(&p) {
                    found.push(p);
                }
            }
        }
    }
    if cfg!(windows) {
        for base in [
            r"C:\Program Files\Eclipse Adoptium",
            r"C:\Program Files\Java",
            r"C:\Program Files\Microsoft",
            r"C:\Program Files\BellSoft",
            r"C:\Program Files\Zulu",
        ] {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    for bin_name in ["javaw.exe", "java.exe"] {
                        let java_bin = entry.path().join("bin").join(bin_name);
                        if java_bin.exists() {
                            let s = java_bin.to_string_lossy().to_string();
                            if !found.contains(&s) {
                                found.push(s);
                            }
                            break;
                        }
                    }
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(entries) = std::fs::read_dir("/Library/Java/JavaVirtualMachines") {
            for entry in entries.flatten() {
                let java_bin = entry.path().join("Contents/Home/bin/java");
                if java_bin.exists() {
                    let s = java_bin.to_string_lossy().to_string();
                    if !found.contains(&s) {
                        found.push(s);
                    }
                }
            }
        }
    } else if let Ok(entries) = std::fs::read_dir("/usr/lib/jvm") {
        for entry in entries.flatten() {
            let java_bin = entry.path().join("bin").join("java");
            if java_bin.exists() {
                let s = java_bin.to_string_lossy().to_string();
                if !found.contains(&s) {
                    found.push(s);
                }
            }
        }
    }
    if found.is_empty() {
        found.push(if cfg!(windows) {
            "javaw".to_string()
        } else {
            "java".to_string()
        });
    }
    found
}

fn process_library(
    lib: &PrismLibrary,
    libraries_dir: &Path,
    natives_dir: &Path,
    download_tasks: &mut Vec<DownloadTask>,
    classpath_entries: &mut Vec<PathBuf>,
    seen_ga: &mut HashSet<String>,
    add_to_cp: bool,
    override_ga: bool,
) {
    if !check_os_rules(&lib.rules) {
        return;
    }
    let lname = lib.name.to_lowercase();
    if cfg!(target_arch = "x86_64")
        && (lname.contains("aarch_64")
            || lname.contains("arm64")
            || lname.contains("arm32")
            || lname.contains("riscv64")
            || lname.contains("x86:")
            || lname.ends_with("-x86"))
    {
        return;
    }
    if cfg!(target_arch = "aarch64")
        && (lname.contains("x86_64")
            || lname.contains("arm32")
            || lname.contains("riscv64")
            || lname.ends_with("-x86"))
    {
        return;
    }
    // Filter out native jars explicitly meant for other operating systems
    if cfg!(target_os = "windows") && (lname.contains("natives-linux") || lname.contains("natives-macos") || lname.contains("natives-osx")) {
        return;
    }
    if cfg!(target_os = "macos") && (lname.contains("natives-linux") || lname.contains("natives-windows")) {
        return;
    }
    if cfg!(target_os = "linux") && (lname.contains("natives-windows") || lname.contains("natives-macos") || lname.contains("natives-osx")) {
        return;
    }

    let ga_key = library_ga_key(&lib.name);
    if override_ga && seen_ga.contains(&ga_key) {
        return;
    }

    if let Some(downloads) = &lib.downloads {
        if let Some(artifact) = &downloads.artifact {
            let rel_path = artifact
                .path
                .clone()
                .or_else(|| maven_coord_to_path(&lib.name))
                .unwrap_or_else(|| format!("misc/{}.jar", lib.name.replace(':', "_")));
            let dest = libraries_dir.join(&rel_path);
            let is_native_jar = if cfg!(target_os = "windows") {
                lname.contains("natives-windows")
            } else if cfg!(target_os = "macos") {
                lname.contains("natives-macos") || lname.contains("natives-osx")
            } else {
                lname.contains("natives-linux")
            };
            download_tasks.push(DownloadTask {
                url: artifact.url.clone(),
                dest: dest.clone(),
                expected_sha1: artifact.sha1.clone(),
                extract_natives_to: if is_native_jar {
                    Some(natives_dir.to_path_buf())
                } else {
                    None
                },
            });
            if add_to_cp {
                seen_ga.insert(ga_key);
                classpath_entries.push(dest);
            }
        }

        let native_key_opt = if cfg!(target_os = "windows") {
            lib.natives.get("windows")
        } else if cfg!(target_os = "macos") {
            lib.natives.get("osx").or_else(|| lib.natives.get("macos"))
        } else {
            lib.natives.get("linux")
        };
        if let Some(native_key) = native_key_opt {
            let resolved_key = native_key.replace("${arch}", "64");
            if let Some(classifier) = downloads.classifiers.get(&resolved_key) {
                let rel_path = classifier.path.clone().unwrap_or_else(|| {
                    format!(
                        "natives/{}-{}.jar",
                        lib.name.replace(':', "_"),
                        resolved_key
                    )
                });
                let dest = libraries_dir.join(&rel_path);
                download_tasks.push(DownloadTask {
                    url: classifier.url.clone(),
                    dest,
                    expected_sha1: classifier.sha1.clone(),
                    extract_natives_to: Some(natives_dir.to_path_buf()),
                });
            }
        }
    } else if let Some(rel_path) = maven_coord_to_path(&lib.name) {
        let base_url = lib
            .url
            .as_deref()
            .unwrap_or("https://libraries.minecraft.net/")
            .trim_end_matches('/');
        let full_url = format!("{}/{}", base_url, rel_path);
        let dest = libraries_dir.join(&rel_path);
        download_tasks.push(DownloadTask {
            url: full_url,
            dest: dest.clone(),
            expected_sha1: None,
            extract_natives_to: None,
        });
        if add_to_cp {
            seen_ga.insert(ga_key);
            classpath_entries.push(dest);
        }
    }
}

/// Download and prepare all instance files (version JSON, client JAR, libraries, natives, assets, modloader, Java/Horizon)
/// without starting the game process.
pub async fn prepare_instance<FLog, FProg>(
    meta: Arc<MetaClient>,
    config: LauncherConfig,
    instance: Instance,
    data_dir: PathBuf,
    log_cb: FLog,
    prog_cb: FProg,
) -> Result<()>
where
    FLog: Fn(String) + Send + Sync + 'static,
    FProg: Fn(f32, String) + Send + Sync + 'static,
{
    let (_tx, rx) = tokio::sync::oneshot::channel();
    let dummy_account = Account {
        id: "prepare".to_string(),
        username: "Player".to_string(),
        uuid: "00000000-0000-0000-0000-000000000000".to_string(),
        account_type: AccountType::Offline,
        access_token: "0".to_string(),
        refresh_token: None,
        auth_server_url: None,
        auth_login_url: None,
        auth_refresh_url: None,
    };
    let _ = launch_instance_internal(
        meta,
        config,
        instance,
        dummy_account,
        data_dir,
        true,
        rx,
        true,
        log_cb,
        prog_cb,
    )
    .await?;
    Ok(())
}

/// Execute full instance preparation (metadata resolution, libraries, assets, modloader, LabyMod/OptiFine/Horizon) and launch
pub async fn launch_instance<FLog, FProg>(
    meta: Arc<MetaClient>,
    config: LauncherConfig,
    instance: Instance,
    account: Account,
    data_dir: PathBuf,
    force_offline: bool,
    kill_rx: tokio::sync::oneshot::Receiver<()>,
    log_cb: FLog,
    prog_cb: FProg,
) -> Result<LaunchOutcome>
where
    FLog: Fn(String) + Send + Sync + 'static,
    FProg: Fn(f32, String) + Send + Sync + 'static,
{
    launch_instance_internal(
        meta,
        config,
        instance,
        account,
        data_dir,
        force_offline,
        kill_rx,
        false,
        log_cb,
        prog_cb,
    )
    .await
}

async fn launch_instance_internal<FLog, FProg>(
    meta: Arc<MetaClient>,
    config: LauncherConfig,
    instance: Instance,
    mut account: Account,
    data_dir: PathBuf,
    force_offline: bool,
    mut kill_rx: tokio::sync::oneshot::Receiver<()>,
    prepare_only: bool,
    log_cb: FLog,
    prog_cb: FProg,
) -> Result<LaunchOutcome>
where
    FLog: Fn(String) + Send + Sync + 'static,
    FProg: Fn(f32, String) + Send + Sync + 'static,
{
    let log_cb = Arc::new(log_cb);
    let prog_cb = Arc::new(prog_cb);
    let session_start = std::time::Instant::now();

    if force_offline {
        account.account_type = AccountType::Offline;
        account.access_token = "0".to_string();
    }

    if prepare_only {
        log_cb(format!(
            "=== Downloading instance '{}' (Minecraft {} | {}) ===",
            instance.name,
            instance.mc_version,
            instance.loader.as_str()
        ));
    } else {
        log_cb(format!(
            "=== Launching instance '{}' (Minecraft {} | {}) ===",
            instance.name,
            instance.mc_version,
            instance.loader.as_str()
        ));
        log_cb(format!(
            "Account: {} [{}] (UUID: {})",
            account.username,
            account.account_type.label(),
            account.uuid
        ));
    }

    let instances_dir = config.resolve_instances_dir(&data_dir);
    let libraries_dir = config.resolve_libraries_dir(&data_dir);
    let assets_dir = config.resolve_assets_dir(&data_dir);
    let java_dir = config.resolve_java_dir(&data_dir);
    let instance_root = instances_dir.join(&instance.id);
    let instance_dir = instance_root.join(".minecraft");
    tokio::fs::create_dir_all(&instance_dir).await?;

    // ========================================================================
    // HORIZON (NATIVE MINECRAFT ON RUST — `horizon-rust/horizon`)
    // ========================================================================
    if instance.loader == LoaderType::Horizon {
        prog_cb(0.1, "Поиск релиза Horizon (Minecraft on Rust)...".to_string());
        let (tag, asset) = meta
            .fetch_horizon_linux_asset(&instance.loader_version)
            .await?;
        log_cb(format!(
            "[Horizon] Selected release {} -> file {}",
            tag, asset.name
        ));

        let horizon_dir = config.resolve_horizon_dir(&data_dir).join(&tag);
        tokio::fs::create_dir_all(&horizon_dir).await?;
        let asset_path = horizon_dir.join(&asset.name);

        if !asset_path.exists() {
            prog_cb(
                0.35,
                format!("Скачивание Horizon {} ({})...", tag, asset.name),
            );
            log_cb(format!(
                "[Horizon] Downloading {}...",
                asset.browser_download_url
            ));
            let bytes = meta
                .http()
                .get(&asset.browser_download_url)
                .send()
                .await
                .with_context(|| format!("Ошибка скачивания Horizon: {}", asset.browser_download_url))?
                .error_for_status()?
                .bytes()
                .await?;
            tokio::fs::write(&asset_path, &bytes).await?;
        } else {
            log_cb(format!(
                "[Horizon] Using cached file {}",
                asset_path.display()
            ));
        }

        // Determine executable path (extract tar.gz if needed, else chmod +x)
        let exec_path = if asset.name.ends_with(".tar.gz") || asset.name.ends_with(".tgz") {
            prog_cb(0.75, "Распаковка архива Horizon...".to_string());
            let extract_dir = horizon_dir.join("extracted");
            tokio::fs::create_dir_all(&extract_dir).await?;
            let status = Command::new("tar")
                .args([
                    "-xzf",
                    &asset_path.to_string_lossy(),
                    "-C",
                    &extract_dir.to_string_lossy(),
                ])
                .status()
                .await?;
            if !status.success() {
                return Err(anyhow!("Не удалось распаковать архив Horizon tar.gz"));
            }
            find_first_executable_in_dir(&extract_dir).unwrap_or(asset_path.clone())
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta_fs) = std::fs::metadata(&asset_path) {
                    let mut perms = meta_fs.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&asset_path, perms);
                }
            }
            asset_path.clone()
        };

        if prepare_only {
            prog_cb(1.0, format!("Horizon {} загружен и готов к запуску!", tag));
            log_cb(format!("[Horizon] Preparation complete: {}", exec_path.display()));
            return Ok(LaunchOutcome {
                exit_code: 0,
                played_minutes: 0,
            });
        }

        prog_cb(0.95, format!("Запуск Horizon {}...", tag));
        log_cb(format!(
            "[Horizon] Starting native Rust client: {} (dir: {})",
            exec_path.display(),
            instance_dir.display()
        ));

        let wrapper = if instance.override_commands {
            instance.custom_wrapper_cmd.clone().unwrap_or_default()
        } else {
            config.wrapper_cmd.clone()
        };
        let mut cmd = if !wrapper.trim().is_empty() {
            let mut parts = wrapper.split_whitespace();
            let prog = parts.next().unwrap();
            let mut c = Command::new(prog);
            for p in parts {
                c.arg(p);
            }
            c.arg(&exec_path);
            c
        } else {
            Command::new(&exec_path)
        };

        let mut child = cmd
            .arg("--dir")
            .arg(&instance_dir)
            .current_dir(&instance_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Не удалось запустить Horizon: {}", exec_path.display()))?;

        prog_cb(1.0, format!("Horizon {} запущен!", tag));
        stream_child_logs(&mut child, log_cb.clone());
        let exit_code = tokio::select! {
            status = child.wait() => {
                let s = status?;
                log_cb(format!("=== Horizon process exited with code: {} ===", s));
                s.code().unwrap_or(0)
            }
            _ = &mut kill_rx => {
                log_cb("=== Stopping Horizon process by user request ===".to_string());
                let _ = child.kill().await;
                let _ = child.wait().await;
                -1
            }
        };
        prog_cb(0.0, "Готов".to_string());
        return Ok(LaunchOutcome {
            exit_code,
            played_minutes: session_start.elapsed().as_secs() / 60,
        });
    }

    let natives_dir = instance_root.join("natives");
    let mods_dir = instance_dir.join("mods");

    tokio::fs::create_dir_all(&libraries_dir).await?;
    tokio::fs::create_dir_all(&assets_dir.join("indexes")).await?;
    tokio::fs::create_dir_all(&assets_dir.join("objects")).await?;
    tokio::fs::create_dir_all(&natives_dir).await?;
    tokio::fs::create_dir_all(&mods_dir).await?;

    prog_cb(0.05, "Получение метаданных из репозиториев...".to_string());

    let mut download_tasks: Vec<DownloadTask> = Vec::new();
    let mut classpath_entries: Vec<PathBuf> = Vec::new();
    let mut seen_ga: HashSet<String> = HashSet::new();
    let mut main_class: String;
    let mut minecraft_args: String;
    let mut tweakers: Vec<String> = Vec::new();
    let mut extra_jvm_args: Vec<String> = Vec::new();
    let asset_index_info: Option<AssetIndexInfo>;
    let mut required_java_major: u32 = infer_java_major_from_mc(&instance.mc_version);

    if instance.loader == LoaderType::LabyMod {
        required_java_major = 21;
        // ====================================================================
        // LABYMOD 4 REPOSITORY RESOLUTION (`https://releases.r2.labymod.net`)
        // ====================================================================
        log_cb("Fetching LabyMod 4 manifest from releases.r2.labymod.net...".to_string());
        let laby_manifest = meta.fetch_labymod_manifest().await?;
        let laby_libs = meta.fetch_labymod_libraries().await?;

        let mc_entry = laby_manifest
            .minecraft_versions
            .iter()
            .find(|v| v.version == instance.mc_version || v.tag == instance.mc_version)
            .or_else(|| laby_manifest.minecraft_versions.first())
            .ok_or_else(|| anyhow!("Версия Minecraft не найдена в манифесте LabyMod 4"))?;

        log_cb(format!(
            "LabyMod v{} (commit {}) for Minecraft {}",
            laby_manifest.labymod_version, laby_manifest.commit_reference, mc_entry.version
        ));

        let custom_ver = meta
            .fetch_labymod_version_manifest(&mc_entry.custom_manifest_url)
            .await?;

        main_class = custom_ver
            .main_class
            .unwrap_or_else(|| "net.minecraft.launchwrapper.Launch".to_string());
        asset_index_info = custom_ver.asset_index;
        minecraft_args = "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userType ${user_type} --versionType ${version_type} --tweakClass net.labymod.core.loader.vanilla.launchwrapper.LabyModLaunchWrapperTweaker".to_string();

        extra_jvm_args.push(format!(
            "-Dnet.labymod.running-version={}",
            mc_entry.version
        ));
        extra_jvm_args.push("--add-opens=java.base/java.net=ALL-UNNAMED".to_string());

        // 1. LabyMod-4 core jar
        let laby_jar_dest = libraries_dir
            .join("net/labymod/LabyMod")
            .join(format!("4-{}", laby_manifest.commit_reference))
            .join(format!("LabyMod-4-{}.jar", laby_manifest.commit_reference));
        let laby_jar_url = format!(
            "{}/api/v1/download/labymod4/production/{}.jar",
            config.labymod_repo_url.trim_end_matches('/'),
            laby_manifest.commit_reference
        );
        download_tasks.push(DownloadTask {
            url: laby_jar_url,
            dest: laby_jar_dest.clone(),
            expected_sha1: Some(laby_manifest.sha1.clone()),
            extract_natives_to: None,
        });
        classpath_entries.push(laby_jar_dest);

        // 2. LabyMod 4 assets (shader, common, fonts, vanilla-theme, fancy-theme, i18n)
        let laby_assets_dir = config
            .resolve_labymod_dir(&data_dir)
            .join("production")
            .join("assets");
        tokio::fs::create_dir_all(&laby_assets_dir).await?;
        extra_jvm_args.push(format!("-Dnet.labymod.assets-dir={}", laby_assets_dir.display()));
        for (asset_name, asset_sha1) in &laby_manifest.assets {
            let asset_url = format!(
                "{}/api/v1/download/assets/labymod4/production/{}/{}/{}.jar",
                config.labymod_repo_url.trim_end_matches('/'),
                laby_manifest.commit_reference,
                asset_name,
                asset_sha1
            );
            let asset_dest = laby_assets_dir.join(format!("{}.jar", asset_name));
            download_tasks.push(DownloadTask {
                url: asset_url,
                dest: asset_dest.clone(),
                expected_sha1: Some(asset_sha1.clone()),
                extract_natives_to: None,
            });
            classpath_entries.push(asset_dest);
        }

        // 3. LabyMod 4 production libraries (`libraries/production.json`)
        for llib in &laby_libs.libraries {
            if llib.minecraft_version == "all" || llib.minecraft_version == mc_entry.version {
                let rel_path = maven_coord_to_path(&llib.name)
                    .unwrap_or_else(|| format!("labymod/{}.jar", llib.name.replace(':', "_")));
                let dest = libraries_dir.join(&rel_path);
                download_tasks.push(DownloadTask {
                    url: llib.url.clone(),
                    dest: dest.clone(),
                    expected_sha1: llib.sha1.clone(),
                    extract_natives_to: None,
                });
                classpath_entries.push(dest);
            }
        }

        // 4. Custom manifest libraries + client jar
        for lib in &custom_ver.libraries {
            process_library(
                lib,
                &libraries_dir,
                &natives_dir,
                &mut download_tasks,
                &mut classpath_entries,
                &mut seen_ga,
                true,
                true,
            );
        }
        if let Some(downloads) = &custom_ver.downloads {
            if let Some(client) = &downloads.client {
                let client_dest = libraries_dir
                    .join("com/mojang/minecraft")
                    .join(&mc_entry.version)
                    .join(format!("minecraft-{}-client.jar", mc_entry.version));
                download_tasks.push(DownloadTask {
                    url: client.url.clone(),
                    dest: client_dest.clone(),
                    expected_sha1: client.sha1.clone(),
                    extract_natives_to: None,
                });
                classpath_entries.push(client_dest);
            }
        }
    } else {
        // ====================================================================
        // PRISM LAUNCHER META REPOSITORY RESOLUTION
        // (`net.minecraft`, `org.lwjgl3`/`org.lwjgl`, `net.fabricmc.*`,
        //  `net.minecraftforge`, `net.neoforged`, `org.quiltmc.*`, `com.mumfrey.liteloader`)
        // ====================================================================
        let effective_mc_ver = instance
            .components
            .iter()
            .find(|c| c.kind == ComponentKind::Minecraft && c.enabled && !c.version.trim().is_empty())
            .map(|c| c.version.trim().to_string())
            .unwrap_or_else(|| instance.mc_version.clone());

        log_cb(format!(
            "Fetching net.minecraft ({}) from Prism Meta...",
            effective_mc_ver
        ));
        let mc_pkg = meta
            .fetch_prism_package("net.minecraft", &effective_mc_ver)
            .await?;

        main_class = mc_pkg
            .main_class
            .clone()
            .unwrap_or_else(|| "net.minecraft.client.main.Main".to_string());
        minecraft_args = mc_pkg.minecraft_arguments.clone().unwrap_or_else(|| {
            "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userType ${user_type} --versionType ${version_type}".to_string()
        });
        asset_index_info = mc_pkg.asset_index.clone();
        extra_jvm_args.extend(mc_pkg.plus_jvm_args.clone());
        if let Some(&major) = mc_pkg.compatible_java_majors.first() {
            required_java_major = major;
        }

        // Resolve Modloader component first so its libraries (e.g., ASM 9.7, Mixin) take precedence
        let loader_comp_opt = instance
            .components
            .iter()
            .find(|c| c.kind == ComponentKind::Loader);
        let loader_enabled = loader_comp_opt.map(|c| c.enabled).unwrap_or(true);
        let effective_loader_ver = loader_comp_opt
            .map(|c| c.version.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| instance.loader_version.clone());

        if loader_enabled {
        match instance.loader {
            LoaderType::Vanilla | LoaderType::OptiFine => {}
            LoaderType::Fabric | LoaderType::Quilt => {
                let mappings_comp = instance
                    .components
                    .iter()
                    .find(|c| c.kind == ComponentKind::Mappings);
                let mappings_enabled = mappings_comp.map(|c| c.enabled).unwrap_or(true);
                let mappings_ver = mappings_comp
                    .map(|c| c.version.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| effective_mc_ver.clone());

                if mappings_enabled {
                    // Both Fabric and Quilt require `net.fabricmc.intermediary` for the MC version
                    log_cb(format!(
                        "Fetching net.fabricmc.intermediary ({}) from Prism Meta...",
                        mappings_ver
                    ));
                    if let Ok(inter_pkg) = meta
                        .fetch_prism_package("net.fabricmc.intermediary", &mappings_ver)
                        .await
                    {
                        for lib in &inter_pkg.libraries {
                            process_library(
                                lib,
                                &libraries_dir,
                                &natives_dir,
                                &mut download_tasks,
                                &mut classpath_entries,
                                &mut seen_ga,
                                true,
                                true,
                            );
                        }
                    }
                }

                let uid = instance.loader.prism_uid().unwrap();
                let loader_ver = if effective_loader_ver.is_empty()
                    || effective_loader_ver == "latest"
                {
                    let idx = meta.fetch_prism_index(uid).await?;
                    idx.versions
                        .first()
                        .map(|v| v.version.clone())
                        .unwrap_or_else(|| "0.16.9".to_string())
                } else {
                    instance.loader_version.clone()
                };

                log_cb(format!(
                    "Fetching {} ({}) from Prism Meta...",
                    uid, loader_ver
                ));
                let loader_pkg = meta.fetch_prism_package(uid, &loader_ver).await?;
                if let Some(mc) = loader_pkg.main_class {
                    main_class = mc;
                }
                tweakers.extend(loader_pkg.tweakers);
                for lib in &loader_pkg.libraries {
                    process_library(
                        lib,
                        &libraries_dir,
                        &natives_dir,
                        &mut download_tasks,
                        &mut classpath_entries,
                        &mut seen_ga,
                        true,
                        true,
                    );
                }
            }
            LoaderType::Forge | LoaderType::NeoForge | LoaderType::LiteLoader => {
                let uid = instance.loader.prism_uid().unwrap();
                let loader_ver = if instance.loader_version.is_empty()
                    || instance.loader_version == "latest"
                {
                    let vers = meta
                        .fetch_loader_versions(&instance.loader, &instance.mc_version)
                        .await?;
                    vers.first()
                        .cloned()
                        .ok_or_else(|| {
                            anyhow!(
                                "В репозитории Prism не найдено версий {} для Minecraft {}",
                                instance.loader.as_str(),
                                instance.mc_version
                            )
                        })?
                } else {
                    instance.loader_version.clone()
                };

                log_cb(format!(
                    "Fetching {} ({}) from Prism Meta...",
                    uid, loader_ver
                ));
                let loader_pkg = meta.fetch_prism_package(uid, &loader_ver).await?;
                if let Some(mc) = loader_pkg.main_class {
                    main_class = mc;
                }
                if let Some(ma) = loader_pkg.minecraft_arguments {
                    minecraft_args = ma;
                }
                tweakers.extend(loader_pkg.tweakers);
                extra_jvm_args.extend(loader_pkg.plus_jvm_args);

                for lib in &loader_pkg.libraries {
                    process_library(
                        lib,
                        &libraries_dir,
                        &natives_dir,
                        &mut download_tasks,
                        &mut classpath_entries,
                        &mut seen_ga,
                        true,
                        true,
                    );
                }
                // `mavenFiles` in Prism Meta (such as Forge installer.jar used by ForgeWrapper) are downloaded to libraries/ but not put directly on -cp
                for mfile in &loader_pkg.maven_files {
                    process_library(
                        mfile,
                        &libraries_dir,
                        &natives_dir,
                        &mut download_tasks,
                        &mut classpath_entries,
                        &mut seen_ga,
                        false,
                        false,
                    );
                }

                // If Prism uses ForgeWrapper (`io.github.zekerzhayard.forgewrapper.installer.Main`), pass the required `-Dforgewrapper.*` JVM properties!
                if main_class.contains("forgewrapper") {
                    if let Some(inst_lib) = loader_pkg
                        .maven_files
                        .iter()
                        .find(|l| l.name.contains(":installer"))
                    {
                        let rel = inst_lib
                            .downloads
                            .as_ref()
                            .and_then(|d| d.artifact.as_ref())
                            .and_then(|a| a.path.clone())
                            .or_else(|| maven_coord_to_path(&inst_lib.name));
                        if let Some(rel_path) = rel {
                            let installer_path = libraries_dir.join(rel_path);
                            let mc_jar_path = libraries_dir
                                .join("com/mojang/minecraft")
                                .join(&instance.mc_version)
                                .join(format!("minecraft-{}-client.jar", instance.mc_version));
                            extra_jvm_args.push(format!(
                                "-Dforgewrapper.librariesDir={}",
                                libraries_dir.display()
                            ));
                            extra_jvm_args.push(format!(
                                "-Dforgewrapper.installer={}",
                                installer_path.display()
                            ));
                            extra_jvm_args.push(format!(
                                "-Dforgewrapper.minecraft={}",
                                mc_jar_path.display()
                            ));
                        }
                    }
                }
            }
            LoaderType::LabyMod | LoaderType::Horizon => unreachable!(),
        }
        }

        // Resolve LWJGL (`org.lwjgl3` or `org.lwjgl`) required by `net.minecraft`
        let lwjgl_comp = instance
            .components
            .iter()
            .find(|c| c.kind == ComponentKind::Lwjgl);
        let lwjgl_enabled = lwjgl_comp.map(|c| c.enabled).unwrap_or(true);
        if lwjgl_enabled {
            for req in &mc_pkg.requires {
                if req.uid == "org.lwjgl3" || req.uid == "org.lwjgl" {
                    let lwjgl_ver = lwjgl_comp
                        .map(|c| c.version.trim().to_string())
                        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
                        .unwrap_or_else(|| {
                            req.suggests
                                .clone()
                                .or_else(|| req.equals.clone())
                                .unwrap_or_else(|| "3.3.1".to_string())
                        });
                    log_cb(format!(
                        "Fetching {} ({}) from Prism Meta...",
                        req.uid, lwjgl_ver
                    ));
                    if let Ok(lwjgl_pkg) = meta.fetch_prism_package(&req.uid, &lwjgl_ver).await {
                        for lib in &lwjgl_pkg.libraries {
                            process_library(
                                lib,
                                &libraries_dir,
                                &natives_dir,
                                &mut download_tasks,
                                &mut classpath_entries,
                                &mut seen_ga,
                                true,
                                false,
                            );
                        }
                    }
                }
            }
        }

        // Base Minecraft libraries & mainJar
        for lib in &mc_pkg.libraries {
            process_library(
                lib,
                &libraries_dir,
                &natives_dir,
                &mut download_tasks,
                &mut classpath_entries,
                &mut seen_ga,
                true,
                true,
            );
        }

        let replace_jar_path = instance
            .components
            .iter()
            .find(|c| c.kind == ComponentKind::ReplaceJar && c.enabled && !c.file_path.trim().is_empty())
            .map(|c| PathBuf::from(c.file_path.trim()))
            .filter(|p| p.is_file());

        if let Some(custom_client) = replace_jar_path {
            log_cb(format!(
                "[Components] Replacing Minecraft.jar: {}",
                custom_client.display()
            ));
            classpath_entries.push(custom_client);
        } else if let Some(main_jar) = &mc_pkg.main_jar {
            if let Some(downloads) = &main_jar.downloads {
                if let Some(artifact) = &downloads.artifact {
                    let client_dest = libraries_dir
                        .join("com/mojang/minecraft")
                        .join(&effective_mc_ver)
                        .join(format!("minecraft-{}-client.jar", effective_mc_ver));
                    download_tasks.push(DownloadTask {
                        url: artifact.url.clone(),
                        dest: client_dest.clone(),
                        expected_sha1: artifact.sha1.clone(),
                        extract_natives_to: None,
                    });
                    classpath_entries.push(client_dest);
                }
            }
        }

        // Apply JarMod, JavaAgent, and CustomPatch / customized JSON components
        for comp in &instance.components {
            if !comp.enabled {
                continue;
            }
            match comp.kind {
                ComponentKind::JarMod => {
                    let p = PathBuf::from(comp.file_path.trim());
                    if p.is_file() {
                        log_cb(format!("[Components] JarMod -> Minecraft.jar: {}", p.display()));
                        classpath_entries.insert(0, p);
                    }
                }
                ComponentKind::JavaAgent => {
                    let p = comp.file_path.trim();
                    if !p.is_empty() {
                        let arg = if comp.extra_args.trim().is_empty() {
                            format!("-javaagent:{}", p)
                        } else {
                            format!("-javaagent:{}={}", p, comp.extra_args.trim())
                        };
                        log_cb(format!("[Components] Java Agent: {}", arg));
                        extra_jvm_args.push(arg);
                    }
                }
                ComponentKind::CustomPatch => {
                    let p = comp.file_path.trim();
                    if !p.is_empty() && p.to_ascii_lowercase().ends_with(".json") {
                        if let Ok(txt) = tokio::fs::read_to_string(p).await {
                            if let Ok(pkg) = serde_json::from_str::<PrismComponentPackage>(&txt) {
                                if let Some(mc) = pkg.main_class.filter(|s| !s.trim().is_empty()) {
                                    main_class = mc;
                                }
                                for t in pkg.tweakers {
                                    if !tweakers.contains(&t) {
                                        tweakers.push(t);
                                    }
                                }
                                extra_jvm_args.extend(pkg.plus_jvm_args);
                                for lib in &pkg.libraries {
                                    process_library(
                                        lib,
                                        &libraries_dir,
                                        &natives_dir,
                                        &mut download_tasks,
                                        &mut classpath_entries,
                                        &mut seen_ga,
                                        false,
                                        true,
                                    );
                                }
                                log_cb(format!("[Components] Applied JSON patch: {}", comp.name));
                            }
                        }
                    } else if !p.is_empty() && Path::new(p).is_file() {
                        classpath_entries.push(PathBuf::from(p));
                    }
                    if !comp.extra_args.trim().is_empty() {
                        main_class = comp.extra_args.trim().to_string();
                    }
                }
                _ => {
                    if comp.customized && !comp.file_path.trim().is_empty() && comp.file_path.trim().to_ascii_lowercase().ends_with(".json") {
                        if let Ok(txt) = tokio::fs::read_to_string(comp.file_path.trim()).await {
                            if let Ok(pkg) = serde_json::from_str::<PrismComponentPackage>(&txt) {
                                if let Some(mc) = pkg.main_class.filter(|s| !s.trim().is_empty()) {
                                    main_class = mc;
                                }
                                for t in pkg.tweakers {
                                    if !tweakers.contains(&t) {
                                        tweakers.push(t);
                                    }
                                }
                                extra_jvm_args.extend(pkg.plus_jvm_args);
                                for lib in &pkg.libraries {
                                    process_library(
                                        lib,
                                        &libraries_dir,
                                        &natives_dir,
                                        &mut download_tasks,
                                        &mut classpath_entries,
                                        &mut seen_ga,
                                        false,
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ========================================================================
    // OPTIFINE REPOSITORY RESOLUTION (`bmclapi2.bangbang93.com/optifine`)
    // ========================================================================
    if instance.loader == LoaderType::OptiFine || instance.optifine_enabled {
        log_cb(format!(
            "Checking OptiFine for Minecraft {}...",
            instance.mc_version
        ));
        let of_list = meta
            .fetch_optifine_versions(&instance.mc_version)
            .await
            .unwrap_or_default();

        let selected_of = if let Some(wanted) = &instance.optifine_version {
            of_list
                .iter()
                .find(|v| &v.full_version() == wanted || &v.patch == wanted)
                .or_else(|| of_list.first())
                .cloned()
        } else {
            of_list.first().cloned()
        };

        if let Some(of_entry) = selected_of {
            let url = meta.optifine_download_url(
                &of_entry.mcversion,
                &of_entry.of_type,
                &of_entry.patch,
            );
            let filename = if of_entry.filename.is_empty() {
                format!(
                    "OptiFine_{}_{}_{}.jar",
                    of_entry.mcversion, of_entry.of_type, of_entry.patch
                )
            } else {
                of_entry.filename.clone()
            };

            let target_path = if instance.loader == LoaderType::OptiFine {
                // Standalone OptiFine via LaunchWrapper
                let p = libraries_dir
                    .join("optifine/OptiFine")
                    .join(format!("{}_{}_{}", of_entry.mcversion, of_entry.of_type, of_entry.patch))
                    .join(&filename);
                classpath_entries.push(p.clone());

                // Add LaunchWrapper 1.12 for standalone OptiFine
                let lw_rel = "net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar";
                let lw_dest = libraries_dir.join(lw_rel);
                download_tasks.push(DownloadTask {
                    url: format!("https://libraries.minecraft.net/{}", lw_rel),
                    dest: lw_dest.clone(),
                    expected_sha1: None,
                    extract_natives_to: None,
                });
                classpath_entries.push(lw_dest);
                main_class = "net.minecraft.launchwrapper.Launch".to_string();
                tweakers.push("optifine.OptiFineTweaker".to_string());
                p
            } else {
                // Forge / NeoForge / Modloader mode: place OptiFine jar directly in instance `mods/`
                mods_dir.join(&filename)
            };

            log_cb(format!(
                "Added OptiFine {} ({}) -> {}",
                of_entry.full_version(),
                url,
                target_path.display()
            ));
            download_tasks.push(DownloadTask {
                url,
                dest: target_path,
                expected_sha1: None,
                extract_natives_to: None,
            });
        } else {
            log_cb(format!(
                "Warning: no OptiFine builds found in repository for Minecraft {}",
                instance.mc_version
            ));
        }
    }

    // ========================================================================
    // ADVANCED: CUSTOM LOADER OVERRIDE (.json profile, .jar / URL, MainClass)
    // ========================================================================
    if instance.use_custom_loader {
        let loader_label = if instance.custom_loader_name.trim().is_empty() {
            "Custom Loader"
        } else {
            instance.custom_loader_name.trim()
        };
        let path_or_url = instance.custom_loader_path.trim();
        log_cb(format!("[Custom Loader] Activated: {}", loader_label));

        if !path_or_url.is_empty() {
            let is_http = path_or_url.starts_with("http://") || path_or_url.starts_with("https://");
            let is_json = path_or_url
                .split('?')
                .next()
                .unwrap_or(path_or_url)
                .to_ascii_lowercase()
                .ends_with(".json");

            if is_json {
                let json_text_res: Result<String> = if is_http {
                    match meta.http().get(path_or_url).send().await {
                        Ok(r) if r.status().is_success() => r.text().await.map_err(Into::into),
                        Ok(r) => Err(anyhow!("HTTP {}", r.status())),
                        Err(e) => Err(e.into()),
                    }
                } else {
                    tokio::fs::read_to_string(path_or_url)
                        .await
                        .map_err(Into::into)
                };

                match json_text_res
                    .and_then(|txt| serde_json::from_str::<PrismComponentPackage>(&txt).map_err(Into::into))
                {
                    Ok(pkg) => {
                        if let Some(mc) = pkg.main_class.filter(|s| !s.trim().is_empty()) {
                            main_class = mc;
                        }
                        for t in pkg.tweakers {
                            if !tweakers.contains(&t) {
                                tweakers.push(t);
                            }
                        }
                        for j in pkg.plus_jvm_args {
                            extra_jvm_args.push(j);
                        }
                        for lib in &pkg.libraries {
                            process_library(
                                lib,
                                &libraries_dir,
                                &natives_dir,
                                &mut download_tasks,
                                &mut classpath_entries,
                                &mut seen_ga,
                                false,
                                true,
                            );
                        }
                        log_cb(format!(
                            "[Custom Loader] Loaded profile {} ({} libraries)",
                            loader_label,
                            pkg.libraries.len()
                        ));
                    }
                    Err(e) => {
                        log_cb(format!(
                            "[Custom Loader] Warning: failed to parse JSON {}: {}",
                            path_or_url, e
                        ));
                    }
                }
            } else if is_http {
                let fname = path_or_url
                    .split('?')
                    .next()
                    .unwrap_or(path_or_url)
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("custom-loader.jar");
                let dest = libraries_dir.join("custom").join(fname);
                download_tasks.push(DownloadTask {
                    url: path_or_url.to_string(),
                    dest: dest.clone(),
                    expected_sha1: None,
                    extract_natives_to: None,
                });
                classpath_entries.push(dest);
            } else {
                let local_jar = PathBuf::from(path_or_url);
                if local_jar.is_file() {
                    classpath_entries.push(local_jar);
                } else {
                    log_cb(format!(
                        "[Custom Loader] Warning: file not found: {}",
                        path_or_url
                    ));
                }
            }
        }

        if !instance.custom_loader_main_class.trim().is_empty() {
            main_class = instance.custom_loader_main_class.trim().to_string();
        }
    }

    // ========================================================================
    // ASSET INDEX RESOLUTION
    // ========================================================================
    let mut asset_index_id = instance.mc_version.clone();
    if let Some(asset_idx) = &asset_index_info {
        asset_index_id = asset_idx.id.clone();
        let idx_path = assets_dir
            .join("indexes")
            .join(format!("{}.json", asset_idx.id));

        if !verify_file_sha1(&idx_path, Some(&asset_idx.sha1)) {
            log_cb(format!(
                "Downloading asset index {}.json...",
                asset_idx.id
            ));
            let bytes = meta
                .http()
                .get(&asset_idx.url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            tokio::fs::write(&idx_path, &bytes).await?;
        }

        // Parse asset objects
        if let Ok(idx_text) = tokio::fs::read_to_string(&idx_path).await {
            if let Ok(idx_json) = serde_json::from_str::<serde_json::Value>(&idx_text) {
                if let Some(objects) = idx_json["objects"].as_object() {
                    for (_name, obj) in objects {
                        if let Some(hash) = obj["hash"].as_str() {
                            if hash.len() >= 2 {
                                let prefix = &hash[0..2];
                                let obj_dest =
                                    assets_dir.join("objects").join(prefix).join(hash);
                                if !obj_dest.exists() {
                                    let assets_base = if config.assets_server_url.trim().is_empty() {
                                        "https://resources.download.minecraft.net"
                                    } else {
                                        config.assets_server_url.trim().trim_end_matches('/')
                                    };
                                    let obj_url = format!("{}/{}/{}", assets_base, prefix, hash);
                                    download_tasks.push(DownloadTask {
                                        url: obj_url,
                                        dest: obj_dest,
                                        expected_sha1: Some(hash.to_string()),
                                        extract_natives_to: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ========================================================================
    // FILTER & DOWNLOAD LIBRARIES + ASSETS CONCURRENTLY
    // ========================================================================
    let mut pending_tasks = Vec::new();
    let mut seen_dests = HashSet::new();
    for task in download_tasks {
        if !seen_dests.insert(task.dest.clone()) {
            continue;
        }
        if verify_file_sha1(&task.dest, task.expected_sha1.as_deref()) {
            if let Some(nat_dir) = &task.extract_natives_to {
                let _ = extract_native_jar(&task.dest, nat_dir);
            }
            continue;
        }
        pending_tasks.push(task);
    }

    let total_files = pending_tasks.len();
    if total_files > 0 {
        log_cb(format!(
            "Скачивание {} файлов (потоков: {})...",
            total_files,
            config.download_concurrency.max(4)
        ));
        prog_cb(
            0.15,
            format!("Скачивание файлов (0 / {})...", total_files),
        );

        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let http = meta.http().clone();
        let concurrency = config.download_concurrency.clamp(4, 32);

        let results: Vec<Result<()>> = stream::iter(pending_tasks)
            .map(|task| {
                let http = http.clone();
                let completed = completed.clone();
                let prog_cb = prog_cb.clone();
                let log_cb = log_cb.clone();
                async move {
                    if let Some(parent) = task.dest.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    let resp = http
                        .get(&task.url)
                        .send()
                        .await
                        .with_context(|| format!("Ошибка загрузки {}", task.url))?
                        .error_for_status()?;
                    let bytes = resp.bytes().await?;
                    tokio::fs::write(&task.dest, &bytes).await?;

                    if let Some(nat_dir) = &task.extract_natives_to {
                        if let Err(e) = extract_native_jar(&task.dest, nat_dir) {
                            log_cb(format!(
                                "Warning extracting natives {:?}: {}",
                                task.dest.file_name(),
                                e
                            ));
                        }
                    }

                    let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if done % 10 == 0 || done == total_files {
                        let ratio = 0.15 + 0.75 * (done as f32 / total_files as f32);
                        prog_cb(
                            ratio,
                            format!("Скачано файлов: {} / {}", done, total_files),
                        );
                    }
                    Ok(())
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        let mut failed_count = 0usize;
        for r in results {
            if let Err(e) = r {
                failed_count += 1;
                log_cb(format!("File download error: {}", e));
            }
        }
        if failed_count > 0 {
            log_cb(format!(
                "Warning: failed to download {} files, continuing launch...",
                failed_count
            ));
        }
    } else {
        log_cb("All libraries and assets are already cached and SHA-1 verified.".to_string());
    }

    // ========================================================================
    // FREESMLAUNCHER ELY.BY SKIN MODE, AUTHLIB-INJECTOR & UNMOJANG/LOKI SUPPORT
    // ========================================================================
    prog_cb(0.92, "Подготовка параметров JVM и авторизации...".to_string());
    let elyby_mode = if instance.override_elyby {
        instance.custom_elyby_skin_mode
    } else {
        config.elyby_skin_mode
    };
    // elyby_mode: 0=Never, 1=Always (except custom), 2=With Ely.by, 3=With Ely.by & Offline
    let effective_auth_url: Option<String> = match account.account_type {
        AccountType::CustomYggdrasil | AccountType::LittleSkin => account.auth_server_url.clone(),
        AccountType::ElyBy => {
            if elyby_mode == 0 {
                None
            } else {
                Some(
                    account
                        .auth_server_url
                        .clone()
                        .unwrap_or_else(|| "https://authserver.ely.by".to_string()),
                )
            }
        }
        AccountType::Offline => {
            if elyby_mode == 1 || elyby_mode == 3 {
                Some("https://authserver.ely.by".to_string())
            } else {
                None
            }
        }
        AccountType::Microsoft => {
            if elyby_mode == 1 {
                Some("https://authserver.ely.by".to_string())
            } else {
                None
            }
        }
    };

    if let Some(api_url) = effective_auth_url {
        let effective_backend = if instance.override_elyby {
            AuthAgentBackend::from_index(instance.custom_auth_backend)
        } else {
            config.auth_agent_backend
        };
        let effective_version = if instance.override_elyby && !instance.custom_auth_version.trim().is_empty() {
            instance.custom_auth_version.as_str()
        } else {
            config.auth_agent_version.as_str()
        };
        let custom_jar = PathBuf::from(config.custom_patcher_path.trim());
        let use_custom_jar = !config.custom_patcher_path.trim().is_empty() && custom_jar.is_file();

        match effective_backend {
            AuthAgentBackend::Disabled => {
                log_cb("[Auth] Authlib patcher agent is disabled in settings.".to_string());
            }
            AuthAgentBackend::Loki => {
                if use_custom_jar {
                    log_cb(format!(
                        "[unmojang/Loki] Using custom .jar: {}",
                        custom_jar.display()
                    ));
                    extra_jvm_args.push(format!(
                        "-javaagent:{}={}",
                        custom_jar.display(),
                        api_url
                    ));
                } else {
                    log_cb(format!(
                        "[unmojang/Loki] Подготовка Loki ({}) для сервера авторизации: {}",
                        effective_version, api_url
                    ));
                    match ensure_loki_agent(meta.http(), &libraries_dir, effective_version).await {
                        Ok(loki_jar) => {
                            extra_jvm_args.push(format!(
                                "-javaagent:{}={}",
                                loki_jar.display(),
                                api_url
                            ));
                        }
                        Err(e) => {
                            log_cb(format!(
                                "[unmojang/Loki] Ошибка ({}), откат на authlib-injector...",
                                e
                            ));
                            let injector_jar =
                                ensure_authlib_injector(meta.http(), &libraries_dir, "latest").await?;
                            extra_jvm_args.push(format!(
                                "-javaagent:{}={}",
                                injector_jar.display(),
                                api_url
                            ));
                        }
                    }
                }
            }
            AuthAgentBackend::AuthlibInjector => {
                let injector_jar = if use_custom_jar {
                    log_cb(format!(
                        "[authlib-injector] Using custom .jar: {}",
                        custom_jar.display()
                    ));
                    custom_jar
                } else {
                    log_cb(format!(
                        "[authlib-injector] Активация ({}) для сервера авторизации: {}",
                        effective_version, api_url
                    ));
                    ensure_authlib_injector(meta.http(), &libraries_dir, effective_version).await?
                };
                if config.patcher_prefetch_metadata {
                    if let Some(prefetched) =
                        fetch_yggdrasil_prefetched_base64(meta.http(), &api_url).await
                    {
                        extra_jvm_args.push(format!(
                            "-Dauthlibinjector.yggdrasil.prefetched={}",
                            prefetched
                        ));
                    }
                }
                extra_jvm_args.push(format!(
                    "-javaagent:{}={}",
                    injector_jar.display(),
                    api_url
                ));
            }
        }
    }

    // System GLFW / OpenAL on Linux (global & per-instance overrides!)
    if cfg!(target_os = "linux") {
        let use_glfw = if instance.override_linux_tweaks {
            instance.custom_use_system_glfw
        } else {
            config.use_system_glfw
        };
        let glfw_custom = if instance.override_linux_tweaks && !instance.custom_glfw_path.trim().is_empty() {
            Some(instance.custom_glfw_path.trim().to_string())
        } else if !config.custom_glfw_path.trim().is_empty() {
            Some(config.custom_glfw_path.trim().to_string())
        } else {
            None
        };
        if use_glfw {
            if let Some(glfw) = glfw_custom.or_else(|| find_system_library(&["libglfw.so", "libglfw.so.3"])) {
                log_cb(format!("[Linux] Using system GLFW: {}", glfw));
                extra_jvm_args.push(format!("-Dorg.lwjgl.glfw.libname={}", glfw));
            }
        }

        let use_openal = if instance.override_linux_tweaks {
            instance.custom_use_system_openal
        } else {
            config.use_system_openal
        };
        let openal_custom = if instance.override_linux_tweaks && !instance.custom_openal_path.trim().is_empty() {
            Some(instance.custom_openal_path.trim().to_string())
        } else if !config.custom_openal_path.trim().is_empty() {
            Some(config.custom_openal_path.trim().to_string())
        } else {
            None
        };
        if use_openal {
            if let Some(al) = openal_custom.or_else(|| find_system_library(&["libopenal.so", "libopenal.so.1"])) {
                log_cb(format!("[Linux] Using system OpenAL: {}", al));
                extra_jvm_args.push(format!("-Dorg.lwjgl.openal.libname={}", al));
            }
        }
    }

    // ========================================================================
    // CONSTRUCT JVM & GAME ARGUMENTS (Always Auto-Download Java per MC + Overrides)
    // ========================================================================
    let java_bin = if instance.override_java {
        instance
            .custom_java_path
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| config.java_path.clone())
    } else if !config.auto_download_java {
        config.java_path.clone()
    } else {
        match ensure_managed_java(
            meta.http(),
            &java_dir,
            required_java_major,
            log_cb.clone(),
            prog_cb.clone(),
        )
        .await
        {
            Ok(managed_bin) => managed_bin.to_string_lossy().to_string(),
            Err(e) => {
                log_cb(format!(
                    "[Java] Не удалось скачать Java {} ({}), используется системная: {}",
                    required_java_major, e, config.java_path
                ));
                config.java_path.clone()
            }
        }
    };

    let max_mem = if instance.override_memory {
        instance
            .custom_max_memory_mb
            .unwrap_or(config.max_memory_mb)
            .max(8)
    } else {
        config.max_memory_mb.max(8)
    };
    let min_mem = if instance.override_memory {
        instance
            .custom_min_memory_mb
            .unwrap_or(config.min_memory_mb)
            .min(max_mem)
            .max(8)
    } else {
        config.min_memory_mb.min(max_mem).max(8)
    };

    let mut jvm_args: Vec<String> = Vec::new();
    jvm_args.push(format!("-Xms{}M", min_mem));
    jvm_args.push(format!("-Xmx{}M", max_mem));

    let user_jvm_str = if instance.override_jvm_args {
        instance
            .custom_jvm_args
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&config.jvm_args)
    } else {
        &config.jvm_args
    };
    for arg in user_jvm_str.split_whitespace() {
        jvm_args.push(arg.to_string());
    }

    jvm_args.push(format!("-Djava.library.path={}", natives_dir.display()));
    jvm_args.push(format!("-Djna.tmpdir={}", natives_dir.display()));
    jvm_args.push(format!(
        "-Dorg.lwjgl.system.SharedLibraryExtractPath={}",
        natives_dir.display()
    ));
    jvm_args.push(format!(
        "-Dio.netty.native.workdir={}",
        natives_dir.display()
    ));
    jvm_args.push("-Dminecraft.launcher.brand=AxyLauncher".to_string());
    jvm_args.push("-Dminecraft.launcher.version=0.1.0".to_string());

    for arg in extra_jvm_args {
        jvm_args.push(arg);
    }

    // Build deduplicated classpath string
    let mut unique_cp = Vec::new();
    let mut cp_set = HashSet::new();
    for p in classpath_entries {
        if cp_set.insert(p.clone()) {
            unique_cp.push(p.to_string_lossy().to_string());
        }
    }
    let cp_separator = if cfg!(windows) { ";" } else { ":" };
    let classpath_str = unique_cp.join(cp_separator);

    jvm_args.push("-cp".to_string());
    jvm_args.push(classpath_str);
    jvm_args.push(main_class.clone());

    // Replace placeholders in minecraftArguments
    let mut replacements: HashMap<&str, String> = HashMap::new();
    replacements.insert("${auth_player_name}", account.username.clone());
    replacements.insert("${version_name}", instance.mc_version.clone());
    replacements.insert(
        "${game_directory}",
        instance_dir.to_string_lossy().to_string(),
    );
    replacements.insert("${assets_root}", assets_dir.to_string_lossy().to_string());
    replacements.insert("${game_assets}", assets_dir.to_string_lossy().to_string());
    replacements.insert("${assets_index_name}", asset_index_id);
    replacements.insert("${auth_uuid}", account.uuid.clone());
    replacements.insert("${auth_access_token}", account.access_token.clone());
    replacements.insert("${auth_session}", account.access_token.clone());
    replacements.insert(
        "${user_type}",
        account.account_type.user_type_arg().to_string(),
    );
    replacements.insert(
        "${version_type}",
        format!("AxyLauncher/{}", instance.loader.as_str()),
    );
    replacements.insert("${user_properties}", "{}".to_string());
    replacements.insert("${clientid}", "0".to_string());
    replacements.insert("${auth_xuid}", "0".to_string());

    let mut game_args: Vec<String> = Vec::new();
    for raw_token in minecraft_args.split_whitespace() {
        let mut token = raw_token.to_string();
        for (k, v) in &replacements {
            if token.contains(k) {
                token = token.replace(k, v);
            }
        }
        game_args.push(token);
    }

    let (win_w, win_h) = if instance.override_window {
        (
            instance.custom_window_width.unwrap_or(config.window_width),
            instance.custom_window_height.unwrap_or(config.window_height),
        )
    } else {
        (config.window_width, config.window_height)
    };
    if win_w > 0 && win_h > 0 {
        game_args.push("--width".to_string());
        game_args.push(win_w.to_string());
        game_args.push("--height".to_string());
        game_args.push(win_h.to_string());
    }
    let start_max = if instance.override_window {
        instance.custom_start_maximized
    } else {
        config.start_maximized
    };
    if start_max {
        game_args.push("--fullscreen".to_string());
    }

    // Quick Play server OR singleplayer world auto-join (Freesm / Prism MinecraftSettingsWidget!)
    let qp_world = instance.quick_play_world.trim();
    let qp_server = instance.quick_play_server.trim();
    if instance.enable_auto_join || !qp_server.is_empty() || !qp_world.is_empty() {
        if !qp_world.is_empty() {
            game_args.push("--quickPlaySingleplayer".to_string());
            game_args.push(qp_world.to_string());
            log_cb(format!("[Auto-Join] Auto-joining singleplayer world: {}", qp_world));
        } else if !qp_server.is_empty() {
            let minor: u32 = instance
                .mc_version
                .split('.')
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(20);
            if instance.mc_version.starts_with("26.") || minor >= 20 {
                game_args.push("--quickPlayMultiplayer".to_string());
                game_args.push(qp_server.to_string());
            } else {
                let (host, port) = qp_server.split_once(':').unwrap_or((qp_server, "25565"));
                game_args.push("--server".to_string());
                game_args.push(host.to_string());
                game_args.push("--port".to_string());
                game_args.push(port.to_string());
            }
            log_cb(format!("[Auto-Join] Auto-connecting to server: {}", qp_server));
        }
    }

    for tweaker in &tweakers {
        game_args.push("--tweakClass".to_string());
        game_args.push(tweaker.clone());
    }

    let mut full_args = jvm_args;
    full_args.extend(game_args);

    let pre_cmd = if instance.override_commands {
        instance.custom_pre_launch_cmd.clone().unwrap_or_default()
    } else {
        config.pre_launch_cmd.clone()
    };
    let post_cmd = if instance.override_commands {
        instance.custom_post_exit_cmd.clone().unwrap_or_default()
    } else {
        config.post_exit_cmd.clone()
    };
    let wrapper = if instance.override_commands {
        instance.custom_wrapper_cmd.clone().unwrap_or_default()
    } else {
        config.wrapper_cmd.clone()
    };

    if prepare_only {
        prog_cb(1.0, format!("Сборка «{}» полностью загружена!", instance.name));
        log_cb(format!(
            "=== Instance '{}' downloaded and ready to launch ===",
            instance.name
        ));
        return Ok(LaunchOutcome {
            exit_code: 0,
            played_minutes: 0,
        });
    }

    if !pre_cmd.trim().is_empty() {
        log_cb(format!("[Pre-launch] {}", pre_cmd));
        let _ = if cfg!(windows) {
            Command::new("cmd")
                .args(["/C", &pre_cmd])
                .current_dir(&instance_dir)
                .status()
                .await
        } else {
            Command::new("sh")
                .args(["-c", &pre_cmd])
                .current_dir(&instance_dir)
                .status()
                .await
        };
    }

    let use_gamemode = cfg!(target_os = "linux")
        && if instance.override_linux_tweaks {
            instance.custom_enable_gamemode
        } else {
            config.enable_gamemode
        };
    let use_mangohud = cfg!(target_os = "linux")
        && if instance.override_linux_tweaks {
            instance.custom_enable_mangohud
        } else {
            config.enable_mangohud
        };
    let use_dgpu = cfg!(target_os = "linux")
        && if instance.override_linux_tweaks {
            instance.custom_use_discrete_gpu
        } else {
            config.use_discrete_gpu
        };
    let use_zink = cfg!(target_os = "linux")
        && if instance.override_linux_tweaks {
            instance.custom_use_zink
        } else {
            config.use_zink
        };

    let mut wrapper_tokens: Vec<String> = Vec::new();
    if use_gamemode {
        wrapper_tokens.push("gamemoderun".to_string());
    }
    if use_mangohud {
        wrapper_tokens.push("mangohud".to_string());
    }
    for tok in wrapper.split_whitespace() {
        wrapper_tokens.push(tok.to_string());
    }

    log_cb(format!(
        "Starting Java {} (RAM: {}M-{}M, MainClass: {})",
        java_bin, min_mem, max_mem, main_class
    ));
    prog_cb(0.98, "Запуск процесса Minecraft...".to_string());

    let mut cmd = if !wrapper_tokens.is_empty() {
        let mut c = Command::new(&wrapper_tokens[0]);
        for t in &wrapper_tokens[1..] {
            c.arg(t);
        }
        c.arg(&java_bin);
        c
    } else {
        Command::new(&java_bin)
    };

    // Standard Prism/FreesmLauncher environment variables for wrapper/game/hooks
    cmd.env("INST_NAME", &instance.name);
    cmd.env("INST_ID", &instance.id);
    cmd.env("INST_DIR", &instance_root);
    cmd.env("INST_MC_DIR", &instance_dir);
    cmd.env("INST_JAVA", &java_bin);

    if config.force_x11_backend && cfg!(target_os = "linux") {
        cmd.env("GLFW_PLATFORM", "x11");
        cmd.env("SDL_VIDEODRIVER", "x11");
        log_cb("[Linux] Принудительно включён X11 backend (GLFW_PLATFORM=x11)".to_string());
    }
    if config.disable_hidpi_scaling {
        cmd.env("GDK_SCALE", "1");
        cmd.env("QT_AUTO_SCREEN_SCALE_FACTOR", "0");
    }
    if use_dgpu {
        cmd.env("DRI_PRIME", "1");
        cmd.env("__NV_PRIME_RENDER_OFFLOAD", "1");
        cmd.env("__GLX_VENDOR_LIBRARY_NAME", "nvidia");
    }
    if use_zink {
        cmd.env("MESA_LOADER_DRIVER_OVERRIDE", "zink");
        cmd.env("__GLX_VENDOR_LIBRARY_NAME", "mesa");
        log_cb("[Linux] Включён рендерер Zink (OpenGL over Vulkan)".to_string());
    }
    let env_str = if instance.override_env {
        instance.custom_env_vars.clone().unwrap_or_default()
    } else {
        config.env_vars.clone()
    };
    for line in env_str.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            if !key.is_empty() {
                cmd.env(key, v.trim());
            }
        }
    }

    let mut child = cmd
        .args(&full_args)
        .current_dir(&instance_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "Не удалось запустить Java по пути «{}». Проверьте настройки Java в лаунчере.",
                java_bin
            )
        })?;

    prog_cb(1.0, format!("Minecraft {} запущен!", instance.mc_version));
    stream_child_logs(&mut child, log_cb.clone());

    // Start FreesmLauncher automatic in-game screenshot -> clipboard watcher!
    let screenshots_dir = instance_dir.join("screenshots");
    let _ = tokio::fs::create_dir_all(&screenshots_dir).await;
    let do_copy_screenshots = if instance.override_window {
        instance.custom_copy_screenshots
    } else {
        config.auto_copy_screenshots
    };
    let screenshot_stop = if do_copy_screenshots {
        Some(spawn_screenshot_watcher(screenshots_dir, log_cb.clone()))
    } else {
        None
    };

    let exit_code = tokio::select! {
        status = child.wait() => {
            let s = status?;
            log_cb(format!(
                "=== Minecraft process exited with code: {} ===",
                s
            ));
            s.code().unwrap_or(0)
        }
        _ = &mut kill_rx => {
            log_cb("=== Stopping Minecraft process by user request ===".to_string());
            let _ = child.kill().await;
            let _ = child.wait().await;
            -1
        }
    };

    if let Some(stop_tx) = screenshot_stop {
        let _ = stop_tx.send(true);
    }

    if !post_cmd.trim().is_empty() {
        log_cb(format!("[Post-exit] {}", post_cmd));
        let _ = if cfg!(windows) {
            Command::new("cmd")
                .args(["/C", &post_cmd])
                .current_dir(&instance_dir)
                .status()
                .await
        } else {
            Command::new("sh")
                .args(["-c", &post_cmd])
                .current_dir(&instance_dir)
                .status()
                .await
        };
    }

    prog_cb(0.0, "Готов".to_string());

    Ok(LaunchOutcome {
        exit_code,
        played_minutes: session_start.elapsed().as_secs() / 60,
    })
}

/// Infer required Java major version from Minecraft version string
pub fn infer_java_major_from_mc(mc_ver: &str) -> u32 {
    if mc_ver.starts_with("26.") || mc_ver.starts_with("25.") {
        return 25;
    }
    let parts: Vec<u32> = mc_ver
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect();
    if parts.len() >= 2 {
        let minor = parts[1];
        let patch = parts.get(2).copied().unwrap_or(0);
        if minor >= 21 || (minor == 20 && patch >= 5) {
            return 21;
        } else if minor >= 17 {
            return 17;
        } else {
            return 8;
        }
    }
    21
}

/// Automatically download & cache a dedicated Adoptium Temurin Java runtime per major version (8 / 17 / 21 / 25)
pub async fn ensure_managed_java<FLog, FProg>(
    http: &reqwest::Client,
    java_dir: &Path,
    major: u32,
    log_cb: Arc<FLog>,
    prog_cb: Arc<FProg>,
) -> Result<PathBuf>
where
    FLog: Fn(String) + Send + Sync + 'static,
    FProg: Fn(f32, String) + Send + Sync + 'static,
{
    let java_root = java_dir.join(format!("java-{}", major));
    if let Some(existing) = find_java_bin_in_dir(&java_root) {
        log_cb(format!(
            "[Java {}] Using installed managed Java: {}",
            major,
            existing.display()
        ));
        return Ok(existing);
    }

    tokio::fs::create_dir_all(&java_root).await?;
    log_cb(format!(
        "[Java {}] Downloading dedicated Java {} for this Minecraft version...",
        major, major
    ));
    prog_cb(
        0.88,
        format!("Скачивание Java {} (Adoptium Temurin)...", major),
    );

    let adoptium_os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let adoptium_arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };

    let jre_url = format!(
        "https://api.adoptium.net/v3/binary/latest/{}/ga/{}/{}/jre/hotspot/normal/eclipse",
        major, adoptium_os, adoptium_arch
    );
    let jdk_url = format!(
        "https://api.adoptium.net/v3/binary/latest/{}/ga/{}/{}/jdk/hotspot/normal/eclipse",
        major, adoptium_os, adoptium_arch
    );

    let resp = match http.get(&jre_url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => http
            .get(&jdk_url)
            .send()
            .await
            .context("Не удалось скачать архив Java")?
            .error_for_status()?,
    };

    let bytes = resp.bytes().await?;
    let is_zip = cfg!(target_os = "windows");
    let ext = if is_zip { "zip" } else { "tar.gz" };
    let archive_path = java_root.join(format!("temurin-{}.{}", major, ext));
    tokio::fs::write(&archive_path, &bytes).await?;

    prog_cb(0.93, format!("Распаковка Java {}...", major));
    if is_zip {
        let zip_file = std::fs::File::open(&archive_path)?;
        let mut zip = zip::ZipArchive::new(zip_file)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
                continue;
            };
            let out_p = java_root.join(rel);
            if entry.is_dir() {
                let _ = std::fs::create_dir_all(&out_p);
            } else {
                if let Some(parent) = out_p.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let mut out_f = std::fs::File::create(&out_p)?;
                std::io::copy(&mut entry, &mut out_f)?;
            }
        }
        let _ = tokio::fs::remove_file(&archive_path).await;
    } else {
        let status = Command::new("tar")
            .args([
                "-xzf",
                &archive_path.to_string_lossy(),
                "-C",
                &java_root.to_string_lossy(),
            ])
            .status()
            .await?;
        let _ = tokio::fs::remove_file(&archive_path).await;
        if !status.success() {
            return Err(anyhow!("Ошибка распаковки архива Java {}", major));
        }
    }

    find_java_bin_in_dir(&java_root)
        .ok_or_else(|| anyhow!("Исполняемый файл Java не найден после распаковки Java {}", major))
}

fn find_java_bin_in_dir(dir: &Path) -> Option<PathBuf> {
    let bin_names: &[&str] = if cfg!(windows) {
        &["javaw.exe", "java.exe"]
    } else {
        &["java"]
    };
    for b in bin_names {
        let direct = dir.join("bin").join(b);
        if direct.exists() {
            return Some(direct);
        }
        let mac_home = dir.join("Contents/Home/bin").join(b);
        if mac_home.exists() {
            return Some(mac_home);
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                for b in bin_names {
                    let candidate = p.join("bin").join(b);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                    let mac_candidate = p.join("Contents/Home/bin").join(b);
                    if mac_candidate.exists() {
                        return Some(mac_candidate);
                    }
                }
            }
        }
    }
    None
}

fn stream_child_logs<FLog>(child: &mut tokio::process::Child, log_cb: Arc<FLog>)
where
    FLog: Fn(String) + Send + Sync + 'static,
{
    if let Some(stdout) = child.stdout.take() {
        let log_cb_out = log_cb.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log_cb_out(format!("[STDOUT] {}", line));
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let log_cb_err = log_cb.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log_cb_err(format!("[STDERR] {}", line));
            }
        });
    }
}

fn find_first_executable_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name()?.to_string_lossy().to_lowercase();
            if name.contains("horizon") && !name.ends_with(".txt") && !name.ends_with(".md") {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(m) = std::fs::metadata(&path) {
                        let mut p = m.permissions();
                        p.set_mode(0o755);
                        let _ = std::fs::set_permissions(&path, p);
                    }
                }
                return Some(path);
            }
        } else if path.is_dir() {
            if let Some(found) = find_first_executable_in_dir(&path) {
                return Some(found);
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct JavaInstallationInfo {
    pub version: String,
    pub arch: String,
    pub path: String,
    pub is_managed: bool,
}

fn probe_java_version_and_arch(java_bin: &Path) -> (String, String) {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "amd64"
    }
    .to_string();
    let out = std::process::Command::new(java_bin).arg("-version").output();
    if let Ok(o) = out {
        let txt = format!(
            "{}\n{}",
            String::from_utf8_lossy(&o.stderr),
            String::from_utf8_lossy(&o.stdout)
        );
        if let Some(first_q) = txt.find('"') {
            if let Some(end_q) = txt[first_q + 1..].find('"') {
                let ver = txt[first_q + 1..first_q + 1 + end_q].to_string();
                let det_arch = if txt.contains("aarch64") || txt.contains("arm64") {
                    "aarch64".to_string()
                } else if txt.contains("64-Bit") {
                    "amd64".to_string()
                } else {
                    arch
                };
                return (ver, det_arch);
            }
        }
    }
    ("unknown".to_string(), arch)
}

/// Scan managed launcher Java installations and system JVM directories (like FreesmLauncher Java -> Installations)
pub fn scan_java_installations(java_dir: &Path) -> Vec<JavaInstallationInfo> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let bin_name = if cfg!(windows) { "java.exe" } else { "java" };

    // 1. Scan managed java_dir
    if let Ok(entries) = std::fs::read_dir(java_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(jb) = find_java_bin_in_dir(&p) {
                    let s = jb.to_string_lossy().to_string();
                    if seen.insert(s.clone()) {
                        let (ver, arch) = probe_java_version_and_arch(&jb);
                        found.push(JavaInstallationInfo {
                            version: ver,
                            arch,
                            path: s,
                            is_managed: true,
                        });
                    }
                }
            }
        }
    }

    // 2. Scan system JVM directories
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "linux") {
        for base in ["/usr/lib/jvm", "/usr/lib64/jvm", "/opt"] {
            if let Ok(entries) = std::fs::read_dir(base) {
                for e in entries.flatten() {
                    let jb = e.path().join("bin").join("java");
                    if jb.is_file() {
                        candidates.push(jb);
                    }
                }
            }
        }
        let sys_java = PathBuf::from("/usr/bin/java");
        if sys_java.is_file() {
            candidates.push(sys_java);
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(entries) = std::fs::read_dir("/Library/Java/JavaVirtualMachines") {
            for e in entries.flatten() {
                let jb = e.path().join("Contents/Home/bin/java");
                if jb.is_file() {
                    candidates.push(jb);
                }
            }
        }
    }

    for jb in candidates {
        let canon = std::fs::canonicalize(&jb).unwrap_or(jb);
        let s = canon.to_string_lossy().to_string();
        if seen.insert(s.clone()) {
            let (ver, arch) = probe_java_version_and_arch(&canon);
            found.push(JavaInstallationInfo {
                version: ver,
                arch,
                path: s,
                is_managed: false,
            });
        }
    }

    found.sort_by(|a, b| a.version.cmp(&b.version));
    found
}
