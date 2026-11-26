use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub const APP_VERSION: &str = "0.2.0";
pub const GITHUB_REPO: &str = "Xeone52670v3/AxyLauncher";

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum InstallKind {
    WindowsMsi {
        install_dir: PathBuf,
        exe_path: PathBuf,
    },
    WindowsPortableExe {
        exe_path: PathBuf,
    },
    LinuxAppImage {
        appimage_path: PathBuf,
    },
    LinuxFlatpak,
    LinuxSystemPackage,
    MacOsAppBundle {
        app_bundle_path: PathBuf,
    },
    MacOsManual,
}

impl InstallKind {
    pub fn label(&self) -> &'static str {
        match self {
            InstallKind::WindowsMsi { .. } => "Windows MSI",
            InstallKind::WindowsPortableExe { .. } => "Windows Portable (.exe)",
            InstallKind::LinuxAppImage { .. } => "Linux AppImage",
            InstallKind::LinuxFlatpak => "Linux Flatpak",
            InstallKind::LinuxSystemPackage => "Linux Package (Manual)",
            InstallKind::MacOsAppBundle { .. } => "macOS App (.dmg)",
            InstallKind::MacOsManual => "macOS (.dmg)",
        }
    }

    pub fn supports_auto_install(&self) -> bool {
        !matches!(self, InstallKind::LinuxSystemPackage)
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseCheckResult {
    pub tag: String,
    pub html_url: String,
    pub is_newer: bool,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub install_kind: InstallKind,
}

pub fn detect_install_kind() -> InstallKind {
    #[allow(unused_variables)]
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("axylauncher"));

    #[cfg(target_os = "windows")]
    {
        let install_dir = exe_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        if is_windows_msi_install(&exe_path) {
            return InstallKind::WindowsMsi {
                install_dir,
                exe_path,
            };
        }
        return InstallKind::WindowsPortableExe { exe_path };
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(appimage) = std::env::var_os("APPIMAGE") {
            let p = PathBuf::from(appimage);
            if !p.as_os_str().is_empty() {
                return InstallKind::LinuxAppImage { appimage_path: p };
            }
        }
        if std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
            return InstallKind::LinuxFlatpak;
        }
        return InstallKind::LinuxSystemPackage;
    }

    #[cfg(target_os = "macos")]
    {
        for ancestor in exe_path.ancestors() {
            if ancestor
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("app"))
                .unwrap_or(false)
            {
                return InstallKind::MacOsAppBundle {
                    app_bundle_path: ancestor.to_path_buf(),
                };
            }
        }
        return InstallKind::MacOsManual;
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = exe_path;
        InstallKind::LinuxSystemPackage
    }
}

#[cfg(target_os = "windows")]
fn is_windows_msi_install(exe_path: &Path) -> bool {
    if let Ok(out) = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Xeone52670v3\AxyLauncher",
            "/v",
            "installed",
        ])
        .output()
    {
        if out.status.success() {
            let s = exe_path.to_string_lossy().to_lowercase();
            if s.contains("\\appdata\\local\\axylauncher\\")
                || s.contains("\\program files\\axylauncher\\")
            {
                return true;
            }
        }
    }
    false
}

pub fn parse_ver_tuple(raw: &str) -> Vec<u64> {
    let clean = raw.trim().trim_start_matches(['v', 'V']);
    let core = clean.split(['-', '+']).next().unwrap_or(clean);
    let mut parts: Vec<u64> = core
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect();
    while parts.len() > 1 && *parts.last().unwrap_or(&0) == 0 {
        parts.pop();
    }
    parts
}

fn match_asset_for_kind(
    assets: &[serde_json::Value],
    kind: &InstallKind,
) -> Option<(String, String)> {
    let find_ext = |ext: &str| -> Option<(String, String)> {
        let ext_low = ext.to_lowercase();
        for a in assets {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let url = a
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !url.is_empty() && name.to_lowercase().ends_with(&ext_low) {
                return Some((name.to_string(), url.to_string()));
            }
        }
        None
    };

    match kind {
        InstallKind::WindowsMsi { .. } => find_ext(".msi").or_else(|| find_ext(".exe")),
        InstallKind::WindowsPortableExe { .. } => find_ext(".exe"),
        InstallKind::LinuxAppImage { .. } => find_ext(".appimage"),
        InstallKind::LinuxFlatpak => find_ext(".flatpak"),
        InstallKind::LinuxSystemPackage => None,
        InstallKind::MacOsAppBundle { .. } | InstallKind::MacOsManual => find_ext(".dmg"),
    }
}

