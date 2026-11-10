//! RG353PS BaseOS boot-logo contract: 640x480 BMP validation, durable
//! selection state, and the generated-default state when no custom image is selected.

mod common;

use slot::bootlogo::{self, BYTES, Failure};
use std::path::Path;

/// A complete BMP3 pixel array. Pixels are zeroed; tests exercise the headers
/// and dimensions rather than the artwork.
fn bmp(width: i32, height: i32, bpp: u16) -> Vec<u8> {
    let mut b = vec![0u8; BYTES];
    b[0..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&(BYTES as u32).to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&width.to_le_bytes());
    b[22..26].copy_from_slice(&height.to_le_bytes());
    b[26..28].copy_from_slice(&1u16.to_le_bytes());
    b[28..30].copy_from_slice(&bpp.to_le_bytes());
    b[30..34].copy_from_slice(&0u32.to_le_bytes());
    b
}

fn boot_dir(root: &Path) -> std::path::PathBuf {
    let dir = root.join(bootlogo::DIR);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn baseos_boot_is_mounted() -> bool {
    std::fs::read_to_string("/proc/mounts")
        .map(|text| {
            text.lines()
                .any(|line| line.split_whitespace().nth(1) == Some(bootlogo::DEFAULT_MOUNT))
        })
        .unwrap_or(false)
}

#[test]
fn only_bmps_are_listed_in_stable_order() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = boot_dir(d.path());
    std::fs::write(dir.join("notes.txt"), b"hello").unwrap();
    std::fs::write(dir.join("cover.png"), b"not a bmp").unwrap();
    std::fs::write(dir.join("._psyduck.bmp"), b"sidecar").unwrap();
    assert!(bootlogo::list(d.path()).is_empty());

    std::fs::write(dir.join("b.bmp"), bmp(640, 480, 24)).unwrap();
    std::fs::write(dir.join("a.BMP"), bmp(640, 480, 24)).unwrap();
    let names: Vec<String> = bootlogo::list(d.path())
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["a.BMP", "b.bmp"]);
}

#[test]
fn the_label_drops_the_extension() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = boot_dir(d.path());
    std::fs::write(dir.join("sunset.bmp"), bmp(640, 480, 24)).unwrap();
    let logos = bootlogo::list(d.path());
    assert_eq!(logos[0].label(), "sunset");
    assert_eq!(logos[0].name, "sunset.bmp");
}

#[test]
fn no_selection_means_generated_baseos_default() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = boot_dir(d.path());
    for n in ["a.bmp", "b.bmp", "c.bmp"] {
        std::fs::write(dir.join(n), bmp(640, 480, 24)).unwrap();
    }
    let logos = bootlogo::list(d.path());
    assert!(bootlogo::current(d.path()).is_none());
    assert_eq!(bootlogo::selected(d.path(), &logos), None);

    bootlogo::save(d.path(), &logos[2].name).unwrap();
    assert_eq!(bootlogo::current(d.path()).as_deref(), Some("c.bmp"));
    assert_eq!(bootlogo::selected(d.path(), &logos), Some(2));

    bootlogo::save(d.path(), "gone.bmp").unwrap();
    assert_eq!(bootlogo::selected(d.path(), &logos), None);

    bootlogo::reset(d.path()).unwrap();
    assert!(bootlogo::current(d.path()).is_none());
    assert!(!d.path().join(bootlogo::STATE).exists());
}

#[test]
fn only_complete_rg353ps_bmp3_images_are_accepted() {
    assert_eq!(BYTES, 921_654);
    assert!(bootlogo::is_boot_picture(&bmp(640, 480, 24)));
    assert!(bootlogo::is_boot_picture(&bmp(640, -480, 24)), "top-down BMP");
    assert!(!bootlogo::is_boot_picture(&bmp(720, 480, 24)), "old 720-wide format");
    assert!(!bootlogo::is_boot_picture(&bmp(640, 481, 24)), "a row too tall");
    assert!(!bootlogo::is_boot_picture(&bmp(641, 480, 24)), "a column too wide");
    assert!(!bootlogo::is_boot_picture(&bmp(640, 480, 32)), "32 bit");
    assert!(!bootlogo::is_boot_picture(&bmp(640, 480, 8)), "palettised");
    assert!(!bootlogo::is_boot_picture(b"BM"), "a stub");
    assert!(!bootlogo::is_boot_picture(b""), "nothing at all");

    let mut compressed = bmp(640, 480, 24);
    compressed[30..34].copy_from_slice(&1u32.to_le_bytes());
    assert!(!bootlogo::is_boot_picture(&compressed), "compressed BMP");
    let mut invalid_header = bmp(640, 480, 24);
    invalid_header[14..18].copy_from_slice(&124u32.to_le_bytes());
    assert!(!bootlogo::is_boot_picture(&invalid_header), "pixel data overlaps DIB header");
    let mut truncated = bmp(640, 480, 24);
    truncated.truncate(BYTES - 1);
    assert!(!bootlogo::is_boot_picture(&truncated), "truncated pixel array");
}

#[test]
fn a_failed_apply_does_not_commit_a_selection_or_backup() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = boot_dir(d.path());
    std::fs::write(dir.join("a.bmp"), bmp(640, 480, 24)).unwrap();
    let logos = bootlogo::list(d.path());

    // Do not let a host test write to a real device. On hardware the integration
    // test is skipped; the unit suite only exercises validation/state helpers.
    if baseos_boot_is_mounted() {
        return;
    }
    assert_eq!(bootlogo::apply(d.path(), &logos[0]), Err(Failure::Mount));
    assert!(!d.path().join(bootlogo::BACKUP).exists());
    assert!(bootlogo::current(d.path()).is_none());
}

#[test]
fn an_invalid_picture_is_rejected_before_boot_volume_access() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = boot_dir(d.path());
    std::fs::write(dir.join("small.bmp"), bmp(320, 240, 24)).unwrap();
    let logos = bootlogo::list(d.path());
    assert_eq!(
        bootlogo::apply(d.path(), &logos[0]),
        Err(Failure::NotAPicture)
    );
    assert!(bootlogo::current(d.path()).is_none());
}