//! Pictures for the touch buttons, cut from Black Ops' own art in the install: the round
//! Perk-a-Cola seals off the perk bottles' labels (and Mule Kick's off its machine), the
//! chalk drawings the wall buys are marked with, the power-up icons and the HUD's own
//! pictures. Each comes out as a square RGBA picture; nothing of it ships with the app.

use std::path::Path;

/// How a picture is cut from its image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconCut {
    /// A round seal: the ellipse at (`x`, `y`) with radii `rx`, `ry` in the image's pixels
    /// (a bottle's label is stretched round the bottle), drawn round. It is the button's
    /// whole face.
    Seal { x: f32, y: f32, rx: f32, ry: f32 },
    /// A chalk drawing (white strokes on black): its brightness becomes the picture, in
    /// chalk white, fitted to the picture.
    Chalk,
    /// The image as it is, fitted to the picture.
    Plain,
    /// A small icon blown up: fitted, its edges sharpened again.
    Small,
    /// Turned a quarter left (a round lying pointing right stands up), then fitted.
    Upright,
}

/// One picture the touch buttons can wear.
#[derive(Clone, Copy, Debug)]
pub struct TouchIconSpec {
    pub key: &'static str,
    /// As the options show it.
    pub label: &'static str,
    /// BO1's image names, tried in turn (patches carry some under another name).
    pub images: &'static [&'static str],
    pub cut: IconCut,
}

impl TouchIconSpec {
    /// Whether the picture is the button's whole face (a seal) rather than an icon drawn
    /// on the button's badge.
    pub fn is_face(&self) -> bool {
        matches!(self.cut, IconCut::Seal { .. })
    }
}

/// The label seal on every perk bottle (512 square), to the outside of its white rim: the
/// same place on each.
const BOTTLE_SEAL: IconCut = IconCut::Seal {
    x: 204.5,
    y: 361.0,
    rx: 85.0,
    ry: 94.0,
};

const fn bottle(key: &'static str, label: &'static str, images: &'static [&'static str]) -> TouchIconSpec {
    TouchIconSpec {
        key,
        label,
        images,
        cut: BOTTLE_SEAL,
    }
}

const fn icon(
    key: &'static str,
    label: &'static str,
    images: &'static [&'static str],
    cut: IconCut,
) -> TouchIconSpec {
    TouchIconSpec {
        key,
        label,
        images,
        cut,
    }
}

/// Every picture there is, in the order the options go through them.
pub const TOUCH_ICON_LIBRARY: &[TouchIconSpec] = &[
    bottle("doubletap", "DOUBLE TAP", &["~-gzombie_perkbottle_dbltap_c"]),
    bottle("deadshot", "DEADSHOT", &["~-gzombie_perkbottle_ads_c"]),
    bottle("speedcola", "SPEED COLA", &["~-gzombie_perkbottle_sleight_c"]),
    bottle("juggernog", "JUGGERNOG", &["~-gzombie_perkbottle_jugg_c"]),
    bottle("quickrevive", "QUICK REVIVE", &["~-gzombie_perkbottle_revive_c"]),
    bottle("staminup", "STAMIN-UP", &["~-gzombie_perkbottle_marathon_c"]),
    bottle("phdflopper", "PHD FLOPPER", &["~-gzombie_perkbottle_nuke_c"]),
    icon(
        "mulekick",
        "MULE KICK",
        &["~-gzombie_vending_three_gun_logo_c", "zombie_vending_three_gun_logo_c"],
        IconCut::Seal {
            x: 153.0,
            y: 155.0,
            rx: 95.0,
            ry: 95.0,
        },
    ),
    icon("chalk_knife", "CHALK KNIFE", &["chalk_weapon_bowie_knife"], IconCut::Chalk),
    icon("chalk_mp40", "CHALK MP40", &["chalk_weapon_mp40_c"], IconCut::Chalk),
    icon("chalk_m14", "CHALK M14", &["chalk_weapon_m14_c"], IconCut::Chalk),
    icon("chalk_shotgun", "CHALK SHOTGUN", &["chalk_weapon_ithaca_c"], IconCut::Chalk),
    icon("chalk_grenades", "CHALK GRENADES", &["grenade_bag_chalk_c"], IconCut::Chalk),
    icon("chalk_betty", "CHALK BETTY", &["bouncing_betty_chalk_c"], IconCut::Chalk),
    icon("instakill", "INSTA-KILL", &["specialty_instakill_zombies"], IconCut::Small),
    icon("doublepoints", "DOUBLE POINTS", &["specialty_2x_zombies"], IconCut::Small),
    icon("firesale", "FIRE SALE", &["specialty_firesale_zombies"], IconCut::Small),
    icon("nuke", "NUKE", &["hud_zombie_bomb"], IconCut::Plain),
    icon("skull", "SKULL MEDAL", &["zom_medals_skull_face"], IconCut::Plain),
    icon("reel", "FILM REEL", &["zom_icon_theater_reel"], IconCut::Plain),
    icon("hud_knife", "HUD KNIFE", &["hud_knife"], IconCut::Plain),
    icon("hud_grenade", "HUD GRENADE", &["hud_us_grenade"], IconCut::Plain),
    icon("hud_round", "HUD ROUND", &["ammo_counter_riflebullet"], IconCut::Upright),
];