pub async fn check_latest_release(client: &Client) -> Result<Option<ReleaseCheckResult>> {
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);
    let repo_releases_url = format!("https://github.com/{}/releases", GITHUB_REPO);

    let resp = client
        .get(&api_url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", format!("AxyLauncher/{}", APP_VERSION))
        .send()
        .await
        .context("GitHub API request failed")?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(anyhow!("GitHub API status {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().await.context("Invalid GitHub JSON")?;
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let html_url = json
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or(&repo_releases_url)
        .to_string();
    let assets = json
        .get("assets")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let install_kind = detect_install_kind();
    let cur_v = parse_ver_tuple(APP_VERSION);
    let latest_v = parse_ver_tuple(&tag);
    let is_newer = !tag.is_empty() && latest_v > cur_v;
    let matched = match_asset_for_kind(&assets, &install_kind);

    Ok(Some(ReleaseCheckResult {
        tag,
        html_url,
        is_newer,
        asset_name: matched.as_ref().map(|m| m.0.clone()),
        asset_url: matched.as_ref().map(|m| m.1.clone()),
        install_kind,
    }))
}

async fn download_file_with_progress<F>(
    client: &Client,
    url: &str,
    dest: &Path,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let resp = client
        .get(url)
        .header("User-Agent", format!("AxyLauncher/{}", APP_VERSION))
        .send()
        .await
        .context("Download request failed")?;
    if !resp.status().is_success() {
        return Err(anyhow!("Download HTTP status {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("Failed to create {}", dest.display()))?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_reported: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("Download stream error")?;
        file.write_all(&bytes).await?;
        downloaded += bytes.len() as u64;
        if downloaded - last_reported >= 256 * 1024 || (total > 0 && downloaded >= total) {
            last_reported = downloaded;
            on_progress(downloaded, total);
        }
    }
    file.flush().await?;
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Downloads the update asset and spawns a detached platform-specific updater helper.
/// Returns `Ok(true)` if the caller should immediately exit the current process so the
/// updater can replace the running executable/package and relaunch it.
pub async fn download_and_apply_update<F>(
    client: &Client,
    asset_url: &str,
    asset_name: &str,
    install_kind: &InstallKind,
    on_progress: F,
) -> Result<bool>
where
    F: FnMut(u64, u64),
{
    let safe_name = sanitize_filename(asset_name);
    let pid = std::process::id();

    match install_kind {
        InstallKind::LinuxSystemPackage => {
            Err(anyhow!("System package installation must be updated manually"))
        }

        InstallKind::WindowsMsi {
            install_dir,
            exe_path,
        } => {
            let dest = std::env::temp_dir().join(format!("axylauncher_update_{}", safe_name));
            download_file_with_progress(client, asset_url, &dest, on_progress).await?;
            spawn_windows_msi_updater(pid, &dest, install_dir, exe_path)?;
            Ok(true)
        }

        InstallKind::WindowsPortableExe { exe_path } => {
            let dest = std::env::temp_dir().join(format!("axylauncher_update_{}", safe_name));
            download_file_with_progress(client, asset_url, &dest, on_progress).await?;
            spawn_windows_exe_updater(pid, &dest, exe_path)?;
            Ok(true)
        }

        InstallKind::LinuxAppImage { appimage_path } => {
            let dest = appimage_path
                .parent()
                .map(|p| p.join(format!(".axylauncher_update_{}.tmp", safe_name)))
                .unwrap_or_else(|| std::env::temp_dir().join(format!("axylauncher_update_{}", safe_name)));
            download_file_with_progress(client, asset_url, &dest, on_progress).await?;
            spawn_linux_appimage_updater(pid, &dest, appimage_path)?;
            Ok(true)
        }

        InstallKind::LinuxFlatpak => {
            // Save under $HOME/.cache so host `flatpak` can read the file outside the sandbox
            let home_cache = std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".cache"))
                .unwrap_or_else(std::env::temp_dir);
            let dest = home_cache.join(format!("axylauncher_update_{}", safe_name));
            download_file_with_progress(client, asset_url, &dest, on_progress).await?;
            spawn_linux_flatpak_updater(pid, &dest)?;
            Ok(true)
        }

        InstallKind::MacOsAppBundle { app_bundle_path } => {
            let dest = std::env::temp_dir().join(format!("axylauncher_update_{}", safe_name));
            download_file_with_progress(client, asset_url, &dest, on_progress).await?;
            spawn_macos_dmg_updater(pid, &dest, app_bundle_path)?;
            Ok(true)
        }

        InstallKind::MacOsManual => {
            let dest = std::env::temp_dir().join(format!("axylauncher_update_{}", safe_name));
            download_file_with_progress(client, asset_url, &dest, on_progress).await?;
            let _ = open::that(&dest);
            Ok(false)
        }
    }
}

#[cfg(unix)]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn spawn_windows_msi_updater(
    pid: u32,
    msi_path: &Path,
    install_dir: &Path,
    exe_path: &Path,
) -> Result<()> {
    let script_path = std::env::temp_dir().join("axylauncher_msi_update.cmd");
    let script = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         taskkill /PID {pid} /F >nul 2>&1\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         msiexec /i \"{msi}\" INSTALLFOLDER=\"{dir}\" /passive /norestart\r\n\
         if exist \"{exe}\" start \"\" \"{exe}\"\r\n\
         del /Q \"{msi}\" >nul 2>&1\r\n\
         del \"%~f0\"\r\n",
        pid = pid,
        msi = msi_path.display(),
        dir = install_dir.display(),
        exe = exe_path.display(),
    );
    std::fs::write(&script_path, script)?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        std::process::Command::new("cmd")
            .args(["/C", &script_path.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .context("Failed to spawn Windows MSI updater")?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = script_path;
    }
    Ok(())
}

fn spawn_windows_exe_updater(pid: u32, new_exe: &Path, target_exe: &Path) -> Result<()> {
    let script_path = std::env::temp_dir().join("axylauncher_exe_update.cmd");
    let script = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         taskkill /PID {pid} /F >nul 2>&1\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         :retry\r\n\
         move /Y \"{src}\" \"{dst}\" >nul 2>&1\r\n\
         if errorlevel 1 (\r\n\
             timeout /t 1 /nobreak >nul\r\n\
             goto retry\r\n\
         )\r\n\
         start \"\" \"{dst}\"\r\n\
         del \"%~f0\"\r\n",
        pid = pid,
        src = new_exe.display(),
        dst = target_exe.display(),
    );
    std::fs::write(&script_path, script)?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        std::process::Command::new("cmd")
            .args(["/C", &script_path.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .context("Failed to spawn Windows EXE updater")?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = script_path;
    }
    Ok(())
}

fn spawn_linux_appimage_updater(pid: u32, new_file: &Path, target_appimage: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(new_file, std::fs::Permissions::from_mode(0o755));
        let cmd = format!(
            "sleep 1; kill -TERM {pid} 2>/dev/null || true; sleep 1; \
             mv -f {src} {dst}; chmod +x {dst}; \
             nohup {dst} >/dev/null 2>&1 &",
            pid = pid,
            src = shell_quote(&new_file.to_string_lossy()),
            dst = shell_quote(&target_appimage.to_string_lossy()),
        );
        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to spawn AppImage updater")?;
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, new_file, target_appimage);
    }
    Ok(())
}

