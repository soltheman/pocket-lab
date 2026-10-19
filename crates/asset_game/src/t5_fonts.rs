//! Black Ops' own fonts (`fonts/normalFont`, `fonts/bigFont` ...), read from the user's
//! install for a HUD that draws its text in them: the glyph tables from the language's
//! `<lang>_code_post_gfx` zone and their shared atlas image (`gamefonts_pc`) from the
//! IWDs.

use std::collections::BTreeMap;
use std::path::Path;

use asset_transport::{GamesRoot, T5ZoneMemory, find_zone_file, open_zone};

/// One glyph: where it sits against the pen (x0, y0 from the baseline), how far the pen
/// moves on (dx), its size in the font's pixels, and its box in the atlas (s0..t1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct T5Glyph {
    pub letter: u16,
    pub x0: i8,
    pub y0: i8,
    pub dx: u8,
    pub width: u8,
    pub height: u8,
    pub s0: f32,
    pub t0: f32,
    pub s1: f32,
    pub t1: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct T5Font {
    /// Lowercase, as `fonts/normalfont`.
    pub name: String,
    pub pixel_height: i32,
    pub glyphs: Vec<T5Glyph>,
}

impl T5Font {
    /// The glyph for `c`, else the font's fallback ('.'), as the engine draws a letter it
    /// lacks.
    pub fn glyph(&self, c: char) -> Option<&T5Glyph> {
        let code = u32::from(c);
        self.glyphs
            .iter()
            .find(|glyph| u32::from(glyph.letter) == code)
            .or_else(|| self.glyphs.iter().find(|glyph| glyph.letter == u16::from(b'.')))
    }

    /// Its width in the font's pixels.
    pub fn width(&self, text: &str) -> f32 {
        text.chars()
            .filter_map(|c| self.glyph(c))
            .map(|glyph| f32::from(glyph.dx))
            .sum()
    }
}

/// The game fonts and the atlas they share (RGBA).
#[derive(Clone, Debug, Default)]
pub struct T5FontSet {
    pub fonts: BTreeMap<String, T5Font>,
    pub atlas: Option<(u32, u32, Vec<u8>)>,
    /// BO1's hurt overlay (`overlay_low_health`), drawn by the same HUD.
    pub hurt_overlay: Option<(u32, u32, Vec<u8>)>,
    /// Pictures for the touch buttons cut from BO1's art, by library key
    /// (`t5_touch_icons::TOUCH_ICON_LIBRARY`).
    pub button_icons: Vec<(&'static str, (u32, u32, Vec<u8>))>,
}

/// One BO1 UI image from the install's IWDs, if it has it.
pub(crate) fn decode_main_image(main: &Path, name: &str) -> Option<(u32, u32, Vec<u8>)> {
    asset_material::decode_ui_image_from_main(main, name).ok().flatten()
}

#[derive(Default)]
struct FontSink {
    fonts: Vec<T5Font>,
}

impl fastfile_t5::AssetLinkSink for FontSink {
    fn loaded(
        &mut self,
        _s: &fastfile_t5::ZoneStream<'_>,
        _ty: fastfile_t5::AssetType,
        _slot: fastfile_t5::Ptr,
        _insert_slot: Option<fastfile_t5::Ptr>,
    ) -> fastfile_t5::Result<()> {
        Ok(())
    }

    fn alias(
        &mut self,
        _ty: fastfile_t5::AssetType,
        _slot: fastfile_t5::Ptr,
        _target: fastfile_t5::Ptr,
    ) -> fastfile_t5::Result<()> {
        Ok(())
    }

