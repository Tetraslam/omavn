//! End-to-end unwrap: nested archives in, a classified game directory out.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use omavn_core::detect::{self, Engine, Payload};
use omavn_core::ingest;
use omavn_core::report::Silent;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omavn-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn zip(path: &Path, files: &[(&str, &[u8])]) {
    let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (name, data) in files {
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap();
}

#[test]
fn nested_zips_unwrap_to_a_game() {
    let dir = scratch("nested");
    let inner = dir.join("inner.zip");
    zip(
        &inner,
        &[
            ("Game_final/Game/Game.exe", b"MZ fake"),
            ("Game_final/Game/data.xp3", b"XP3"),
            ("__MACOSX/._Game.exe", b"junk"),
        ],
    );
    let download = dir.join("download");
    fs::create_dir_all(&download).unwrap();
    zip(
        &download.join("outer.zip"),
        &[("inner.zip", &fs::read(&inner).unwrap())],
    );

    let staged = ingest::unwrap(&download, &dir.join("staging"), &Silent).unwrap();
    let root = staged.root();
    assert!(root.join("Game.exe").is_file(), "root: {}", root.display());
    assert!(!staged.dir.join("__MACOSX").exists());
    match detect::classify(&root).unwrap() {
        Payload::Game { engine, .. } => assert_eq!(engine, Engine::Kirikiri),
        other => panic!("expected a game, got {other:?}"),
    }
    // The source is never touched.
    assert!(download.join("outer.zip").is_file());
    drop(staged);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn data_archives_next_to_an_installer_stay_packed() {
    let dir = scratch("installer");
    let download = dir.join("download");
    fs::create_dir_all(&download).unwrap();
    zip(
        &download.join("Setup.zip"),
        &[("Setup.exe", b"MZ NullsoftInst")],
    );
    // Yumina-style: data zips with a non-archive extension, read by the installer.
    zip(&download.join("game1.dat"), &[("bg.arc", b"data")]);

    let staged = ingest::unwrap(&download, &dir.join("staging"), &Silent).unwrap();
    let root = staged.root();
    assert!(root.join("Setup.exe").is_file());
    assert!(root.join("game1.dat").exists());
    assert!(!root.join("bg.arc").exists());
    drop(staged);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn disc_images_unwrap() {
    if omavn_core::runtime::which("bsdtar").is_none() {
        eprintln!("skipping: no bsdtar");
        return;
    }
    let dir = scratch("iso");
    let disc = dir.join("disc");
    fs::create_dir_all(disc.join("GAME")).unwrap();
    fs::write(disc.join("GAME/GAME.EXE"), b"MZ fake").unwrap();
    fs::write(disc.join("GAME/DATA.XP3"), b"XP3").unwrap();
    let download = dir.join("download");
    fs::create_dir_all(&download).unwrap();
    let status = std::process::Command::new("bsdtar")
        .args(["--format", "iso9660", "-cf"])
        .arg(download.join("game.iso"))
        .arg("-C")
        .arg(&disc)
        .arg("GAME")
        .status()
        .unwrap();
    assert!(status.success());

    let staged = ingest::unwrap(&download, &dir.join("staging"), &Silent).unwrap();
    let root = staged.root();
    assert!(root.join("GAME.EXE").is_file(), "root: {}", root.display());
    match detect::classify(&root).unwrap() {
        Payload::Game { engine, .. } => assert_eq!(engine, Engine::Kirikiri),
        other => panic!("expected a game, got {other:?}"),
    }
    drop(staged);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_bare_installer_download() {
    let dir = scratch("bare");
    let exe = dir.join("muramasa_installer_1.04.exe");
    fs::write(&exe, b"MZ ... Setup Factory ...").unwrap();
    let staged = ingest::unwrap(&exe, &dir.join("staging"), &Silent).unwrap();
    match detect::classify(&staged.root()).unwrap() {
        Payload::Installer { exe, .. } => assert!(exe.ends_with("muramasa_installer_1.04.exe")),
        other => panic!("expected an installer, got {other:?}"),
    }
    drop(staged);
    fs::remove_dir_all(dir).unwrap();
}
