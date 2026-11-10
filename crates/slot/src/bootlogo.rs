//! RG353PS BaseOS boot-logo selection.
//!
//! BaseOS owns the boot partition and its mount lifecycle. The frontend stores
//! the durable source/selection on /mnt/SDCARD and installs only the active
//! cache at BASEOS_BOOT=/mnt/boot. With no selection record, BaseOS draws its
//! generated wordmark; a leftover bootlogo.bmp is not an implicit selection.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DIR: &str = "Bootlogos";
pub const STATE: &str = "System/bootlogo.txt";
pub const BACKUP: &str = "System/bootlogo-original.bmp";
pub const ACTIVE_NAME: &str = "bootlogo.bmp";
pub const DEFAULT_MOUNT: &str = "/mnt/boot";
pub const DATA_MOUNT: &str = "/mnt/SDCARD";

pub const W: u32 = 640;
pub const H: u32 = 480;
pub const BYTES: usize = 54 + (W as usize) * (H as usize) * 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
}

impl Entry {
    pub fn label(&self) -> &str {
        self.name
            .rsplit_once('.')
            .map_or(self.name.as_str(), |(stem, _)| stem)
    }
}

fn mounted_at(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string("/proc/mounts") else {
        return false;
    };
    let Some(want) = path.to_str() else {
        return false;
    };
    text.lines().any(|line| line.split_whitespace().nth(1) == Some(want))
}

/// The shared BaseOS user volume is always the owner of logo state, even when
/// the active frontend itself is running from TF2. Host tests fall back to the
/// supplied temporary root because /mnt/SDCARD is not mounted there.
fn data_root(root: &Path) -> PathBuf {
    let shared = Path::new(DATA_MOUNT);
    if mounted_at(shared) {
        shared.to_path_buf()
    } else {
        root.to_path_buf()
    }
}

fn valid_basename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
}

fn candidate_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let shared = data_root(root).join(DIR);
    let local = root.join(DIR);
    dirs.push(shared.clone());
    if local != shared {
        dirs.push(local);
    }
    dirs
}

/// List candidate BMP files. When the frontend runs from TF2, also include
/// candidates on the shared TF1 user-data volume that BaseOS reads at boot.
/// Shared-volume names win when both cards contain the same basename.
pub fn list(root: &Path) -> Vec<Entry> {
    let mut by_name = BTreeMap::<String, Entry>::new();
    for dir in candidate_dirs(root) {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for path in entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| !slot_store::is_hidden(p))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bmp")))
        {
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                continue;
            };
            if valid_basename(&name) {
                by_name.entry(name.clone()).or_insert(Entry { name, path });
            }
        }
    }
    by_name.into_values().collect()
}

/// The selected basename lives on the shared BaseOS user volume. Missing,
/// empty or malformed state means the generated BaseOS default is active.
pub fn current(root: &Path) -> Option<String> {
    let text = fs::read_to_string(data_root(root).join(STATE)).ok()?;
    let name = text.lines().next()?.trim();
    valid_basename(name).then(|| name.to_string())
}

