//! Text in Black Ops' own fonts for the zombies HUD, menus and touch labels: the glyph
//! tables and atlas read from the install (`asset_game::t5_fonts`), each line laid out as
//! glyph quads the way the engine draws them (pen + x0, line bottom + y0, advance dx).
//! Until the fonts are in, or with no BO1 install, a line falls back to the UI font.

use std::collections::HashMap;
use std::path::PathBuf;

use asset_game::t5_fonts::{T5Font, T5FontSet};
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use ui::GameUiFont;

/// Which of the game's fonts a line is set in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameFont {
    /// `fonts/bigFont`: the points, the clip, titles.
    Big,
    /// `fonts/smallFont`: the reserve, the weapon's name, small labels.
    Small,
    /// `fonts/normalFont`: hints and menu entries.
    Normal,
    /// `fonts/extraBigFont`.
    ExtraBig,
}

impl GameFont {
    fn asset_name(self) -> &'static str {
        match self {
            GameFont::Big => "fonts/bigfont",
            GameFont::Small => "fonts/smallfont",
            GameFont::Normal => "fonts/normalfont",
            GameFont::ExtraBig => "fonts/extrabigfont",
        }
    }
}

/// A line of text in a game font. `size` is the size the UI font would have had: the font
/// is drawn at `size / pixelHeight`, which gives capitals of about the same height. The
/// node is as tall as the capitals (top of `H` to the baseline); descenders hang below.
#[derive(Component, Clone, Debug, PartialEq)]
#[require(Node)]
pub struct GameText {
    pub text: String,
    pub font: GameFont,
    pub size: f32,
    pub color: Color,
    /// BO1's drop shadow (its shadowed text style): a black copy down and right.
    pub shadow: bool,
}

impl GameText {
    pub fn new(font: GameFont, size: f32, color: Color, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font,
            size,
            color,
            shadow: true,
        }
    }
}

/// Sets a line's text, size or colour only when it differs, so unchanged lines are not
/// laid out again.
pub trait GameTextMut {
    fn set_text(&mut self, text: &str);
    fn set_size(&mut self, size: f32);
    fn set_color(&mut self, color: Color);
}

impl GameTextMut for Mut<'_, GameText> {
    fn set_text(&mut self, text: &str) {
        if self.text != text {
            self.text = text.to_owned();
        }
    }

    fn set_size(&mut self, size: f32) {
        if self.size != size {
            self.size = size;
        }
    }

    fn set_color(&mut self, color: Color) {
        if self.color != color {
            self.color = color;
        }
    }
}

struct LoadedFont {
    font: T5Font,
    /// The capitals' top and the baseline, in the font's pixels from the line bottom.
    cap_top: f32,
    baseline: f32,
}

/// The game fonts and their atlas, once read.
#[derive(Resource)]
pub struct GameFonts {
    fonts: HashMap<GameFont, LoadedFont>,
    atlas: Handle<Image>,
    atlas_size: Vec2,
    /// BO1's hurt overlay, red blood at the screen's edges.
    pub hurt_overlay: Option<Handle<Image>>,
    /// BO1's HUD pictures for touch buttons, by button key.
    pub button_icons: HashMap<&'static str, Handle<Image>>,
}

#[derive(Resource)]
pub(crate) struct GameFontTask(Task<Result<T5FontSet, String>>);

/// Reads the fonts off the main thread once the games root is known.
pub(crate) fn load_game_fonts(
    mut commands: Commands,
    identity: Option<Res<frame::LaunchIdentity>>,
    task: Option<ResMut<GameFontTask>>,
    loaded: Option<Res<GameFonts>>,
    mut images: ResMut<Assets<Image>>,
    mut tried: Local<Option<PathBuf>>,
) {
    if loaded.is_some() {
        return;
    }
    if let Some(mut task) = task {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            return;
        };
        commands.remove_resource::<GameFontTask>();
        match result.and_then(|set| fonts_resource(set, &mut images)) {
            Ok(fonts) => {
                diag::info!(Ui, "game text: BO1 fonts in ({} fonts)", fonts.fonts.len());
                commands.insert_resource(fonts);
            }
            Err(error) => diag::info!(Ui, "game text: no BO1 fonts, using the UI font: {error}"),
        }
        return;
    }
    let Some(root) = identity
        .as_deref()
        .map(|identity| identity.games_root.clone())
        .filter(|root| !root.as_os_str().is_empty())
    else {
        return;
    };
    if tried.as_ref() == Some(&root) {
        return;
    }
    *tried = Some(root.clone());
    let task = AsyncComputeTaskPool::get().spawn(async move {
        asset_game::t5_fonts::load_t5_fonts(&asset_transport::GamesRoot(root))
    });
    commands.insert_resource(GameFontTask(task));
}

