#![allow(dead_code)]
use crate::models::{Account, AccountType, AuthAgentBackend};
use anyhow::{anyhow, Context, Result};
use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Microsoft OAuth Client ID used by FreesmLauncher / Prism Launcher device code flow
pub const MS_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MsTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct McLoginResponse {
    access_token: String,
}

#[derive(Debug, Clone, Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YggdrasilAuthResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "clientToken")]
    client_token: Option<String>,
    #[serde(rename = "selectedProfile")]
    selected_profile: Option<YggdrasilProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YggdrasilProfile {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthlibInjectorLatest {
    version: String,
    download_url: String,
}

/// Cryptographically random Minecraft nickname generator (FreesmLauncher Lavarand-style)
pub fn generate_random_nickname() -> String {
    const ADJ: &[&str] = &[
        "Swift", "Frost", "Lunar", "Prism", "Ender", "Nether", "Pixel", "Cinder", "Cobalt",
        "Amber", "Vortex", "Maple", "Nova", "Solar", "Quartz", "Emerald", "Obsidian", "Crimson",
        "Velvet", "Mossy", "Crystal", "Shadow", "Breezy", "Starlit",
    ];
    const NOUN: &[&str] = &[
        "Fox", "Cat", "Wolf", "Creeper", "Strider", "Axolotl", "Allay", "Warden", "Blaze",
        "Enderman", "Phantom", "Miner", "Crafter", "Builder", "Knight", "Nomad", "Drifter",
        "Lynx", "Otter", "Panda", "Raven", "Kitsune", "Badger", "Falcon",
    ];
    let bytes = *Uuid::new_v4().as_bytes();
    let a = ADJ[(bytes[0] as usize) % ADJ.len()];
    let n = NOUN[(bytes[1] as usize) % NOUN.len()];
    let num = ((bytes[2] as u16) << 8 | (bytes[3] as u16)) % 900 + 100;
    let candidate = format!("{}{}{}", a, n, num);
    if candidate.len() > 16 {
        candidate.chars().take(16).collect()
    } else {
        candidate
    }
}

/// Random icon index (`0..60`) for AxyLauncher random instance icon feature
pub fn random_icon_id() -> i32 {
    let b = Uuid::new_v4().as_bytes()[0];
    (b as i32).rem_euclid(crate::instance_icons::INSTANCE_ICON_COUNT)
}

/// Minimal standard Base64 encoder (used for `-Dauthlibinjector.yggdrasil.prefetched=...`)
pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 1. Offline / Cracked (FreesmLauncher mode)
/// Generates a deterministic UUIDv3 from `OfflinePlayer:<username>` just like Minecraft & FreesmLauncher.
pub fn create_offline_account(username: &str, custom_uuid: Option<&str>) -> Result<Account> {
    let clean = username.trim();
    if clean.is_empty() {
        return Err(anyhow!("Имя пользователя не может быть пустым"));
    }

    let uuid = match custom_uuid.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(u) => u.replace('-', ""),
        None => generate_offline_uuid(clean),
    };

    Ok(Account {
        id: Uuid::new_v4().to_string(),
        username: clean.to_string(),
        uuid,
        access_token: "0".to_string(),
        refresh_token: None,
        account_type: AccountType::Offline,
        auth_server_url: None,
        auth_login_url: None,
        auth_refresh_url: None,
    })
}

pub fn generate_offline_uuid(username: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{}", username).as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    hex::encode(bytes)
}

/// Resolve Authlib-Injector API Location (ALI) header if server redirects to a Yggdrasil root
pub async fn resolve_yggdrasil_api_root(http: &reqwest::Client, input_url: &str) -> String {
    let mut clean = input_url.trim().trim_end_matches('/').to_string();
    if !clean.starts_with("http://") && !clean.starts_with("https://") {
        clean = format!("https://{}", clean);
    }
    if clean.contains("authserver.ely.by") {
        return "https://authserver.ely.by".to_string();
    }
    if let Ok(resp) = http.get(&clean).send().await {
        if let Some(ali) = resp
            .headers()
            .get("x-authlib-injector-api-location")
            .and_then(|v| v.to_str().ok())
        {
            let ali_trim = ali.trim().trim_end_matches('/');
            if ali_trim.starts_with("http://") || ali_trim.starts_with("https://") {
                return ali_trim.to_string();
            } else if ali_trim.starts_with('/') {
                if let Some(idx) = clean.find("://") {
                    if let Some(slash) = clean[idx + 3..].find('/') {
                        let origin = &clean[..idx + 3 + slash];
                        return format!("{}{}", origin, ali_trim);
                    }
                    return format!("{}{}", clean, ali_trim);
                }
            }
        }
    }
    clean
}

