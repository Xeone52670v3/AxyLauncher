use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::path::PathBuf;

/// Sample anti-aliased RGBA pixel `(r, g, b, a)` for the AxyLauncher emblem in normalized `[0, 1]` coordinates.
fn sample_logo_pixel(u: f32, v: f32) -> [u8; 4] {
    // Signed distance to rounded squircle box centered at (0.5, 0.5)
    let cx = (u - 0.5).abs();
    let cy = (v - 0.5).abs();

    let sd_rrect = |hx: f32, hy: f32, r: f32| -> f32 {
        let qx = cx - (hx - r);
        let qy = cy - (hy - r);
        let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        let inside = qx.max(qy).min(0.0);
        out + inside - r
    };

    // Outer rounded badge
    let d_outer = sd_rrect(0.46, 0.46, 0.21);
    if d_outer > 0.015 {
        return [0, 0, 0, 0];
    }
    let outer_alpha = ((0.012 - d_outer) / 0.024).clamp(0.0, 1.0);

    // Rich diagonal M3 blue-indigo gradient background
    let t = ((u * 0.55 + v * 0.45) - 0.1).clamp(0.0, 1.0);
    let mut r = 26.0 + (58.0 - 26.0) * (1.0 - t);
    let mut g = 62.0 + (118.0 - 62.0) * (1.0 - t);
    let mut b = 138.0 + (232.0 - 138.0) * (1.0 - t);

    // Inner dark card with crisp #A8C7FA border
    let d_border = sd_rrect(0.365, 0.365, 0.13);
    let d_inner = sd_rrect(0.335, 0.335, 0.105);

    if d_border < 0.01 {
        let w_border = ((0.01 - d_border) / 0.02).clamp(0.0, 1.0);
        r = r * (1.0 - w_border) + 168.0 * w_border;
        g = g * (1.0 - w_border) + 199.0 * w_border;
        b = b * (1.0 - w_border) + 250.0 * w_border;
    }
    if d_inner < 0.01 {
        let w_in = ((0.01 - d_inner) / 0.02).clamp(0.0, 1.0);
        let bg_r = 20.0 + 8.0 * (1.0 - v);
        let bg_g = 24.0 + 10.0 * (1.0 - v);
        let bg_b = 36.0 + 14.0 * (1.0 - v);
        r = r * (1.0 - w_in) + bg_r * w_in;
        g = g * (1.0 - w_in) + bg_g * w_in;
        b = b * (1.0 - w_in) + bg_b * w_in;
    }

    // Crisp 8x8 stylized Creeper / Block emblem inside [0.24 .. 0.76]
    if (0.24..0.76).contains(&u) && (0.24..0.76).contains(&v) {
        let gx = ((u - 0.24) / 0.52 * 8.0).floor() as i32;
        let gy = ((v - 0.24) / 0.52 * 8.0).floor() as i32;
        let is_eye = ((1..=2).contains(&gx) || (5..=6).contains(&gx)) && (1..=2).contains(&gy);
        let is_nose = (3..=4).contains(&gx) && (3..=5).contains(&gy);
        let is_mouth = ((2..=2).contains(&gx) || (5..=5).contains(&gx)) && (4..=6).contains(&gy);

        if is_eye {
            // Bright M3 primary ice-blue eyes
            r = 168.0;
            g = 199.0;
            b = 250.0;
        } else if is_nose || is_mouth {
            // Emerald accent nose/mouth
            r = 118.0;
            g = 218.0;
            b = 152.0;
        }
    }

    [
        r.round().clamp(0.0, 255.0) as u8,
        g.round().clamp(0.0, 255.0) as u8,
        b.round().clamp(0.0, 255.0) as u8,
        (outer_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// Render RGBA8 `[R, G, B, A]` buffer with 2x2 supersampling for crisp edges at any size.
pub fn render_logo_rgba(size: u32) -> Vec<u8> {
    let sz = size.max(16);
    let mut buf = Vec::with_capacity((sz * sz * 4) as usize);
    for y in 0..sz {
        for x in 0..sz {
            let mut acc = [0u32; 4];
            for sy in [0.25f32, 0.75f32] {
                for sx in [0.25f32, 0.75f32] {
                    let u = (x as f32 + sx) / sz as f32;
                    let v = (y as f32 + sy) / sz as f32;
                    let px = sample_logo_pixel(u, v);
                    acc[0] += px[0] as u32;
                    acc[1] += px[1] as u32;
                    acc[2] += px[2] as u32;
                    acc[3] += px[3] as u32;
                }
            }
            buf.push((acc[0] / 4) as u8);
            buf.push((acc[1] / 4) as u8);
            buf.push((acc[2] / 4) as u8);
            buf.push((acc[3] / 4) as u8);
        }
    }
    buf
}

/// Render Slint `Image` for `Window.icon` (`_NET_WM_ICON`), top-left toolbar logo, and bottom status bar.
pub fn render_logo_slint_image(size: u32) -> Image {
    let sz = size.max(16);
    let rgba = render_logo_rgba(sz);
    let mut pixel_buf = SharedPixelBuffer::<Rgba8Pixel>::new(sz, sz);
    pixel_buf.make_mut_bytes().copy_from_slice(&rgba);
    Image::from_rgba8(pixel_buf)
}

/// Render ARGB32 network-byte-order (`[A, R, G, B]`) pixmap for Linux StatusNotifierItem (`ksni::Icon`).
#[cfg(target_os = "linux")]
pub fn render_logo_argb32_be(size: u32) -> Vec<u8> {
    let rgba = render_logo_rgba(size);
    let mut argb = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        argb.push(px[3]); // A
        argb.push(px[0]); // R
        argb.push(px[1]); // G
        argb.push(px[2]); // B
    }
    argb
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Minimal RFC-2083 valid RGBA PNG encoder in pure Rust (zero extra dependencies).
pub fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw_scanlines = Vec::with_capacity(((width * 4 + 1) * height) as usize);
    let stride = (width * 4) as usize;
    for y in 0..height as usize {
        raw_scanlines.push(0u8); // Filter type 0 (None)
        let row_start = y * stride;
        raw_scanlines.extend_from_slice(&rgba[row_start..row_start + stride]);
    }

    // Wrap raw_scanlines in valid zlib stream using uncompressed DEFLATE blocks (max 65535 bytes per block)
    let mut zlib = Vec::with_capacity(raw_scanlines.len() + 32);
    zlib.extend_from_slice(&[0x78, 0x01]); // CMF/FLG (zlib header, no preset dict)
    let chunks: Vec<&[u8]> = raw_scanlines.chunks(65_535).collect();
    for (idx, block) in chunks.iter().enumerate() {
        let is_last = idx + 1 == chunks.len();
        zlib.push(if is_last { 0x01 } else { 0x00 });
        let len = block.len() as u16;
        let nlen = !len;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&nlen.to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw_scanlines).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG magic

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type 6 = RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &zlib);
    write_png_chunk(&mut png, b"IEND", &[]);
    png
}

pub const XDG_APP_ID: &str = "org.axylauncher.AxyLauncher";

pub const AXYLAUNCHER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#3A76E8"/>
      <stop offset="100%" stop-color="#1A3E8A"/>
    </linearGradient>
  </defs>
  <rect x="10" y="10" width="236" height="236" rx="54" fill="url(#bg)"/>
  <rect x="36" y="36" width="184" height="184" rx="34" fill="#151924" stroke="#A8C7FA" stroke-width="8"/>
  <path d="M78 82 h34 v34 h-34 z M144 82 h34 v34 h-34 z" fill="#A8C7FA"/>
  <path d="M112 116 h32 v48 h-32 z M94 134 h18 v46 h-18 z M144 134 h18 v46 h-18 z" fill="#76DA98"/>
</svg>
"##;

/// Register `.desktop` entry (`org.axylauncher.AxyLauncher.desktop`) and `hicolor`/`pixmaps` icons
/// in `~/.local/share/` on Linux, and refresh KDE/XDG desktop caches when updated so Wayland
/// compositors (KDE KWin, Plasma Task Manager, GNOME, Hyprland) display the window & taskbar icon
/// instead of the generic Wayland "W" placeholder.
pub fn ensure_linux_desktop_icon_integration() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let data_local = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let Some(share_dir) = data_local else {
        return;
    };

    let mut needs_cache_refresh = false;

    for sz in [16u32, 24, 32, 48, 64, 128, 256] {
        let icon_dir = share_dir.join(format!("icons/hicolor/{sz}x{sz}/apps"));
        if std::fs::create_dir_all(&icon_dir).is_ok() {
            let primary_png = icon_dir.join(format!("{XDG_APP_ID}.png"));
            let short_png = icon_dir.join("axylauncher.png");
            if !primary_png.exists() || !short_png.exists() {
                let png_bytes = encode_png_rgba(sz, sz, &render_logo_rgba(sz));
                let _ = std::fs::write(&primary_png, &png_bytes);
                let _ = std::fs::write(&short_png, &png_bytes);
                needs_cache_refresh = true;
            }
        }
    }

    let pixmaps_dir = share_dir.join("pixmaps");
    if std::fs::create_dir_all(&pixmaps_dir).is_ok() {
        let pix_png = pixmaps_dir.join(format!("{XDG_APP_ID}.png"));
        if !pix_png.exists() {
            let png_bytes = encode_png_rgba(128, 128, &render_logo_rgba(128));
            let _ = std::fs::write(&pix_png, &png_bytes);
            let _ = std::fs::write(pixmaps_dir.join("axylauncher.png"), &png_bytes);
            needs_cache_refresh = true;
        }
    }

    let svg_dir = share_dir.join("icons/hicolor/scalable/apps");
    if std::fs::create_dir_all(&svg_dir).is_ok() {
        let primary_svg = svg_dir.join(format!("{XDG_APP_ID}.svg"));
        if std::fs::read_to_string(&primary_svg).ok().as_deref() != Some(AXYLAUNCHER_SVG) {
            let _ = std::fs::write(&primary_svg, AXYLAUNCHER_SVG);
            let _ = std::fs::write(svg_dir.join("axylauncher.svg"), AXYLAUNCHER_SVG);
            needs_cache_refresh = true;
        }
    }

    let apps_dir = share_dir.join("applications");
    if std::fs::create_dir_all(&apps_dir).is_ok() {
        // Remove obsolete desktop entries from previous builds so KDE doesn't show duplicates
        for old_name in [
            "minelauncher.desktop",
            "MineLauncher.desktop",
            "AxyLauncher.desktop",
            "axylauncher.desktop",
        ] {
            let old_path = apps_dir.join(old_name);
            if old_path.exists() {
                let _ = std::fs::remove_file(&old_path);
                needs_cache_refresh = true;
            }
        }

        let exe_path = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "axylauncher".to_string());
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nVersion=1.0\nName=AxyLauncher\nGenericName=Minecraft Launcher\nComment=AxyLauncher — Minecraft Launcher in Rust\nExec=\"{exe_path}\"\nIcon={XDG_APP_ID}\nTerminal=false\nCategories=Game;\nStartupWMClass={XDG_APP_ID}\n"
        );
        let desktop_path = apps_dir.join(format!("{XDG_APP_ID}.desktop"));
        if std::fs::read_to_string(&desktop_path).ok().as_deref() != Some(desktop.as_str()) {
            let _ = std::fs::write(&desktop_path, &desktop);
            needs_cache_refresh = true;
        }
    }

    if needs_cache_refresh {
        let hicolor_dir = share_dir.join("icons/hicolor");
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args(["-q", "-f", "-t"])
            .arg(&hicolor_dir)
            .status();
        let _ = std::process::Command::new("update-desktop-database")
            .arg("-q")
            .arg(&apps_dir)
            .status();
        if std::process::Command::new("kbuildsycoca6")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            let _ = std::process::Command::new("kbuildsycoca5")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}
