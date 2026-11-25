#![allow(dead_code)]
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer};
use std::path::{Path, PathBuf};

// ============================================================================
// DYNAMIC MATERIAL DESIGN 3 COLOR PALETTE GENERATOR (CUSTOM ACCENT COLOR)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct DynamicM3Palette {
    pub dark_bg: Color,
    pub dark_surface_low: Color,
    pub dark_surface: Color,
    pub dark_surface_high: Color,
    pub dark_surface_highest: Color,
    pub dark_primary: Color,
    pub dark_on_primary: Color,
    pub dark_primary_container: Color,
    pub dark_on_primary_container: Color,
    pub dark_secondary_container: Color,
    pub dark_on_secondary_container: Color,
    pub dark_outline_variant: Color,

    pub light_bg: Color,
    pub light_surface_low: Color,
    pub light_surface: Color,
    pub light_surface_high: Color,
    pub light_surface_highest: Color,
    pub light_primary: Color,
    pub light_primary_container: Color,
    pub light_on_primary_container: Color,
    pub light_secondary_container: Color,
    pub light_outline_variant: Color,
}

pub fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
        let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
        let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
        Some((r, g, b))
    } else if clean.len() == 3 {
        let r = u8::from_str_radix(&clean[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&clean[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&clean[2..3], 16).ok()? * 17;
        Some((r, g, b))
    } else {
        None
    }
}

pub fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let mut h = if (max - rf).abs() < 1e-6 {
        (gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }
    } else if (max - gf).abs() < 1e-6 {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };
    h /= 6.0;
    (h, s, l)
}

fn hsl_to_color(h: f32, s: f32, l: f32) -> Color {
    let h = h.rem_euclid(1.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let (r, g, b) = if s < 1e-6 {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        let hue2rgb = |mut t: f32| -> f32 {
            if t < 0.0 {
                t += 1.0;
            }
            if t > 1.0 {
                t -= 1.0;
            }
            if t < 1.0 / 6.0 {
                return p + (q - p) * 6.0 * t;
            }
            if t < 0.5 {
                return q;
            }
            if t < 2.0 / 3.0 {
                return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
            }
            p
        };
        (
            hue2rgb(h + 1.0 / 3.0),
            hue2rgb(h),
            hue2rgb(h - 1.0 / 3.0),
        )
    };
    Color::from_rgb_u8(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// Generate a complete harmonious Material Design 3 Dark + Light palette from any hex color!
pub fn generate_m3_palette_from_hex(hex: &str) -> DynamicM3Palette {
    let (r, g, b) = parse_hex_rgb(hex).unwrap_or((139, 124, 246));
    let (h, s, _l) = rgb_to_hsl(r, g, b);
    let sat_surf = (s * 0.28).min(0.22);
    let sat_prim = if s < 0.08 { 0.05 } else { s.clamp(0.35, 0.88) };

    DynamicM3Palette {
        dark_bg: hsl_to_color(h, sat_surf, 0.065),
        dark_surface_low: hsl_to_color(h, sat_surf, 0.098),
        dark_surface: hsl_to_color(h, sat_surf, 0.128),
        dark_surface_high: hsl_to_color(h, sat_surf * 1.1, 0.175),
        dark_surface_highest: hsl_to_color(h, sat_surf * 1.15, 0.225),
        dark_primary: hsl_to_color(h, sat_prim, 0.78),
        dark_on_primary: hsl_to_color(h, (sat_prim * 0.9).min(0.85), 0.16),
        dark_primary_container: hsl_to_color(h, (sat_prim * 0.72).min(0.75), 0.30),
        dark_on_primary_container: hsl_to_color(h, (sat_prim * 0.9).min(0.95), 0.91),
        dark_secondary_container: hsl_to_color(h, (sat_surf * 1.35).min(0.32), 0.27),
        dark_on_secondary_container: hsl_to_color(h, sat_surf * 1.4, 0.89),
        dark_outline_variant: hsl_to_color(h, sat_surf * 1.1, 0.24),

        light_bg: hsl_to_color(h, sat_surf * 0.8, 0.965),
        light_surface_low: hsl_to_color(h, sat_surf * 0.95, 0.935),
        light_surface: Color::from_rgb_u8(255, 255, 255),
        light_surface_high: hsl_to_color(h, sat_surf * 1.05, 0.895),
        light_surface_highest: hsl_to_color(h, sat_surf * 1.1, 0.855),
        light_primary: hsl_to_color(h, sat_prim, 0.41),
        light_primary_container: hsl_to_color(h, (sat_prim * 0.85).min(0.90), 0.88),
        light_on_primary_container: hsl_to_color(h, sat_prim, 0.14),
        light_secondary_container: hsl_to_color(h, sat_surf * 1.25, 0.88),
        light_outline_variant: hsl_to_color(h, sat_surf, 0.80),
    }
}

// ============================================================================
// MINECRAFT SERVICES SKIN & CAPE API (MICROSOFT ACCOUNTS)
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct McProfileSkin {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub variant: String, // "CLASSIC" or "SLIM"
}

#[derive(Debug, Clone, Deserialize)]
pub struct McProfileCape {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub alias: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McSkinCapeProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<McProfileSkin>,
    #[serde(default)]
    pub capes: Vec<McProfileCape>,
}

pub async fn fetch_mc_profile_skins_capes(
    http: &reqwest::Client,
    access_token: &str,
) -> Result<McSkinCapeProfile> {
    let resp = http
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access_token)
        .send()
        .await
        .context("Ошибка подключения к Minecraft Services")?;
    if !resp.status().is_success() {
        let code = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Не удалось получить профиль скинов/плащей (HTTP {}): {}",
            code,
            body
        ));
    }
    resp.json::<McSkinCapeProfile>()
        .await
        .context("Ошибка разбора ответа профиля Minecraft")
}