/// 2. Ely.by, 3. LittleSkin & 4. Custom Yggdrasil AuthServer login (FreesmLauncher third-party auth methods)
/// Supports Ely.by, LittleSkin, Blessing Skin, Drasl, and any ALI/Yggdrasil server.
pub async fn authenticate_yggdrasil(
    http: &reqwest::Client,
    account_type: AccountType,
    server_base_url: &str,
    username_or_email: &str,
    password: &str,
    custom_login_url: Option<&str>,
    custom_refresh_url: Option<&str>,
) -> Result<Account> {
    let login_override = custom_login_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let refresh_override = custom_refresh_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    if server_base_url.trim().is_empty() && login_override.is_none() {
        return Err(anyhow!("URL сервера авторизации не указан"));
    }
    if username_or_email.trim().is_empty() || password.is_empty() {
        return Err(anyhow!("Введите логин (или email) и пароль"));
    }

    let resolved_base = if account_type == AccountType::ElyBy {
        "https://authserver.ely.by".to_string()
    } else if account_type == AccountType::LittleSkin {
        "https://littleskin.cn/api/yggdrasil".to_string()
    } else if !server_base_url.trim().is_empty() {
        resolve_yggdrasil_api_root(http, server_base_url).await
    } else if let Some(ref lu) = login_override {
        let trimmed = lu.trim_end_matches('/');
        trimmed
            .strip_suffix("/authserver/authenticate")
            .or_else(|| trimmed.strip_suffix("/auth/authenticate"))
            .or_else(|| trimmed.strip_suffix("/authenticate"))
            .unwrap_or(trimmed)
            .to_string()
    } else {
        String::new()
    };

    let auth_endpoint = if let Some(ref lu) = login_override {
        lu.clone()
    } else if account_type == AccountType::ElyBy
        || resolved_base.contains("authserver.ely.by")
    {
        format!("{}/auth/authenticate", resolved_base)
    } else if resolved_base.ends_with("/authserver") {
        format!("{}/authenticate", resolved_base)
    } else {
        format!("{}/authserver/authenticate", resolved_base)
    };

    let client_token = Uuid::new_v4().to_string().replace('-', "");
    let payload = json!({
        "agent": {
            "name": "Minecraft",
            "version": 1
        },
        "username": username_or_email.trim(),
        "password": password,
        "clientToken": client_token,
        "requestUser": true
    });

    let resp = http
        .post(&auth_endpoint)
        .json(&payload)
        .send()
        .await
        .with_context(|| format!("Не удалось подключиться к {}", auth_endpoint))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Ошибка авторизации Yggdrasil/Ely.by ({}): {}",
            status,
            body
        ));
    }

    let parsed: YggdrasilAuthResponse = resp.json().await?;
    let profile = parsed.selected_profile.ok_or_else(|| {
        anyhow!("В аккаунте не найден игровой профиль Minecraft (selectedProfile пуст)")
    })?;

    Ok(Account {
        id: Uuid::new_v4().to_string(),
        username: profile.name,
        uuid: profile.id.replace('-', ""),
        access_token: parsed.access_token,
        refresh_token: parsed.client_token,
        account_type,
        auth_server_url: Some(resolved_base),
        auth_login_url: login_override,
        auth_refresh_url: refresh_override,
    })
}

