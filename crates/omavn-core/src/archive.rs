//! Archive sniffing and extraction: zip (incl. Shift-JIS names), rar (incl.
//! multi-volume and SFX), 7z, and split `.001` sets of either.

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use zip::read::ArchiveOffset;

use crate::report::Reporter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Zip,
    Rar,
    SevenZip,
    /// ISO 9660 disc image.
    Iso,
}

/// An archive made of one or more files on disk, in volume order.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArchiveSet {
    pub kind: Kind,
    pub volumes: Vec<PathBuf>,
}

impl ArchiveSet {
    pub fn first(&self) -> &Path {
        &self.volumes[0]
    }

    pub fn total_size(&self) -> u64 {
        self.volumes
            .iter()
            .filter_map(|v| fs::metadata(v).ok())
            .map(|m| m.len())
            .sum()
    }
}

const RAR4_SIG: &[u8] = b"Rar!\x1a\x07\x00";
const RAR5_SIG: &[u8] = b"Rar!\x1a\x07\x01\x00";
const SEVENZ_SIG: &[u8] = b"7z\xbc\xaf\x27\x1c";

/// How far into an `.exe` to look for an embedded RAR (SFX stubs are small).
const SFX_SCAN: usize = 4 << 20;

/// Identify an archive by content. `.exe` files only count when they are a
/// RAR self-extractor; other installers are handled by [`crate::detect`].
pub fn sniff(path: &Path) -> Option<Kind> {
    let mut f = File::open(path).ok()?;
    let mut head = [0u8; 8];
    let n = f.read(&mut head).ok()?;
    let head = &head[..n];
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        return Some(Kind::Zip);
    }
    if head.starts_with(RAR4_SIG) || head.starts_with(RAR5_SIG) {
        return Some(Kind::Rar);
    }
    if is_iso(&mut f) {
        return Some(Kind::Iso);
    }
    if head.starts_with(SEVENZ_SIG) {
        return Some(Kind::SevenZip);
    }
    if head.starts_with(b"MZ") && is_rar_sfx(path) {
        return Some(Kind::Rar);
    }
    None
}

/// ISO 9660 puts `CD001` in the first volume descriptor, at 0x8001.
fn is_iso(f: &mut File) -> bool {
    let mut magic = [0u8; 5];
    f.seek(SeekFrom::Start(0x8001)).is_ok()
        && f.read_exact(&mut magic).is_ok()
        && &magic == b"CD001"
}

fn is_rar_sfx(path: &Path) -> bool {
    let Ok(f) = File::open(path) else {
        return false;
    };
    let mut buf = Vec::with_capacity(SFX_SCAN);
    if f.take(SFX_SCAN as u64).read_to_end(&mut buf).is_err() {
        return false;
    }
    let found =
        memchr::memmem::find(&buf, RAR5_SIG).or_else(|| memchr::memmem::find(&buf, RAR4_SIG));
    // Confirm with unrar itself: installers sometimes embed RAR-looking bytes.
    found.is_some() && unrar::Archive::new(path).open_for_listing().is_ok()
}

