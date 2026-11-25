use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::sync::OnceLock;

const CAT_SIZE: usize = 256;

struct Canvas {
    buf: Vec<u8>,
}

impl Canvas {
    fn new() -> Self {
        Self {
            buf: vec![0u8; CAT_SIZE * CAT_SIZE * 4],
        }
    }

    fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= CAT_SIZE as i32 || y >= CAT_SIZE as i32 || color[3] == 0 {
            return;
        }
        let idx = (y as usize * CAT_SIZE + x as usize) * 4;
        let sa = color[3] as f32 / 255.0;
        let da = self.buf[idx + 3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0001 {
            return;
        }
        for c in 0..3 {
            let sc = color[c] as f32;
            let dc = self.buf[idx + c] as f32;
            let out_c = (sc * sa + dc * da * (1.0 - sa)) / out_a;
            self.buf[idx + c] = out_c.round().clamp(0.0, 255.0) as u8;
        }
        self.buf[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: [u8; 4]) {
        let min_x = (cx - rx - 2.0).floor() as i32;
        let max_x = (cx + rx + 2.0).ceil() as i32;
        let min_y = (cy - ry - 2.0).floor() as i32;
        let max_y = (cy + ry + 2.0).ceil() as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= 1.02 {
                    let aa = ((1.02 - dist) * 28.0).clamp(0.0, 1.0);
                    self.blend_pixel(x, y, [color[0], color[1], color[2], (color[3] as f32 * aa) as u8]);
                }
            }
        }
    }

    fn fill_triangle(&mut self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), color: [u8; 4]) {
        let min_x = p0.0.min(p1.0).min(p2.0).floor() as i32 - 1;
        let max_x = p0.0.max(p1.0).max(p2.0).ceil() as i32 + 1;
        let min_y = p0.1.min(p1.1).min(p2.1).floor() as i32 - 1;
        let max_y = p0.1.max(p1.1).max(p2.1).ceil() as i32 + 1;
        let edge = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| -> f32 {
            (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)
        };
        let area = edge(p0, p1, p2);
        if area.abs() < 0.001 {
            return;
        }
        let sign = area.signum();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let pt = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = edge(p1, p2, pt) * sign;
                let w1 = edge(p2, p0, pt) * sign;
                let w2 = edge(p0, p1, pt) * sign;
                if w0 >= -1.0 && w1 >= -1.0 && w2 >= -1.0 {
                    let m = w0.min(w1).min(w2);
                    let aa = (m * 0.12 + 0.85).clamp(0.0, 1.0);
                    self.blend_pixel(x, y, [color[0], color[1], color[2], (color[3] as f32 * aa) as u8]);
                }
            }
        }
    }

    fn draw_capsule(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32, color: [u8; 4]) {
        let min_x = (x0.min(x1) - radius - 2.0).floor() as i32;
        let max_x = (x0.max(x1) + radius + 2.0).ceil() as i32;
        let min_y = (y0.min(y1) - radius - 2.0).floor() as i32;
        let max_y = (y0.max(y1) + radius + 2.0).ceil() as i32;
        let vx = x1 - x0;
        let vy = y1 - y0;
        let len2 = vx * vx + vy * vy;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = if len2 > 0.0001 {
                    (((px - x0) * vx + (py - y0) * vy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cx = x0 + vx * t;
                let cy = y0 + vy * t;
                let dist = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                if dist <= radius + 1.0 {
                    let aa = (radius + 1.0 - dist).clamp(0.0, 1.0);
                    self.blend_pixel(x, y, [color[0], color[1], color[2], (color[3] as f32 * aa) as u8]);
                }
            }
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

fn image_from_rgba(bytes: &[u8]) -> Image {
    let mut pb = SharedPixelBuffer::<Rgba8Pixel>::new(CAT_SIZE as u32, CAT_SIZE as u32);
    pb.make_mut_bytes().copy_from_slice(bytes);
    Image::from_rgba8(pb)
}

struct CatPalette {
    coat: [u8; 4],
    coat_shade: [u8; 4],
    ear_inner: [u8; 4],
    belly: [u8; 4],
    muzzle: [u8; 4],
    eye: [u8; 4],
    nose: [u8; 4],
    stripe: Option<[u8; 4]>,
    patch_a: Option<[u8; 4]>,
    patch_b: Option<[u8; 4]>,
    sparkle: bool,
}

fn draw_detailed_cat(pal: CatPalette) -> Vec<u8> {
    let mut c = Canvas::new();

    // Soft ground shadow
    c.fill_ellipse(128.0, 232.0, 76.0, 14.0, [0, 0, 0, 75]);

    // Curled tail wrapping around right side
    let tail_pts = [
        (170.0, 204.0),
        (194.0, 200.0),
        (214.0, 184.0),
        (222.0, 158.0),
        (214.0, 134.0),
        (198.0, 126.0),
    ];
    for w in tail_pts.windows(2) {
        c.draw_capsule(w[0].0, w[0].1, w[1].0, w[1].1, 13.0, pal.coat);
    }
    if let Some(st) = pal.stripe {
        c.fill_ellipse(208.0, 190.0, 6.0, 10.0, st);
        c.fill_ellipse(219.0, 162.0, 10.0, 5.0, st);
        c.fill_ellipse(210.0, 134.0, 7.0, 9.0, st);
    } else if pal.belly != pal.coat {
        c.fill_ellipse(199.0, 126.0, 11.0, 11.0, pal.belly);
    }

    // Body (sitting plump silhouette)
    c.fill_ellipse(128.0, 174.0, 58.0, 56.0, pal.coat_shade);
    c.fill_ellipse(128.0, 169.0, 55.0, 52.0, pal.coat);

    // Optional calico patches on body
    if let Some(pa) = pal.patch_a {
        c.fill_ellipse(92.0, 168.0, 22.0, 26.0, pa);
    }
    if let Some(pb) = pal.patch_b {
        c.fill_ellipse(164.0, 178.0, 20.0, 24.0, pb);
    }

    // Chest bib / belly
    c.fill_ellipse(128.0, 176.0, 32.0, 38.0, pal.belly);

    // Tabby body stripes
    if let Some(st) = pal.stripe {
        c.draw_capsule(79.0, 156.0, 96.0, 162.0, 4.0, st);
        c.draw_capsule(77.0, 174.0, 95.0, 178.0, 4.0, st);
        c.draw_capsule(160.0, 162.0, 177.0, 156.0, 4.0, st);
        c.draw_capsule(161.0, 178.0, 179.0, 174.0, 4.0, st);
    }

    // Back paws tucked at sides
    c.fill_ellipse(82.0, 214.0, 18.0, 13.0, pal.coat_shade);
    c.fill_ellipse(174.0, 214.0, 18.0, 13.0, pal.coat_shade);

    // Front paws
    c.fill_ellipse(106.0, 220.0, 15.0, 12.0, pal.belly);
    c.fill_ellipse(150.0, 220.0, 15.0, 12.0, pal.belly);
    // Toe beans / paw lines
    c.draw_capsule(101.0, 222.0, 101.0, 228.0, 1.3, [40, 35, 45, 110]);
    c.draw_capsule(111.0, 222.0, 111.0, 228.0, 1.3, [40, 35, 45, 110]);
    c.draw_capsule(145.0, 222.0, 145.0, 228.0, 1.3, [40, 35, 45, 110]);
    c.draw_capsule(155.0, 222.0, 155.0, 228.0, 1.3, [40, 35, 45, 110]);

    // Ears
    c.fill_triangle((72.0, 92.0), (64.0, 34.0), (108.0, 66.0), pal.coat);
    c.fill_triangle((184.0, 92.0), (192.0, 34.0), (148.0, 66.0), pal.coat);
    if let Some(pa) = pal.patch_a {
        c.fill_triangle((72.0, 92.0), (64.0, 34.0), (104.0, 66.0), pa);
    }
    if let Some(pb) = pal.patch_b {
        c.fill_triangle((184.0, 92.0), (192.0, 34.0), (152.0, 66.0), pb);
    }
    // Inner pink ears
    c.fill_triangle((77.0, 84.0), (72.0, 46.0), (100.0, 68.0), pal.ear_inner);
    c.fill_triangle((179.0, 84.0), (184.0, 46.0), (156.0, 68.0), pal.ear_inner);

    // Head
    c.fill_ellipse(128.0, 102.0, 56.0, 48.0, pal.coat_shade);
    c.fill_ellipse(128.0, 100.0, 55.0, 46.0, pal.coat);

    if let Some(pa) = pal.patch_a {
        c.fill_ellipse(92.0, 82.0, 24.0, 22.0, pa);
    }
    if let Some(pb) = pal.patch_b {
        c.fill_ellipse(166.0, 84.0, 22.0, 20.0, pb);
    }

    // Forehead tabby M-mark
    if let Some(st) = pal.stripe {
        c.draw_capsule(116.0, 64.0, 116.0, 76.0, 3.0, st);
        c.draw_capsule(128.0, 61.0, 128.0, 75.0, 3.2, st);
        c.draw_capsule(140.0, 64.0, 140.0, 76.0, 3.0, st);
        c.draw_capsule(79.0, 96.0, 94.0, 98.0, 2.8, st);
        c.draw_capsule(162.0, 98.0, 177.0, 96.0, 2.8, st);
    }

    // Muzzle / cheeks
    c.fill_ellipse(117.0, 118.0, 22.0, 18.0, pal.muzzle);
    c.fill_ellipse(139.0, 118.0, 22.0, 18.0, pal.muzzle);
    c.fill_ellipse(128.0, 125.0, 18.0, 13.0, pal.muzzle);

    // Rosy blush
    c.fill_ellipse(89.0, 115.0, 10.0, 6.0, [255, 138, 168, 125]);
    c.fill_ellipse(167.0, 115.0, 10.0, 6.0, [255, 138, 168, 125]);

    // Expressive eyes
    for ex in [103.0_f32, 153.0_f32] {
        c.fill_ellipse(ex, 97.0, 11.5, 13.5, [22, 20, 28, 255]);
        c.fill_ellipse(ex, 97.0, 10.0, 12.0, pal.eye);
        c.fill_ellipse(ex, 97.0, 5.2, 8.8, [18, 16, 24, 255]);
        // Catchlights
        c.fill_ellipse(ex - 3.6, 92.0, 3.4, 3.4, [255, 255, 255, 255]);
        c.fill_ellipse(ex + 3.2, 101.0, 1.8, 1.8, [255, 255, 255, 220]);
    }

    // Cute nose & ":3" mouth
    c.fill_triangle((122.0, 111.0), (134.0, 111.0), (128.0, 118.0), pal.nose);
    let mouth_col = [52, 42, 48, 220];
    c.draw_capsule(128.0, 118.0, 128.0, 123.0, 1.6, mouth_col);
    c.draw_capsule(128.0, 123.0, 120.0, 126.0, 1.6, mouth_col);
    c.draw_capsule(128.0, 123.0, 136.0, 126.0, 1.6, mouth_col);

    // Whiskers
    let whisk_col = [245, 245, 250, 175];
    c.draw_capsule(62.0, 108.0, 92.0, 113.0, 1.2, whisk_col);
    c.draw_capsule(58.0, 118.0, 92.0, 118.0, 1.2, whisk_col);
    c.draw_capsule(64.0, 128.0, 93.0, 123.0, 1.2, whisk_col);
    c.draw_capsule(164.0, 113.0, 194.0, 108.0, 1.2, whisk_col);
    c.draw_capsule(164.0, 118.0, 198.0, 118.0, 1.2, whisk_col);
    c.draw_capsule(163.0, 123.0, 192.0, 128.0, 1.2, whisk_col);

    // Optional sparkle for Void cat
    if pal.sparkle {
        c.draw_capsule(202.0, 56.0, 202.0, 74.0, 2.0, [255, 232, 130, 235]);
        c.draw_capsule(193.0, 65.0, 211.0, 65.0, 2.0, [255, 232, 130, 235]);
        c.fill_ellipse(202.0, 65.0, 4.0, 4.0, [255, 250, 210, 255]);
    }

    c.into_bytes()
}

pub fn render_cat_preset(preset: i32) -> Option<Image> {
    static CAT_1: OnceLock<Vec<u8>> = OnceLock::new();
    static CAT_2: OnceLock<Vec<u8>> = OnceLock::new();
    static CAT_3: OnceLock<Vec<u8>> = OnceLock::new();
    static CAT_4: OnceLock<Vec<u8>> = OnceLock::new();

    match preset {
        1 => Some(image_from_rgba(CAT_1.get_or_init(|| {
            draw_detailed_cat(CatPalette {
                coat: [34, 36, 44, 255],
                coat_shade: [24, 25, 32, 255],
                ear_inner: [244, 152, 178, 255],
                belly: [248, 249, 252, 255],
                muzzle: [248, 249, 252, 255],
                eye: [72, 214, 132, 255],
                nose: [248, 132, 162, 255],
                stripe: None,
                patch_a: None,
                patch_b: None,
                sparkle: false,
            })
        }))),
        2 => Some(image_from_rgba(CAT_2.get_or_init(|| {
            draw_detailed_cat(CatPalette {
                coat: [240, 146, 62, 255],
                coat_shade: [212, 114, 38, 255],
                ear_inner: [255, 176, 178, 255],
                belly: [255, 236, 206, 255],
                muzzle: [255, 236, 206, 255],
                eye: [242, 184, 52, 255],
                nose: [244, 118, 132, 255],
                stripe: Some([178, 76, 22, 220]),
                patch_a: None,
                patch_b: None,
                sparkle: false,
            })
        }))),
        3 => Some(image_from_rgba(CAT_3.get_or_init(|| {
            draw_detailed_cat(CatPalette {
                coat: [252, 246, 236, 255],
                coat_shade: [232, 222, 208, 255],
                ear_inner: [248, 164, 182, 255],
                belly: [255, 252, 246, 255],
                muzzle: [255, 252, 246, 255],
                eye: [86, 188, 236, 255],
                nose: [246, 136, 158, 255],
                stripe: None,
                patch_a: Some([238, 136, 56, 255]),
                patch_b: Some([48, 44, 56, 255]),
                sparkle: false,
            })
        }))),
        4 => Some(image_from_rgba(CAT_4.get_or_init(|| {
            draw_detailed_cat(CatPalette {
                coat: [26, 24, 36, 255],
                coat_shade: [16, 14, 24, 255],
                ear_inner: [112, 78, 122, 255],
                belly: [36, 34, 48, 255],
                muzzle: [38, 36, 50, 255],
                eye: [252, 208, 68, 255],
                nose: [92, 74, 96, 255],
                stripe: None,
                patch_a: None,
                patch_b: None,
                sparkle: true,
            })
        }))),
        _ => None,
    }
}