/// Refresh an existing Yggdrasil / Ely.by / LittleSkin token
pub async fn refresh_yggdrasil_account(
    http: &reqwest::Client,
    acc: &Account,
) -> Result<Account> {
    let base = acc
        .auth_server_url
        .as_deref()
        .unwrap_or("https://authserver.ely.by")
        .trim_end_matches('/');

    let endpoint = if let Some(custom_refresh) = acc
        .auth_refresh_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        custom_refresh.to_string()
    } else if acc.account_type == AccountType::ElyBy || base.contains("authserver.ely.by") {
        format!("{}/auth/refresh", base)
    } else if base.ends_with("/authserver") {
        format!("{}/refresh", base)
    } else {
        format!("{}/authserver/refresh", base)
    };

    let payload = json!({
        "accessToken": acc.access_token,
        "clientToken": acc.refresh_token.clone().unwrap_or_else(|| Uuid::new_v4().to_string().replace('-', "")),
        "requestUser": true
    });

    let resp = http
        .post(&endpoint)
        .json(&payload)
        .send()
        .await
        .with_context(|| format!("Не удалось обновить токен через {}", endpoint))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Не удалось обновить токен ({}): {}", status, body));
    }

    let parsed: YggdrasilAuthResponse = resp.json().await?;
    let mut updated = acc.clone();
    updated.access_token = parsed.access_token;
    if let Some(ct) = parsed.client_token {
        updated.refresh_token = Some(ct);
    }
    if let Some(p) = parsed.selected_profile {
        updated.username = p.name;
        updated.uuid = p.id.replace('-', "");
    }
    Ok(updated)
}

/// Pre-fetch Yggdrasil API metadata as Base64 for `-Dauthlibinjector.yggdrasil.prefetched=...`
pub async fn fetch_yggdrasil_prefetched_base64(
    http: &reqwest::Client,
    api_url: &str,
) -> Option<String> {
    let clean = api_url.trim_end_matches('/');
    let meta_url = if clean.contains("authserver.ely.by") {
        "https://authserver.ely.by/api/authlib-injector".to_string()
    } else {
        clean.to_string()
    };
    let resp = http.get(&meta_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(base64_encode(&bytes))
}

/// 4. Microsoft OAuth 2.0 Device Code Flow (Step 1: Request User Code)
pub async fn start_microsoft_device_flow(
    http: &reqwest::Client,
    custom_client_id: Option<&str>,
) -> Result<DeviceCodeResponse> {
    let client_id = custom_client_id
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(MS_CLIENT_ID);

    let resp = http
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", client_id),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .context("Ошибка запроса кода устройства Microsoft OAuth")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Microsoft OAuth вернул ошибку: {}", body));
    }

    let code = resp.json::<DeviceCodeResponse>().await?;
    Ok(code)
}

/// 4. Microsoft OAuth 2.0 Device Code Flow (Step 2: Poll & Exchange MS -> XBL -> XSTS -> Minecraft Profile)
pub async fn poll_microsoft_device_flow(
    http: &reqwest::Client,
    device_code: &str,
    interval_secs: u64,
    custom_client_id: Option<&str>,
) -> Result<Account> {
    let client_id = custom_client_id
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(MS_CLIENT_ID);

    let wait = interval_secs.max(3);
    let mut ms_token: Option<MsTokenResponse> = None;

    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        let resp = http
            .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", device_code),
            ])
            .send()
            .await?;

        if resp.status().is_success() {
            ms_token = Some(resp.json::<MsTokenResponse>().await?);
            break;
        } else {
            let err_json: serde_json::Value = resp.json().await.unwrap_or_default();
            let err_code = err_json["error"].as_str().unwrap_or("");
            if err_code == "authorization_pending" || err_code == "slow_down" {
                continue;
            }
            return Err(anyhow!(
                "Авторизация Microsoft прервана: {}",
                err_json["error_description"]
                    .as_str()
                    .unwrap_or(err_code)
            ));
        }
    }

    let ms_token = ms_token.ok_or_else(|| anyhow!("Время ожидания входа Microsoft истекло"))?;
    complete_xbox_and_minecraft_auth(http, &ms_token.access_token, ms_token.refresh_token, None).await
}

/// Refresh a Microsoft account using its stored `refresh_token`
pub async fn refresh_microsoft_account(
    http: &reqwest::Client,
    acc: &Account,
    custom_client_id: Option<&str>,
) -> Result<Account> {
    let Some(ref rt) = acc.refresh_token else {
        return Err(anyhow!("У аккаунта отсутствует refresh_token"));
    };
    let client_id = custom_client_id
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(MS_CLIENT_ID);

    let resp = http
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", rt.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .context("Ошибка обновления токена Microsoft")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Не удалось обновить токен Microsoft: {}", body));
    }

    let ms_token: MsTokenResponse = resp.json().await?;
    complete_xbox_and_minecraft_auth(
        http,
        &ms_token.access_token,
        ms_token.refresh_token.or_else(|| acc.refresh_token.clone()),
        Some(acc.id.clone()),
    )
    .await
}

