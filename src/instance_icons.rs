use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

pub const INSTANCE_ICON_COUNT: i32 = 60;

// Widget style controls the icon frame geometry (Material squircle / Fluent acrylic / Compact classic)
const FRAME_MATERIAL: u8 = 0;
const FRAME_FLUENT: u8 = 1;
const FRAME_CLASSIC: u8 = 2;

static CURRENT_FRAME_STYLE: AtomicU8 = AtomicU8::new(FRAME_MATERIAL);

pub fn widget_frame_id_from_str(style: &str) -> u8 {
    let s = style.to_lowercase();
    if s.contains("fluent") {
        FRAME_FLUENT
    } else if s.contains("компакт") || s.contains("compact") || s.contains("класс") || s.contains("classic") {
        FRAME_CLASSIC
    } else {
        FRAME_MATERIAL
    }
}

pub fn set_widget_frame_style(style: &str) -> u8 {
    let id = widget_frame_id_from_str(style);
    CURRENT_FRAME_STYLE.store(id, Ordering::Relaxed);
    id
}

pub fn detect_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        Some("png")
    } else if bytes.len() >= 3 && &bytes[0..3] == b"\xFF\xD8\xFF" {
        Some("jpg")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 2 && &bytes[0..2] == b"BM" {
        Some("bmp")
    } else if bytes.len() >= 4 && (&bytes[0..4] == b"\x00\x00\x01\x00" || &bytes[0..4] == b"\x00\x00\x02\x00") {
        Some("ico")
    } else {
        let trimmed = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        let trimmed = match trimmed.iter().position(|b| !b.is_ascii_whitespace()) {
            Some(idx) => &trimmed[idx..],
            None => return None,
        };
        if trimmed.starts_with(b"<svg") || trimmed.starts_with(b"<?xml") {
            Some("svg")
        } else {
            None
        }
    }
}