fn spawn_linux_flatpak_updater(pid: u32, bundle_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let bundle_q = shell_quote(&bundle_path.to_string_lossy());
        let host_cmd = format!(
            "sleep 1; kill -TERM {pid} 2>/dev/null || true; sleep 1; \
             flatpak install --user --or-update -y {bundle}; \
             rm -f {bundle}; \
             nohup flatpak run io.github.xeone52670v3.AxyLauncher >/dev/null 2>&1 &",
            pid = pid,
            bundle = bundle_q,
        );
        // Inside Flatpak sandbox, prefer `flatpak-spawn --host`
        let spawned = std::process::Command::new("flatpak-spawn")
            .args(["--host", "sh", "-c", &host_cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if spawned.is_err() {
            std::process::Command::new("sh")
                .args(["-c", &host_cmd])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .context("Failed to spawn Flatpak updater")?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, bundle_path);
    }
    Ok(())
}

fn spawn_macos_dmg_updater(pid: u32, dmg_path: &Path, app_bundle_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let dmg_q = shell_quote(&dmg_path.to_string_lossy());
        let app_q = shell_quote(&app_bundle_path.to_string_lossy());
        let cmd = format!(
            "sleep 1; kill -TERM {pid} 2>/dev/null || true; sleep 1; \
             MNT=\"/tmp/axylauncher_dmg_mount_$$\"; mkdir -p \"$MNT\"; \
             hdiutil attach {dmg} -mountpoint \"$MNT\" -nobrowse -quiet && \
             if [ -d \"$MNT/AxyLauncher.app\" ]; then \
               rm -rf {app} && cp -R \"$MNT/AxyLauncher.app\" {app}; \
             fi; \
             hdiutil detach \"$MNT\" -quiet || true; rmdir \"$MNT\" 2>/dev/null || true; \
             rm -f {dmg}; \
             open {app}",
            pid = pid,
            dmg = dmg_q,
            app = app_q,
        );
        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to spawn macOS DMG updater")?;
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, dmg_path, app_bundle_path);
    }
    Ok(())
}