/// Upload a PNG skin to Microsoft/Mojang account (`POST /minecraft/profile/skins`, raw multipart/form-data)
pub async fn upload_mc_skin(
    http: &reqwest::Client,
    access_token: &str,
    slim: bool,
    png_bytes: &[u8],
) -> Result<()> {
    let variant = if slim { "slim" } else { "classic" };
    let boundary = "----AxyLauncherSkinUploadBoundary7MA4YWxkTrZu0gW";
    let mut body: Vec<u8> = Vec::with_capacity(png_bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"variant\"\r\n\r\n{variant}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(png_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let resp = http
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(access_token)
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .context("Ошибка отправки скина в Minecraft Services")?;

    if !resp.status().is_success() {
        let code = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Ошибка загрузки скина (HTTP {}): {}", code, txt));
    }
    Ok(())
}

/// Reset active skin back to default Steve/Alex (`DELETE /minecraft/profile/skins/active`)
pub async fn reset_mc_skin(http: &reqwest::Client, access_token: &str) -> Result<()> {
    let resp = http
        .delete("https://api.minecraftservices.com/minecraft/profile/skins/active")
        .bearer_auth(access_token)
        .send()
        .await
        .context("Ошибка сброса скина")?;
    if !resp.status().is_success() {
        let code = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Не удалось сбросить скин (HTTP {}): {}", code, txt));
    }
    Ok(())
}

/// Equip or hide a cape (`PUT` or `DELETE /minecraft/profile/capes/active`)
pub async fn set_mc_cape(
    http: &reqwest::Client,
    access_token: &str,
    cape_id: Option<&str>,
) -> Result<()> {
    let resp = match cape_id {
        Some(id) if !id.is_empty() => http
            .put("https://api.minecraftservices.com/minecraft/profile/capes/active")
            .bearer_auth(access_token)
            .json(&serde_json::json!({ "capeId": id }))
            .send()
            .await
            .context("Ошибка установки плаща")?,
        _ => http
            .delete("https://api.minecraftservices.com/minecraft/profile/capes/active")
            .bearer_auth(access_token)
            .send()
            .await
            .context("Ошибка скрытия плаща")?,
    };
    if !resp.status().is_success() {
        let code = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Не удалось обновить плащ (HTTP {}): {}",
            code,
            txt
        ));
    }
    Ok(())
}

