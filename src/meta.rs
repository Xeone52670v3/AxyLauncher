use crate::models::*;
use anyhow::{anyhow, Context, Result};
use futures::stream::{self, StreamExt};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

fn resolve_image_ext(bytes: &[u8], content_type: Option<&str>, url: &str) -> Option<&'static str> {
    if let Some(ext) = crate::instance_icons::detect_image_ext(bytes) {
        return Some(ext);
    }
    if let Some(ct) = content_type {
        let mime = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
        match mime.as_str() {
            "image/png" => return Some("png"),
            "image/jpeg" | "image/jpg" => return Some("jpg"),
            "image/webp" => return Some("webp"),
            "image/gif" => return Some("gif"),
            "image/svg+xml" | "image/svg" => return Some("svg"),
            "image/bmp" | "image/x-ms-bmp" => return Some("bmp"),
            "image/x-icon" | "image/vnd.microsoft.icon" => return Some("ico"),
            _ => {}
        }
    }
    let path_part = url.split('?').next().unwrap_or(url).split('#').next().unwrap_or(url);
    let ext = path_part.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        "gif" => Some("gif"),
        "svg" => Some("svg"),
        "bmp" => Some("bmp"),
        "ico" => Some("ico"),
        _ => None,
    }
}

pub struct MetaClient {
    http: reqwest::Client,
    prism_base: String,
    labymod_base: String,
    optifine_base: String,
    horizon_repo: String,
    curseforge_key: String,
}

impl MetaClient {
    pub fn from_config(cfg: &LauncherConfig) -> Result<Self> {
        let ua = if cfg.custom_user_agent.trim().is_empty() {
            "AxyLauncher/1.0 (FreesmLauncher-Rust; Horizon-Enabled)".to_string()
        } else {
            cfg.custom_user_agent.trim().to_string()
        };
        let mut builder = reqwest::Client::builder()
            .user_agent(ua)
            .timeout(Duration::from_secs(cfg.network_timeout_secs.clamp(10, 300)));

        // proxy_mode: 0=None, 1=System (default reqwest), 2=HTTP, 3=SOCKS5
        if cfg.proxy_mode == 0 && !cfg.proxy_enabled {
            builder = builder.no_proxy();
        } else if (cfg.proxy_mode == 2 || cfg.proxy_mode == 3 || cfg.proxy_enabled)
            && !cfg.proxy_host.trim().is_empty()
        {
            let scheme = if cfg.proxy_mode == 3 || cfg.proxy_type.eq_ignore_ascii_case("SOCKS5") {
                "socks5h"
            } else {
                "http"
            };
            let proxy_url = format!("{}://{}:{}", scheme, cfg.proxy_host.trim(), cfg.proxy_port);
            if let Ok(mut proxy) = reqwest::Proxy::all(&proxy_url) {
                if !cfg.proxy_user.trim().is_empty() {
                    proxy = proxy.basic_auth(cfg.proxy_user.trim(), &cfg.proxy_pass);
                }
                builder = builder.proxy(proxy);
            }
        }

        let http = builder.build()?;
        let base_meta = if cfg.meta_url.trim().is_empty() {
            DEFAULT_FREESM_META_URL
        } else {
            cfg.meta_url.trim()
        };
        Ok(Self {
            http,
            prism_base: base_meta.trim_end_matches('/').to_string(),
            labymod_base: cfg.labymod_repo_url.trim_end_matches('/').to_string(),
            optifine_base: cfg.optifine_repo_url.trim_end_matches('/').to_string(),
            horizon_repo: cfg.horizon_repo_url.trim_end_matches('/').to_string(),
            curseforge_key: cfg.curseforge_api_key.clone(),
        })
    }