pub fn scan_saved_custom_instance_icons(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg") {
                let name = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Своя иконка")
                    .to_string();
                out.push((name, p));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

pub fn load_custom_icon_image(path: &Path) -> Option<Image> {
    if !path.is_file() {
        return None;
    }
    Image::load_from_path(path).ok()
}

pub fn resolve_instance_icon_image(icon_id: i32, custom_icon_path: &str) -> Image {
    let trimmed = custom_icon_path.trim();
    if !trimmed.is_empty() {
        if let Some(img) = load_custom_icon_image(Path::new(trimmed)) {
            return img;
        }
    }
    instance_icon_image(icon_id)
}

struct IconDef {
    name: &'static str,
    bg: [u8; 3],
    art: [&'static str; 16],
}

fn palette_color(ch: char) -> Option<[u8; 4]> {
    match ch {
        '.' | ' ' => None,
        // Metals & Woods & Stones
        'w' => Some([143, 101, 56, 255]),  // Oak wood / stick
        'W' => Some([184, 138, 84, 255]),  // Light wood
        'd' => Some([96, 65, 36, 255]),    // Dark bark / handle
        's' => Some([132, 136, 145, 255]), // Stone / grey
        'S' => Some([182, 188, 198, 255]), // Light iron / silver
        'k' => Some([52, 48, 58, 255]),    // Netherite / dark outline
        'K' => Some([86, 78, 92, 255]),    // Netherite highlight
        // Gems & Ores & Magic
        'c' => Some([74, 237, 217, 255]),  // Diamond cyan
        'C' => Some([168, 255, 245, 255]), // Diamond bright glint
        'g' => Some([246, 196, 48, 255]),  // Gold
        'G' => Some([255, 238, 132, 255]), // Gold bright
        'e' => Some([42, 209, 105, 255]),  // Emerald / green
        'E' => Some([126, 247, 162, 255]), // Light green / slime / grass top
        'r' => Some([228, 48, 48, 255]),   // Redstone / TNT / heart / lava red
        'R' => Some([255, 118, 82, 255]),  // Bright orange-red / fire
        'o' => Some([245, 140, 38, 255]),  // Orange / blaze / lava
        'p' => Some([172, 88, 245, 255]),  // Purple / amethyst / enchanted / portal
        'P' => Some([224, 166, 255, 255]), // Light magenta / dragon / glint
        'b' => Some([56, 128, 242, 255]),  // Water / lapis blue
        'B' => Some([138, 194, 255, 255]), // Ice / sky / soul blue
        't' => Some([26, 118, 108, 255]),  // Ender pearl teal
        'T' => Some([92, 214, 192, 255]),  // Ender highlight
        'u' => Some([245, 247, 252, 255]), // Pure white (paper / snow / iron shine / TNT band)
        'm' => Some([122, 84, 58, 255]),   // Dirt brown
        'x' => Some([22, 18, 30, 255]),    // Obsidian / enderman black
        _ => None,
    }
}

// Helper generators for parametric weapon/tool/block templates so all 60 icons have rich 16x16 pixel art!
const SWORD_DIAMOND: [&str; 16] = [
    ".............cC.",
    "............cCc.",
    "...........cCc..",
    "..........cCc...",
    ".........cCc....",
    "........cCc.....",
    ".......cCc......",
    "...kk.cCc.......",
    "....kcCc........",
    "....cCk.........",
    "...wkk..........",
    "..wW..k.........",
    ".dw.............",
    "dd..............",
    "................",
    "................",
];

const SWORD_NETHERITE: [&str; 16] = [
    ".............kK.",
    "............kKk.",
    "...........kKk..",
    "..........kKk...",
    ".........kKk....",
    "........kKk.....",
    ".......kKk......",
    "...gg.kKk.......",
    "....gkKk........",
    "....kKg.........",
    "...wgg..........",
    "..wW..g.........",
    ".dw.............",
    "dd..............",
    "................",
    "................",
];

const SWORD_GOLD: [&str; 16] = [
    ".............gG.",
    "............gGg.",
    "...........gGg..",
    "..........gGg...",
    ".........gGg....",
    "........gGg.....",
    ".......gGg......",
    "...dd.gGg.......",
    "....dgGg........",
    "....gGd.........",
    "...wdd..........",
    "..wW..d.........",
    ".dw.............",
    "dd..............",
    "................",
    "................",
];

const SWORD_IRON: [&str; 16] = [
    ".............sS.",
    "............sSs.",
    "...........sSs..",
    "..........sSs...",
    ".........sSs....",
    "........sSs.....",
    ".......sSs......",
    "...dd.sSs.......",
    "....dsSs........",
    "....sSd.........",
    "...wdd..........",
    "..wW..d.........",
    ".dw.............",
    "dd..............",
    "................",
    "................",
];

const SWORD_BLAZE: [&str; 16] = [
    "............oRG.",
    "...........oRGo.",
    "..........rRGo..",
    ".........rRGo...",
    "........rRGo....",
    ".......rRGo.....",
    "......rRGo......",
    "...kk.RGo.......",
    "....krRo........",
    "....rRk.........",
    "...okk..........",
    "..gG..k.........",
    ".rg.............",
    "rr..............",
    "................",
    "................",
];

const PICK_DIAMOND: [&str; 16] = [
    "......ccccc.....",
    "....ccCCCCCcc...",
    "...cCCc...cCCc..",
    "..cCc....w..cCc.",
    "..cc....wW...cc.",
    ".......wW...c...",
    "......wW........",
    ".....wW.........",
    "....wW..........",
    "...wW...........",
    "..wW............",
    ".dw.............",
    "dd..............",
    "................",
    "................",
    "................",
];

const PICK_NETHERITE: [&str; 16] = [
    "......kkkkk.....",
    "....kkKKKKKkk...",
    "...kKKk...kKKk..",
    "..kKk....w..kKk.",
    "..kk....wW...kk.",
    ".......wW...k...",
    "......wW........",
    ".....wW.........",
    "....wW..........",
    "...wW...........",
    "..wW............",
    ".dw.............",
    "dd..............",
    "................",
    "................",
    "................",
];

const PICK_IRON: [&str; 16] = [
    "......sssss.....",
    "....ssSSSSSss...",
    "...sSSs...sSSs..",
    "..sSs....w..sSs.",
    "..ss....wW...ss.",
    ".......wW...s...",
    "......wW........",
    ".....wW.........",
    "....wW..........",
    "...wW...........",
    "..wW............",
    ".dw.............",
    "dd..............",
    "................",
    "................",
    "................",
];

const PICK_GOLD: [&str; 16] = [
    "......ggggg.....",
    "....ggGGGGGgg...",
    "...gGGg...gGGg..",
    "..gGg....w..gGg.",
    "..gg....wW...gg.",
    ".......wW...g...",
    "......wW........",
    ".....wW.........",
    "....wW..........",
    "...wW...........",
    "..wW............",
    ".dw.............",
    "dd..............",
    "................",
    "................",
    "................",
];

const AXE_DIAMOND: [&str; 16] = [
    ".......cccc.....",
    "......cCCCCc....",
    ".....cCCCwCC....",
    ".....cCCwWcc....",
    "......ccwW......",
    ".......wW.......",
    "......wW........",
    ".....wW.........",
    "....wW..........",
    "...wW...........",
    "..wW............",
    ".dw.............",
    "dd..............",
    "................",
    "................",
    "................",
];

const AXE_NETHERITE: [&str; 16] = [
    ".......kkkk.....",
    "......kKKKKk....",
    ".....kKKKwKK....",
    ".....kKKwWkk....",
    "......kkwW......",
    ".......wW.......",
    "......wW........",
    ".....wW.........",
    "....wW..........",
    "...wW...........",
    "..wW............",
    ".dw.............",
    "dd..............",
    "................",
    "................",
    "................",
];

const SHOVEL_DIAMOND: [&str; 16] = [
    "..........ccc...",
    ".........cCCCc..",
    ".........cCCCc..",
    ".........cCCc...",
    "........wWc.....",
    ".......wW.......",
    "......wW........",
    ".....wW.........",
    "....wW..........",
    "...wW...........",
    "..wW............",
    ".dw.............",
    "dd..............",
    "................",
    "................",
    "................",
];

const TRIDENT: [&str; 16] = [
    "..........u..u..",
    ".........uT.uT.u",
    "........uT.uT.uT",
    ".........tTTTtT.",
    "..........tTTt..",
    ".........tTtt...",
    "........tT......",
    ".......tT.......",
    "......tT........",
    ".....tT.........",
    "....tT..........",
    "...tT...........",
    "..sS............",
    ".sS.............",
    "................",
    "................",
];

const MACE: [&str; 16] = [
    "........sSSSs...",
    ".......sSCCCSs..",
    ".......sSCCCSs..",
    ".......sSSSSSs..",
    "........sSSSs...",
    ".......kKs......",
    "......kK........",
    ".....kK.........",
    "....kK..........",
    "...kK...........",
    "..gG............",
    ".kK.............",
    "kk..............",
    "................",
    "................",
    "................",
];

const BOW: [&str; 16] = [
    "......wwww......",
    "....ww....u.....",
    "...w.....u......",
    "..w.....u...C...",
    "..w....u...Cc...",
    ".w....u...w.....",
    ".w...u...w......",
    ".w..u...w.......",
    ".w...u.w........",
    "..w...u.........",
    "..w....u........",
    "...w....u.......",
    "....ww...u......",
    "......wwww......",
    "................",
    "................",
];

const CROSSBOW: [&str; 16] = [
    "....wwwwwwww....",
    "...w..sSSs..w...",
    "..w....cC....w..",
    "..u...wWWw...u..",
    "...u..wWWw..u...",
    "....uuwWWwuu....",
    "......wWWw......",
    "......dwwd......",
    ".....dw..wd.....",
    "....dw..........",
    "...dd...........",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const SHIELD: [&str; 16] = [
    "..ssssssssssss..",
    "..sWWWWWWWWWWs..",
    "..sWwwWWWWwwWs..",
    "..sWWWWccWWWWs..",
    "..ssssWCCWssss..",
    "..sWWWWCCWWWWs..",
    "..sWWWWccWWWWs..",
    "...sWWWWWWWWs...",
    "...sWWWWWWWWs...",
    "....sWWWWWWs....",
    "....ssWWWWss....",
    "......sWWs......",
    ".......ss.......",
    "................",
    "................",
    "................",
];

const TOTEM: [&str; 16] = [
    "....gggggggg....",
    "...gGGGGGGGGg...",
    "...gGeesSeeGg...",
    "...gGeesSeeGg...",
    "..ggGGGGGGGGgg..",
    ".gGGGGgGGgGGGGg.",
    ".gGGgGGGGGGgGGg.",
    "..ggGGeGGeGGgg..",
    "....gGGGGGGg....",
    "....gGGGGGGg....",
    ".....gGGGGg.....",
    ".....gGGGGg.....",
    "......gggg......",
    "................",
    "................",
    "................",
];

const GOLDEN_APPLE: [&str; 16] = [
    ".......ee.......",
    "......ee........",
    "....gggwwggg....",
    "...gGGGggGGGg...",
    "..gGGuuGGGGGGg..",
    "..gGuGGGGGGGGg..",
    "..gGGGGGGGGGGg..",
    "..gGGGGGGGGGGg..",
    "..gGGGGGGGGGGg..",
    "...gGGGGGGGGg...",
    "...gGGGGGGGGg...",
    "....ggGGGGgg....",
    "......gggg......",
    "................",
    "................",
    "................",
];

const ENCH_APPLE: [&str; 16] = [
    ".......ee.......",
    "......ee........",
    "....pppwwppp....",
    "...pPPPggPPPp...",
    "..pPPuuGGGGGgp..",
    "..pPuGGGGGGGPp..",
    "..pGGGGGGGGPPp..",
    "..pGPGGGGGGGGp..",
    "..pGGPPGGGGGGp..",
    "...pGGGGGGPPp...",
    "...pGGPPGGGGp...",
    "....ppGGGGpp....",
    "......pppp......",
    "................",
    "................",
    "................",
];

const ENDER_PEARL: [&str; 16] = [
    ".....tttttt.....",
    "...ttTTTTTTtt...",
    "..tTTuuTTTTTTt..",
    ".tTTuTTTTTTTTTt.",
    ".tTTTTTTTTttttT.",
    ".tTTTTTTttttttT.",
    ".tTTTTTtttttttt.",
    ".tTTTTTtttttttt.",
    ".tTTTTTTttttttT.",
    "..tTTTTTTttttT..",
    "...ttTTTTTTtt...",
    ".....tttttt.....",
    "................",
    "................",
    "................",
    "................",
];

const EYE_OF_ENDER: [&str; 16] = [
    "................",
    ".....eeeeee.....",
    "...eeEEEEEEee...",
    "..eEEttttttEEe..",
    ".eEEttgGGgttEEe.",
    ".eEttgGxxGgttEe.",
    ".eEttgGxxGgttEe.",
    ".eEttgGxxGgttEe.",
    ".eEEttgGGgttEEe.",
    "..eEEttttttEEe..",
    "...eeEEEEEEee...",
    ".....eeeeee.....",
    "................",
    "................",
    "................",
    "................",
];

const POTION_RED: [&str; 16] = [
    "......wwww......",
    "......sSSs......",
    ".......ss.......",
    "......suus......",
    "....ss....ss....",
    "...s..rrrr..s...",
    "..s..rRRRRr..s..",
    "..s.rRuuRRRr.s..",
    "..s.rRRRRRRr.s..",
    "..s..rRRRRr..s..",
    "...s..rrrr..s...",
    "....ssssssss....",
    "................",
    "................",
    "................",
    "................",
];

const POTION_BLUE: [&str; 16] = [
    "......wwww......",
    "......sSSs......",
    ".......ss.......",
    "......suus......",
    "....ss....ss....",
    "...s..bbbb..s...",
    "..s..bBBBBb..s..",
    "..s.bBuuBBBb.s..",
    "..s.bBBBBBBb.s..",
    "..s..bBBBBb..s..",
    "...s..bbbb..s...",
    "....ssssssss....",
    "................",
    "................",
    "................",
    "................",
];

const ENCHANTED_BOOK: [&str; 16] = [
    "....dddddddd....",
    "...dwwwwwwwwd...",
    "..dwppppppppwd..",
    "..dwpguuuugpwd..",
    "..dwpgGGGGgpwd..",
    "..dwppppppppwd..",
    "..gggggggggggg..",
    "..dwppppppppwd..",
    "..dwpPPPPPPpwd..",
    "..dwuuuuuuuuwd..",
    "...dddddddddd...",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const GRASS_BLOCK: [&str; 16] = [
    "..eeeeeeeeeeee..",
    ".eEEEEEEEEEEEEe.",
    ".eEEEEEEEEEEEEe.",
    ".eeeeeEEEEeeeee.",
    ".mmeeemmmmeeeem.",
    ".mmmemmmmmmmemm.",
    ".mmmmmmwmmmmmmm.",
    ".mmmwmmmmmmwmmm.",
    ".mmmmmmmmmmmmmm.",
    ".mmwmmmmwmmmmmm.",
    ".mmmmmmmmmmmwmm.",
    ".mmmwmmmmmmmmmm.",
    "..mmmmmmmmmmmm..",
    "................",
    "................",
    "................",
];

const CRAFTING_TABLE: [&str; 16] = [
    "..dddddddddddd..",
    ".dWWWWdWWdWWWWd.",
    ".dWwwWdwwdWwwWd.",
    ".dddddddddddddd.",
    ".dwWWWWddWWWWwd.",
    ".dwWssWddWssWwd.",
    ".dwWssWddWssWwd.",
    ".dwWWWWddWWWWwd.",
    ".dwwwwwwwwwwwwd.",
    ".dwWWWWddWWWWwd.",
    ".dwWWWWddWWWWwd.",
    ".dwwwwwwwwwwwwd.",
    "..dddddddddddd..",
    "................",
    "................",
    "................",
];

const FURNACE: [&str; 16] = [
    "..kkkkkkkkkkkk..",
    ".kssssssssssssk.",
    ".ksSSSSSSSSSSsk.",
    ".ksskkkkkkkkssk.",
    ".ksskxxxxxxkssk.",
    ".ksskxxxxxxkssk.",
    ".ksskkkkkkkkssk.",
    ".kssssssssssssk.",
    ".ksskkkkkkkkssk.",
    ".ksskrRRRRrkssk.",
    ".ksskoGGGGokssk.",
    ".kssssssssssssk.",
    "..kkkkkkkkkkkk..",
    "................",
    "................",
    "................",
];

const CHEST: [&str; 16] = [
    "..dddddddddddd..",
    ".dWWWWWWWWWWWWd.",
    ".dWwwwwwwwwwwWd.",
    ".dWwwwwwwwwwwWd.",
    ".ddddddsSdddddd.",
    ".dwwwwwgGwwwwwd.",
    ".dwwwwwgGwwwwwd.",
    ".dwwwwwwwwwwwwd.",
    ".dwwwwwwwwwwwwd.",
    ".dwwwwwwwwwwwwd.",
    ".dWWWWWWWWWWWWd.",
    "..dddddddddddd..",
    "................",
    "................",
    "................",
    "................",
];

const ENDER_CHEST: [&str; 16] = [
    "..xxxxxxxxxxxx..",
    ".xkkkkkkkkkkkkx.",
    ".xkttttttttttkx.",
    ".xktTTTTTTTTtkx.",
    ".xxxxxxgGxxxxxx.",
    ".xkkkkkgGkkkkkx.",
    ".xkkttteEtttksx.",
    ".xkktTTeeTTtkkx.",
    ".xkkttttttttkkx.",
    ".xkkkkkkkkkkkkx.",
    "..xxxxxxxxxxxx..",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const TNT_BLOCK: [&str; 16] = [
    "..rrrrrrrrrrrr..",
    ".rRRrRRrRRrRRrr.",
    ".rRRrRRrRRrRRrr.",
    ".rRRrRRrRRrRRrr.",
    ".uuuuuuuuuuuuuu.",
    ".ukkkukukkkukku.",
    ".uuk.ukkuk..uku.",
    ".uuk.uk.uk..uku.",
    ".uuuuuuuuuuuuuu.",
    ".rRRrRRrRRrRRrr.",
    ".rRRrRRrRRrRRrr.",
    ".rRRrRRrRRrRRrr.",
    "..rrrrrrrrrrrr..",
    "................",
    "................",
    "................",
];

const DIAMOND_ORE: [&str; 16] = [
    "..ssssssssssss..",
    ".sSSssssssssSSs.",
    ".ssscCCssssssSs.",
    ".ssscCCssssccss.",
    ".ssssssssscCCss.",
    ".ssccssssssccss.",
    ".sscCssssssssss.",
    ".ssssssssccssss.",
    ".ssccsssscCCsss.",
    ".sscCCssssccsss.",
    ".sssccssssssssS.",
    ".sSSssssssssSSs.",
    "..ssssssssssss..",
    "................",
    "................",
    "................",
];

const EMERALD_ORE: [&str; 16] = [
    "..ssssssssssss..",
    ".sSSssssssssSSs.",
    ".ssseEEssssssSs.",
    ".ssseEEsssseess.",
    ".ssssssssseEEss.",
    ".sseesssssseess.",
    ".sseEssssssssss.",
    ".sssssssseessss.",
    ".sseesssseEEsss.",
    ".sseEEsssseesss.",
    ".ssseessssssssS.",
    ".sSSssssssssSSs.",
    "..ssssssssssss..",
    "................",
    "................",
    "................",
];

const GOLD_ORE: [&str; 16] = [
    "..ssssssssssss..",
    ".sSSssssssssSSs.",
    ".sssgGGssssssSs.",
    ".sssgGGssssggss.",
    ".sssssssssgGGss.",
    ".ssggssssssggss.",
    ".ssgGssssssssss.",
    ".ssssssssggssss.",
    ".ssggssssgGGsss.",
    ".ssgGGssssggsss.",
    ".sssggssssssssS.",
    ".sSSssssssssSSs.",
    "..ssssssssssss..",
    "................",
    "................",
    "................",
];

const REDSTONE_DUST: [&str; 16] = [
    ".......rr.......",
    "......rRRr......",
    ".....rRRRRr.....",
    "...rrRRRRRRrr...",
    "..rRRRruurRRRr..",
    "..rRRrRRRRrRRr..",
    "...rrRRRRRRrr...",
    "..rRRRRRRRRRRr..",
    "..rRRRRRRRRRRr..",
    "...rrRRRRRRrr...",
    ".....rrrrrr.....",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const EMERALD_GEM: [&str; 16] = [
    ".....eeeeee.....",
    "....eEEEEEEe....",
    "...eEEuuuuEEe...",
    "...eEuEEEEuEe...",
    "...eEuEEEEuEe...",
    "...eEuEEEEuEe...",
    "...eEuEEEEuEe...",
    "...eEuEEEEuEe...",
    "...eEEuuuuEEe...",
    "....eEEEEEEe....",
    ".....eeeeee.....",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const DIAMOND_GEM: [&str; 16] = [
    ".....cccccc.....",
    "...ccCCCCCCcc...",
    "..cCCuuuuuuCCc..",
    "..cCuCCCCCCuCc..",
    "..cCCCCCCCCCCc..",
    "...cCCCCCCCCc...",
    "....cCCCCCCc....",
    ".....cCCCCc.....",
    "......cCCc......",
    ".......cc.......",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const AMETHYST: [&str; 16] = [
    ".......pp.......",
    "......pPPp......",
    "....ppPPPPpp....",
    "...pPPuPPPPPp...",
    "...pPPuPPpPPp...",
    "..pPPPPPPpPPPp..",
    "..pPPPpPPPPPPp..",
    "...ppPPPPPPpp...",
    ".....ppPPpp.....",
    ".......pp.......",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const NETHERITE_INGOT: [&str; 16] = [
    "................",
    "................",
    "....kkkkkkkk....",
    "..kkKKKKKKKKkk..",
    ".kKKssssssssKKk.",
    ".kKKKKKKKKKKKKk.",
    ".kKkkkkkkkkkkKk.",
    "..kkkkkkkkkkkk..",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const BEACON_BLOCK: [&str; 16] = [
    "..BBBBBBBBBBBB..",
    ".BccccccccccccB.",
    ".BcCCCCCCCCCCcB.",
    ".BcCCuuuuuuCCcB.",
    ".BcCCuCCCCuCCcB.",
    ".BcCCuCCCCuCCcB.",
    ".BcCCuuuuuuCCcB.",
    ".BcCCCCCCCCCCcB.",
    ".BccccccccccccB.",
    ".xxxxxxxxxxxxxx.",
    ".xkkkkkkkkkkkkx.",
    "..xxxxxxxxxxxx..",
    "................",
    "................",
    "................",
    "................",
];

const COMMAND_BLOCK: [&str; 16] = [
    "..oooooooooooo..",
    ".oRRRRooooRRRRo.",
    ".oRsssoooosssRo.",
    ".oRscCoooscCsRo.",
    ".ooooogGGgooooo.",
    ".ooooogpPgooooo.",
    ".ooooogGGgooooo.",
    ".oRscCoooscCsRo.",
    ".oRsssoooosssRo.",
    ".oRRRRooooRRRRo.",
    "..oooooooooooo..",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const OBSIDIAN_BLOCK: [&str; 16] = [
    "..xxxxxxxxxxxx..",
    ".xkkxxpppxxkkkx.",
    ".xkxxxpxxxxxkkx.",
    ".xxxxppxxxxpxxx.",
    ".xxpxxxxxxppxxx.",
    ".xxppxxxxpxxxkx.",
    ".xxxpxxxxxxxkkx.",
    ".xkkxxxxppxxxxx.",
    ".xkxxxxppxxxppx.",
    ".xxxxxxpxxxxpxx.",
    ".xkkkxxxxxxkkkx.",
    "..xxxxxxxxxxxx..",
    "................",
    "................",
    "................",
    "................",
];

const OAK_LOG: [&str; 16] = [
    "..dddddddddddd..",
    ".dWWWWWWWWWWWWd.",
    ".dWwwwwwwwwwwWd.",
    ".dddddddddddddd.",
    ".ddwdwwdwwdwddd.",
    ".ddwdwwdwwdwddd.",
    ".ddwdwwdwwdwddd.",
    ".ddwdwwdwwdwddd.",
    ".ddwdwwdwwdwddd.",
    ".ddwdwwdwwdwddd.",
    ".ddwdwwdwwdwddd.",
    "..dddddddddddd..",
    "................",
    "................",
    "................",
    "................",
];

const COBBLESTONE: [&str; 16] = [
    "..kkkkkkkkkkkk..",
    ".kSSSskkkkSSSSk.",
    ".kSSssksSSkSSsk.",
    ".kkkkkksSSkkkkk.",
    ".ksSSSskkkksSSk.",
    ".kssSSskSSsksSk.",
    ".kkkkkkksSSkkkk.",
    ".kSSSSskkkksSSk.",
    ".kSSssksSSsksSk.",
    ".kkkkkksSSskkkk.",
    ".ksSSSskkkksSSk.",
    "..kkkkkkkkkkkk..",
    "................",
    "................",
    "................",
    "................",
];

const CREEPER_HEAD: [&str; 16] = [
    "..eeeeeeeeeeee..",
    ".eEEeeEEEEeeEEe.",
    ".eExxxEEEEexxxe.",
    ".eExxxEEEEexxxe.",
    ".eExxxEEEEexxxe.",
    ".eEEEEexxeEEEEe.",
    ".eEEEexxxxeEEEe.",
    ".eEEEexxxxeEEEe.",
    ".eEEEexeeexEEEe.",
    ".eEEEexeeexEEEe.",
    ".eEEEEEEEEEEEEe.",
    "..eeeeeeeeeeee..",
    "................",
    "................",
    "................",
    "................",
];

const ENDERMAN_HEAD: [&str; 16] = [
    "..xxxxxxxxxxxx..",
    ".xkkkkkkkkkkkkx.",
    ".xkkkkkkkkkkkkx.",
    ".xxxxxxxxxxxxxx.",
    ".xxxxxxxxxxxxxx.",
    ".xPPpxxxxxxpPPx.",
    ".xPPpxxxxxxpPPx.",
    ".xxxxxxxxxxxxxx.",
    ".xxxxkkkkkkxxxx.",
    ".xxxxxxxxxxxxxx.",
    ".xxxxxxxxxxxxxx.",
    "..xxxxxxxxxxxx..",
    "................",
    "................",
    "................",
    "................",
];

const SLIME_BLOCK: [&str; 16] = [
    "..EEEEEEEEEEEE..",
    ".EeeeeeeeeeeeeE.",
    ".EeeEEEEEEEEeeE.",
    ".EeeExxEExxeEeE.",
    ".EeeExxEExxeEeE.",
    ".EeeEEEEEEEEeeE.",
    ".EeeEEEexxEEeeE.",
    ".EeeEEEexxEEeeE.",
    ".EeeEEEEEEEEeeE.",
    ".EeeeeeeeeeeeeE.",
    "..EEEEEEEEEEEE..",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const BLAZE_HEAD: [&str; 16] = [
    "..gggggggggggg..",
    ".gGGGGGGGGGGGGg.",
    ".goooooooooooog.",
    ".goxxgoooogxxog.",
    ".goxxuoooouxxog.",
    ".goooooooooooog.",
    ".gGGooooooooGGg.",
    "..o..gGGGGg..o..",
    "..o....gg....o..",
    ".....o....o.....",
    ".....o....o.....",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const NETHER_STAR: [&str; 16] = [
    ".......uu.......",
    "......uCCu......",
    "......uCCu......",
    ".....uCuuCu.....",
    ".uuuuCuuuuCuuuu.",
    "uCCCCuuuuuuCCCCu",
    ".uCCuuuuuuuuCCu.",
    "....uCuuuuCu....",
    ".....uCCuCCu....",
    "......uCCu......",
    "......uCCu......",
    ".......uu.......",
    "................",
    "................",
    "................",
    "................",
];

const DRAGON_EGG: [&str; 16] = [
    "......xxxx......",
    "....xxpxxpxx....",
    "...xxxxPPxxxx...",
    "..xxpxxxxxxpxx..",
    "..xxxxpPPxxxxx..",
    ".xxxpxxxxxxpxxx.",
    ".xxxxxxPPxxxxxx.",
    ".xxppxxxxxxxxpx.",
    "..xxxxxxpPPxxx..",
    "..xxxpxxxxxxxx..",
    "...xxxxxxxxxx...",
    "....xxxxxxxx....",
    "................",
    "................",
    "................",
    "................",
];

const ELYTRA: [&str; 16] = [
    "....sss..sss....",
    "...sSSs..sSSs...",
    "..sSSSs..sSSSs..",
    "..sSSssggsSSSs..",
    ".sSSSs.gg.sSSSs.",
    ".sSSSs....sSSSs.",
    ".sSSss....ssSSs.",
    ".sSSs......sSSs.",
    "..sSs......sSs..",
    "..ss........ss..",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const FIREWORK: [&str; 16] = [
    ".......rr.......",
    "......rRRr......",
    ".....rRRRRr.....",
    "....rrrrrrrr....",
    ".....uuuuuu.....",
    ".....urrruu.....",
    ".....uuuuuu.....",
    ".....urrruu.....",
    ".....uuuuuu.....",
    ".......ww.......",
    ".......ww.......",
    "......oGo.......",
    "................",
    "................",
    "................",
    "................",
];

const LAVA_BUCKET: [&str; 16] = [
    "....oooooooo....",
    "...orRRRRRRro...",
    "..ssGGGGGGGGss..",
    "..sSrrrrrrrrSs..",
    "..sSSSSSSSSSSs..",
    "...sSSSSSSSSs...",
    "...sSSSSSSSSs...",
    "....sSSSSSSs....",
    "....sSSSSSSs....",
    ".....ssssss.....",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const WATER_BUCKET: [&str; 16] = [
    "....bbbbbbbb....",
    "...bBBBBBBBBb...",
    "..ssbbbbbbbbss..",
    "..sSBBBBBBBBSs..",
    "..sSSSSSSSSSSs..",
    "...sSSSSSSSSs...",
    "...sSSSSSSSSs...",
    "....sSSSSSSs....",
    "....sSSSSSSs....",
    ".....ssssss.....",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const COMPASS: [&str; 16] = [
    "....ssssssss....",
    "..ssSSSSSSSSss..",
    ".sSSxxxrrxxxSSs.",
    ".sSxxxxrrxxxxSs.",
    ".sSxxxxrrxxxxSs.",
    ".sSxxxxuuxxxxSs.",
    ".sSxxxxssxxxxSs.",
    ".sSxxxxssxxxxSs.",
    ".sSSxxxxxxxxSSs.",
    "..ssSSSSSSSSss..",
    "....ssssssss....",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const HARDCORE_HEART: [&str; 16] = [
    "..rrrr....rrrr..",
    ".rRRRRr..rRRRRr.",
    "rRuRRRRrrRRRRRrr",
    "rRxRRRRRRRRRRxr.",
    "rRxxRRRRRRRRxxr.",
    ".rRxxRRRRRRxxr..",
    "..rRxxxxxxxxr...",
    "...rRRRRRRRRr...",
    "....rRRRRRRr....",
    ".....rRRRRr.....",
    "......rRRr......",
    ".......rr.......",
    "................",
    "................",
    "................",
    "................",
];

const CROWN: [&str; 16] = [
    "................",
    ".g.....gg.....g.",
    ".gG...gGGg...Gg.",
    ".gGg..gGGg..gGg.",
    ".gGGggGGGGggGGg.",
    ".gGGGGGGGGGGGGg.",
    ".gGrrGGccGGeeGg.",
    ".gGGGGGGGGGGGGg.",
    "..gggggggggggg..",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const SOUL_LANTERN: [&str; 16] = [
    ".......ss.......",
    "......s..s......",
    ".....kkkkkk.....",
    "....kkkkkkkk....",
    "....kBBBBBBk....",
    "....kBccccBk....",
    "....kBcCCCck....",
    "....kBBBBBBk....",
    "....kkkkkkkk....",
    ".....kkkkkk.....",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const ENCHANTING_TABLE: [&str; 16] = [
    ".....uuuuuu.....",
    "...uuwwwwwwuu...",
    "....cccccccc....",
    "..crrxxxxxxrrc..",
    ".crRRxxxxxxRRrc.",
    ".cxxxxxxxxxxxxc.",
    ".cxkxxkxxkxxkxc.",
    ".cxxxxxxxxxxxxc.",
    ".xxxxxxxxxxxxxx.",
    "..xxxxxxxxxxxx..",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const MUSIC_DISC: [&str; 16] = [
    "................",
    "....xxxxxxxx....",
    "..xxkkkkkkkkxx..",
    ".xkkxxxxxxxxkkx.",
    ".xkxxpppppppxkx.",
    ".xkxpPPxxPPpxkx.",
    ".xkxpPPxxPPpxkx.",
    ".xkxxpppppppxkx.",
    ".xkkxxxxxxxxkkx.",
    "..xxkkkkkkkkxx..",
    "....xxxxxxxx....",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const ICON_DEFS: [IconDef; 60] = [
    IconDef { name: "Блок травы", bg: [34, 62, 42], art: GRASS_BLOCK },
    IconDef { name: "Алмазный меч", bg: [24, 58, 72], art: SWORD_DIAMOND },
    IconDef { name: "Незеритовый меч", bg: [46, 38, 54], art: SWORD_NETHERITE },
    IconDef { name: "Золотой меч", bg: [68, 54, 24], art: SWORD_GOLD },
    IconDef { name: "Железный меч", bg: [46, 52, 64], art: SWORD_IRON },
    IconDef { name: "Огненный меч", bg: [72, 34, 26], art: SWORD_BLAZE },
    IconDef { name: "Алмазная кирка", bg: [24, 62, 74], art: PICK_DIAMOND },
    IconDef { name: "Незеритовая кирка", bg: [44, 38, 52], art: PICK_NETHERITE },
    IconDef { name: "Железная кирка", bg: [48, 54, 66], art: PICK_IRON },
    IconDef { name: "Золотая кирка", bg: [68, 56, 24], art: PICK_GOLD },
    IconDef { name: "Алмазный топор", bg: [26, 60, 76], art: AXE_DIAMOND },
    IconDef { name: "Незеритовый топор", bg: [48, 38, 56], art: AXE_NETHERITE },
    IconDef { name: "Алмазная лопата", bg: [26, 58, 70], art: SHOVEL_DIAMOND },
    IconDef { name: "Трезубец", bg: [22, 64, 66], art: TRIDENT },
    IconDef { name: "Булава", bg: [42, 48, 68], art: MACE },
    IconDef { name: "Лук и стрела", bg: [56, 44, 30], art: BOW },
    IconDef { name: "Арбалет", bg: [54, 42, 32], art: CROSSBOW },
    IconDef { name: "Щит", bg: [52, 46, 36], art: SHIELD },
    IconDef { name: "Тотем бессмертия", bg: [64, 56, 26], art: TOTEM },
    IconDef { name: "Золотое яблоко", bg: [68, 52, 22], art: GOLDEN_APPLE },
    IconDef { name: "Зачарованное яблоко", bg: [58, 34, 72], art: ENCH_APPLE },
    IconDef { name: "Эндер-жемчуг", bg: [20, 56, 56], art: ENDER_PEARL },
    IconDef { name: "Око Эндера", bg: [26, 62, 48], art: EYE_OF_ENDER },
    IconDef { name: "Зелье силы", bg: [68, 28, 40], art: POTION_RED },
    IconDef { name: "Зелье скорости", bg: [26, 48, 76], art: POTION_BLUE },
    IconDef { name: "Чародейская книга", bg: [56, 34, 70], art: ENCHANTED_BOOK },
    IconDef { name: "Верстак", bg: [62, 44, 28], art: CRAFTING_TABLE },
    IconDef { name: "Горящая печь", bg: [54, 42, 40], art: FURNACE },
    IconDef { name: "Сундук", bg: [64, 46, 26], art: CHEST },
    IconDef { name: "Эндер-сундук", bg: [24, 44, 48], art: ENDER_CHEST },
    IconDef { name: "Динамит (TNT)", bg: [74, 28, 28], art: TNT_BLOCK },
    IconDef { name: "Алмазная руда", bg: [32, 56, 66], art: DIAMOND_ORE },
    IconDef { name: "Изумрудная руда", bg: [28, 58, 42], art: EMERALD_ORE },
    IconDef { name: "Золотая руда", bg: [60, 52, 32], art: GOLD_ORE },
    IconDef { name: "Редстоун", bg: [72, 24, 28], art: REDSTONE_DUST },
    IconDef { name: "Изумруд", bg: [22, 64, 40], art: EMERALD_GEM },
    IconDef { name: "Алмаз", bg: [22, 62, 74], art: DIAMOND_GEM },
    IconDef { name: "Аметист", bg: [56, 32, 74], art: AMETHYST },
    IconDef { name: "Незеритовый слиток", bg: [42, 38, 46], art: NETHERITE_INGOT },
    IconDef { name: "Маяк", bg: [26, 62, 76], art: BEACON_BLOCK },
    IconDef { name: "Командный блок", bg: [68, 44, 28], art: COMMAND_BLOCK },
    IconDef { name: "Обсидиан", bg: [34, 24, 52], art: OBSIDIAN_BLOCK },
    IconDef { name: "Дубовое бревно", bg: [56, 42, 26], art: OAK_LOG },
    IconDef { name: "Булыжник", bg: [46, 48, 54], art: COBBLESTONE },
    IconDef { name: "Крипер", bg: [28, 66, 34], art: CREEPER_HEAD },
    IconDef { name: "Эндермен", bg: [32, 22, 46], art: ENDERMAN_HEAD },
    IconDef { name: "Слайм", bg: [32, 68, 38], art: SLIME_BLOCK },
    IconDef { name: "Ифрит", bg: [72, 44, 20], art: BLAZE_HEAD },
    IconDef { name: "Звезда Незера", bg: [42, 52, 76], art: NETHER_STAR },
    IconDef { name: "Яйцо дракона", bg: [40, 24, 56], art: DRAGON_EGG },
    IconDef { name: "Элитры", bg: [44, 48, 62], art: ELYTRA },
    IconDef { name: "Фейерверк", bg: [66, 30, 38], art: FIREWORK },
    IconDef { name: "Ведро лавы", bg: [74, 38, 22], art: LAVA_BUCKET },
    IconDef { name: "Ведро воды", bg: [24, 48, 78], art: WATER_BUCKET },
    IconDef { name: "Компас", bg: [44, 48, 58], art: COMPASS },
    IconDef { name: "Хардкор-сердце", bg: [74, 22, 30], art: HARDCORE_HEART },
    IconDef { name: "Золотая корона", bg: [68, 54, 22], art: CROWN },
    IconDef { name: "Фонарь душ", bg: [24, 52, 68], art: SOUL_LANTERN },
    IconDef { name: "Стол зачарований", bg: [52, 28, 64], art: ENCHANTING_TABLE },
    IconDef { name: "Пластинка", bg: [38, 30, 56], art: MUSIC_DISC },
];

// 3 widget-style frame geometries cached lazily
static CACHED_FRAMES: [OnceLock<Vec<Vec<u8>>>; 3] = [
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
];

fn build_all_rgba_for_frame(frame_id: u8) -> Vec<Vec<u8>> {
    let out_sz: usize = 64;
    let corner_r: f32 = match frame_id {
        FRAME_FLUENT => 0.14,
        FRAME_CLASSIC => 0.04,
        _ => 0.24,
    };
    ICON_DEFS
        .iter()
        .map(|def| {
            let mut buf = vec![0u8; out_sz * out_sz * 4];
            let base_bg_r = def.bg[0];
            let base_bg_g = def.bg[1];
            let base_bg_b = def.bg[2];
            for y in 0..out_sz {
                for x in 0..out_sz {
                    let u = (x as f32 + 0.5) / out_sz as f32;
                    let v = (y as f32 + 0.5) / out_sz as f32;
                    let qx = (u - 0.5).abs() - (0.48 - corner_r);
                    let qy = (v - 0.5).abs() - (0.48 - corner_r);
                    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt()
                        + qx.max(qy).min(0.0)
                        - corner_r;
                    if d > 0.015 {
                        continue;
                    }
                    let alpha = ((0.015 - d) / 0.03).clamp(0.0, 1.0);

                    let (mut r, mut g, mut b) = match frame_id {
                        FRAME_FLUENT => {
                            let sheen = if v < 0.45 { 1.28 - 0.35 * v } else { 0.92 };
                            let mut rr = (base_bg_r as f32 * sheen + 16.0).clamp(0.0, 255.0);
                            let mut gg = (base_bg_g as f32 * sheen + 18.0).clamp(0.0, 255.0);
                            let mut bb = (base_bg_b as f32 * sheen + 24.0).clamp(0.0, 255.0);
                            if d > -0.045 {
                                rr = (rr + 68.0).min(255.0);
                                gg = (gg + 74.0).min(255.0);
                                bb = (bb + 86.0).min(255.0);
                            }
                            (rr as u8, gg as u8, bb as u8)
                        }
                        FRAME_CLASSIC => {
                            let is_top_left = x < 4 || y < 4;
                            let is_bot_right = x >= out_sz - 4 || y >= out_sz - 4;
                            if is_bot_right {
                                (225, 225, 225)
                            } else if is_top_left {
                                (55, 55, 55)
                            } else {
                                (
                                    ((base_bg_r as u16 + 115) / 2) as u8,
                                    ((base_bg_g as u16 + 115) / 2) as u8,
                                    ((base_bg_b as u16 + 115) / 2) as u8,
                                )
                            }
                        }
                        _ => {
                            let shade = 1.12 - 0.24 * v;
                            (
                                (base_bg_r as f32 * shade).clamp(0.0, 255.0) as u8,
                                (base_bg_g as f32 * shade).clamp(0.0, 255.0) as u8,
                                (base_bg_b as f32 * shade).clamp(0.0, 255.0) as u8,
                            )
                        }
                    };

                    if (8..56).contains(&x) && (8..56).contains(&y) {
                        let gx = (x - 8) / 3;
                        let gy = (y - 8) / 3;
                        if gy < def.art.len() {
                            if let Some(ch) = def.art[gy].as_bytes().get(gx).map(|&b| b as char) {
                                if let Some([pr, pg, pb, _]) = palette_color(ch) {
                                    r = pr;
                                    g = pg;
                                    b = pb;
                                }
                            }
                        }
                    }

                    let idx = (y * out_sz + x) * 4;
                    buf[idx] = r;
                    buf[idx + 1] = g;
                    buf[idx + 2] = b;
                    buf[idx + 3] = (alpha * 255.0).round() as u8;
                }
            }
            buf
        })
        .collect()
}

pub fn normalize_icon_id(id: i32) -> i32 {
    id.rem_euclid(INSTANCE_ICON_COUNT)
}

pub fn instance_icon_name(id: i32) -> &'static str {
    ICON_DEFS[normalize_icon_id(id) as usize].name
}

pub fn instance_icon_image(id: i32) -> Image {
    let norm = normalize_icon_id(id) as usize;
    let frame_id = CURRENT_FRAME_STYLE.load(Ordering::Relaxed).min(2);
    let all = CACHED_FRAMES[frame_id as usize]
        .get_or_init(|| build_all_rgba_for_frame(frame_id));
    let bytes = &all[norm];
    let mut pb = SharedPixelBuffer::<Rgba8Pixel>::new(64, 64);
    pb.make_mut_bytes().copy_from_slice(bytes);
    Image::from_rgba8(pb)
}