/// Group archive files found among `files` into sets. Files that are not
/// archives (or that are non-first volumes) are left out.
pub fn group(files: &[PathBuf]) -> Vec<ArchiveSet> {
    let mut sets = Vec::new();
    let mut claimed = std::collections::HashSet::new();

    for file in files {
        if claimed.contains(file) {
            continue;
        }
        let name = file_name_lower(file);

        // name.part01.rar / name.part1.exe (RAR 3+ volume naming)
        if let Some((stem, n)) = part_number(&name) {
            if n != 1 {
                continue;
            }
            if sniff(file) != Some(Kind::Rar) {
                continue;
            }
            let mut vols: Vec<(u32, PathBuf)> = files
                .iter()
                .filter_map(|f| {
                    let name = file_name_lower(f);
                    let (s, n) = part_number(&name)?;
                    (s == stem).then(|| (n, f.clone()))
                })
                .collect();
            vols.sort();
            for (_, v) in &vols {
                claimed.insert(v.clone());
            }
            sets.push(ArchiveSet {
                kind: Kind::Rar,
                volumes: vols.into_iter().map(|(_, v)| v).collect(),
            });
            continue;
        }

        // name.7z.001 / name.zip.001 (plain byte splits)
        if let Some(stem) = name.strip_suffix(".001") {
            let kind = if stem.ends_with(".7z") {
                Kind::SevenZip
            } else if stem.ends_with(".zip") {
                Kind::Zip
            } else {
                continue;
            };
            let mut vols: Vec<PathBuf> = files
                .iter()
                .filter(|f| {
                    let n = file_name_lower(f);
                    n.strip_prefix(stem)
                        .and_then(|rest| rest.strip_prefix('.'))
                        .is_some_and(|num| {
                            num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit())
                        })
                })
                .cloned()
                .collect();
            vols.sort();
            claimed.extend(vols.iter().cloned());
            sets.push(ArchiveSet {
                kind,
                volumes: vols,
            });
            continue;
        }

        // Old-style name.rar + name.r00, name.r01, ...
        if name.ends_with(".rar") {
            if sniff(file) != Some(Kind::Rar) {
                continue;
            }
            let stem = &name[..name.len() - 4];
            let mut rest: Vec<PathBuf> = files
                .iter()
                .filter(|f| {
                    let n = file_name_lower(f);
                    n.strip_prefix(stem)
                        .and_then(|r| r.strip_prefix(".r"))
                        .is_some_and(|num| {
                            num.len() == 2 && num.bytes().all(|b| b.is_ascii_digit())
                        })
                })
                .cloned()
                .collect();
            rest.sort();
            claimed.insert(file.clone());
            claimed.extend(rest.iter().cloned());
            let mut volumes = vec![file.clone()];
            volumes.extend(rest);
            sets.push(ArchiveSet {
                kind: Kind::Rar,
                volumes,
            });
            continue;
        }

        let by_ext = [".zip", ".7z", ".exe", ".iso"]
            .iter()
            .any(|e| name.ends_with(e));
        if by_ext && let Some(kind) = sniff(file) {
            claimed.insert(file.clone());
            sets.push(ArchiveSet {
                kind,
                volumes: vec![file.clone()],
            });
        }
    }
    sets
}

fn file_name_lower(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// `foo.part03.rar` -> ("foo", 3). Also accepts `.exe` for SFX first volumes.
fn part_number(name: &str) -> Option<(&str, u32)> {
    let base = name
        .strip_suffix(".rar")
        .or_else(|| name.strip_suffix(".exe"))?;
    let idx = base.rfind(".part")?;
    let num = &base[idx + 5..];
    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((&base[..idx], num.parse().ok()?))
}

/// Extract `set` into `dest` (created if needed). Returns bytes written.
pub fn extract(set: &ArchiveSet, dest: &Path, reporter: &dyn Reporter) -> Result<u64> {
    fs::create_dir_all(dest)?;
    match set.kind {
        Kind::Zip => extract_zip(set, dest, reporter),
        Kind::Rar => extract_rar(set, dest, reporter),
        Kind::SevenZip => extract_7z(set, dest, reporter),
        Kind::Iso => extract_iso(set, dest, reporter),
    }
    .with_context(|| format!("extracting {}", set.first().display()))
}

/// Turn an archive member name into a safe relative path, or `None` if it
/// tries to escape (absolute paths, `..`).
pub fn safe_relative(name: &str) -> Option<PathBuf> {
    let name = name.replace('\\', "/");
    let mut out = PathBuf::new();
    for comp in Path::new(&name).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Zip names without the UTF-8 flag are in whatever codepage the packer used.
/// For VNs that is overwhelmingly Shift-JIS (CP932), so try that before CP437.
fn decode_zip_name(raw: &[u8], fallback: impl FnOnce() -> String) -> String {
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_owned();
    }
    let (s, _, had_errors) = encoding_rs::SHIFT_JIS.decode(raw);
    if !had_errors {
        return s.into_owned();
    }
    fallback()
}

fn open_volumes(set: &ArchiveSet) -> Result<Box<dyn ReadSeek>> {
    Ok(Box::new(BufReader::with_capacity(
        1 << 20,
        Volumes::open(&set.volumes)?,
    )))
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

fn extract_zip(set: &ArchiveSet, dest: &Path, reporter: &dyn Reporter) -> Result<u64> {
    let volumes = Volumes::open(&set.volumes)?;
    let mut head = [0u8; 4];
    volumes.read_at(0, &mut head)?;
    // Plain zips start at offset 0; only SFX zips need the (slow) offset scan.
    let offset = if &head == b"PK\x03\x04" {
        ArchiveOffset::Known(0)
    } else {
        ArchiveOffset::Detect
    };
    let config = zip::read::Config {
        archive_offset: offset,
    };
    let archive = zip::ZipArchive::with_config(config, volumes)?;
    let total: u64 = (0..archive.len())
        .filter_map(|i| archive.by_index_data(i).ok().map(|e| e.size()))
        .sum();
    let done = AtomicU64::new(0);
    reporter.progress(0, total);

    // Clones share the parsed central directory; reads are positional, so
    // members inflate in parallel without fighting over a file cursor.
    (0..archive.len()).into_par_iter().try_for_each_init(
        || archive.clone(),
        |archive, i| -> Result<()> {
            let mut entry = archive.by_index(i)?;
            if entry.is_symlink() {
                return Ok(());
            }
            let name = decode_zip_name(entry.name_raw(), || {
                entry.name().map(|n| n.into_owned()).unwrap_or_default()
            });
            let Some(rel) = safe_relative(&name) else {
                return Ok(());
            };
            if is_junk(&rel) {
                return Ok(());
            }
            let out = dest.join(&rel);
            if entry.is_dir() {
                fs::create_dir_all(&out)?;
                return Ok(());
            }
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = Uncached::new(
                File::create(&out).with_context(|| format!("creating {}", out.display()))?,
            );
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n])?;
                let d = done.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
                reporter.progress(d, total);
            }
            file.finish()?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let mode = (mode & 0o777) | 0o600;
                let _ = fs::set_permissions(&out, fs::Permissions::from_mode(mode));
            }
            Ok(())
        },
    )?;
    Ok(done.into_inner())
}