/// Import skin PNG from a direct URL into `skins/`
pub async fn import_skin_from_url(
    http: &reqwest::Client,
    url: &str,
    skins_dir: &Path,
    preferred_name: Option<&str>,
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(skins_dir).await?;
    let bytes = http
        .get(url.trim())
        .send()
        .await
        .context("Не удалось скачать скин по URL")?
        .error_for_status()?
        .bytes()
        .await?;
    if bytes.len() < 8 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(anyhow!("Файл по ссылке не является корректным PNG-изображением"));
    }
    let raw_name = preferred_name.unwrap_or_else(|| {
        url.trim()
            .rsplit('/')
            .next()
            .and_then(|s| s.split('?').next())
            .unwrap_or("imported_skin")
    });
    let stem = raw_name.trim_end_matches(".png");
    let safe: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let fname = if safe.is_empty() {
        "imported_skin.png".to_string()
    } else {
        format!("{}.png", safe)
    };
    let dest = skins_dir.join(fname);
    tokio::fs::write(&dest, &bytes).await?;
    Ok(dest)
}

/// Import skin PNG by Minecraft username via Mojang Profile + SessionServer API
pub async fn import_skin_from_username(
    http: &reqwest::Client,
    username: &str,
    skins_dir: &Path,
) -> Result<PathBuf> {
    let clean = username.trim();
    if clean.is_empty() {
        return Err(anyhow!("Введите никнейм игрока"));
    }
    let prof_url = format!(
        "https://api.mojang.com/users/profiles/minecraft/{}",
        clean
    );
    let prof_resp = http
        .get(&prof_url)
        .send()
        .await
        .context("Ошибка запроса к Mojang API")?;
    if !prof_resp.status().is_success() {
        return Err(anyhow!("Игрок «{}» не найден в Mojang", clean));
    }
    let prof_json: serde_json::Value = prof_resp.json().await?;
    let uuid = prof_json["id"]
        .as_str()
        .ok_or_else(|| anyhow!("Mojang не вернул UUID игрока"))?;

    let sess_url = format!(
        "https://sessionserver.mojang.com/session/minecraft/profile/{}",
        uuid
    );
    let sess_json: serde_json::Value = http
        .get(&sess_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let val_b64 = sess_json["properties"]
        .as_array()
        .and_then(|arr| arr.iter().find(|p| p["name"].as_str() == Some("textures")))
        .and_then(|p| p["value"].as_str())
        .ok_or_else(|| anyhow!("У игрока «{}» не найдены текстуры скина", clean))?;

    let decoded = base64_decode(val_b64).ok_or_else(|| anyhow!("Ошибка декодирования текстур"))?;
    let tex_json: serde_json::Value = serde_json::from_slice(&decoded)?;
    let skin_url = tex_json["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or_else(|| anyhow!("У игрока «{}» установлен стандартный скин без URL", clean))?;

    import_skin_from_url(http, skin_url, skins_dir, Some(clean)).await
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut val = 0u32;
    let mut bits = 0i32;
    for b in input.bytes() {
        let v = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return None,
        };
        val = (val << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((val >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

// ============================================================================
// PURE-RUST NEAREST-NEIGHBOR SKIN & CAPE/ELYTRA 2D COMPOSITOR (SLINT IMAGE)
// ============================================================================

fn blit_rect(
    src: &SharedPixelBuffer<Rgba8Pixel>,
    dst: &mut SharedPixelBuffer<Rgba8Pixel>,
    sx: u32,
    sy: u32,
    sw: u32,
    sh: u32,
    dx: u32,
    dy: u32,
    flip_x: bool,
    scale: u32,
    alpha_blend: bool,
) {
    let src_w = src.width();
    let src_h = src.height();
    let dst_w = dst.width();
    let dst_h = dst.height();
    let src_px = src.as_slice();
    let dst_px = dst.make_mut_slice();

    for y in 0..sh {
        let py = sy + y;
        if py >= src_h {
            continue;
        }
        for x in 0..sw {
            let px = sx + if flip_x { sw - 1 - x } else { x };
            if px >= src_w {
                continue;
            }
            let sp = src_px[(py * src_w + px) as usize];
            if sp.a == 0 {
                continue;
            }
            for sy_s in 0..scale {
                let oy = (dy + y) * scale + sy_s;
                if oy >= dst_h {
                    continue;
                }
                for sx_s in 0..scale {
                    let ox = (dx + x) * scale + sx_s;
                    if ox >= dst_w {
                        continue;
                    }
                    let idx = (oy * dst_w + ox) as usize;
                    if !alpha_blend || sp.a == 255 {
                        dst_px[idx] = sp;
                    } else {
                        let dp = dst_px[idx];
                        let a = sp.a as u32;
                        let inv = 255 - a;
                        dst_px[idx] = Rgba8Pixel::new(
                            ((sp.r as u32 * a + dp.r as u32 * inv) / 255) as u8,
                            ((sp.g as u32 * a + dp.g as u32 * inv) / 255) as u8,
                            ((sp.b as u32 * a + dp.b as u32 * inv) / 255) as u8,
                            (a + (dp.a as u32 * inv) / 255).min(255) as u8,
                        );
                    }
                }
            }
        }
    }
}

/// Render crisp Front + Back 2D Minecraft skin preview directly from a 64x64 or 64x32 PNG!
pub fn render_skin_front_back(png_path: &Path, slim: bool, scale: u32) -> Option<Image> {
    let raw_img = Image::load_from_path(png_path).ok()?;
    let src = raw_img.to_rgba8()?;
    let modern = src.height() >= 64;
    let arm_w = if slim && modern { 3 } else { 4 };
    let arm_x_l = 4 - arm_w; // right-align left side of front view against torso

    // Canvas: 16px front + 4px gap + 16px back = 36 x 32 logical pixels
    let mut dst = SharedPixelBuffer::<Rgba8Pixel>::new(36 * scale, 32 * scale);

    // === FRONT (x offset = 0) ===
    // Head + Hat
    blit_rect(&src, &mut dst, 8, 8, 8, 8, 4, 0, false, scale, false);
    blit_rect(&src, &mut dst, 40, 8, 8, 8, 4, 0, false, scale, true);
    // Torso + Jacket
    blit_rect(&src, &mut dst, 20, 20, 8, 12, 4, 8, false, scale, false);
    if modern {
        blit_rect(&src, &mut dst, 20, 36, 8, 12, 4, 8, false, scale, true);
    }
    // Right Arm (viewer's left) + Sleeve
    blit_rect(&src, &mut dst, 44, 20, arm_w, 12, arm_x_l, 8, false, scale, false);
    if modern {
        blit_rect(&src, &mut dst, 44, 36, arm_w, 12, arm_x_l, 8, false, scale, true);
    }
    // Left Arm (viewer's right) + Sleeve
    if modern {
        blit_rect(&src, &mut dst, 36, 52, arm_w, 12, 12, 8, false, scale, false);
        blit_rect(&src, &mut dst, 52, 52, arm_w, 12, 12, 8, false, scale, true);
    } else {
        blit_rect(&src, &mut dst, 44, 20, 4, 12, 12, 8, true, scale, false);
    }
    // Right Leg (viewer's left) + Pant
    blit_rect(&src, &mut dst, 4, 20, 4, 12, 4, 20, false, scale, false);
    if modern {
        blit_rect(&src, &mut dst, 4, 36, 4, 12, 4, 20, false, scale, true);
    }
    // Left Leg (viewer's right) + Pant
    if modern {
        blit_rect(&src, &mut dst, 20, 52, 4, 12, 8, 20, false, scale, false);
        blit_rect(&src, &mut dst, 4, 52, 4, 12, 8, 20, false, scale, true);
    } else {
        blit_rect(&src, &mut dst, 4, 20, 4, 12, 8, 20, true, scale, false);
    }

    // === BACK (x offset = 20) ===
    let bx = 20;
    // Head back + Hat back
    blit_rect(&src, &mut dst, 24, 8, 8, 8, bx + 4, 0, false, scale, false);
    blit_rect(&src, &mut dst, 56, 8, 8, 8, bx + 4, 0, false, scale, true);
    // Torso back + Jacket back
    blit_rect(&src, &mut dst, 32, 20, 8, 12, bx + 4, 8, false, scale, false);
    if modern {
        blit_rect(&src, &mut dst, 32, 36, 8, 12, bx + 4, 8, false, scale, true);
    }
    // Left Arm back (viewer's left)
    if modern {
        let la_back_x = if slim { 43 } else { 44 };
        blit_rect(&src, &mut dst, la_back_x, 52, arm_w, 12, bx + arm_x_l, 8, false, scale, false);
        blit_rect(&src, &mut dst, la_back_x + 16, 52, arm_w, 12, bx + arm_x_l, 8, false, scale, true);
    } else {
        blit_rect(&src, &mut dst, 52, 20, 4, 12, bx, 8, true, scale, false);
    }
    // Right Arm back (viewer's right)
    let ra_back_x = if slim && modern { 51 } else { 52 };
    blit_rect(&src, &mut dst, ra_back_x, 20, arm_w, 12, bx + 12, 8, false, scale, false);
    if modern {
        blit_rect(&src, &mut dst, ra_back_x, 36, arm_w, 12, bx + 12, 8, false, scale, true);
    }
    // Left Leg back (viewer's left)
    if modern {
        blit_rect(&src, &mut dst, 28, 52, 4, 12, bx + 4, 20, false, scale, false);
        blit_rect(&src, &mut dst, 12, 52, 4, 12, bx + 4, 20, false, scale, true);
    } else {
        blit_rect(&src, &mut dst, 12, 20, 4, 12, bx + 4, 20, true, scale, false);
    }
    // Right Leg back (viewer's right)
    blit_rect(&src, &mut dst, 12, 20, 4, 12, bx + 8, 20, false, scale, false);
    if modern {
        blit_rect(&src, &mut dst, 12, 36, 4, 12, bx + 8, 20, false, scale, true);
    }

    Some(Image::from_rgba8(dst))
}

/// Render crisp Cape (10x16) OR Elytra wings (22x20) preview from a standard 64x32 Minecraft cape PNG!
pub fn render_cape_or_elytra(png_path: &Path, preview_elytra: bool, scale: u32) -> Option<Image> {
    let raw_img = Image::load_from_path(png_path).ok()?;
    let src = raw_img.to_rgba8()?;
    let factor = (src.width() / 64).max(1);
    if preview_elytra {
        let mut dst = SharedPixelBuffer::<Rgba8Pixel>::new(22 * factor * scale, 20 * factor * scale);
        // Left wing from (22, 2, 10, 20) + mirrored Right wing
        blit_rect(
            &src,
            &mut dst,
            22 * factor,
            2 * factor,
            10 * factor,
            20 * factor,
            0,
            0,
            false,
            scale,
            false,
        );
        blit_rect(
            &src,
            &mut dst,
            22 * factor,
            2 * factor,
            10 * factor,
            20 * factor,
            12 * factor,
            0,
            true,
            scale,
            false,
        );
        Some(Image::from_rgba8(dst))
    } else {
        let mut dst = SharedPixelBuffer::<Rgba8Pixel>::new(10 * factor * scale, 16 * factor * scale);
        blit_rect(
            &src,
            &mut dst,
            1 * factor,
            1 * factor,
            10 * factor,
            16 * factor,
            0,
            0,
            false,
            scale,
            false,
        );
        Some(Image::from_rgba8(dst))
    }
}

/// Synthesize a crisp placeholder Minecraft character preview (used before any PNG is loaded or in tests)
pub fn render_placeholder_skin(slim: bool, scale: u32) -> Image {
    let mut dst = SharedPixelBuffer::<Rgba8Pixel>::new(36 * scale, 32 * scale);
    let fill = |buf: &mut SharedPixelBuffer<Rgba8Pixel>,
                x: u32,
                y: u32,
                w: u32,
                h: u32,
                c: Rgba8Pixel| {
        let bw = buf.width();
        let px = buf.make_mut_slice();
        for yy in (y * scale)..((y + h) * scale) {
            for xx in (x * scale)..((x + w) * scale) {
                px[(yy * bw + xx) as usize] = c;
            }
        }
    };
    let arm_w = if slim { 3 } else { 4 };
    let ax = 4 - arm_w;
    let hair = Rgba8Pixel::new(34, 30, 44, 255);
    let accent = Rgba8Pixel::new(118, 88, 198, 255);
    let face = Rgba8Pixel::new(235, 206, 194, 255);
    let coat = Rgba8Pixel::new(40, 38, 52, 255);
    let pants = Rgba8Pixel::new(28, 28, 38, 255);
    let shoes = Rgba8Pixel::new(230, 232, 240, 255);

    // Front
    fill(&mut dst, 4, 0, 8, 8, hair);
    fill(&mut dst, 5, 4, 6, 3, face);
    fill(&mut dst, 4, 8, 8, 12, coat);
    fill(&mut dst, 5, 11, 6, 2, accent);
    fill(&mut dst, ax, 8, arm_w, 12, coat);
    fill(&mut dst, 12, 8, arm_w, 12, coat);
    fill(&mut dst, 4, 20, 8, 10, pants);
    fill(&mut dst, 4, 30, 4, 2, shoes);
    fill(&mut dst, 8, 30, 4, 2, shoes);

    // Back
    let bx = 20;
    fill(&mut dst, bx + 4, 0, 8, 8, hair);
    fill(&mut dst, bx + 5, 5, 6, 2, accent);
    fill(&mut dst, bx + 4, 8, 8, 12, coat);
    fill(&mut dst, bx + ax, 8, arm_w, 12, coat);
    fill(&mut dst, bx + 12, 8, arm_w, 12, coat);
    fill(&mut dst, bx + 4, 20, 8, 10, pants);
    fill(&mut dst, bx + 4, 30, 4, 2, shoes);
    fill(&mut dst, bx + 8, 30, 4, 2, shoes);
    Image::from_rgba8(dst)
}

/// Synthesize a crisp placeholder cape/elytra preview (used in headless tests or when cape PNG is unavailable)
pub fn render_placeholder_cape(preview_elytra: bool) -> Image {
    let scale = 12u32;
    if preview_elytra {
        let mut dst = SharedPixelBuffer::<Rgba8Pixel>::new(22 * scale, 20 * scale);
        let bw = dst.width();
        let px = dst.make_mut_slice();
        let wing = Rgba8Pixel::new(98, 102, 126, 255);
        let trim = Rgba8Pixel::new(150, 130, 220, 255);
        for y in 0..20 {
            for x in 0..22 {
                if x == 10 || x == 11 {
                    continue;
                }
                let c = if y < 2 || y > 17 || x == 0 || x == 21 {
                    trim
                } else {
                    wing
                };
                for yy in (y * scale)..((y + 1) * scale) {
                    for xx in (x * scale)..((x + 1) * scale) {
                        px[(yy * bw + xx) as usize] = c;
                    }
                }
            }
        }
        Image::from_rgba8(dst)
    } else {
        let mut dst = SharedPixelBuffer::<Rgba8Pixel>::new(10 * scale, 16 * scale);
        let bw = dst.width();
        let px = dst.make_mut_slice();
        let bg = Rgba8Pixel::new(36, 36, 40, 255);
        let teal = Rgba8Pixel::new(26, 152, 156, 255);
        let magenta = Rgba8Pixel::new(196, 36, 86, 255);
        let white = Rgba8Pixel::new(242, 244, 248, 255);
        for y in 0..16 {
            for x in 0..10 {
                let c = if x == 0 {
                    teal
                } else if x == 9 {
                    magenta
                } else if (y == 5 || y == 6) && (x == 2 || x == 3) {
                    teal
                } else if (y == 5 || y == 6) && (x == 6 || x == 7) {
                    magenta
                } else if (y == 7 || y == 8) && (x == 4 || x == 5) {
                    white
                } else if (y == 9 || y == 10) && (x == 3 || x == 6) {
                    white
                } else {
                    bg
                };
                for yy in (y * scale)..((y + 1) * scale) {
                    for xx in (x * scale)..((x + 1) * scale) {
                        px[(yy * bw + xx) as usize] = c;
                    }
                }
            }
        }
        Image::from_rgba8(dst)
    }
}