    #[allow(dead_code)]
    pub fn new(
        prism_base: &str,
        labymod_base: &str,
        optifine_base: &str,
        horizon_repo: &str,
        curseforge_key: &str,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("AxyLauncher/1.0 (FreesmLauncher-Rust; Horizon-Enabled)")
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            prism_base: prism_base.trim_end_matches('/').to_string(),
            labymod_base: labymod_base.trim_end_matches('/').to_string(),
            optifine_base: optifine_base.trim_end_matches('/').to_string(),
            horizon_repo: horizon_repo.trim_end_matches('/').to_string(),
            curseforge_key: curseforge_key.to_string(),
        })
    }

    pub fn curseforge_key(&self) -> &str {
        &self.curseforge_key
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub async fn fetch_prism_index(&self, uid: &str) -> Result<PrismIndex> {
        let primary_url = format!("{}/{}/index.json", self.prism_base, uid);
        if let Ok(resp) = self.http.get(&primary_url).send().await {
            if resp.status().is_success() {
                if let Ok(idx) = resp.json::<PrismIndex>().await {
                    return Ok(idx);
                }
            }
        }
        // Fallback to Prism Meta mirror if Freesm Meta is unreachable
        let fallback_url = format!("https://meta.prismlauncher.org/v1/{}/index.json", uid);
        let resp = self
            .http
            .get(&fallback_url)
            .send()
            .await
            .with_context(|| format!("Ошибка подключения к репозиторию метаданных: {}", primary_url))?
            .error_for_status()?;
        Ok(resp.json::<PrismIndex>().await?)
    }

    pub async fn fetch_prism_package(
        &self,
        uid: &str,
        version: &str,
    ) -> Result<PrismComponentPackage> {
        let primary_url = format!("{}/{}/{}.json", self.prism_base, uid, version);
        if let Ok(resp) = self.http.get(&primary_url).send().await {
            if resp.status().is_success() {
                if let Ok(pkg) = resp.json::<PrismComponentPackage>().await {
                    return Ok(pkg);
                }
            }
        }
        let fallback_url = format!("https://meta.prismlauncher.org/v1/{}/{}.json", uid, version);
        let resp = self
            .http
            .get(&fallback_url)
            .send()
            .await
            .with_context(|| format!("Ошибка загрузки пакета метаданных: {}", primary_url))?
            .error_for_status()?;
        Ok(resp.json::<PrismComponentPackage>().await?)
    }

    pub async fn fetch_minecraft_versions(&self, include_snapshots: bool) -> Result<Vec<String>> {
        let idx = self.fetch_prism_index("net.minecraft").await?;
        let versions = idx
            .versions
            .into_iter()
            .filter(|v| {
                if include_snapshots {
                    true
                } else {
                    v.r#type.as_deref() == Some("release")
                }
            })
            .map(|v| v.version)
            .collect();
        Ok(versions)
    }

    pub async fn fetch_loader_versions(
        &self,
        loader: &LoaderType,
        mc_version: &str,
    ) -> Result<Vec<String>> {
        match loader {
            LoaderType::Vanilla => Ok(vec!["Vanilla".to_string()]),
            LoaderType::Fabric => {
                let intermediary = self.fetch_prism_index("net.fabricmc.intermediary").await?;
                if !intermediary.versions.iter().any(|v| v.version == mc_version) {
                    return Ok(vec![]);
                }
                let uid = loader.prism_uid().unwrap();
                let idx = self.fetch_prism_index(uid).await?;
                Ok(idx.versions.into_iter().map(|v| v.version).collect())
            }
            LoaderType::Quilt => {
                let supported = if let Ok(hashed) = self.fetch_prism_index("org.quiltmc.hashed").await {
                    hashed.versions.iter().any(|v| v.version == mc_version)
                } else if let Ok(intermediary) = self.fetch_prism_index("net.fabricmc.intermediary").await {
                    intermediary.versions.iter().any(|v| v.version == mc_version)
                } else {
                    false
                };
                if !supported {
                    return Ok(vec![]);
                }
                let uid = loader.prism_uid().unwrap();
                let idx = self.fetch_prism_index(uid).await?;
                Ok(idx.versions.into_iter().map(|v| v.version).collect())
            }
            LoaderType::Forge | LoaderType::NeoForge | LoaderType::LiteLoader => {
                let uid = loader.prism_uid().unwrap();
                let idx = self.fetch_prism_index(uid).await?;
                let filtered: Vec<String> = idx
                    .versions
                    .into_iter()
                    .filter(|v| {
                        v.requires.iter().any(|req| {
                            req.uid == "net.minecraft"
                                && req.equals.as_deref() == Some(mc_version)
                        })
                    })
                    .map(|v| v.version)
                    .collect();
                Ok(filtered)
            }
            LoaderType::LabyMod => {
                let manifest = self.fetch_labymod_manifest().await?;
                let mut res = Vec::new();
                for v in &manifest.minecraft_versions {
                    if v.version == mc_version || v.tag == mc_version {
                        res.push(format!(
                            "LabyMod {} ({})",
                            manifest.labymod_version, manifest.commit_reference
                        ));
                    }
                }
                Ok(res)
            }
            LoaderType::OptiFine => {
                let versions = self.fetch_optifine_versions(mc_version).await?;
                Ok(versions.into_iter().map(|v| v.full_version()).collect())
            }
            LoaderType::Horizon => {
                let mut tags = Vec::new();
                if let Ok(releases) = self.fetch_horizon_releases().await {
                    for r in releases {
                        if !r.tag_name.trim().is_empty() && !tags.contains(&r.tag_name) {
                            tags.push(r.tag_name);
                        }
                    }
                }
                if tags.is_empty() {
                    let tags_url = self.horizon_repo.replace("/releases", "/tags");
                    if let Ok(resp) = self
                        .http
                        .get(&tags_url)
                        .header("Accept", "application/vnd.github+json")
                        .send()
                        .await
                    {
                        if let Ok(arr) = resp.json::<Vec<serde_json::Value>>().await {
                            for item in arr {
                                if let Some(name) = item["name"].as_str() {
                                    let t = name.trim().to_string();
                                    if !t.is_empty() && !tags.contains(&t) {
                                        tags.push(t);
                                    }
                                }
                            }
                        }
                    }
                }
                if tags.is_empty() {
                    tags.push("latest".to_string());
                } else if !tags.iter().any(|t| t == "latest") {
                    tags.insert(0, "latest".to_string());
                }
                Ok(tags)
            }
        }
    }

    /// Fetch Horizon (Minecraft on Rust — `horizon-rust/horizon`) GitHub releases
    pub async fn fetch_horizon_releases(&self) -> Result<Vec<HorizonRelease>> {
        let resp = self
            .http
            .get(&self.horizon_repo)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .with_context(|| {
                format!(
                    "Ошибка подключения к репозиторию Horizon (Rust): {}",
                    self.horizon_repo
                )
            })?
            .error_for_status()?;
        let releases = resp.json::<Vec<HorizonRelease>>().await?;
        Ok(releases)
    }

    /// Resolve Linux x86_64 release asset for a given Horizon tag (or latest)
    pub async fn fetch_horizon_linux_asset(
        &self,
        wanted_tag: &str,
    ) -> Result<(String, HorizonReleaseAsset)> {
        let releases = self.fetch_horizon_releases().await?;
        let chosen = if wanted_tag.is_empty() || wanted_tag == "latest" {
            releases
                .iter()
                .find(|r| !r.assets.is_empty())
                .or_else(|| releases.first())
        } else {
            releases
                .iter()
                .find(|r| r.tag_name == wanted_tag)
                .or_else(|| releases.first())
        }
        .ok_or_else(|| anyhow!("В репозитории Horizon не найдено релизов"))?;

        // Prefer Linux x86_64 AppImage or Linux client binary/archive (avoid server-only if desktop exists)
        let asset = chosen
            .assets
            .iter()
            .find(|a| {
                let n = a.name.to_lowercase();
                n.ends_with(".appimage") && !n.contains("arm64") && !n.contains("aarch64")
            })
            .or_else(|| {
                chosen.assets.iter().find(|a| {
                    let n = a.name.to_lowercase();
                    n.contains("linux")
                        && !n.contains("server")
                        && !n.contains("arm64")
                        && !n.contains("aarch64")
                })
            })
            .or_else(|| {
                chosen
                    .assets
                    .iter()
                    .find(|a| a.name.to_lowercase().contains("linux"))
            })
            .or_else(|| chosen.assets.first())
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "В релизе Horizon {} отсутствуют файлы для Linux",
                    chosen.tag_name
                )
            })?;

        Ok((chosen.tag_name.clone(), asset))
    }

    pub async fn fetch_labymod_manifest(&self) -> Result<LabyModLatestManifest> {
        let url = format!("{}/api/v1/manifest/production/latest.json", self.labymod_base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Ошибка подключения к репозиторию LabyMod: {}", url))?
            .error_for_status()?;
        Ok(resp.json::<LabyModLatestManifest>().await?)
    }

    pub async fn fetch_labymod_libraries(&self) -> Result<LabyModLibrariesManifest> {
        let url = format!("{}/api/v1/libraries/production.json", self.labymod_base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Ошибка загрузки библиотек LabyMod: {}", url))?
            .error_for_status()?;
        Ok(resp.json::<LabyModLibrariesManifest>().await?)
    }

    pub async fn fetch_labymod_version_manifest(
        &self,
        url: &str,
    ) -> Result<MojangStyleVersionJson> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("Ошибка загрузки манифеста версии LabyMod: {}", url))?
            .error_for_status()?;
        Ok(resp.json::<MojangStyleVersionJson>().await?)
    }

    pub async fn fetch_optifine_versions(
        &self,
        mc_version: &str,
    ) -> Result<Vec<OptiFineVersionEntry>> {
        let url = format!("{}/versionList", self.optifine_base);
        match self.http.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let all: Vec<OptiFineVersionEntry> = resp.json().await?;
                let mut filtered: Vec<OptiFineVersionEntry> = all
                    .into_iter()
                    .filter(|e| e.mcversion == mc_version)
                    .collect();
                filtered.reverse();
                return Ok(filtered);
            }
            _ => {}
        }

        let html = self
            .http
            .get("https://optifine.net/downloads")
            .send()
            .await?
            .text()
            .await?;
        let re = regex::Regex::new(r"OptiFine_([0-9.]+)_([A-Z_]+)_([A-Z0-9_]+)\.jar")?;
        let mut list = Vec::new();
        for cap in re.captures_iter(&html) {
            let mcv = cap[1].to_string();
            if mcv == mc_version {
                list.push(OptiFineVersionEntry {
                    mcversion: mcv,
                    of_type: cap[2].to_string(),
                    patch: cap[3].to_string(),
                    filename: cap[0].to_string(),
                    forge: None,
                });
            }
        }
        Ok(list)
    }

    pub fn optifine_download_url(&self, mc_version: &str, of_type: &str, patch: &str) -> String {
        format!(
            "{}/{}/{}/{}",
            self.optifine_base, mc_version, of_type, patch
        )
    }

    // ========================================================================
    // PRISM / FREESMLAUNCHER EXTERNAL MOD & MODPACK SOURCES
    // 1. Modrinth (Modpacks, Mods, ResourcePacks, Shaders)
    // 2. CurseForge (Modpacks & Mods)
    // 3. FTB (Feed The Beast Public Modpacks API)
    // 4. Technic Platform (Technic Search API)
    // 5. ATLauncher (ATLauncher Public Packs Repo)
    // ========================================================================

    async fn search_ftb_modpacks(&self, query: &str) -> Result<Vec<ExternalPackItem>> {
        let search_term = if query.trim().is_empty() {
            "FTB"
        } else {
            query.trim()
        };
        let url = format!(
            "https://api.modpacks.ch/public/modpack/search/15?term={}",
            urlencoding_simple(search_term)
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Ошибка подключения к репозиторию FTB (api.modpacks.ch)")?;
        let json: serde_json::Value = resp.json().await?;
        let pack_ids: Vec<i64> = json["packs"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_i64())
                    .take(10)
                    .collect()
            })
            .unwrap_or_default();

        let mut results = Vec::new();
        for pid in pack_ids {
            let detail_url = format!("https://api.modpacks.ch/public/modpack/{}", pid);
            if let Ok(r) = self.http.get(&detail_url).send().await {
                if let Ok(d) = r.json::<serde_json::Value>().await {
                    let title = d["name"].as_str().unwrap_or("FTB Modpack").to_string();
                    let desc = d["synopsis"]
                        .as_str()
                        .or_else(|| d["description"].as_str())
                        .unwrap_or("Официальный модпак Feed The Beast")
                        .to_string();
                    let installs = d["installs"].as_u64().unwrap_or(0);
                    let author = d["authors"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|a| a["name"].as_str())
                        .unwrap_or("FTB Team")
                        .to_string();
                    let mc_ver = d["tags"]
                        .as_array()
                        .and_then(|tags| {
                            tags.iter()
                                .filter_map(|t| t["name"].as_str())
                                .find(|n| n.starts_with("1."))
                        })
                        .unwrap_or("1.20.1")
                        .to_string();

                    let art_url = d["art"]
                        .as_array()
                        .and_then(|arr| {
                            arr.iter()
                                .find(|a| a["type"].as_str() == Some("square"))
                                .or_else(|| arr.first())
                                .and_then(|a| a["url"].as_str())
                        })
                        .unwrap_or("")
                        .to_string();

                    results.push(ExternalPackItem {
                        id: pid.to_string(),
                        provider: "FTB".to_string(),
                        title,
                        author,
                        description: desc,
                        downloads: format!("{}", installs),
                        mc_version: mc_ver,
                        loader: "Forge".to_string(),
                        download_url: None,
                        icon_url: art_url,
                    });
                }
            }
        }
        Ok(results)
    }

    async fn search_technic_modpacks(&self, query: &str) -> Result<Vec<ExternalPackItem>> {
        let search_term = if query.trim().is_empty() {
            "Tekkit"
        } else {
            query.trim()
        };
        let url = format!(
            "https://api.technicpack.net/search?q={}&build=999",
            urlencoding_simple(search_term)
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Ошибка подключения к репозиторию Technic Platform")?;
        let json: serde_json::Value = resp.json().await?;
        let mut results = Vec::new();
        if let Some(packs) = json["modpacks"].as_array() {
            for p in packs.iter().take(20) {
                let slug = p["slug"].as_str().unwrap_or("").to_string();
                let name = p["name"].as_str().unwrap_or(&slug).to_string();
                let icon_url = p["iconUrl"]
                    .as_str()
                    .or_else(|| p["icon"]["url"].as_str())
                    .unwrap_or("")
                    .to_string();
                results.push(ExternalPackItem {
                    id: slug.clone(),
                    provider: "Technic".to_string(),
                    title: name,
                    author: "Technic Pack".to_string(),
                    description: format!("Модпак Technic Platform (slug: {})", slug),
                    downloads: "Technic".to_string(),
                    mc_version: "1.20.1".to_string(),
                    loader: "Forge".to_string(),
                    download_url: None,
                    icon_url,
                });
            }
        }
        Ok(results)
    }

    async fn search_atlauncher_modpacks(&self, query: &str) -> Result<Vec<ExternalPackItem>> {
        let url = "https://download.nodecdn.net/containers/atl/launcher/json/packsnew.json";
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("Ошибка подключения к репозиторию ATLauncher")?;
        let json: serde_json::Value = resp.json().await?;
        let q_lower = query.trim().to_lowercase();
        let mut results = Vec::new();
        if let Some(arr) = json.as_array() {
            for p in arr {
                if p["type"].as_str() == Some("private") {
                    continue;
                }
                let name = p["name"].as_str().unwrap_or("").to_string();
                let desc = p["description"].as_str().unwrap_or("").to_string();
                if !q_lower.is_empty()
                    && !name.to_lowercase().contains(&q_lower)
                    && !desc.to_lowercase().contains(&q_lower)
                {
                    continue;
                }
                let id = p["id"].as_i64().unwrap_or(0).to_string();
                let mc_ver = p["versions"]
                    .as_array()
                    .and_then(|v| v.first())
                    .and_then(|v| v["minecraft"].as_str())
                    .unwrap_or("1.20.1")
                    .to_string();

                let safe_name: String = name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_lowercase();
                let icon_url = if safe_name.is_empty() {
                    String::new()
                } else {
                    format!(
                        "https://download.nodecdn.net/containers/atl/launcher/images/{}.png",
                        safe_name
                    )
                };

                results.push(ExternalPackItem {
                    id,
                    provider: "ATLauncher".to_string(),
                    title: name,
                    author: "ATLauncher".to_string(),
                    description: desc,
                    downloads: "ATLauncher".to_string(),
                    mc_version: mc_ver,
                    loader: "Forge".to_string(),
                    download_url: None,
                    icon_url,
                });
                if results.len() >= 25 {
                    break;
                }
            }
        }
        Ok(results)
    }

    pub async fn fetch_modrinth_download(
        &self,
        project_id: &str,
        mc_version: &str,
        loader: &LoaderType,
    ) -> Result<(String, String)> {
        let url = format!("https://api.modrinth.com/v2/project/{}/version", project_id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Ошибка получения версий проекта с Modrinth")?
            .error_for_status()?;
        let versions: Vec<ModrinthVersion> = resp.json().await?;

        let target_loader = match loader {
            LoaderType::Fabric => "fabric",
            LoaderType::Forge => "forge",
            LoaderType::NeoForge => "neoforge",
            LoaderType::Quilt => "quilt",
            _ => "",
        };

        let chosen = versions
            .iter()
            .find(|v| {
                let mc_ok = mc_version.is_empty() || v.game_versions.iter().any(|gv| gv == mc_version);
                let loader_ok = target_loader.is_empty()
                    || v.loaders
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(target_loader));
                mc_ok && loader_ok
            })
            .or_else(|| {
                versions
                    .iter()
                    .find(|v| v.game_versions.iter().any(|gv| gv == mc_version))
            })
            .or_else(|| versions.first())
            .context("На Modrinth не найдено файлов для этого проекта")?;

        let file = chosen
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| chosen.files.first())
            .context("У версии проекта отсутствуют файлы")?;

        Ok((file.filename.clone(), file.url.clone()))
    }

    /// Download official modpack artwork from Modrinth / CurseForge / FTB / Technic / ATLauncher
    /// and save into `save_dir` as `pack_<inst_id>.<ext>`.
    pub async fn fetch_external_modpack_icon(
        &self,
        provider: ExternalProvider,
        pack_id: &str,
        pack_title: &str,
        save_path_no_ext: &Path,
    ) -> Option<std::path::PathBuf> {
        let icon_url: Option<String> = match provider {
            ExternalProvider::Modrinth => {
                let url = format!("https://api.modrinth.com/v2/project/{}", pack_id);
                let json: serde_json::Value = self.http.get(&url).send().await.ok()?.json().await.ok()?;
                json["icon_url"].as_str().filter(|s| !s.is_empty()).map(String::from)
            }
            ExternalProvider::CurseForge => {
                let key = self.curseforge_key.trim();
                let url = format!("https://api.curseforge.com/v1/mods/{}", pack_id);
                let mut req = self.http.get(&url);
                if !key.is_empty() {
                    req = req.header("x-api-key", key);
                }
                let json: serde_json::Value = req.send().await.ok()?.json().await.ok()?;
                json["data"]["logo"]["url"]
                    .as_str()
                    .or_else(|| json["data"]["logo"]["thumbnailUrl"].as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            }
            ExternalProvider::Ftb => {
                let url = format!(
                    "https://api.feed-the-beast.com/v1/modpacks/public/modpack/{}",
                    pack_id
                );
                let json: serde_json::Value = self.http.get(&url).send().await.ok()?.json().await.ok()?;
                json["art"].as_array().and_then(|arr| {
                    arr.iter()
                        .find(|a| a["type"].as_str() == Some("square"))
                        .or_else(|| arr.first())
                        .and_then(|a| a["url"].as_str())
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                })
            }
            ExternalProvider::Technic => {
                let url = format!("https://api.technicpack.net/modpack/{}?build=999", pack_id);
                let json: serde_json::Value = self.http.get(&url).send().await.ok()?.json().await.ok()?;
                json["icon"]["url"]
                    .as_str()
                    .or_else(|| json["logo"]["url"].as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            }
            ExternalProvider::AtLauncher => {
                let safe_name: String = pack_title
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_lowercase();
                if safe_name.is_empty() {
                    None
                } else {
                    Some(format!(
                        "https://download.nodecdn.net/containers/atl/launcher/images/{}.png",
                        safe_name
                    ))
                }
            }
        };

        let icon_url = icon_url?;
        let resp = self.http.get(&icon_url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = resp.bytes().await.ok()?;
        let ext = resolve_image_ext(&bytes, content_type.as_deref(), &icon_url)?;
        let final_path = save_path_no_ext.with_extension(ext);
        if let Some(parent) = final_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&final_path, &bytes).ok()?;
        Some(final_path)
    }

    /// Download and install a Modpack from an external provider (Modrinth .mrpack, Technic, FTB, ATLauncher, CurseForge)
    /// or from a local/remote `.mrpack` / `.zip` file, creating a ready-to-play `Instance`!
    pub async fn install_external_modpack<FLog>(
        &self,
        provider: ExternalProvider,
        pack_id: &str,
        fallback_title: &str,
        fallback_mc: &str,
        custom_group: &str,
        instances_dir: &Path,
        log_cb: FLog,
    ) -> Result<Instance>
    where
        FLog: Fn(String) + Send + Sync + 'static,
    {
        let _ = tokio::fs::create_dir_all(instances_dir).await;
        let inst_id = dir_name_from_string(fallback_title, instances_dir, None);
        let instance_mc_dir = instances_dir.join(&inst_id).join(".minecraft");
        tokio::fs::create_dir_all(&instance_mc_dir).await?;

        match provider {
            ExternalProvider::Modrinth => {
                log_cb(format!(
                    "[Модпак Modrinth] Получение последней версии модпака «{}» ({})...",
                    fallback_title, pack_id
                ));
                let (_filename, mrpack_url) = self
                    .fetch_modrinth_download(pack_id, "", &LoaderType::Vanilla)
                    .await?;
                log_cb(format!("[Модпак Modrinth] Скачивание архива {}...", mrpack_url));
                let bytes = self
                    .http
                    .get(&mrpack_url)
                    .send()
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await?;

                let tmp_mrpack = instances_dir.join(&inst_id).join("pack.mrpack");
                tokio::fs::write(&tmp_mrpack, &bytes).await?;

                self.extract_and_install_mrpack(
                    &tmp_mrpack,
                    &inst_id,
                    fallback_title,
                    custom_group,
                    &instance_mc_dir,
                    log_cb,
                )
                .await
            }
            ExternalProvider::Technic => {
                log_cb(format!(
                    "[Модпак Technic] Получение метаданных Technic «{}»...",
                    pack_id
                ));
                let url = format!(
                    "https://api.technicpack.net/modpack/{}?build=999",
                    urlencoding_simple(pack_id)
                );
                let resp = self.http.get(&url).send().await?;
                let json: serde_json::Value = resp.json().await?;
                let pack_title = json["displayName"]
                    .as_str()
                    .unwrap_or(fallback_title)
                    .to_string();
                let mc_ver = json["minecraft"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(fallback_mc)
                    .to_string();

                if let Some(zip_url) = json["url"].as_str().filter(|s| !s.is_empty()) {
                    log_cb(format!(
                        "[Модпак Technic] Скачивание архива модпака {}...",
                        zip_url
                    ));
                    if let Ok(r) = self.http.get(zip_url).send().await {
                        if let Ok(bytes) = r.bytes().await {
                            let tmp_zip =
                                instances_dir.join(&inst_id).join("technic.zip");
                            let _ = tokio::fs::write(&tmp_zip, &bytes).await;
                            let _ = extract_generic_zip(&tmp_zip, &instance_mc_dir);
                        }
                    }
                }

                Ok(Instance::new_default(
                    inst_id,
                    pack_title,
                    custom_group.to_string(),
                    mc_ver,
                    LoaderType::Forge,
                    "latest".to_string(),
                    false,
                    None,
                    "Technic Модпак",
                ))
            }
            ExternalProvider::Ftb | ExternalProvider::CurseForge | ExternalProvider::AtLauncher => {
                log_cb(format!(
                    "[Модпак {}] Создание и настройка профиля модпака «{}» (MC {})...",
                    provider.as_str(),
                    fallback_title,
                    fallback_mc
                ));

                // If FTB, fetch the manifest file list from api.modpacks.ch and download mods!
                if provider == ExternalProvider::Ftb {
                    let detail_url = format!("https://api.modpacks.ch/public/modpack/{}", pack_id);
                    if let Ok(r) = self.http.get(&detail_url).send().await {
                        if let Ok(d) = r.json::<serde_json::Value>().await {
                            if let Some( latest_ver ) = d["versions"].as_array().and_then(|a| a.last()) {
                                if let Some(vid) = latest_ver["id"].as_i64() {
                                    let ver_url = format!(
                                        "https://api.modpacks.ch/public/modpack/{}/{}",
                                        pack_id, vid
                                    );
                                    if let Ok(vr) = self.http.get(&ver_url).send().await {
                                        if let Ok(vj) = vr.json::<serde_json::Value>().await {
                                            if let Some(files) = vj["files"].as_array() {
                                                log_cb(format!(
                                                    "[Модпак FTB] Найдено файлов в манифесте: {} (скачивание основных модов)...",
                                                    files.len()
                                                ));
                                                let mut tasks = Vec::new();
                                                for f in files.iter().take(60) {
                                                    let path_str = f["path"].as_str().unwrap_or("./");
                                                    let name_str = f["name"].as_str().unwrap_or("");
                                                    let url_str = f["url"].as_str().unwrap_or("");
                                                    if !name_str.is_empty() && !url_str.is_empty() {
                                                        let clean_p = path_str.trim_start_matches("./").trim_start_matches('/');
                                                        let dest = instance_mc_dir.join(clean_p).join(name_str);
                                                        tasks.push((url_str.to_string(), dest));
                                                    }
                                                }
                                                self.download_modpack_files(tasks).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(Instance::new_default(
                    inst_id,
                    fallback_title.to_string(),
                    custom_group.to_string(),
                    if fallback_mc.is_empty() {
                        "1.20.1".to_string()
                    } else {
                        fallback_mc.to_string()
                    },
                    LoaderType::Forge,
                    "latest".to_string(),
                    false,
                    None,
                    &format!("{} Модпак", provider.as_str()),
                ))
            }
        }
    }

    /// Import a local or remote `.mrpack` / `.zip` archive directly
    pub async fn import_modpack_from_path_or_url<FLog>(
        &self,
        path_or_url: &str,
        custom_name: &str,
        custom_group: &str,
        instances_dir: &Path,
        log_cb: FLog,
    ) -> Result<Instance>
    where
        FLog: Fn(String) + Send + Sync + 'static,
    {
        let clean = path_or_url.trim();
        if clean.is_empty() {
            return Err(anyhow!("Укажите путь к файлу (.mrpack / .zip) или прямую URL-ссылку"));
        }

        let _ = tokio::fs::create_dir_all(instances_dir).await;
        let seed_name = if !custom_name.trim().is_empty() {
            custom_name.trim().to_string()
        } else {
            Path::new(clean)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported Pack")
                .to_string()
        };
        let inst_id = dir_name_from_string(&seed_name, instances_dir, None);
        let instance_mc_dir = instances_dir.join(&inst_id).join(".minecraft");
        tokio::fs::create_dir_all(&instance_mc_dir).await?;

        let archive_path = if clean.starts_with("http://") || clean.starts_with("https://") {
            log_cb(format!("[Импорт] Скачивание архива модпака по ссылке: {}...", clean));
            let bytes = self
                .http
                .get(clean)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            let tmp = instances_dir
                .join(&inst_id)
                .join("imported_pack.mrpack");
            tokio::fs::write(&tmp, &bytes).await?;
            tmp
        } else {
            let p = Path::new(clean).to_path_buf();
            if !p.exists() {
                return Err(anyhow!("Файл не найден: {}", clean));
            }
            p
        };

        self.extract_and_install_mrpack(
            &archive_path,
            &inst_id,
            custom_name,
            custom_group,
            &instance_mc_dir,
            log_cb,
        )
        .await
    }

    async fn extract_and_install_mrpack<FLog>(
        &self,
        archive_path: &Path,
        inst_id: &str,
        fallback_title: &str,
        custom_group: &str,
        instance_mc_dir: &Path,
        log_cb: FLog,
    ) -> Result<Instance>
    where
        FLog: Fn(String) + Send + Sync + 'static,
    {
        let mut mc_version = "1.21.1".to_string();
        let mut loader = LoaderType::Fabric;
        let mut loader_version = "latest".to_string();
        let mut pack_name = fallback_title.to_string();
        let mut file_downloads: Vec<(String, std::path::PathBuf)> = Vec::new();

        {
            let file = std::fs::File::open(archive_path)?;
            let mut zip = zip::ZipArchive::new(file)?;

            // Check if `modrinth.index.json` exists (.mrpack format)
            if let Ok(mut idx_entry) = zip.by_name("modrinth.index.json") {
                let mut json_str = String::new();
                idx_entry.read_to_string(&mut json_str)?;
                if let Ok(mr_idx) = serde_json::from_str::<MrpackIndexJson>(&json_str) {
                    if !mr_idx.name.trim().is_empty() && (pack_name.is_empty() || pack_name == "Новая сборка") {
                        pack_name = mr_idx.name.clone();
                    }
                    if let Some(mcv) = mr_idx.dependencies.get("minecraft") {
                        mc_version = mcv.clone();
                    }
                    if let Some(fv) = mr_idx.dependencies.get("fabric-loader") {
                        loader = LoaderType::Fabric;
                        loader_version = fv.clone();
                    } else if let Some(qv) = mr_idx.dependencies.get("quilt-loader") {
                        loader = LoaderType::Quilt;
                        loader_version = qv.clone();
                    } else if let Some(nfv) = mr_idx.dependencies.get("neoforge") {
                        loader = LoaderType::NeoForge;
                        loader_version = nfv.clone();
                    } else if let Some(fgv) = mr_idx.dependencies.get("forge") {
                        loader = LoaderType::Forge;
                        loader_version = fgv.clone();
                    }

                    for f in mr_idx.files {
                        if let Some(url) = f.downloads.first() {
                            let dest = instance_mc_dir.join(&f.path);
                            file_downloads.push((url.clone(), dest));
                        }
                    }
                }
            }

            // Extract `overrides/` or `client-overrides/` or top-level files
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name().to_string();
                let rel = if let Some(stripped) = name.strip_prefix("overrides/") {
                    Some(stripped)
                } else if let Some(stripped) = name.strip_prefix("client-overrides/") {
                    Some(stripped)
                } else if name != "modrinth.index.json" && name != "manifest.json" {
                    Some(name.as_str())
                } else {
                    None
                };

                if let Some(rel_path) = rel.filter(|s| !s.is_empty()) {
                    let out_path = instance_mc_dir.join(rel_path);
                    if let Some(parent) = out_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Ok(mut out_file) = std::fs::File::create(&out_path) {
                        let _ = std::io::copy(&mut entry, &mut out_file);
                    }
                }
            }
        }

        if !file_downloads.is_empty() {
            log_cb(format!(
                "[Модпак] Скачивание {} модов и ресурсов сборки «{}» (MC {} | {} {})...",
                file_downloads.len(),
                pack_name,
                mc_version,
                loader.as_str(),
                loader_version
            ));
            self.download_modpack_files(file_downloads).await;
        }

        Ok(Instance::new_default(
            inst_id.to_string(),
            if pack_name.trim().is_empty() {
                format!("Модпак {}", mc_version)
            } else {
                pack_name
            },
            if custom_group.trim().is_empty() {
                "Модпаки".to_string()
            } else {
                custom_group.to_string()
            },
            mc_version,
            loader,
            loader_version,
            false,
            None,
            "Модпак установлен",
        ))
    }

    pub async fn search_external_asset(
        &self,
        provider: ExternalProvider,
        kind: AssetKind,
        query: &str,
        mc_version: Option<&str>,
        loader: Option<&LoaderType>,
    ) -> Result<Vec<ExternalPackItem>> {
        match provider {
            ExternalProvider::Modrinth => {
                self.search_modrinth_by_type(query, kind.modrinth_type(), mc_version, loader)
                    .await
            }
            ExternalProvider::CurseForge => {
                self.search_curseforge_by_class(query, kind.curseforge_class_id(), mc_version)
                    .await
            }
            ExternalProvider::Ftb => self.search_ftb_modpacks(query).await,
            ExternalProvider::Technic => self.search_technic_modpacks(query).await,
            ExternalProvider::AtLauncher => self.search_atlauncher_modpacks(query).await,
        }
    }

    pub async fn search_curseforge_by_class(
        &self,
        query: &str,
        class_id: &str,
        mc_version: Option<&str>,
    ) -> Result<Vec<ExternalPackItem>> {
        let mut req = self
            .http
            .get("https://api.curseforge.com/v1/mods/search")
            .header("x-api-key", &self.curseforge_key)
            .header("Accept", "application/json")
            .query(&[
                ("gameId", "432"),
                ("classId", class_id),
                ("pageSize", "24"),
                ("sortField", "2"),
                ("sortOrder", "desc"),
            ]);
        let clean_q = query.trim();
        if !clean_q.is_empty() {
            req = req.query(&[("searchFilter", clean_q)]);
        }
        if class_id != "4471" && class_id != "17" {
            if let Some(v) = mc_version.filter(|s| !s.is_empty()) {
                req = req.query(&[("gameVersion", v)]);
            }
        }

        if let Ok(resp) = req.send().await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = json["data"].as_array() {
                        let mut out = Vec::new();
                        for item in arr {
                            let id = item["id"].as_i64().unwrap_or(0).to_string();
                            let title = item["name"].as_str().unwrap_or("CurseForge").to_string();
                            let summary = item["summary"].as_str().unwrap_or("").to_string();
                            let downloads = item["downloadCount"].as_u64().unwrap_or(0);
                            let author = item["authors"]
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|a| a["name"].as_str())
                                .unwrap_or("CurseForge")
                                .to_string();
                            let latest_file = item["latestFiles"].as_array().and_then(|a| a.last());
                            let dl_url = latest_file
                                .and_then(|f| f["downloadUrl"].as_str())
                                .map(|s| s.to_string());
                            let mc_ver = latest_file
                                .and_then(|f| f["gameVersions"].as_array())
                                .and_then(|gv| {
                                    gv.iter()
                                        .filter_map(|v| v.as_str())
                                        .find(|v| v.starts_with("1."))
                                })
                                .unwrap_or("1.20.1")
                                .to_string();

                            let icon_url = item["logo"]["thumbnailUrl"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .or_else(|| item["logo"]["url"].as_str().filter(|s| !s.is_empty()))
                                .or_else(|| {
                                    item["screenshots"]
                                        .as_array()
                                        .and_then(|arr| arr.first())
                                        .and_then(|s| {
                                            s["thumbnailUrl"]
                                                .as_str()
                                                .filter(|u| !u.is_empty())
                                                .or_else(|| s["url"].as_str().filter(|u| !u.is_empty()))
                                        })
                                })
                                .unwrap_or("")
                                .to_string();

                            out.push(ExternalPackItem {
                                id,
                                provider: "CurseForge".to_string(),
                                title,
                                author,
                                description: summary,
                                downloads: format!("{}", downloads),
                                mc_version: mc_ver,
                                loader: "CurseForge".to_string(),
                                download_url: dl_url,
                                icon_url,
                            });
                        }
                        if !out.is_empty() {
                            return Ok(out);
                        }
                    }
                }
            }
        }

        let fallback_type = match class_id {
            "4471" => "modpack",
            "12" => "resourcepack",
            "6552" => "shader",
            "6945" | "17" => "datapack",
            _ => "mod",
        };
        let mut fallback = self
            .search_modrinth_by_type(query, fallback_type, mc_version, None)
            .await?;
        for item in &mut fallback {
            item.provider = "Modrinth (Mirror)".to_string();
        }
        Ok(fallback)
    }

    pub async fn search_modrinth_by_type(
        &self,
        query: &str,
        project_type: &str,
        mc_version: Option<&str>,
        loader: Option<&LoaderType>,
    ) -> Result<Vec<ExternalPackItem>> {
        let mut facets = vec![format!("[\"project_type:{}\"]", project_type)];
        if project_type != "modpack" {
            if let Some(v) = mc_version.filter(|s| !s.is_empty()) {
                facets.push(format!("[\"versions:{}\"]", v));
            }
            if project_type == "mod" {
                if let Some(l) = loader {
                    let l_str = match l {
                        LoaderType::Fabric => Some("fabric"),
                        LoaderType::Forge => Some("forge"),
                        LoaderType::NeoForge => Some("neoforge"),
                        LoaderType::Quilt => Some("quilt"),
                        LoaderType::LiteLoader => Some("liteloader"),
                        _ => None,
                    };
                    if let Some(ls) = l_str {
                        facets.push(format!("[\"categories:{}\"]", ls));
                    }
                }
            }
        }
        let facets_param = format!("[{}]", facets.join(","));
        let resp = self
            .http
            .get("https://api.modrinth.com/v2/search")
            .query(&[("query", query), ("limit", "25"), ("facets", &facets_param)])
            .send()
            .await
            .context("Ошибка запроса к Modrinth API")?
            .error_for_status()?;
        let data = resp.json::<ModrinthSearchResponse>().await?;

        let items = data
            .hits
            .into_iter()
            .map(|h| {
                let detected_mc = h
                    .versions
                    .iter()
                    .rev()
                    .find(|v| v.starts_with("1."))
                    .or_else(|| h.versions.last())
                    .cloned()
                    .unwrap_or_else(|| "1.21.1".to_string());
                let detected_loader = if h.categories.iter().any(|c| c == "neoforge") {
                    "NeoForge"
                } else if h.categories.iter().any(|c| c == "forge") {
                    "Forge"
                } else if h.categories.iter().any(|c| c == "quilt") {
                    "Quilt"
                } else {
                    "Fabric"
                };
                ExternalPackItem {
                    id: h.project_id,
                    provider: "Modrinth".to_string(),
                    title: h.title,
                    author: h.author,
                    description: h.description,
                    downloads: format!("{}", h.downloads),
                    mc_version: detected_mc,
                    loader: detected_loader.to_string(),
                    download_url: None,
                    icon_url: h.icon_url.unwrap_or_default(),
                }
            })
            .collect();
        Ok(items)
    }

    /// Download and cache a catalog/news thumbnail image by SHA-1 hash of URL
    pub async fn download_cached_icon(&self, url: &str, cache_dir: &Path) -> Option<std::path::PathBuf> {
        let u = url.trim();
        if u.is_empty() || (!u.starts_with("http://") && !u.starts_with("https://")) {
            return None;
        }
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(u.as_bytes());
        let hash = hex::encode(hasher.finalize());
        for ext in ["png", "jpg", "webp", "gif", "svg", "bmp", "ico"] {
            let p = cache_dir.join(format!("{}.{}", hash, ext));
            if p.is_file() {
                return Some(p);
            }
        }
        for attempt in 0..2 {
            let resp = match self.http.get(u).send().await {
                Ok(r) if r.status().is_success() => r,
                _ if attempt == 0 => {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    continue;
                }
                _ => return None,
            };
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let bytes = match resp.bytes().await {
                Ok(b) if !b.is_empty() => b,
                _ if attempt == 0 => {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    continue;
                }
                _ => return None,
            };
            let ext = resolve_image_ext(&bytes, content_type.as_deref(), u)?;
            let _ = std::fs::create_dir_all(cache_dir);
            let final_path = cache_dir.join(format!("{}.{}", hash, ext));
            std::fs::write(&final_path, &bytes).ok()?;
            return Some(final_path);
        }
        None
    }

    pub async fn fetch_mojang_news(&self) -> Result<Vec<NewsItem>> {
        let resp = self
            .http
            .get("https://launchercontent.mojang.com/v2/news.json")
            .send()
            .await
            .context("Ошибка загрузки новостей Mojang")?
            .error_for_status()?;
        let json: serde_json::Value = resp.json().await?;
        let mut items = Vec::new();
        if let Some(entries) = json["entries"].as_array() {
            for e in entries.iter().take(20) {
                let title = e["title"].as_str().unwrap_or("").trim().to_string();
                if title.is_empty() {
                    continue;
                }
                let summary = e["text"].as_str().unwrap_or("").trim().to_string();
                let date = e["date"].as_str().unwrap_or("").trim().to_string();
                let link = e["readMoreLink"].as_str().unwrap_or("").trim().to_string();
                let raw_img = e["newsPageImage"]["url"]
                    .as_str()
                    .or_else(|| e["playPageImage"]["url"].as_str())
                    .unwrap_or("")
                    .trim();
                let image_url = if raw_img.starts_with("http://") || raw_img.starts_with("https://") {
                    raw_img.to_string()
                } else if raw_img.starts_with('/') {
                    format!("https://launchercontent.mojang.com{}", raw_img)
                } else {
                    String::new()
                };
                items.push(NewsItem {
                    title,
                    summary,
                    date,
                    source: "Mojang".to_string(),
                    url: link,
                    image_url,
                });
            }
        }
        Ok(items)
    }

    pub async fn fetch_custom_news(&self, url: &str) -> Result<Vec<NewsItem>> {
        let u = url.trim();
        if u.is_empty() {
            return Ok(Vec::new());
        }
        let resp = self
            .http
            .get(u)
            .send()
            .await
            .with_context(|| format!("Ошибка загрузки новостей с {}", u))?
            .error_for_status()?;
        let text = resp.text().await?;
        let mut items = Vec::new();
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            let arr = json
                .as_array()
                .or_else(|| json["entries"].as_array())
                .or_else(|| json["items"].as_array())
                .or_else(|| json["news"].as_array());
            if let Some(list) = arr {
                for e in list.iter().take(20) {
                    let title = e["title"].as_str().unwrap_or("").trim().to_string();
                    if title.is_empty() {
                        continue;
                    }
                    let summary = e["summary"]
                        .as_str()
                        .or_else(|| e["text"].as_str())
                        .or_else(|| e["description"].as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let date = e["date"]
                        .as_str()
                        .or_else(|| e["published"].as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let link = e["url"]
                        .as_str()
                        .or_else(|| e["link"].as_str())
                        .or_else(|| e["readMoreLink"].as_str())
                        .unwrap_or(u)
                        .trim()
                        .to_string();
                    let image_url = e["image_url"]
                        .as_str()
                        .or_else(|| e["image"].as_str())
                        .or_else(|| e["thumbnail"].as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let source = e["source"]
                        .as_str()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or("Лаунчер")
                        .to_string();
                    items.push(NewsItem {
                        title,
                        summary,
                        date,
                        source,
                        url: link,
                        image_url,
                    });
                }
            }
            return Ok(items);
        }
        // Simple RSS/Atom <item> fallback
        let re_item = regex::Regex::new(r"(?s)<item>(.*?)</item>")?;
        let re_title = regex::Regex::new(r"(?s)<title>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</title>")?;
        let re_link = regex::Regex::new(r"(?s)<link>(.*?)</link>")?;
        let re_desc = regex::Regex::new(r"(?s)<description>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</description>")?;
        let re_date = regex::Regex::new(r"(?s)<pubDate>(.*?)</pubDate>")?;
        for cap in re_item.captures_iter(&text).take(20) {
            let block = &cap[1];
            let title = re_title
                .captures(block)
                .map(|c| c[1].trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let summary = re_desc
                .captures(block)
                .map(|c| c[1].trim().to_string())
                .unwrap_or_default();
            let link = re_link
                .captures(block)
                .map(|c| c[1].trim().to_string())
                .unwrap_or_else(|| u.to_string());
            let date = re_date
                .captures(block)
                .map(|c| c[1].trim().to_string())
                .unwrap_or_default();
            items.push(NewsItem {
                title,
                summary,
                date,
                source: "Лаунчер".to_string(),
                url: link,
                image_url: String::new(),
            });
        }
        Ok(items)
    }

    pub async fn fetch_curseforge_download(
        &self,
        mod_id: &str,
        mc_version: &str,
    ) -> Result<(String, String)> {
        let url = format!("https://api.curseforge.com/v1/mods/{}/files", mod_id);
        let mut req = self
            .http
            .get(&url)
            .header("x-api-key", &self.curseforge_key)
            .header("Accept", "application/json")
            .query(&[("pageSize", "20")]);
        if !mc_version.is_empty() {
            req = req.query(&[("gameVersion", mc_version)]);
        }
        let resp = req
            .send()
            .await
            .context("Ошибка получения файлов с CurseForge")?
            .error_for_status()?;
        let json: serde_json::Value = resp.json().await?;
        let arr = json["data"]
            .as_array()
            .ok_or_else(|| anyhow!("CurseForge не вернул список файлов"))?;
        let chosen = arr
            .iter()
            .find(|f| f["downloadUrl"].as_str().map(|s| !s.is_empty()).unwrap_or(false))
            .ok_or_else(|| anyhow!("У проекта CurseForge нет прямой ссылки для скачивания"))?;
        let fname = chosen["fileName"]
            .as_str()
            .unwrap_or("mod.jar")
            .to_string();
        let dl = chosen["downloadUrl"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok((fname, dl))
    }

    /// Upload launcher/game log to mclo.gs (Prism/Freesm "Upload Log" feature)
    pub async fn update_instance_mods(
        &self,
        mods_dir: &Path,
        mc_version: &str,
        loader: &LoaderType,
        log_cb: impl Fn(String),
    ) -> Result<usize> {
        let loader_slug = match loader {
            LoaderType::Fabric => "fabric",
            LoaderType::Forge => "forge",
            LoaderType::NeoForge => "neoforge",
            LoaderType::Quilt => "quilt",
            _ => return Ok(0),
        };
        if !mods_dir.is_dir() {
            return Ok(0);
        }

        use sha1::{Digest, Sha1};
        let mut hash_to_path: std::collections::HashMap<String, std::path::PathBuf> =
            std::collections::HashMap::new();
        if let Ok(entries) = std::fs::read_dir(mods_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let Some(fname) = p.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if !fname.ends_with(".jar") {
                    continue;
                }
                if let Ok(bytes) = std::fs::read(&p) {
                    let digest = Sha1::digest(&bytes);
                    let sha = hex::encode(digest);
                    hash_to_path.insert(sha, p);
                }
            }
        }

        if hash_to_path.is_empty() {
            return Ok(0);
        }

        let hashes: Vec<String> = hash_to_path.keys().cloned().collect();
        let body = serde_json::json!({
            "hashes": hashes,
            "algorithm": "sha1",
            "loaders": [loader_slug],
            "game_versions": [mc_version]
        });

        let resp = self
            .http
            .post("https://api.modrinth.com/v2/version_files/update")
            .json(&body)
            .send()
            .await
            .context("Не удалось запросить обновления модов на Modrinth")?
            .error_for_status()?;

        let map: serde_json::Value = resp.json().await?;
        let Some(obj) = map.as_object() else {
            return Ok(0);
        };

        let mut updated = 0usize;
        for (old_hash, ver_obj) in obj {
            let Some(old_path) = hash_to_path.get(old_hash) else {
                continue;
            };
            let Some(files) = ver_obj["files"].as_array() else {
                continue;
            };
            let chosen = files
                .iter()
                .find(|f| f["primary"].as_bool() == Some(true))
                .or_else(|| files.first());
            let Some(file_obj) = chosen else {
                continue;
            };
            let new_sha = file_obj["hashes"]["sha1"].as_str().unwrap_or("");
            if new_sha.is_empty() || new_sha.eq_ignore_ascii_case(old_hash) {
                continue;
            }
            let Some(dl_url) = file_obj["url"].as_str() else {
                continue;
            };
            let Some(new_fname) = file_obj["filename"].as_str() else {
                continue;
            };
            if let Ok(resp_file) = self.http.get(dl_url).send().await {
                if resp_file.status().is_success() {
                    if let Ok(bytes) = resp_file.bytes().await {
                        let new_path = mods_dir.join(new_fname);
                        if tokio::fs::write(&new_path, &bytes).await.is_ok() {
                            if &new_path != old_path {
                                let _ = tokio::fs::remove_file(old_path).await;
                            }
                            let old_name = old_path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("mod.jar");
                            log_cb(format!("[Моды] Обновлён: {} → {}", old_name, new_fname));
                            updated += 1;
                        }
                    }
                }
            }
        }
        Ok(updated)
    }

    pub async fn upload_log(
        &self,
        content: &str,
        service_type: &str,
        custom_base_url: &str,
    ) -> Result<String> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("Журнал пуст"));
        }
        let custom = custom_base_url.trim().trim_end_matches('/');
        match service_type {
            "0x0.st" => {
                let base = if custom.is_empty() {
                    "https://0x0.st"
                } else {
                    custom
                };
                let boundary = "----AxyLauncherPasteBoundary7MA4YWxkTrZu0gW";
                let mut body = Vec::new();
                body.extend_from_slice(
                    format!(
                        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"axylauncher.log\"\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(trimmed.as_bytes());
                body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
                let resp = self
                    .http
                    .post(base)
                    .header(
                        "Content-Type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(body)
                    .send()
                    .await
                    .with_context(|| format!("Не удалось отправить лог на {}", base))?
                    .error_for_status()?;
                let url = resp.text().await?.trim().to_string();
                if url.starts_with("http") {
                    Ok(url)
                } else {
                    Err(anyhow!("Неожиданный ответ 0x0.st: {}", url))
                }
            }
            "paste.gg" => {
                let base = if custom.is_empty() {
                    "https://api.paste.gg/v1/pastes"
                } else {
                    custom
                };
                let body = serde_json::json!({
                    "name": "AxyLauncher Log",
                    "visibility": "unlisted",
                    "files": [{
                        "name": "latest.log",
                        "content": { "format": "text", "value": trimmed }
                    }]
                });
                let resp = self
                    .http
                    .post(base)
                    .json(&body)
                    .send()
                    .await
                    .context("Не удалось отправить лог на paste.gg")?
                    .error_for_status()?;
                let json: serde_json::Value = resp.json().await?;
                if let Some(id) = json["result"]["id"].as_str() {
                    Ok(format!("https://paste.gg/p/anonymous/{}", id))
                } else {
                    Err(anyhow!("paste.gg вернул ошибку"))
                }
            }
            "hastebin" => {
                let base = if custom.is_empty() {
                    "https://hastebin.com"
                } else {
                    custom
                };
                let resp = self
                    .http
                    .post(format!("{}/documents", base))
                    .body(trimmed.to_string())
                    .send()
                    .await
                    .with_context(|| format!("Не удалось отправить лог на {}", base))?
                    .error_for_status()?;
                let json: serde_json::Value = resp.json().await?;
                if let Some(key) = json["key"].as_str() {
                    Ok(format!("{}/{}", base, key))
                } else {
                    Err(anyhow!("hastebin не вернул ключ документа"))
                }
            }
            "paste.ee" => {
                let base = if custom.is_empty() {
                    "https://api.paste.ee/v1/pastes"
                } else {
                    custom
                };
                let body = serde_json::json!({
                    "description": "AxyLauncher Log",
                    "sections": [{ "name": "latest.log", "contents": trimmed }]
                });
                let resp = self
                    .http
                    .post(base)
                    .json(&body)
                    .send()
                    .await
                    .context("Не удалось отправить лог на paste.ee")?
                    .error_for_status()?;
                let json: serde_json::Value = resp.json().await?;
                if let Some(link) = json["link"].as_str() {
                    Ok(link.to_string())
                } else {
                    Err(anyhow!("paste.ee вернул ошибку"))
                }
            }
            _ => {
                let endpoint = if custom.is_empty() {
                    "https://api.mclo.gs/1/log".to_string()
                } else {
                    format!("{}/1/log", custom)
                };
                let resp = self
                    .http
                    .post(&endpoint)
                    .form(&[("content", trimmed)])
                    .send()
                    .await
                    .context("Не удалось отправить лог на mclo.gs")?
                    .error_for_status()?;
                let json: serde_json::Value = resp.json().await?;
                if json["success"].as_bool() == Some(true) {
                    if let Some(url) = json["url"].as_str() {
                        return Ok(url.to_string());
                    }
                }
                Err(anyhow!(
                    "mclo.gs вернул ошибку: {}",
                    json["error"].as_str().unwrap_or("неизвестно")
                ))
            }
        }
    }

    async fn download_modpack_files(&self, files: Vec<(String, std::path::PathBuf)>) {
        let http = self.http.clone();
        let _: Vec<()> = stream::iter(files)
            .map(|(url, dest)| {
                let http = http.clone();
                async move {
                    if let Some(parent) = dest.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    if let Ok(resp) = http.get(&url).send().await {
                        if let Ok(bytes) = resp.bytes().await {
                            let _ = tokio::fs::write(&dest, &bytes).await;
                        }
                    }
                }
            })
            .buffer_unordered(12)
            .collect()
            .await;
    }
}

fn extract_generic_zip(zip_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let out_path = dest_dir.join(entry.name());
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

fn urlencoding_simple(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Export an instance to Prism/Freesm/MultiMC `.zip` (default), CurseForge `.zip`, or Modrinth `.mrpack`
pub fn export_instance_archive(
    instances_dir: &Path,
    exports_dir: &Path,
    inst: &Instance,
    format: ExportFormat,
) -> Result<std::path::PathBuf> {
    use std::io::Write;
    std::fs::create_dir_all(exports_dir)?;
    let safe_name: String = inst
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base_name = if safe_name.trim().is_empty() {
        "instance".to_string()
    } else {
        safe_name
    };

    let out_path = match format {
        ExportFormat::PrismZip => exports_dir.join(format!("{}.zip", base_name)),
        ExportFormat::CurseForgeZip => exports_dir.join(format!("{}-curseforge.zip", base_name)),
        ExportFormat::ModrinthMrpack => exports_dir.join(format!("{}.mrpack", base_name)),
    };

    let file = std::fs::File::create(&out_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mc_dir = instances_dir
        .join(&inst.id)
        .join(".minecraft");

    match format {
        ExportFormat::PrismZip => {
            // Standard PrismLauncher / FreesmLauncher / MultiMC instance archive:
            // instance.cfg + mmc-pack.json + .minecraft/
            let mut cfg_lines = vec![
                "InstanceType=OneSix".to_string(),
                format!("name={}", inst.name),
                "iconKey=default".to_string(),
            ];
            if !inst.notes.trim().is_empty() {
                cfg_lines.push(format!(
                    "notes={}",
                    inst.notes.replace('\n', "\\n").replace('\r', "")
                ));
            }
            if inst.override_memory {
                cfg_lines.push("OverrideMemory=true".to_string());
                if let Some(min_m) = inst.custom_min_memory_mb {
                    cfg_lines.push(format!("MinMemAlloc={}", min_m));
                }
                if let Some(max_m) = inst.custom_max_memory_mb {
                    cfg_lines.push(format!("MaxMemAlloc={}", max_m));
                }
            }
            if inst.override_jvm_args {
                cfg_lines.push("OverrideJavaArgs=true".to_string());
                if let Some(args) = &inst.custom_jvm_args {
                    cfg_lines.push(format!("JvmArgs={}", args));
                }
            }
            if inst.override_window {
                cfg_lines.push("OverrideWindow=true".to_string());
                if let Some(w) = inst.custom_window_width {
                    cfg_lines.push(format!("MinecraftWinWidth={}", w));
                }
                if let Some(h) = inst.custom_window_height {
                    cfg_lines.push(format!("MinecraftWinHeight={}", h));
                }
                if inst.custom_start_maximized {
                    cfg_lines.push("LaunchMaximized=true".to_string());
                }
            }
            let cfg_content = cfg_lines.join("\n") + "\n";
            zip.start_file("instance.cfg", opts)?;
            zip.write_all(cfg_content.as_bytes())?;

            let mut components = vec![serde_json::json!({
                "cachedName": "Minecraft",
                "important": true,
                "uid": "net.minecraft",
                "version": inst.mc_version
            })];
            if let Some(uid) = inst.loader.prism_uid() {
                if uid != "net.minecraft" && !inst.loader_version.trim().is_empty() {
                    components.push(serde_json::json!({
                        "cachedName": inst.loader.as_str(),
                        "uid": uid,
                        "version": inst.loader_version
                    }));
                }
            }
            let mmc_pack = serde_json::json!({
                "components": components,
                "formatVersion": 1
            });
            zip.start_file("mmc-pack.json", opts)?;
            zip.write_all(serde_json::to_string_pretty(&mmc_pack)?.as_bytes())?;

            if mc_dir.exists() {
                add_dir_to_zip(&mut zip, &mc_dir, &mc_dir, ".minecraft", opts)?;
            }
        }
        ExportFormat::CurseForgeZip => {
            // Standard CurseForge modpack zip: manifest.json + overrides/
            let mut mod_loaders = Vec::new();
            let loader_id = match inst.loader {
                LoaderType::Fabric => Some(format!("fabric-{}", inst.loader_version)),
                LoaderType::Forge => Some(format!("forge-{}", inst.loader_version)),
                LoaderType::NeoForge => Some(format!("neoforge-{}", inst.loader_version)),
                LoaderType::Quilt => Some(format!("quilt-{}", inst.loader_version)),
                _ => None,
            };
            if let Some(lid) = loader_id {
                mod_loaders.push(serde_json::json!({
                    "id": lid,
                    "primary": true
                }));
            }
            let manifest = serde_json::json!({
                "minecraft": {
                    "version": inst.mc_version,
                    "modLoaders": mod_loaders
                },
                "manifestType": "minecraftModpack",
                "manifestVersion": 1,
                "name": inst.name,
                "version": "1.0.0",
                "author": "AxyLauncher",
                "files": [],
                "overrides": "overrides"
            });
            zip.start_file("manifest.json", opts)?;
            zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

            if mc_dir.exists() {
                add_dir_to_zip(&mut zip, &mc_dir, &mc_dir, "overrides", opts)?;
            }
        }
        ExportFormat::ModrinthMrpack => {
            // Standard Modrinth .mrpack: modrinth.index.json + overrides/
            let mut deps = serde_json::Map::new();
            deps.insert(
                "minecraft".to_string(),
                serde_json::Value::String(inst.mc_version.clone()),
            );
            match inst.loader {
                LoaderType::Fabric => {
                    deps.insert(
                        "fabric-loader".to_string(),
                        serde_json::Value::String(inst.loader_version.clone()),
                    );
                }
                LoaderType::Quilt => {
                    deps.insert(
                        "quilt-loader".to_string(),
                        serde_json::Value::String(inst.loader_version.clone()),
                    );
                }
                LoaderType::NeoForge => {
                    deps.insert(
                        "neoforge".to_string(),
                        serde_json::Value::String(inst.loader_version.clone()),
                    );
                }
                LoaderType::Forge => {
                    deps.insert(
                        "forge".to_string(),
                        serde_json::Value::String(inst.loader_version.clone()),
                    );
                }
                _ => {}
            }
            let idx = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "1.0.0",
                "name": inst.name,
                "summary": inst.notes,
                "files": [],
                "dependencies": deps
            });
            zip.start_file("modrinth.index.json", opts)?;
            zip.write_all(serde_json::to_string_pretty(&idx)?.as_bytes())?;

            if mc_dir.exists() {
                add_dir_to_zip(&mut zip, &mc_dir, &mc_dir, "overrides", opts)?;
            }
        }
    }

    zip.finish()?;
    Ok(out_path)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    dir: &Path,
    prefix: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name_os = entry.file_name();
        let name_str = name_os.to_string_lossy();
        // Skip huge transient folders
        if name_str == "logs" || name_str == "crash-reports" || name_str == ".fabric" {
            continue;
        }
        if path.is_dir() {
            add_dir_to_zip(zip, root, &path, prefix, opts)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let zip_name = format!(
                "{}/{}",
                prefix,
                rel.to_string_lossy().replace('\\', "/")
            );
            zip.start_file(zip_name, opts)?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Migrate / Import existing instances from PrismLauncher, FreesmLauncher, MultiMC, or PineconeMC!
pub fn import_prism_freesm_instances(
    source_root: Option<&str>,
    dest_instances_dir: &Path,
) -> Result<Vec<Instance>> {
    let mut candidate_dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(custom) = source_root.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let p = std::path::PathBuf::from(custom);
        if p.join("instances").is_dir() {
            candidate_dirs.push(p.join("instances"));
        } else if p.is_dir() {
            candidate_dirs.push(p);
        }
    } else if let Some(home) = directories::BaseDirs::new().map(|d| d.data_local_dir().to_path_buf())
    {
        for name in [
            "FreesmLauncher",
            "PrismLauncher",
            "MultiMC",
            "PineconeMC",
            "PolyMC",
        ] {
            let d = home.join(name).join("instances");
            if d.is_dir() {
                candidate_dirs.push(d);
            }
        }
    }

    if candidate_dirs.is_empty() {
        return Err(anyhow!(
            "Не найдены папки сборок FreesmLauncher / PrismLauncher / MultiMC. Укажите путь вручную."
        ));
    }

    let mut imported = Vec::new();
    for insts_dir in candidate_dirs {
        let Ok(entries) = std::fs::read_dir(&insts_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let folder = entry.path();
            if !folder.is_dir() {
                continue;
            }
            let cfg_file = folder.join("instance.cfg");
            let pack_file = folder.join("mmc-pack.json");
            if !cfg_file.exists() && !pack_file.exists() {
                continue;
            }

            let mut name = folder
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Imported")
                .to_string();
            let mut notes = String::new();
            if let Ok(cfg_text) = std::fs::read_to_string(&cfg_file) {
                for line in cfg_text.lines() {
                    if let Some(v) = line.strip_prefix("name=") {
                        if !v.trim().is_empty() {
                            name = v.trim().to_string();
                        }
                    } else if let Some(v) = line.strip_prefix("notes=") {
                        notes = v.trim().to_string();
                    }
                }
            }

            let mut mc_ver = "1.21.1".to_string();
            let mut loader = LoaderType::Vanilla;
            let mut loader_ver = "Vanilla".to_string();
            if let Ok(pack_text) = std::fs::read_to_string(&pack_file) {
                if let Ok(pack_json) = serde_json::from_str::<serde_json::Value>(&pack_text) {
                    if let Some(comps) = pack_json["components"].as_array() {
                        for c in comps {
                            let uid = c["uid"].as_str().unwrap_or("");
                            let ver = c["version"].as_str().unwrap_or("").to_string();
                            match uid {
                                "net.minecraft" => mc_ver = ver,
                                "net.fabricmc.fabric-loader" => {
                                    loader = LoaderType::Fabric;
                                    loader_ver = ver;
                                }
                                "net.minecraftforge" => {
                                    loader = LoaderType::Forge;
                                    loader_ver = ver;
                                }
                                "net.neoforged" => {
                                    loader = LoaderType::NeoForge;
                                    loader_ver = ver;
                                }
                                "org.quiltmc.quilt-loader" => {
                                    loader = LoaderType::Quilt;
                                    loader_ver = ver;
                                }
                                "com.mumfrey.liteloader" => {
                                    loader = LoaderType::LiteLoader;
                                    loader_ver = ver;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            let _ = std::fs::create_dir_all(dest_instances_dir);
            let new_id = dir_name_from_string(&name, dest_instances_dir, None);
            let dest_mc = dest_instances_dir.join(&new_id).join(".minecraft");
            let _ = std::fs::create_dir_all(&dest_mc);
            let src_mc = if folder.join(".minecraft").is_dir() {
                folder.join(".minecraft")
            } else {
                folder.join("minecraft")
            };
            if src_mc.is_dir() {
                let _ = copy_dir_recursive(&src_mc, &dest_mc);
            }

            let mut inst = Instance::new_default(
                new_id,
                name,
                "Импорт".to_string(),
                mc_ver,
                loader,
                loader_ver,
                false,
                None,
                "Мигрировано",
            );
            inst.notes = notes;
            imported.push(inst);
        }
    }

    if imported.is_empty() {
        return Err(anyhow!(
            "В найденных папках не обнаружено сборок с instance.cfg / mmc-pack.json"
        ));
    }
    Ok(imported)
}

/// Extract a downloaded world `.zip` archive cleanly into `.minecraft/saves/<world_name>/`
pub fn extract_world_zip(zip_path: &Path, saves_dir: &Path, fallback_name: &str) -> Result<String> {
    std::fs::create_dir_all(saves_dir)?;
    let file = std::fs::File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(file)?;

    // Check if zip already has a single top-level directory containing level.dat
    let mut level_dat_prefix: Option<String> = None;
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let n = entry.name();
            if n == "level.dat" {
                level_dat_prefix = Some(String::new());
                break;
            } else if let Some(prefix) = n.strip_suffix("/level.dat") {
                level_dat_prefix = Some(format!("{}/", prefix));
                break;
            }
        }
    }

    let safe_world: String = fallback_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let world_dir_name = if safe_world.trim().is_empty() {
        "Downloaded_World".to_string()
    } else {
        safe_world.trim().to_string()
    };

    let target_root = saves_dir.join(&world_dir_name);
    std::fs::create_dir_all(&target_root)?;

    let prefix = level_dat_prefix.unwrap_or_default();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let rel = if !prefix.is_empty() {
            match name.strip_prefix(&prefix) {
                Some(r) => r,
                None => continue,
            }
        } else {
            name.as_str()
        };
        if rel.is_empty() || rel.contains("..") {
            continue;
        }
        let out_path = target_root.join(rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }
    Ok(world_dir_name)
}

/// PrismLauncher / FreesmLauncher `FS::DirNameFromString`:
/// converts an instance display name into a clean, collision-free folder name inside `instances/`.
pub fn dir_name_from_string(
    name: &str,
    instances_dir: &Path,
    ignore_existing: Option<&str>,
) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control()
                || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    cleaned = cleaned.trim().trim_end_matches('.').trim().to_string();

    let upper = cleaned.to_uppercase();
    let is_reserved = matches!(
        upper.as_str(),
        "" | "."
            | ".."
            | "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    let base = if is_reserved {
        "Instance".to_string()
    } else {
        cleaned
    };

    let mut candidate = base.clone();
    let mut suffix = 1usize;
    loop {
        let is_self = ignore_existing
            .map(|ig| ig == candidate.as_str())
            .unwrap_or(false);
        if is_self || !instances_dir.join(&candidate).exists() {
            return candidate;
        }
        candidate = format!("{} {}", base, suffix);
        suffix += 1;
    }
}

/// Rename an instance's physical directory on disk (`<instances_dir>/<old_id>` -> `<instances_dir>/<new_dir>`).
pub fn rename_instance_folder(instances_dir: &Path, old_id: &str, new_name: &str) -> Result<String> {
    std::fs::create_dir_all(instances_dir)?;
    let new_dir = dir_name_from_string(new_name, instances_dir, Some(old_id));
    if new_dir == old_id {
        return Ok(old_id.to_string());
    }
    let old_path = instances_dir.join(old_id);
    let new_path = instances_dir.join(&new_dir);
    if old_path.exists() {
        if let Err(_) = std::fs::rename(&old_path, &new_path) {
            copy_dir_recursive(&old_path, &new_path)?;
            let _ = std::fs::remove_dir_all(&old_path);
        }
    } else {
        std::fs::create_dir_all(new_path.join(".minecraft"))?;
    }
    Ok(new_dir)
}

/// Write a Prism/Freesm-compatible `instance.cfg` inside `<instances_dir>/<inst.id>/instance.cfg`
pub fn write_instance_cfg(instances_dir: &Path, inst: &Instance) {
    let inst_root = instances_dir.join(&inst.id);
    if std::fs::create_dir_all(&inst_root).is_ok() {
        let mut lines = vec![
            "InstanceType=OneSix".to_string(),
            format!("name={}", inst.name),
            "iconKey=default".to_string(),
        ];
        if !inst.notes.trim().is_empty() {
            lines.push(format!(
                "notes={}",
                inst.notes.replace('\n', "\\n").replace('\r', "")
            ));
        }
        let _ = std::fs::write(inst_root.join("instance.cfg"), lines.join("\n") + "\n");
    }
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_recursive(&p, &target)?;
        } else if p.is_file() {
            let _ = std::fs::copy(&p, &target);
        }
    }
    Ok(())
}