pub fn touch_icon(key: &str) -> Option<&'static TouchIconSpec> {
    TOUCH_ICON_LIBRARY.iter().find(|spec| spec.key == key)
}

/// The pictures' size (square).
pub const TOUCH_ICON_SIZE: u32 = 256;

/// Every library picture the install has, as `(key, (size, size, rgba))`.
pub fn load_touch_icons(main: &Path) -> Vec<(&'static str, (u32, u32, Vec<u8>))> {
    TOUCH_ICON_LIBRARY
        .iter()
        .filter_map(|spec| {
            let image = spec.images.iter().find_map(|name| {
                crate::t5_fonts::decode_main_image(main, name)
            })?;
            Some((spec.key, cut_icon(image, spec.cut)))
        })
        .collect()
}

type Rgba = (u32, u32, Vec<u8>);

/// Cuts `image` into a square picture as `cut` says.
pub fn cut_icon(image: Rgba, cut: IconCut) -> Rgba {
    let size = TOUCH_ICON_SIZE;
    match cut {
        IconCut::Seal { x, y, rx, ry } => seal(&image, x, y, rx, ry, size),
        IconCut::Chalk => thicken(fit(&chalk(image), size, 0.9), 2),
        IconCut::Plain => fit(&image, size, 0.86),
        IconCut::Small => sharpen_alpha(fit(&image, size, 0.86)),
        IconCut::Upright => fit(&turn_left(image), size, 0.86),
    }
}

fn sample(image: &Rgba, x: f32, y: f32) -> [f32; 4] {
    let (w, h, rgba) = image;
    let x = x.clamp(0.0, *w as f32 - 1.0);
    let y = y.clamp(0.0, *h as f32 - 1.0);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |px: u32, py: u32| {
        let i = ((py * w + px) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]].map(f32::from)
    };
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    std::array::from_fn(|k| {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bottom = c[k] + (d[k] - c[k]) * fx;
        top + (bottom - top) * fy
    })
}

/// The ellipse at (`cx`, `cy`) radii `rx`, `ry`, filling the picture round, its edge
/// smoothed.
fn seal(image: &Rgba, cx: f32, cy: f32, rx: f32, ry: f32, size: u32) -> Rgba {
    let half = size as f32 * 0.5;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for py in 0..size {
        for px in 0..size {
            let (u, v) = ((px as f32 + 0.5 - half) / half, (py as f32 + 0.5 - half) / half);
            let d = (u * u + v * v).sqrt();
            // A pixel and a half of soft edge.
            let edge = ((1.0 - d) * half / 1.5).clamp(0.0, 1.0);
            if edge <= 0.0 {
                continue;
            }
            let c = sample(image, cx + u * rx, cy + v * ry);
            let i = ((py * size + px) * 4) as usize;
            out[i] = c[0] as u8;
            out[i + 1] = c[1] as u8;
            out[i + 2] = c[2] as u8;
            out[i + 3] = (255.0 * edge) as u8;
        }
    }
    (size, size, out)
}

/// Chalk strokes: white where the drawing is bright, clear where the board is.
fn chalk((w, h, rgba): Rgba) -> Rgba {
    let out = rgba
        .chunks_exact(4)
        .flat_map(|p| {
            let luma = (0.3 * f32::from(p[0]) + 0.59 * f32::from(p[1]) + 0.11 * f32::from(p[2]))
                / 255.0;
            let alpha = ((luma - 0.18) / 0.45).clamp(0.0, 1.0) * f32::from(p[3]) / 255.0;
            [240, 238, 228, (alpha * 255.0) as u8]
        })
        .collect();
    (w, h, out)
}

