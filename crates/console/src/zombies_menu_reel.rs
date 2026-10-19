//! The menu's map flyby: a recording of Kino made in this engine (slow dolly shots through
//! the foyer, the auditorium and the stage), packed as 854x480 JPEG frames at 15 a second,
//! played behind the menu and on Kino's card. One frame at a time is decoded off the main
//! thread into one texture, so it costs a frame's memory, not the clip's.
//!
//! The pack: `IW4LREEL`, then frame count, frames a second and a spare `u32`, then each
//! frame's offset and length (`u32` pairs), then the JPEGs.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use frame::AppScreen;

const KINO_REEL: &[u8] = include_bytes!("../assets/menu/kino_reel.bin");

struct Reel {
    data: &'static [u8],
    count: usize,
    fps: f32,
}

impl Reel {
    fn parse(data: &'static [u8]) -> Option<Self> {
        let word = |at: usize| -> Option<u32> {
            Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
        };
        if data.get(..8)? != b"IW4LREEL" {
            return None;
        }
        let count = word(8)? as usize;
        let fps = word(12)? as f32;
        (count > 0 && fps > 0.0 && data.len() >= 20 + count * 8).then_some(Self { data, count, fps })
    }

    fn frame(&self, index: usize) -> Option<&'static [u8]> {
        let at = 20 + index * 8;
        let word = |at: usize| u32::from_le_bytes(self.data[at..at + 4].try_into().unwrap()) as usize;
        let (offset, len) = (word(at), word(at + 4));
        self.data.get(offset..offset + len)
    }
}

fn decode(jpeg: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(jpeg, options);
    let pixels = decoder.decode().ok()?;
    let (width, height) = decoder.dimensions()?;
    Some((width as u32, height as u32, pixels))
}

/// The flyby's texture and the frame being decoded.
#[derive(Resource, Default)]
pub(crate) struct MenuReel {
    pub(crate) handle: Option<Handle<Image>>,
    shown: Option<usize>,
    task: Option<Task<Option<(usize, u32, u32, Vec<u8>)>>>,
    started_at: Option<f64>,
}

/// On the main menu, the frame for now (looping) is decoded and put in the texture; off
/// the menu it stops and lets the texture go.
pub(crate) fn play_menu_reel(
    time: Res<Time<Real>>,
    screen: Res<AppScreen>,
    mut reel_state: ResMut<MenuReel>,
    mut images: ResMut<Assets<Image>>,
) {
    if *screen != AppScreen::MainMenu {
        if reel_state.handle.is_some() {
            *reel_state = MenuReel::default();
        }
        return;
    }
    let Some(reel) = Reel::parse(KINO_REEL) else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let started = *reel_state.started_at.get_or_insert(now);
    if let Some(task) = reel_state.task.as_mut()
        && let Some(done) = future::block_on(future::poll_once(task))
    {
        reel_state.task = None;
        if let Some((index, width, height, rgba)) = done {
            let fits = reel_state.handle.as_ref().is_some_and(|handle| {
                images
                    .get(handle)
                    .is_some_and(|image| image.width() == width && image.height() == height)
            });
            if fits {
                let handle = reel_state.handle.clone().expect("checked above");
                if let Some(mut image) = images.get_mut(&handle) {
                    image.data = Some(rgba);
                }
            } else {
                let mut image = Image::new(
                    bevy::render::render_resource::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    rgba,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::default(),
                );
                image.sampler = bevy::image::ImageSampler::linear();
                reel_state.handle = Some(images.add(image));
                diag::info!(Ui, "zombies menu: flyby {} frames at {} fps", reel.count, reel.fps);
            }
            reel_state.shown = Some(index);
        }
    }
    let want = ((now - started) * f64::from(reel.fps)) as usize % reel.count;
    if reel_state.task.is_none() && reel_state.shown != Some(want) {
        let Some(jpeg) = reel.frame(want) else {
            return;
        };
        reel_state.task = Some(AsyncComputeTaskPool::get().spawn(async move {
            decode(jpeg).map(|(w, h, rgba)| (want, w, h, rgba))
        }));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_kino_reel_unpacks() {
        let reel = super::Reel::parse(super::KINO_REEL).expect("pack");
        assert!(reel.count > 100);
        let first = super::decode(reel.frame(0).expect("frame")).expect("jpeg");
        assert_eq!((first.0, first.1), (854, 480));
        let last = super::decode(reel.frame(reel.count - 1).expect("frame")).expect("jpeg");
        assert_eq!(last.2.len(), 854 * 480 * 4);
    }
}
