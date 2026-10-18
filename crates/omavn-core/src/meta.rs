//! Everything around a game that isn't needed to run it: VNDB metadata,
//! art, the library card, and a desktop entry for app launchers.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use image::imageops::FilterType;

use crate::library::Game;
use crate::paths::Paths;
use crate::report::Reporter;
use crate::{art, vndb};

/// Look the game up on VNDB (by `vndb_id`, the game's pinned id, or its
/// title), fetch art, and refresh the card and desktop entry. Network
/// failures only degrade the art; the game stays playable.
pub fn refresh(
    paths: &Paths,
    game: &mut Game,
    vndb_id: Option<&str>,
    reporter: &dyn Reporter,
) -> Result<()> {
    let id = vndb_id.map(String::from).or_else(|| game.vndb.clone());
    let found = match &id {
        Some(id) => vndb::by_id(id),
        None => vndb::search(&game.title),
    };
    match found {
        Ok(Some(meta)) => {
            reporter.info(format!("VNDB: {} ({})", meta.title, meta.id));
            game.vndb = Some(meta.id.clone());
            if game.original_title.is_none() {
                game.original_title = meta.original_title.clone();
            }
            if let Err(e) = art::fetch(paths, game, &meta, reporter) {
                reporter.warn(format!("couldn't download art: {e}"));
            }
        }
        Ok(None) => reporter.info("not found on VNDB; using plain art"),
        Err(e) => reporter.warn(format!("VNDB lookup failed ({e}); using plain art")),
    }
    art::render(paths, game)?;
    write_icon(paths, game);
    write_desktop_entry(paths, game)?;
    write_library_entry()?;
    game.save(paths)?;
    Ok(())
}

const ICON_SVG: &str = include_str!("../assets/omavn.svg");

/// The "Visual Novels" app launcher entry that opens `omavn gui`.
pub fn write_library_entry() -> Result<()> {
    let Some(apps) = applications_dir() else {
        return Ok(());
    };
    let icons = apps.parent().unwrap().join("icons/hicolor/scalable/apps");
    fs::create_dir_all(&icons)?;
    fs::write(icons.join("omavn.svg"), ICON_SVG)?;
    let exe = std::env::current_exe()?;
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Visual Novels\n\
         Comment=Your visual novel library\n\
         Exec=\"{exe}\" gui\n\
         Icon=omavn\n\
         Terminal=false\n\
         Categories=Game;\n\
         Keywords=visual novel;vn;omavn;library;\n",
        exe = exe.display(),
    );
    fs::create_dir_all(&apps)?;
    fs::write(apps.join("omavn.desktop"), entry)?;
    Ok(())
}

pub fn icon_path(paths: &Paths, game: &Game) -> PathBuf {
    game.dir(paths).join("icon.png")
}

/// Square icon cropped from the top of the cover, where the logo and faces
/// usually are.
fn write_icon(paths: &Paths, game: &Game) {
    let Ok(cover) = image::open(art::cover(paths, game)) else {
        if let Some(icon) = art::exe_icon(paths, game) {
            let _ = icon
                .resize(256, 256, FilterType::CatmullRom)
                .save(icon_path(paths, game));
        }
        return;
    };
    let side = cover.width().min(cover.height());
    let square = cover.crop_imm((cover.width() - side) / 2, 0, side, side);
    let _ = square
        .resize_exact(256, 256, FilterType::CatmullRom)
        .save(icon_path(paths, game));
}

fn applications_dir() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("applications"))
}

pub fn desktop_entry_path(game: &Game) -> Option<PathBuf> {
    Some(applications_dir()?.join(format!("omavn-{}.desktop", game.id)))
}

fn write_desktop_entry(paths: &Paths, game: &Game) -> Result<()> {
    let Some(path) = desktop_entry_path(game) else {
        return Ok(());
    };
    let exe = std::env::current_exe()?;
    let icon = icon_path(paths, game);
    let icon = if icon.exists() {
        icon.display().to_string()
    } else {
        "applications-games".into()
    };
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={title}\n\
         Comment=Visual novel, played with omavn\n\
         Exec=\"{exe}\" run {id}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Game;\n\
         Keywords=visual novel;vn;omavn;\n\
         X-Omavn-Id={id}\n",
        title = game.title,
        exe = exe.display(),
        id = game.id,
    );
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, entry)?;
    Ok(())
}

/// Undo [`refresh`]'s footprint outside the game directory.
pub fn remove(paths: &Paths, game: &Game) {
    art::unlink(paths, game);
    if let Some(p) = desktop_entry_path(game) {
        let _ = fs::remove_file(p);
    }
}