/// The visible part of `image` (alpha over a tenth), scaled to fill `share` of a square
/// picture, centred.
fn fit(image: &Rgba, size: u32, share: f32) -> Rgba {
    let (w, h, rgba) = image;
    let (mut x0, mut y0, mut x1, mut y1) = (*w, *h, 0u32, 0u32);
    for y in 0..*h {
        for x in 0..*w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 25 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        (x0, y0, x1, y1) = (0, 0, *w, *h);
    }
    let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
    let scale = (size as f32 * share) / bw.max(bh);
    let (ox, oy) = (
        (size as f32 - bw * scale) * 0.5,
        (size as f32 - bh * scale) * 0.5,
    );
    let mut out = vec![0u8; (size * size * 4) as usize];
    for py in 0..size {
        for px in 0..size {
            let sx = (px as f32 + 0.5 - ox) / scale + x0 as f32 - 0.5;
            let sy = (py as f32 + 0.5 - oy) / scale + y0 as f32 - 0.5;
            if sx < x0 as f32 - 1.0 || sy < y0 as f32 - 1.0 || sx > x1 as f32 || sy > y1 as f32 {
                continue;
            }
            let c = sample(image, sx, sy);
            let i = ((py * size + px) * 4) as usize;
            for k in 0..4 {
                out[i + k] = c[k] as u8;
            }
        }
    }
    (size, size, out)
}

/// Strokes made `by` pixels wider: a wall-sized chalk line is a hair at button size.
fn thicken((w, h, rgba): Rgba, by: i32) -> Rgba {
    let mut out = rgba.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut best = 0u8;
            for dy in -by..=by {
                for dx in -by..=by {
                    if dx * dx + dy * dy > by * by {
                        continue;
                    }
                    let (sx, sy) = (x + dx, y + dy);
                    if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 {
                        continue;
                    }
                    best = best.max(rgba[((sy as u32 * w + sx as u32) * 4 + 3) as usize]);
                }
            }
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            out[i..i + 3].copy_from_slice(&[240, 238, 228]);
            out[i + 3] = best;
        }
    }
    (w, h, out)
}

/// A blown-up small icon's soft edges made firm again.
fn sharpen_alpha((w, h, mut rgba): Rgba) -> Rgba {
    for p in rgba.chunks_exact_mut(4) {
        let a = f32::from(p[3]) / 255.0;
        let t = ((a - 0.3) / 0.4).clamp(0.0, 1.0);
        p[3] = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
    }
    (w, h, rgba)
}

/// A picture turned a quarter left (a round pointing right then points up).
pub fn turn_left((width, height, rgba): Rgba) -> Rgba {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; rgba.len()];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = (y, w - 1 - x);
            let from = (y * w + x) * 4;
            let to = (ny * h + nx) * 4;
            out[to..to + 4].copy_from_slice(&rgba[from..from + 4]);
        }
    }
    (height, width, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique() {
        for (i, a) in TOUCH_ICON_LIBRARY.iter().enumerate() {
            assert!(
                TOUCH_ICON_LIBRARY[i + 1..].iter().all(|b| b.key != a.key),
                "{}",
                a.key
            );
        }
    }

    #[test]
    fn a_seal_is_round_and_fills_the_picture() {
        let image = (64, 64, vec![200u8; 64 * 64 * 4]);
        let (w, h, rgba) = cut_icon(
            image,
            IconCut::Seal {
                x: 32.0,
                y: 32.0,
                rx: 30.0,
                ry: 30.0,
            },
        );
        assert_eq!((w, h), (TOUCH_ICON_SIZE, TOUCH_ICON_SIZE));
        let alpha = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(w / 2, h / 2), 255);
        assert!(alpha(w / 2, 2) > 200);
    }

    #[test]
    fn chalk_keeps_the_strokes_only() {
        let mut rgba = vec![0u8; 8 * 8 * 4];
        for p in rgba.chunks_exact_mut(4) {
            p[3] = 255;
        }
        for x in 2..6 {
            let i = (4 * 8 + x) * 4;
            rgba[i..i + 3].copy_from_slice(&[255, 255, 255]);
        }
        let (w, _, out) = cut_icon((8, 8, rgba), IconCut::Chalk);
        let centre = ((w / 2 * w + w / 2) * 4 + 3) as usize;
        assert!(out[centre] > 200);
        assert_eq!(out[3], 0);
    }

    /// IW4L_ICON_DUMP=<dir>: every library picture from the install IW4L_GAMES names.
    #[test]
    #[ignore]
    fn dump_library() {
        let games = asset_transport::games_root_from_env().expect("IW4L_GAMES");
        let out = std::env::var("IW4L_ICON_DUMP").expect("IW4L_ICON_DUMP");
        let anchor = asset_transport::find_zone_file(&games, "t5:code_post_gfx")
            .expect("zone")
            .path;
        let main = asset_transport::game_root_for_zone(&anchor).expect("root").join("main");
        let icons = load_touch_icons(&main);
        for (key, (w, h, rgba)) in &icons {
            let file = std::fs::File::create(format!("{out}/{key}.png")).expect("out");
            let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), *w, *h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(rgba).unwrap();
        }
        eprintln!("{} of {} pictures", icons.len(), TOUCH_ICON_LIBRARY.len());
    }
}