async fn complete_xbox_and_minecraft_auth(
    http: &reqwest::Client,
    ms_access_token: &str,
    refresh_token: Option<String>,
    existing_id: Option<String>,
) -> Result<Account> {
    // 1. Xbox Live Authenticate
    let xbl_payload = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", ms_access_token)
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let xbl_resp: XblResponse = http
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Accept", "application/json")
        .json(&xbl_payload)
        .send()
        .await
        .context("Ошибка авторизации Xbox Live")?
        .error_for_status()?
        .json()
        .await?;

    let uhs = xbl_resp.display_claims["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| anyhow!("Не удалось получить UHS из ответа Xbox Live"))?
        .to_string();

    // 2. XSTS Authorize
    let xsts_payload = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_resp.token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let xsts_resp: XblResponse = http
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Accept", "application/json")
        .json(&xsts_payload)
        .send()
        .await
        .context("Ошибка авторизации XSTS")?
        .error_for_status()?
        .json()
        .await?;

    // 3. Minecraft Services Login with Xbox
    let mc_payload = json!({
        "identityToken": format!("XBL3.0 x={};{}", uhs, xsts_resp.token)
    });

    let mc_login: McLoginResponse = http
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&mc_payload)
        .send()
        .await
        .context("Ошибка входа в Minecraft Services")?
        .error_for_status()?
        .json()
        .await?;

    // 4. Fetch Minecraft Profile (verify ownership just like FreesmLauncher 2.3.0!)
    let profile_resp = http
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&mc_login.access_token)
        .send()
        .await
        .context("Ошибка получения профиля Minecraft")?;

    if !profile_resp.status().is_success() {
        return Err(anyhow!(
            "На этом аккаунте Microsoft не куплена лицензия Minecraft Java Edition (или профиль не создан)"
        ));
    }

    let profile: McProfileResponse = profile_resp.json().await?;

    Ok(Account {
        id: existing_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        username: profile.name,
        uuid: profile.id.replace('-', ""),
        access_token: mc_login.access_token,
        refresh_token,
        account_type: AccountType::Microsoft,
        auth_server_url: None,
        auth_login_url: None,
        auth_refresh_url: None,
    })
}