fn extract_rar(set: &ArchiveSet, dest: &Path, reporter: &dyn Reporter) -> Result<u64> {
    let mut total = 0u64;
    {
        let mut list = unrar::Archive::new(set.first()).open_for_listing()?;
        while let Some(header) = list.read_header()? {
            total += header.entry().unpacked_size;
            list = header.skip()?;
        }
    }
    reporter.progress(0, total);

    let mut done = 0u64;
    let mut archive = unrar::Archive::new(set.first()).open_for_processing()?;
    while let Some(header) = archive.read_header()? {
        let entry = header.entry();
        let name = entry.filename.to_string_lossy().into_owned();
        let size = entry.unpacked_size;
        archive = match safe_relative(&name) {
            Some(rel) if entry.is_file() && !is_junk(&rel) => {
                let out = dest.join(&rel);
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                let next = header.extract_to(&out)?;
                drop_cache(&out, true);
                for vol in &set.volumes {
                    drop_cache(vol, false);
                }
                done += size;
                reporter.progress(done, total);
                next
            }
            Some(rel) if entry.is_directory() => {
                fs::create_dir_all(dest.join(rel))?;
                header.skip()?
            }
            _ => header.skip()?,
        };
    }
    Ok(done)
}

fn extract_7z(set: &ArchiveSet, dest: &Path, reporter: &dyn Reporter) -> Result<u64> {
    let total = set.total_size();
    let mut done = 0u64;
    reporter.progress(0, total);
    sevenz_rust2::decompress_with_extract_fn(open_volumes(set)?, dest, |entry, reader, _| {
        let Some(rel) = safe_relative(entry.name()) else {
            return Ok(true);
        };
        if is_junk(&rel) {
            return Ok(true);
        }
        let out = dest.join(&rel);
        if entry.is_directory() {
            fs::create_dir_all(&out)?;
            return Ok(true);
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = Uncached::new(File::create(&out)?);
        done += io::copy(reader, &mut file)?;
        file.finish()?;
        reporter.progress(done.min(total), total);
        Ok(true)
    })?;
    Ok(done)
}

/// Disc images go through libarchive's bsdtar, present on most systems.
fn extract_iso(set: &ArchiveSet, dest: &Path, reporter: &dyn Reporter) -> Result<u64> {
    let bsdtar = crate::runtime::which("bsdtar")
        .context("disc images need bsdtar (libarchive); install it with your package manager")?;
    reporter.progress(0, 0);
    let status = std::process::Command::new(bsdtar)
        .arg("-xf")
        .arg(set.first())
        .arg("-C")
        .arg(dest)
        .status()?;
    if !status.success() {
        bail!("bsdtar failed ({status})");
    }
    // Discs mark everything read-only; installers and saves need to write.
    for entry in walkdir::WalkDir::new(dest).into_iter().flatten() {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = entry.metadata() {
            let mode = m.permissions().mode() | 0o200;
            let _ = fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode));
        }
    }
    for entry in walkdir::WalkDir::new(dest).into_iter().flatten() {
        if entry.file_type().is_file() {
            drop_cache(entry.path(), true);
        }
    }
    for vol in &set.volumes {
        drop_cache(vol, false);
    }
    Ok(set.total_size())
}

/// Metadata that archivers (mostly macOS Finder) sneak in.
pub fn is_junk(rel: &Path) -> bool {
    rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "__MACOSX"
            || s == ".DS_Store"
            || s.starts_with("._")
            || s.eq_ignore_ascii_case("thumbs.db")
    })
}