fn fonts_resource(set: T5FontSet, images: &mut Assets<Image>) -> Result<GameFonts, String> {
    let (width, height, rgba) = set.atlas.ok_or("no gamefonts_pc atlas")?;
    let hurt_overlay = set
        .hurt_overlay
        .map(|(w, h, rgba)| images.add(hurt_overlay_image(w, h, rgba)));
    let button_icons = set
        .button_icons
        .into_iter()
        .map(|(button, (w, h, rgba))| (button, images.add(hurt_overlay_image(w, h, rgba))))
        .collect();
    let mut fonts = HashMap::new();
    for kind in [
        GameFont::Big,
        GameFont::Small,
        GameFont::Normal,
        GameFont::ExtraBig,
    ] {
        let Some(font) = set.fonts.get(kind.asset_name()) else {
            continue;
        };
        let (cap_top, baseline) = font
            .glyph('H')
            .map(|h| (f32::from(h.y0), f32::from(h.y0) + f32::from(h.height)))
            .unwrap_or((-(font.pixel_height as f32) * 0.8, 0.0));
        fonts.insert(
            kind,
            LoadedFont {
                font: font.clone(),
                cap_top,
                baseline,
            },
        );
    }
    if !fonts.contains_key(&GameFont::Normal) {
        return Err("no fonts/normalFont".into());
    }
    Ok(GameFonts {
        fonts,
        atlas: images.add(atlas_image(width, height, rgba)),
        atlas_size: Vec2::new(width as f32, height as f32),
        hurt_overlay,
        button_icons,
    })
}

/// BO1's `overlay_low_health`: red, with the blood at the edges in its alpha.
fn hurt_overlay_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

/// The atlas with its mips, so text drawn smaller than the font stays smooth.
fn atlas_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    let mut data = rgba;
    let (mut level, mut w, mut h) = (data.clone(), width as usize, height as usize);
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let at = |sx: usize, sy: usize| {
                        u32::from(level[((sy.min(h - 1)) * w + sx.min(w - 1)) * 4 + c])
                    };
                    let sum = at(2 * x, 2 * y)
                        + at(2 * x + 1, 2 * y)
                        + at(2 * x, 2 * y + 1)
                        + at(2 * x + 1, 2 * y + 1);
                    next[(y * nw + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        level = next;
        (w, h) = (nw, nh);
        levels += 1;
    }
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    image
}

/// What a line was last built from, and what it was built of.
#[derive(Component)]
pub(crate) struct GameTextBuilt {
    text: String,
    font: GameFont,
    size: f32,
    shadow: bool,
    bitmap: bool,
    color: Color,
    shadows: Vec<Entity>,
    glyphs: Vec<Entity>,
}

/// A glyph quad (or the fallback line) under a [`GameText`].
#[derive(Component)]
pub(crate) struct GameTextPart;

const SHADOW_ALPHA: f32 = 0.85;

fn shadow_color(color: Color) -> Color {
    Color::srgba(0.0, 0.0, 0.0, SHADOW_ALPHA * color.alpha())
}

/// Lays each changed line out again; a colour change only recolours its quads.
#[allow(clippy::type_complexity)]
pub(crate) fn build_game_text(
    mut commands: Commands,
    fonts: Option<Res<GameFonts>>,
    fallback: Option<Res<GameUiFont>>,
    mut lines: Query<(Entity, &GameText, Option<&mut GameTextBuilt>, &mut Node)>,
    mut quads: Query<&mut ImageNode, With<GameTextPart>>,
    mut fallback_lines: Query<(&mut TextColor, &mut TextShadow), With<GameTextPart>>,
) {
    let bitmap = fonts.is_some();
    for (entity, line, built, mut node) in &mut lines {
        if let Some(mut built) = built {
            let same_layout = built.bitmap == bitmap
                && built.font == line.font
                && built.size == line.size
                && built.shadow == line.shadow
                && built.text == line.text;
            if same_layout {
                if built.color != line.color {
                    built.color = line.color;
                    recolour(&built, line, &mut quads, &mut fallback_lines);
                }
                continue;
            }
            let built = &mut *built;
            for part in built.shadows.drain(..).chain(built.glyphs.drain(..)) {
                commands.entity(part).try_despawn();
            }
        }
        let mut built = GameTextBuilt {
            text: line.text.clone(),
            font: line.font,
            size: line.size,
            shadow: line.shadow,
            bitmap,
            color: line.color,
            shadows: Vec::new(),
            glyphs: Vec::new(),
        };
        if let Some(fonts) = fonts.as_deref() {
            let (width, height) = lay_out(&mut commands, entity, line, fonts, &mut built);
            node.width = px(width);
            node.height = px(height);
        } else {
            let Some(fallback) = fallback.as_deref() else {
                continue;
            };
            if node.width != Val::Auto || node.height != Val::Auto {
                node.width = Val::Auto;
                node.height = Val::Auto;
            }
            let part = commands
                .spawn((
                    GameTextPart,
                    Text::new(line.text.clone()),
                    TextFont {
                        font: fallback.0.clone().into(),
                        font_size: FontSize::Px(line.size.round().max(1.0)),
                        ..default()
                    },
                    TextColor(line.color),
                    TextShadow {
                        offset: Vec2::splat(if line.shadow { 1.5 } else { 0.0 }),
                        color: shadow_color(line.color),
                    },
                ))
                .id();
            commands.entity(entity).add_child(part);
            built.glyphs.push(part);
        }
        commands.entity(entity).insert(built);
    }
}