/// Fetch available versions for the selected skin/auth patcher (authlib-injector or unmojang/Loki)
pub async fn fetch_patcher_versions(
    http: &reqwest::Client,
    backend: AuthAgentBackend,
) -> Vec<String> {
    let mut list = vec!["Последняя (авто)".to_string()];
    match backend {
        AuthAgentBackend::Disabled => return vec!["Отключён".to_string()],
        AuthAgentBackend::AuthlibInjector => {
            let mut fetched = Vec::new();
            if let Ok(resp) = http
                .get("https://authlib-injector.yushi.moe/artifacts.json")
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(arr) = json["artifacts"].as_array() {
                            for item in arr.iter().rev() {
                                if let Some(v) = item["version"].as_str() {
                                    let s = v.to_string();
                                    if !fetched.contains(&s) {
                                        fetched.push(s);
                                    }
                                    if fetched.len() >= 16 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if fetched.is_empty() {
                fetched = vec![
                    "1.2.5".into(),
                    "1.2.4".into(),
                    "1.2.3".into(),
                    "1.2.2".into(),
                    "1.2.1".into(),
                    "1.2.0".into(),
                    "1.1.49".into(),
                ];
            }
            list.extend(fetched);
        }
        AuthAgentBackend::Loki => {
            let mut fetched = Vec::new();
            if let Ok(resp) = http
                .get("https://api.github.com/repos/unmojang/loki/releases")
                .header("Accept", "application/vnd.github+json")
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(arr) = resp.json::<Vec<serde_json::Value>>().await {
                        for rel in arr.iter().take(16) {
                            if let Some(tag) = rel["tag_name"].as_str() {
                                let s = tag.to_string();
                                if !fetched.contains(&s) {
                                    fetched.push(s);
                                }
                            }
                        }
                    }
                }
            }
            if fetched.is_empty() {
                fetched = vec![
                    "v0.1.3".into(),
                    "v0.1.2".into(),
                    "v0.1.1".into(),
                    "v0.1.0".into(),
                ];
            }
            list.extend(fetched);
        }
    }
    list
}

fn is_latest_version_tag(v: &str) -> bool {
    let t = v.trim();
    t.is_empty()
        || t.eq_ignore_ascii_case("latest")
        || t.starts_with("Последняя")
        || t == "Отключён"
}

/// Ensure `authlib-injector.jar` (latest or specific version) is downloaded for Ely.by and Custom Yggdrasil accounts
pub async fn ensure_authlib_injector(
    http: &reqwest::Client,
    libraries_dir: &Path,
    version: &str,
) -> Result<PathBuf> {
    let tools_dir = libraries_dir
        .join("moe")
        .join("yushi")
        .join("authlibinjector");
    tokio::fs::create_dir_all(&tools_dir).await?;

    let use_latest = is_latest_version_tag(version);
    let safe_ver: String = version
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    let target_jar = if use_latest || safe_ver.is_empty() {
        tools_dir.join("authlib-injector.jar")
    } else {
        tools_dir.join(format!("authlib-injector-{}.jar", safe_ver))
    };

    if target_jar.exists() {
        return Ok(target_jar);
    }

    let artifact_url = if use_latest || safe_ver.is_empty() {
        "https://authlib-injector.yushi.moe/artifact/latest.json".to_string()
    } else {
        let mut found_build: Option<u64> = None;
        if let Ok(resp) = http
            .get("https://authlib-injector.yushi.moe/artifacts.json")
            .send()
            .await
        {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = json["artifacts"].as_array() {
                    for item in arr.iter().rev() {
                        if item["version"].as_str() == Some(safe_ver.as_str()) {
                            found_build = item["build_number"].as_u64();
                            break;
                        }
                    }
                }
            }
        }
        match found_build {
            Some(b) => format!("https://authlib-injector.yushi.moe/artifact/{}.json", b),
            None => "https://authlib-injector.yushi.moe/artifact/latest.json".to_string(),
        }
    };

    let latest: AuthlibInjectorLatest = http
        .get(&artifact_url)
        .send()
        .await
        .context("Не удалось получить метаданные authlib-injector")?
        .error_for_status()?
        .json()
        .await?;

    let bytes = http
        .get(&latest.download_url)
        .send()
        .await
        .with_context(|| format!("Ошибка скачивания authlib-injector {}", latest.version))?
        .error_for_status()?
        .bytes()
        .await?;

    tokio::fs::write(&target_jar, &bytes).await?;
    Ok(target_jar)
}

/// Ensure `unmojang/Loki` agent jar (latest or specific tag) is downloaded
pub async fn ensure_loki_agent(
    http: &reqwest::Client,
    libraries_dir: &Path,
    version: &str,
) -> Result<PathBuf> {
    let tools_dir = libraries_dir
        .join("org")
        .join("unmojang")
        .join("loki");
    tokio::fs::create_dir_all(&tools_dir).await?;

    let use_latest = is_latest_version_tag(version);
    let safe_ver: String = version
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    let target_jar = if use_latest || safe_ver.is_empty() {
        tools_dir.join("loki.jar")
    } else {
        tools_dir.join(format!("loki-{}.jar", safe_ver))
    };

    if target_jar.exists() {
        return Ok(target_jar);
    }

    let api_url = if use_latest || safe_ver.is_empty() {
        "https://api.github.com/repos/unmojang/loki/releases/latest".to_string()
    } else {
        format!(
            "https://api.github.com/repos/unmojang/loki/releases/tags/{}",
            safe_ver
        )
    };

    let resp = http
        .get(&api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Не удалось получить релиз unmojang/Loki")?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;
    let dl_url = json["assets"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|a| a["name"].as_str().map(|n| n.ends_with(".jar")).unwrap_or(false))
        })
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or_else(|| anyhow!("В релизе unmojang/Loki не найден .jar файл"))?;

    let bytes = http
        .get(dl_url)
        .send()
        .await
        .with_context(|| format!("Ошибка скачивания unmojang/Loki: {}", dl_url))?
        .error_for_status()?
        .bytes()
        .await?;

    tokio::fs::write(&target_jar, &bytes).await?;
    Ok(target_jar)
}