    fn capture_font(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        header: fastfile_t5::Ptr,
        glyphs: Option<fastfile_t5::Ptr>,
        glyph_count: usize,
    ) -> fastfile_t5::Result<()> {
        let name = match s.ptr_at(header, 0) {
            Ok(fastfile_t5::ZonePtr::Offset(name)) => s
                .cstr(s.resolve_alias(name))
                .map(|name| name.to_ascii_lowercase())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let pixel_height = s.i32_at(header, 4).unwrap_or(0);
        let mut rows = Vec::with_capacity(glyph_count);
        if let Some(glyphs) = glyphs
            && let Ok(bytes) = s.slice_at(glyphs, 0, glyph_count * 24)
        {
            for row in bytes.chunks_exact(24) {
                let f = |at: usize| f32::from_le_bytes([row[at], row[at + 1], row[at + 2], row[at + 3]]);
                rows.push(T5Glyph {
                    letter: u16::from_le_bytes([row[0], row[1]]),
                    x0: row[2] as i8,
                    y0: row[3] as i8,
                    dx: row[4],
                    width: row[5],
                    height: row[6],
                    s0: f(8),
                    t0: f(12),
                    s1: f(16),
                    t1: f(20),
                });
            }
        }
        if !name.is_empty() && pixel_height > 0 && !rows.is_empty() {
            self.fonts.push(T5Font {
                name,
                pixel_height,
                glyphs: rows,
            });
        }
        Ok(())
    }
}

impl fastfile_t5::AssetSink for FontSink {
    fn set_script_strings(&mut self, _strings: fastfile_t5::ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut fastfile_t5::ZoneStream<'_>,
        _index: usize,
        ty: fastfile_t5::AssetType,
        slot: fastfile_t5::Ptr,
    ) -> fastfile_t5::Result<()> {
        fastfile_t5::load_asset_at_observed(s, ty, slot, self).map(|_| ())
    }
}

/// The fonts a T5 zone holds (a language's `code_post_gfx` has the game fonts).
pub fn load_t5_zone_fonts(path: &Path) -> Result<Vec<T5Font>, String> {
    let image = open_zone(path).map_err(|e| e.to_string())?;
    let header = image.t5_header().map_err(|e| e.to_string())?;
    let mut memory = T5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = FontSink::default();
    if let Err(error) = fastfile_t5::load_zone(&mut stream, &mut sink) {
        // The walk may stop past the fonts; what it found is still good.
        diag::info!(Zone, "t5 fonts: {} stopped: {error}", path.display());
    }
    Ok(sink.fonts)
}

/// Black Ops' game fonts from the install under `games`: the English (else first) language
/// zone's glyph tables and the `gamefonts_pc` atlas from the IWDs.
pub fn load_t5_fonts(games: &GamesRoot) -> Result<T5FontSet, String> {
    let anchor = find_zone_file(games, "t5:code_post_gfx")
        .map_err(|e| format!("no BO1 code_post_gfx: {e}"))?
        .path;
    let zone = asset_transport::discover::find_t5_localized_zone(&anchor, None, "code_post_gfx")?
        .ok_or("no BO1 language code_post_gfx zone")?;
    let fonts = load_t5_zone_fonts(&zone.path)?;
    let main = asset_transport::game_root_for_zone(&anchor)?.join("main");
    let atlas = asset_material::decode_ui_image_from_main(&main, "gamefonts_pc")?;
    let hurt_overlay = asset_material::decode_ui_image_from_main(&main, "overlay_low_health")
        .ok()
        .flatten();
    let button_icons = crate::t5_touch_icons::load_touch_icons(&main);
    Ok(T5FontSet {
        fonts: fonts
            .into_iter()
            .map(|font| (font.name.clone(), font))
            .collect(),
        atlas,
        hurt_overlay,
        button_icons,
    })
}

#[cfg(test)]
mod tests {
    /// Reads the fonts from the install IW4L_GAMES names (skipped without one).
    #[test]
    #[ignore]
    fn reads_install_fonts() {
        let games = asset_transport::games_root_from_env().expect("IW4L_GAMES");
        let set = super::load_t5_fonts(&games).expect("fonts");
        for (name, font) in &set.fonts {
            let a = font.glyph('A').copied().unwrap_or_default();
            eprintln!(
                "{name}: px {} glyphs {} A {:?} width(\"1234\") {}",
                font.pixel_height,
                font.glyphs.len(),
                a,
                font.width("1234")
            );
        }
        for (name, font) in &set.fonts {
            let ascii = font.glyphs.iter().filter(|g| (0x21..0x7f).contains(&g.letter));
            let top = ascii.clone().map(|g| i32::from(g.y0)).min().unwrap_or(0);
            let bottom = ascii.map(|g| i32::from(g.y0) + i32::from(g.height)).max().unwrap_or(0);
            let at = |c: char| font.glyph(c).map(|g| (g.y0, g.height, g.x0, g.dx, g.width));
            eprintln!("{name}: I {:?} l {:?} i {:?}", font.glyph('I'), font.glyph('l'), font.glyph('i'));
            eprintln!("{name}: top {top} bottom {bottom} H {:?} g {:?} 0 {:?} space {:?} 1 {:?}", at('H'), at('g'), at('0'), at(' '), at('1'));
        }
        let (w, h, rgba) = set.atlas.clone().expect("atlas");
        if let Some(out) = std::env::var_os("IW4L_FONT_DUMP") {
            let file = std::fs::File::create(out).expect("dump");
            let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let flat: Vec<u8> = rgba
                .chunks_exact(4)
                .flat_map(|p| {
                    let a = u16::from(p[3]);
                    [(u16::from(p[0]) * a / 255) as u8, (u16::from(p[1]) * a / 255) as u8, (u16::from(p[2]) * a / 255) as u8, 255]
                })
                .collect();
            encoder.write_header().unwrap().write_image_data(&flat).unwrap();
        }
        eprintln!("atlas {w}x{h}");
        assert!(set.fonts.contains_key("fonts/bigfont"));
    }
}

#[cfg(test)]
mod image_dump {
    /// IW4L_IMAGE_DUMP=<image> IW4L_IMAGE_OUT=<png>: one BO1 UI image as decoded.
    #[test]
    #[ignore]
    fn dump_install_image() {
        let games = asset_transport::games_root_from_env().expect("IW4L_GAMES");
        let name = std::env::var("IW4L_IMAGE_DUMP").expect("IW4L_IMAGE_DUMP");
        let out = std::env::var("IW4L_IMAGE_OUT").expect("IW4L_IMAGE_OUT");
        let anchor = asset_transport::find_zone_file(&games, "t5:code_post_gfx")
            .expect("zone")
            .path;
        let main = asset_transport::game_root_for_zone(&anchor).expect("root").join("main");
        let (w, h, rgba) = asset_material::decode_ui_image_from_main(&main, &name)
            .expect("decode")
            .expect("found");
        eprintln!("{name}: {w}x{h}");
        let alpha: Vec<u8> = rgba.chunks_exact(4).map(|p| p[3]).collect();
        eprintln!("alpha min {:?} max {:?}", alpha.iter().min(), alpha.iter().max());
        let file = std::fs::File::create(out).expect("out");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
}