fn sync_storage() {
    let _ = Command::new("sync").status();
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ()> {
    let parent = path.parent().ok_or(())?;
    fs::create_dir_all(parent).map_err(|_| ())?;
    let name = path.file_name().and_then(|n| n.to_str()).ok_or(())?;
    let tmp = parent.join(format!(".{name}.slot-tmp"));
    let _ = fs::remove_file(&tmp);
    fs::write(&tmp, bytes).map_err(|_| ())?;
    let readback = fs::read(&tmp).map_err(|_| ())?;
    if readback != bytes {
        let _ = fs::remove_file(&tmp);
        return Err(());
    }
    fs::rename(&tmp, path).map_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

/// Save state atomically. This remains public for migration tools/tests; the
/// normal UI path commits it inside apply() after the active image is verified.
pub fn save(root: &Path, name: &str) -> Result<(), Failure> {
    if !valid_basename(name) {
        return Err(Failure::State);
    }
    let path = data_root(root).join(STATE);
    atomic_write(&path, format!("{name}\n").as_bytes()).map_err(|_| Failure::State)?;
    sync_storage();
    Ok(())
}

/// Return the zero-based candidate index recorded on the shared data volume.
pub fn selected(root: &Path, logos: &[Entry]) -> Option<usize> {
    let name = current(root)?;
    logos.iter().position(|e| e.name == name)
}

/// Remove the custom selection so the next boot uses BaseOS's generated
/// wordmark. The active cached BMP may remain on BASEOS_BOOT; without a
/// selection record fbsplash deliberately ignores it.
pub fn reset(root: &Path) -> Result<(), Failure> {
    let state = data_root(root).join(STATE);
    match fs::remove_file(state) {
        Ok(()) => sync_storage(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Failure::State),
    }
    Ok(())
}

/// Validate a Windows BMP by its contents, not its extension:
/// 640x480, one plane, 24-bit BI_RGB, a valid DIB header and a complete pixel array.
pub fn is_boot_picture(bytes: &[u8]) -> bool {
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return false;
    }
    let dib_size = le_u32(bytes, 14) as usize;
    if dib_size < 40 || le_u16(bytes, 26) != 1 {
        return false;
    }
    if le_u16(bytes, 28) != 24 || le_u32(bytes, 30) != 0 {
        return false;
    }
    let header_end = 14usize.checked_add(dib_size);
    let Some(header_end) = header_end else {
        return false;
    };
    if header_end > bytes.len() {
        return false;
    }
    let width = le_i32(bytes, 18);
    let height = le_i32(bytes, 22);
    if width != W as i32 || (height != H as i32 && height != -(H as i32)) {
        return false;
    }
    let pixel_offset = le_u32(bytes, 10) as usize;
    if pixel_offset < header_end {
        return false;
    }
    let row_bytes = (W as usize * 3 + 3) & !3;
    let Some(pixel_end) = row_bytes
        .checked_mul(H as usize)
        .and_then(|n| pixel_offset.checked_add(n))
    else {
        return false;
    };
    pixel_end <= bytes.len()
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Failure {
    NotAPicture,
    NoPartition,
    Mount,
    NoTarget,
    Source,
    Backup,
    Write,
    Verify,
    State,
    Rollback,
}

fn mounted_baseos_boot() -> Result<PathBuf, Failure> {
    let mount = std::env::var_os("BASEOS_BOOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MOUNT));
    let mount_text = mount.to_str().ok_or(Failure::Mount)?;
    let mounts = fs::read_to_string("/proc/mounts").map_err(|_| Failure::Mount)?;
    let mut device = None;
    for line in mounts.lines() {
        let mut cols = line.split_whitespace();
        let dev = cols.next().unwrap_or("");
        let at = cols.next().unwrap_or("");
        let fs_type = cols.next().unwrap_or("");
        let options = cols.next().unwrap_or("");
        if at == mount_text {
            if fs_type != "vfat" || !options.split(',').any(|o| o == "rw") {
                return Err(Failure::Mount);
            }
            device = Some(dev.to_string());
            break;
        }
    }
    let device = device.ok_or(Failure::Mount)?;
    let name = Path::new(&device)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(Failure::NoPartition)?;
    let uevent = fs::read_to_string(Path::new("/sys/class/block").join(name).join("uevent"))
        .map_err(|_| Failure::NoPartition)?;
    if !uevent.lines().any(|line| line == "PARTNAME=BASEOS_BOOT") {
        return Err(Failure::NoPartition);
    }
    Ok(mount)
}

/// Install a selected image through the BaseOS contract:
/// validate and preserve its source on /mnt/SDCARD, reuse the existing
/// read-write BASEOS_BOOT mount, preserve the original once, atomically write
/// and verify the active cache, then commit the selection record last.
pub fn apply(root: &Path, entry: &Entry) -> Result<(), Failure> {
    if !valid_basename(&entry.name) {
        return Err(Failure::NotAPicture);
    }
    let bytes = fs::read(&entry.path).map_err(|_| Failure::NotAPicture)?;
    if !is_boot_picture(&bytes) {
        return Err(Failure::NotAPicture);
    }

    let persistent = data_root(root);
    let source = persistent.join(DIR).join(&entry.name);
    fs::create_dir_all(source.parent().ok_or(Failure::Source)?)
        .map_err(|_| Failure::Source)?;
    if entry.path != source {
        let same = fs::read(&source).is_ok_and(|existing| existing == bytes);
        if !same {
            atomic_write(&source, &bytes).map_err(|_| Failure::Source)?;
            sync_storage();
        }
    }

    let boot = mounted_baseos_boot()?;
    let active = boot.join(ACTIVE_NAME);
    if !active.is_file() {
        return Err(Failure::NoTarget);
    }
    let previous = fs::read(&active).map_err(|_| Failure::NoTarget)?;

    // Preserve the original boot image only once; never replace it with a later
    // custom image when the user changes selections again.
    let backup = persistent.join(BACKUP);
    if !backup.exists() {
        atomic_write(&backup, &previous).map_err(|_| Failure::Backup)?;
        sync_storage();
    }

    atomic_write(&active, &bytes).map_err(|_| Failure::Write)?;
    let installed = match fs::read(&active) {
        Ok(installed) => installed,
        Err(_) => {
            if atomic_write(&active, &previous).is_ok() {
                sync_storage();
                return Err(Failure::Verify);
            }
            sync_storage();
            return Err(Failure::Rollback);
        }
    };
    if installed != bytes {
        if atomic_write(&active, &previous).is_ok() {
            sync_storage();
            return Err(Failure::Verify);
        }
        sync_storage();
        return Err(Failure::Rollback);
    }
    sync_storage();

    // Commit last: rcS must never restore a candidate whose active BOOT copy
    // did not verify. If the state write fails, put the previous cache back.
    if save(&persistent, &entry.name).is_err() {
        if atomic_write(&active, &previous).is_ok() {
            sync_storage();
            return Err(Failure::State);
        }
        sync_storage();
        return Err(Failure::Rollback);
    }
    Ok(())
}

fn le_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn le_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ])
}

fn le_i32(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ])
}