fn recolour(
    built: &GameTextBuilt,
    line: &GameText,
    quads: &mut Query<&mut ImageNode, With<GameTextPart>>,
    fallback_lines: &mut Query<(&mut TextColor, &mut TextShadow), With<GameTextPart>>,
) {
    for &part in &built.shadows {
        if let Ok(mut quad) = quads.get_mut(part) {
            quad.color = shadow_color(line.color);
        }
    }
    for &part in &built.glyphs {
        if let Ok(mut quad) = quads.get_mut(part) {
            quad.color = line.color;
        } else if let Ok((mut color, mut shadow)) = fallback_lines.get_mut(part) {
            color.0 = line.color;
            shadow.color = shadow_color(line.color);
        }
    }
}

/// Spawns the line's glyph quads (shadows first, so they draw under) and returns its size.
fn lay_out(
    commands: &mut Commands,
    entity: Entity,
    line: &GameText,
    fonts: &GameFonts,
    built: &mut GameTextBuilt,
) -> (f32, f32) {
    let Some(loaded) = fonts
        .fonts
        .get(&line.font)
        .or_else(|| fonts.fonts.get(&GameFont::Normal))
    else {
        return (0.0, 0.0);
    };
    let font = &loaded.font;
    let k = line.size / font.pixel_height.max(1) as f32;
    let height = (loaded.baseline - loaded.cap_top) * k;
    let shadow = (line.size * 0.06).clamp(1.0, 3.0);
    let mut pen = 0.0f32;
    let mut right = 0.0f32;
    let mut quads = Vec::new();
    for c in line.text.chars() {
        let Some(glyph) = font.glyph(c) else {
            continue;
        };
        if glyph.width > 0 && glyph.height > 0 {
            let left = pen + f32::from(glyph.x0) * k;
            let top = (f32::from(glyph.y0) - loaded.cap_top) * k;
            let size = Vec2::new(f32::from(glyph.width), f32::from(glyph.height)) * k;
            let rect = Rect::new(
                glyph.s0 * fonts.atlas_size.x,
                glyph.t0 * fonts.atlas_size.y,
                glyph.s1 * fonts.atlas_size.x,
                glyph.t1 * fonts.atlas_size.y,
            );
            quads.push((left, top, size, rect));
            right = right.max(left + size.x);
        }
        pen += f32::from(glyph.dx) * k;
    }
    let mut spawn = |offset: f32, color: Color, into: &mut Vec<Entity>| {
        for &(left, top, size, rect) in &quads {
            let part = commands
                .spawn((
                    GameTextPart,
                    ImageNode::new(fonts.atlas.clone())
                        .with_rect(rect)
                        .with_color(color),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(left + offset),
                        top: px(top + offset),
                        width: px(size.x),
                        height: px(size.y),
                        ..default()
                    },
                    bevy::ui::FocusPolicy::Pass,
                ))
                .id();
            commands.entity(entity).add_child(part);
            into.push(part);
        }
    };
    if line.shadow {
        spawn(shadow, shadow_color(line.color), &mut built.shadows);
    }
    spawn(0.0, line.color, &mut built.glyphs);
    (pen.max(right), height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_mips_cover_every_level() {
        let image = atlas_image(8, 4, vec![255; 8 * 4 * 4]);
        assert_eq!(image.texture_descriptor.mip_level_count, 4);
        // 8x4 + 4x2 + 2x1 + 1x1 texels.
        assert_eq!(image.data.as_ref().map(Vec::len), Some((32 + 8 + 2 + 1) * 4));
    }
}