/// Unpacking tens of gigabytes would otherwise fill RAM with page cache
/// (both the archive read and the files written), and on a desktop that
/// memory pressure gets the user's apps killed by systemd-oomd. Written
/// data is flushed and dropped from cache every few megabytes.
struct Uncached {
    file: File,
    written: u64,
    flushed: u64,
}

const FLUSH_EVERY: u64 = 32 << 20;

impl Uncached {
    fn new(file: File) -> Self {
        Self {
            file,
            written: 0,
            flushed: 0,
        }
    }

    fn flush_range(&mut self) {
        use std::os::fd::AsRawFd;
        let len = self.written - self.flushed;
        if len == 0 {
            return;
        }
        // SAFETY: plain syscalls on a file descriptor we own.
        unsafe {
            libc::sync_file_range(
                self.file.as_raw_fd(),
                self.flushed as i64,
                len as i64,
                libc::SYNC_FILE_RANGE_WAIT_BEFORE
                    | libc::SYNC_FILE_RANGE_WRITE
                    | libc::SYNC_FILE_RANGE_WAIT_AFTER,
            );
        }
        advise_dontneed(&self.file, self.flushed, len);
        self.flushed = self.written;
    }

    fn finish(mut self) -> io::Result<()> {
        self.flush_range();
        Ok(())
    }
}

impl Write for Uncached {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.file.write(buf)?;
        self.written += n as u64;
        if self.written - self.flushed >= FLUSH_EVERY {
            self.flush_range();
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn advise_dontneed(file: &File, offset: u64, len: u64) {
    use std::os::fd::AsRawFd;
    // SAFETY: advisory syscall on a file descriptor we own.
    unsafe {
        libc::posix_fadvise(
            file.as_raw_fd(),
            offset as i64,
            len as i64,
            libc::POSIX_FADV_DONTNEED,
        );
    }
}

/// Drop a whole file from the page cache, writing it out first when it
/// was just written (for files another library wrote, like unrar's).
pub fn drop_cache(path: &Path, written: bool) {
    let Ok(f) = File::open(path) else { return };
    if written {
        let _ = f.sync_data();
    }
    advise_dontneed(&f, 0, 0);
}

/// One or more files read back to back as a single stream. Reads are
/// positional (`pread`), so clones can be used from many threads at once.
#[derive(Clone)]
struct Volumes {
    files: std::sync::Arc<Vec<(File, u64, u64)>>, // (handle, start offset, len)
    len: u64,
    pos: u64,
}

impl Volumes {
    fn open(paths: &[PathBuf]) -> Result<Self> {
        let mut files = Vec::new();
        let mut off = 0;
        for p in paths {
            let f = File::open(p)?;
            let len = f.metadata()?.len();
            files.push((f, off, len));
            off += len;
        }
        if files.is_empty() {
            bail!("no volumes");
        }
        Ok(Self {
            files: files.into(),
            len: off,
            pos: 0,
        })
    }

    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        use std::os::unix::fs::FileExt;
        let Some((f, start, len)) = self
            .files
            .iter()
            .find(|(_, s, l)| pos >= *s && pos < *s + *l)
        else {
            return Ok(0);
        };
        let max = ((*start + *len - pos) as usize).min(buf.len());
        let n = f.read_at(&mut buf[..max], pos - *start)?;
        // Archives are read once; keeping them cached only evicts the
        // user's own working set.
        advise_dontneed(f, pos - *start, n as u64);
        Ok(n)
    }
}

impl Read for Volumes {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.read_at(self.pos, buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Volumes {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let new = match to {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.len as i64 + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if new < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_numbers() {
        assert_eq!(part_number("dies irae.part1.exe"), Some(("dies irae", 1)));
        assert_eq!(part_number("dies irae.part03.rar"), Some(("dies irae", 3)));
        assert_eq!(part_number("setup.exe"), None);
    }

    #[test]
    fn safe_paths() {
        assert_eq!(safe_relative("a\\b.txt"), Some(PathBuf::from("a/b.txt")));
        assert_eq!(safe_relative("../etc/passwd"), None);
        assert_eq!(safe_relative("/etc/passwd"), None);
    }

    #[test]
    fn shift_jis_names() {
        // "セーブ" in Shift-JIS
        let raw = [0x83, 0x5a, 0x81, 0x5b, 0x83, 0x75];
        assert_eq!(decode_zip_name(&raw, String::new), "セーブ");
    }

    #[test]
    fn junk() {
        assert!(is_junk(Path::new("__MACOSX/._Setup.exe")));
        assert!(!is_junk(Path::new("Setup.exe")));
    }
}
