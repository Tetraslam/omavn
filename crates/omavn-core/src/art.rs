//! Library art: VNDB covers and screenshots, composed into 16:9 cards that
//! launchers (like Omarchy's image picker) can show in a grid.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::imageops::{self, FilterType};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

use crate::download;
use crate::library::Game;
use crate::paths::Paths;
use crate::report::Reporter;
use crate::vndb::Meta;

pub const CARD_W: u32 = 1536;
pub const CARD_H: u32 = 864;

pub fn cover(paths: &Paths, game: &Game) -> PathBuf {
    game.dir(paths).join("cover.jpg")
}

pub fn screenshot(paths: &Paths, game: &Game) -> PathBuf {
    game.dir(paths).join("screenshot.jpg")
}

pub fn card(paths: &Paths, game: &Game) -> PathBuf {
    game.dir(paths).join("card.jpg")
}

/// Download whatever art `meta` has into the game's directory.
pub fn fetch(paths: &Paths, game: &Game, meta: &Meta, reporter: &dyn Reporter) -> Result<()> {
    let wanted = [
        (&meta.cover_url, cover(paths, game)),
        (&meta.screenshot_url, screenshot(paths, game)),
    ];
    for (url, dest) in wanted {
        if let Some(url) = url {
            download::fetch(url, &dest, reporter)?;
        } else {
            let _ = fs::remove_file(dest);
        }
    }
    Ok(())
}

/// Compose the game's card from its art and link it into the shared
/// library folder as `<title>.jpg`.
pub fn render(paths: &Paths, game: &Game) -> Result<PathBuf> {
    let cover = open(&cover(paths, game));
    let shot = open(&screenshot(paths, game));
    let card = compose(cover.as_ref(), shot.as_ref());
    let out = card_path_tmp(paths, game);
    DynamicImage::ImageRgba8(card)
        .into_rgb8()
        .save(&out)
        .context("writing card")?;
    let dest = self::card(paths, game);
    fs::rename(&out, &dest)?;
    link(paths, game, &dest)?;
    Ok(dest)
}

fn card_path_tmp(paths: &Paths, game: &Game) -> PathBuf {
    game.dir(paths).join(".card.tmp.jpg")
}

fn open(path: &Path) -> Option<DynamicImage> {
    image::open(path).ok()
}

fn compose(cover: Option<&DynamicImage>, shot: Option<&DynamicImage>) -> RgbaImage {
    let mut card = match (shot, cover) {
        (Some(shot), _) => fill(shot, CARD_W, CARD_H),
        (None, Some(cover)) => backdrop(cover),
        (None, None) => RgbaImage::from_pixel(CARD_W, CARD_H, Rgba([24, 24, 32, 255])),
    };
    if let Some(cover) = cover {
        // Darken the left edge so the inset cover reads against any screenshot.
        shade_left(&mut card, CARD_W * 3 / 5);
        let margin = CARD_H / 14;
        let h = CARD_H - 2 * margin;
        let w = (cover.width() as u64 * h as u64 / cover.height().max(1) as u64) as u32;
        let inset = cover.resize_exact(w, h, FilterType::CatmullRom).to_rgba8();
        let x = if shot.is_some() {
            margin
        } else {
            (CARD_W - w) / 2
        };
        drop_shadow(&mut card, x as i64, margin as i64, w, h);
        imageops::overlay(&mut card, &inset, x as i64, margin as i64);
    }
    card
}

/// Scale `img` to cover `w`x`h`, cropping the overflow from the center.
fn fill(img: &DynamicImage, w: u32, h: u32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let scale = (w as f64 / iw as f64).max(h as f64 / ih as f64);
    let (sw, sh) = (
        (iw as f64 * scale).ceil() as u32,
        (ih as f64 * scale).ceil() as u32,
    );
    let scaled = img.resize_exact(sw, sh, FilterType::CatmullRom);
    scaled.crop_imm((sw - w) / 2, (sh - h) / 2, w, h).to_rgba8()
}

/// Blurred, dimmed version of the cover filling the card.
fn backdrop(cover: &DynamicImage) -> RgbaImage {
    let small = fill(cover, CARD_W / 8, CARD_H / 8);
    let blurred = imageops::fast_blur(&small, 6.0);
    let mut big = imageops::resize(&blurred, CARD_W, CARD_H, FilterType::Triangle);
    for p in big.pixels_mut() {
        for c in 0..3 {
            p[c] = (p[c] as u16 * 55 / 100) as u8;
        }
    }
    big
}

fn shade_left(card: &mut RgbaImage, width: u32) {
    for (x, _, p) in card.enumerate_pixels_mut() {
        if x < width {
            let keep = 45 + 55 * x / width;
            for c in 0..3 {
                p[c] = (p[c] as u32 * keep / 100) as u8;
            }
        }
    }
}

fn drop_shadow(card: &mut RgbaImage, x: i64, y: i64, w: u32, h: u32) {
    let spread = 18i64;
    for (px, py, p) in card.enumerate_pixels_mut() {
        let (px, py) = (px as i64, py as i64);
        let dx = (x - px).max(px - (x + w as i64)).max(0);
        let dy = (y - py).max(py - (y + h as i64)).max(0);
        let d = dx.max(dy);
        if d > 0 && d < spread {
            let keep = 40 + 60 * d / spread;
            for c in 0..3 {
                p[c] = (p[c] as i64 * keep / 100) as u8;
            }
        }
    }
}

/// The shared folder of cards, one `<title>.jpg` symlink per game.
pub fn library_dir(paths: &Paths) -> PathBuf {
    paths.data.join("library")
}

fn link(paths: &Paths, game: &Game, card: &Path) -> Result<()> {
    let dir = library_dir(paths);
    fs::create_dir_all(&dir)?;
    unlink(paths, game);
    let title = game.title.replace('/', "∕");
    // Two installs of the same title (say, two editions) both want the name.
    let mut dest = dir.join(format!("{title}.jpg"));
    if fs::symlink_metadata(&dest).is_ok() {
        dest = dir.join(format!("{title} ({}).jpg", game.id));
    }
    std::os::unix::fs::symlink(card, dest)?;
    Ok(())
}

/// Remove the game's entry from the library folder.
pub fn unlink(paths: &Paths, game: &Game) {
    let card = card(paths, game);
    let Ok(rd) = fs::read_dir(library_dir(paths)) else {
        return;
    };
    for e in rd.flatten() {
        if fs::read_link(e.path()).is_ok_and(|t| t == card) {
            let _ = fs::remove_file(e.path());
        }
    }
}

/// Which game a library entry (file name or path) belongs to.
pub fn game_for_entry(paths: &Paths, entry: &str) -> Option<String> {
    let name = Path::new(entry.trim())
        .file_name()?
        .to_string_lossy()
        .into_owned();
    let name = if name.ends_with(".jpg") {
        name
    } else {
        format!("{name}.jpg")
    };
    let target = fs::read_link(library_dir(paths).join(name)).ok()?;
    target
        .parent()?
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}
