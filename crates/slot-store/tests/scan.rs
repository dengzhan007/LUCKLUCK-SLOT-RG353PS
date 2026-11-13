mod common;

use common::tmp_root;
use slot_store::{rescan, scan, scan_cached};
use tempfile::TempDir;

fn write_rom(d: &TempDir, name: &str, title: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    std::fs::write(d.path().join("Games").join(name), rom).expect("write rom");
}

fn write_png(d: &TempDir, name: &str) {
    std::fs::write(d.path().join("Labels").join(name), b"\x89PNG\r\n\x1a\n").expect("write png");
}

#[test]
fn a_png_in_labels_is_paired_to_its_rom_by_stem() {
    let d = tmp_root();
    write_rom(&d, "Pokemon Emerald.gba", "POKEMON EMER");
    write_png(&d, "Pokemon Emerald.png");
    write_rom(&d, "Advance Wars.gba", "ADVANCEWARS");
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 2);
    assert_eq!(carts[0].stem, "Advance Wars");
    assert!(carts[0].label.is_none());
    assert!(
        carts[1].label.is_some(),
        "a label in Labels/ was not picked up"
    );
    assert_eq!(carts[1].title, "POKEMON EMER");
}

#[test]
fn scan_ignores_non_gba_files() {
    let d = tmp_root();
    write_rom(&d, "Real.gba", "REAL");
    std::fs::write(d.path().join("Games/notes.txt"), "hi").unwrap();
    assert_eq!(scan(d.path()).unwrap().len(), 1);
}

#[test]
fn an_appledouble_sidecar_is_not_shelved_as_a_cart() {
    let d = tmp_root();
    write_rom(&d, "Metroid Fusion.gba", "METROID");
    // Copying a rom onto a FAT card from macOS leaves this beside it, carrying the same
    // extension and the same stem, so only the leading dot tells the two apart.
    write_rom(&d, "._Metroid Fusion.gba", "METROID");
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 1, "an AppleDouble sidecar reached the shelf");
    assert_eq!(carts[0].stem, "Metroid Fusion");
}

/// A zip romset is shelved as the machine its **folder** names, not the machine its extension
/// would suggest — and `.zip` says nothing about which one. FBNeo ships both families as zips,
/// so the folder is the only thing that tells a NeoGeo cart from a CPS one without unzipping.
#[test]
fn a_zip_is_shelved_as_the_machine_its_folder_names() {
    let d = tmp_root();
    write_rom(&d, "Pokemon Emerald.gba", "POKEMON EMER");
    std::fs::create_dir_all(d.path().join("Games/NEOGEO")).unwrap();
    std::fs::create_dir_all(d.path().join("Games/ARCADE")).unwrap();
    std::fs::write(d.path().join("Games/NEOGEO/MSLUG.zip"), b"PK\x03\x00fake").unwrap();
    std::fs::write(d.path().join("Games/ARCADE/SF2.zip"), b"PK\x03\x00fake").unwrap();
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 3, "the two arcade zips did not reach the shelf");
    let by_stem: Vec<(&str, slot_store::System)> = carts
        .iter()
        .map(|c| (c.stem.as_str(), c.system()))
        .collect();
    assert!(
        by_stem.contains(&("MSLUG", slot_store::System::Neogeo)),
        "a zip in Games/NEOGEO was not shelved as NeoGeo: {by_stem:?}"
    );
    assert!(
        by_stem.contains(&("SF2", slot_store::System::Arcade)),
        "a zip in Games/ARCADE was not shelved as Arcade: {by_stem:?}"
    );
}

/// A zip sitting loose in `Games/` has no folder to name it and no extension that names one
/// either, so it is no machine at all and never reaches the shelf.
///
/// This used to be shelved anyway and resolve to the default machine, which meant it was handed
/// to a core that could not open it — a "game" on the shelf that could only ever fail. Not
/// shelving it is the same outcome with nothing to click on.
#[test]
fn a_zip_in_the_games_root_names_no_machine_and_is_not_shelved() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/loose.zip"), b"PK\x03\x00fake").unwrap();
    let carts = scan(d.path()).unwrap();
    assert_eq!(
        carts.len(),
        0,
        "a folderless zip reached the shelf with no machine to name it"
    );
}

/// The cache keys off the rom's name, and the name is the relative path — so a zip's folder is
/// part of what it is. A second scan must still answer `NEOGEO`, not fall back to `Games/`.
#[test]
fn the_cache_keeps_a_zips_folder_with_it() {
    let d = tmp_root();
    std::fs::create_dir_all(d.path().join("Games/NEOGEO")).unwrap();
    std::fs::write(d.path().join("Games/NEOGEO/MSLUG.zip"), b"PK\x03\x00fake").unwrap();
    let first = scan(d.path()).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].system(), slot_store::System::Neogeo);
    let second = scan_cached(d.path()).unwrap();
    assert_eq!(second.len(), 1, "the cache lost the arcade zip");
    assert_eq!(
        second[0].system(),
        slot_store::System::Neogeo,
        "the cache answered a different machine than the scan did"
    );
}

#[test]
fn header_title_of_a_truncated_rom_is_none_not_a_panic() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/Tiny.gba"), [0u8; 8]).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_header_title_that_is_not_text_is_dropped_rather_than_mangled() {
    let d = tmp_root();
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xac].copy_from_slice(&[0xffu8; 12]);
    std::fs::write(d.path().join("Games/Garbage.gba"), rom).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_root_with_no_games_directory_scans_as_empty() {
    let d = tempfile::tempdir().unwrap();
    assert!(scan(d.path()).unwrap().is_empty());
}

// The cache is what a boot calls, so everything the plain scan promises has to hold through
// it as well, and the two have to agree about the same card.

#[test]
fn the_cache_agrees_with_a_full_scan() {
    let d = tmp_root();
    write_rom(&d, "Pokemon Emerald.gba", "POKEMON EMER");
    write_rom(&d, "Advance Wars.gba", "ADVANCEWARS");
    write_png(&d, "Pokemon Emerald.png");
    assert_eq!(scan_cached(d.path()).unwrap(), scan(d.path()).unwrap());
}

/// The whole point: a card that has not changed must not go back to the roms. Rewriting the
/// title the cache holds leaves the header on disk saying something else, so a run that
/// returns the cache's answer is one that did not open a single rom.
#[test]
fn an_unchanged_card_is_answered_from_the_cache_without_reading_a_rom() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    assert_eq!(scan_cached(d.path()).unwrap()[0].title, "POKEMON EMER");

    let cache = d.path().join(".userdata/library.cache");
    let text = std::fs::read_to_string(&cache).expect("no cache written");
    std::fs::write(&cache, text.replace("POKEMON EMER", "FROM CACHE")).unwrap();

    assert_eq!(scan_cached(d.path()).unwrap()[0].title, "FROM CACHE");
}

#[test]
fn a_rom_added_after_the_first_scan_is_shelved() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    assert_eq!(scan_cached(d.path()).unwrap().len(), 1);
    // Twelve bytes is the whole of the header title field, so this is a title rather than
    // the start of one.
    write_rom(&d, "Metroid.gba", "METROID");
    let carts = scan_cached(d.path()).unwrap();
    assert_eq!(carts.len(), 2);
    assert!(carts
        .iter()
        .any(|c| c.stem == "Metroid" && c.title == "METROID"));
}

#[test]
fn a_rom_removed_after_the_first_scan_leaves_the_shelf() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    write_rom(&d, "Metroid.gba", "METROID");
    assert_eq!(scan_cached(d.path()).unwrap().len(), 2);
    std::fs::remove_file(d.path().join("Games/Metroid.gba")).unwrap();
    let carts = scan_cached(d.path()).unwrap();
    assert_eq!(carts.len(), 1);
    assert_eq!(carts[0].stem, "Emerald");
}

#[test]
fn a_label_added_after_the_first_scan_is_picked_up() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    assert!(scan_cached(d.path()).unwrap()[0].label.is_none());
    write_png(&d, "Emerald.png");
    assert!(scan_cached(d.path()).unwrap()[0].label.is_some());
}

/// A cache that does not parse is a slow boot, not a failure to boot.
#[test]
fn a_corrupt_cache_is_a_full_scan_rather_than_an_error() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    std::fs::write(
        d.path().join(".userdata/library.cache"),
        b"\x00 not a cache at all",
    )
    .unwrap();
    let carts = scan_cached(d.path()).unwrap();
    assert_eq!(carts.len(), 1);
    assert_eq!(carts[0].title, "POKEMON EMER");
}

/// A cache missing a row would take a cart off the shelf even though nothing on the card
/// moved, so the row count is part of what makes it trustworthy: a `rows 2` header with one
/// row under it has to read as no cache rather than as a one cart shelf.
#[test]
fn a_cache_with_a_row_torn_out_is_a_full_scan() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    write_rom(&d, "Metroid.gba", "METROID FUSION");
    assert_eq!(scan_cached(d.path()).unwrap().len(), 2);

    let cache = d.path().join(".userdata/library.cache");
    let text = std::fs::read_to_string(&cache).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.pop();
    std::fs::write(&cache, lines.join("\n")).unwrap();

    assert_eq!(scan_cached(d.path()).unwrap().len(), 2);
}

/// A card whose games are named with a title that itself carries the cache's own separators
/// must round trip: a rom title is not ours to assume printable.
#[test]
fn a_title_with_a_separator_in_it_survives_the_cache() {
    let d = tmp_root();
    write_rom(&d, "Odd.gba", "ODD\tTITLE");
    assert_eq!(scan_cached(d.path()).unwrap()[0].title, "ODD\tTITLE");
    assert_eq!(scan_cached(d.path()).unwrap()[0].title, "ODD\tTITLE");
}

/// The letter ring files a cart under the name the shelf shows, which is the file name — never
/// the header title. The header is twelve characters of the publisher's language, so a card of
/// Chinese file names filed under it puts `POKEMON RUBY` and `ROCKMANZERO` on the ring and a
/// press of the dial lands on a letter with nothing under it.
#[test]
fn the_ring_files_a_cart_under_its_file_name_and_not_its_header() {
    let d = tmp_root();
    // A rom's header title says one thing and the file name another, which is the whole case.
    write_rom(&d, "宝可梦-红宝石 (CN).gba", "POKEMON RUBY");
    // Bracketed tags are not part of the name, and the letter has to see through them: this
    // cart is an `L`, not a `#`.
    write_rom(&d, "[GBA] 洛克人Zero1.gba", "ROCKMANZERO");
    write_rom(&d, "恶魔城-晓月圆舞曲.gba", "AKUMAJO");

    let carts = scan(d.path()).unwrap();
    // Grouped by letter, which is the shelf's order.
    let filed: Vec<(&str, char)> = carts.iter().map(|c| (c.stem.as_str(), c.initial)).collect();
    assert_eq!(
        filed,
        vec![
            ("宝可梦-红宝石 (CN)", 'B'),
            ("恶魔城-晓月圆舞曲", 'E'),
            ("[GBA] 洛克人Zero1", 'L'),
        ]
    );
    // And the header is still carried, for the things that want it.
    assert!(carts.iter().any(|c| c.title == "ROCKMANZERO"));
}

/// The same cart through the cache: `initial` is derived rather than stored, so it has to be
/// derived the same way from a row as from a rom. A cache that answered with a different letter
/// than the scan that wrote it would put the ring a boot out of step with the shelf.
#[test]
fn the_cache_files_a_cart_under_the_same_letter_as_the_scan() {
    let d = tmp_root();
    write_rom(&d, "宝可梦-红宝石 (CN).gba", "POKEMON RUBY");
    write_rom(&d, "[GBA] 洛克人Zero1.gba", "ROCKMANZERO");

    let first = scan_cached(d.path()).unwrap();
    let again = scan_cached(d.path()).unwrap();
    let letters = |cs: &[slot_store::Cart]| -> Vec<char> { cs.iter().map(|c| c.initial).collect() };
    assert_eq!(
        letters(&first),
        vec!['B', 'L'],
        "the first scan is already wrong"
    );
    assert_eq!(
        letters(&again),
        letters(&first),
        "the cache changed the letters"
    );
}

/// The shelf is grouped by letter, because the ring above it is an index and an index whose
/// entries are out of order is not one. Hanzi in code point order are not in pinyin order, so
/// the file name alone leaves the B carts scattered down the row and stepping the dial to C
/// lands behind the user.
#[test]
fn the_shelf_is_grouped_by_letter() {
    let d = tmp_root();
    // Written in an order that is neither: 宝 (B) sorts before 恶 (E) before 洛 (L) by code
    // point, and the file names below are handed over in the third of those orders.
    write_rom(&d, "恶魔城-晓月圆舞曲.gba", "AKUMAJO");
    write_rom(&d, "宝可梦-红宝石.gba", "POKEMON");
    write_rom(&d, "洛克人Zero1.gba", "ROCKMAN");
    write_rom(&d, "超级机器人大战A.gba", "SRW A");
    write_rom(&d, "宝可梦-蓝宝石.gba", "POKEMON");
    write_rom(&d, "(J) 洛克人Zero2.gba", "ROCKMAN");

    let carts = scan(d.path()).unwrap();
    let filed: Vec<(char, &str)> = carts.iter().map(|c| (c.initial, c.stem.as_str())).collect();
    assert_eq!(
        filed,
        vec![
            ('B', "宝可梦-红宝石"),
            ('B', "宝可梦-蓝宝石"),
            ('C', "超级机器人大战A"),
            ('E', "恶魔城-晓月圆舞曲"),
            ('L', "(J) 洛克人Zero2"),
            ('L', "洛克人Zero1"),
        ],
        "the shelf is not grouped by letter, or not by file name inside a letter"
    );
    // Non-decreasing, which is the property the dial's up and down depend on: down is always
    // forwards along the row and up is always back.
    assert!(carts.windows(2).all(|w| w[0].initial <= w[1].initial));
}

#[test]
fn library_cache_is_written_under_profile_userdata() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");

    assert_eq!(scan_cached(d.path()).unwrap().len(), 1);
    assert!(
        d.path().join(".userdata/library.cache").is_file(),
        "cache was not written under profile-private userdata"
    );
    assert!(
        !d.path().join("System/library.cache").exists(),
        "new cache writes must not target a single-file bind mount in System/"
    );
}

/// **A rom removed from a subfolder of `Games/` must leave the shelf, and does not.**
///
/// `list_names` walks `Games/` recursively — the doc comment on it says so, and says why: a card
/// may file its games per machine (or per machine and language). The stamp, however, only reads
/// the mtime of the *top* `Games/` directory. So adding or removing a file one level down moves
/// the subfolder's mtime and leaves `Games/`'s exactly where it was, which is the one thing the
/// fast path in `scan_cached` takes for "nothing under here changed".
///
/// The result is a cart that is still on the shelf with no rom behind it: the shelf draws it,
/// and pressing A on it finds no file. It cannot be fixed by removing the cache — the next boot
/// rewrites it from the same stale stamp.
///
/// The four-second sleep is what makes the guard reachable: `Stamp::stands_alone` only lets a
/// stamp stand in for the names once it is `COARSENESS + SLACK` older than the cache file itself,
/// so the test has to put the cache's own mtime past the directory it is vouching for. On a real
/// card that gap is the time between copying the games and the first boot, which is why this
/// shows up as "the first scan was fine, then I deleted a game".
#[test]
fn a_rom_removed_from_a_subfolder_leaves_the_shelf() {
    let d = tmp_root();
    let sub = d.path().join("Games/GBA/中文");
    std::fs::create_dir_all(&sub).unwrap();
    write_rom(&d, "GBA/中文/Emerald.gba", "POKEMON EMER");

    // Long enough that the stamp on `Games/` is older than the cache written below.
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert_eq!(scan_cached(d.path()).unwrap().len(), 1);

    // The demotion the shelf has to survive: the rom goes, the folder it was in stays.
    std::fs::remove_file(sub.join("Emerald.gba")).unwrap();
    let carts = scan_cached(d.path()).unwrap();
    assert_eq!(
        carts.len(),
        0,
        "a rom removed from a subfolder is still on the shelf: {:?}",
        carts.iter().map(|c| c.stem.clone()).collect::<Vec<_>>()
    );
}

/// **The same gap, the other way round: a rom *added* two folders down has to reach the shelf.**
///
/// The delete case above shows the stamp has to cover the folder the change happens in; this is
/// the add case, which is the one a user meets when they copy a game into `Games/GBA/中文/` and
/// reboot: neither `Games/` nor `Labels/` moves, so a stamp of the two top directories answers
/// from the old rows and the new game is simply not there.
///
/// It also pins *why* the fast path is allowed to exist at all: the card is proven to be answered
/// from the cache (the title swap) before the rom is added, so a passing run is the fast path
/// being left, not the fast path never having been entered. The four seconds are the same ones
/// the delete case explains.
#[test]
fn a_rom_added_to_a_subfolder_after_the_first_scan_is_shelved() {
    let d = tmp_root();
    let sub = d.path().join("Games/GBA/中文");
    std::fs::create_dir_all(&sub).unwrap();
    write_rom(&d, "GBA/中文/Emerald.gba", "POKEMON EMER");

    std::thread::sleep(std::time::Duration::from_secs(4));
    assert_eq!(scan_cached(d.path()).unwrap().len(), 1);

    // Rewriting the title the cache holds leaves the header on disk saying something else, so a
    // run that answers `FROM CACHE` is a run that did not open a rom — the fast path, live.
    let cache = d.path().join(".userdata/library.cache");
    let text = std::fs::read_to_string(&cache).expect("no cache written");
    std::fs::write(&cache, text.replace("POKEMON EMER", "FROM CACHE")).unwrap();
    assert_eq!(
        scan_cached(d.path()).unwrap()[0].title,
        "FROM CACHE",
        "this card was not answered from the cache, so the case below is not the fast path"
    );

    write_rom(&d, "GBA/中文/Metroid.gba", "METROID");
    let carts = scan_cached(d.path()).unwrap();
    let stems: Vec<&str> = carts.iter().map(|c| c.stem.as_str()).collect();
    assert_eq!(
        stems,
        vec!["Emerald", "Metroid"],
        "a rom added to a subfolder never reached the shelf"
    );
    assert!(
        carts.iter().any(|c| c.title == "METROID"),
        "the new rom is on the shelf with its header unread"
    );
}

// ── The refresh row's scan (`rescan`) ───────────────────────────────────────────────────────
//
// A boot is allowed to be told "nothing changed"; the settings screen's refresh row is not.

/// The whole point: a cache that vouches for the card must not be able to answer this. The title
/// in the cache is false and the header on disk is true, so a run that comes back with the header
/// is a run that read the card rather than the file about it.
#[test]
fn a_rescan_reads_the_card_again_rather_than_trusting_the_cache() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    assert_eq!(scan_cached(d.path()).unwrap()[0].title, "POKEMON EMER");

    let cache = d.path().join(".userdata/library.cache");
    let text = std::fs::read_to_string(&cache).expect("no cache written");
    std::fs::write(&cache, text.replace("POKEMON EMER", "FROM CACHE")).unwrap();

    assert_eq!(
        rescan(d.path()).unwrap()[0].title,
        "POKEMON EMER",
        "rescan answered out of the cache, so the press would look like it did nothing"
    );
}

/// A rom written in place under its own name and its own timestamp is invisible to every
/// directory listing — the one thing the cache can never catch. Re-reading the headers is what
/// sees it, and that is what `trust_cache: false` buys.
#[test]
fn a_rescan_notices_a_rom_rewritten_under_its_own_name() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    assert_eq!(scan_cached(d.path()).unwrap()[0].title, "POKEMON EMER");

    // Same name, same directory, same length: only the bytes moved.
    write_rom(&d, "Emerald.gba", "STILL EMER");

    assert_eq!(
        rescan(d.path()).unwrap()[0].title,
        "STILL EMER",
        "the header was reused from the cache"
    );
}

/// And it leaves the card better off than it found it: the cache it writes is the truth it just
/// read, so the *next* boot does not have to walk again.
#[test]
fn a_rescan_leaves_a_cache_the_next_boot_can_stand_on() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "POKEMON EMER");
    write_rom(&d, "Metroid.gba", "METROID");
    assert_eq!(rescan(d.path()).unwrap().len(), 2);

    assert_eq!(scan_cached(d.path()).unwrap(), scan(d.path()).unwrap());
}

/// A refresh is a way *in*, not a way out: the shelf is a shelf afterwards, and the card it
/// describes is the one on disk. Empty is a shelf too, not an error.
#[test]
fn a_rescan_of_an_empty_card_is_an_empty_shelf() {
    let d = tmp_root();
    assert!(rescan(d.path()).unwrap().is_empty());
}

/// A rom filed in a machine folder is that machine's cart **whatever it is called**. The
/// extension is not read at all, which is the whole point: `.bin` is MD's, PCE's and PS1's, and
/// no reading of the extension can separate them while the folder separates all three free.
#[test]
fn the_folder_names_the_machine_and_the_extension_is_not_consulted() {
    let d = tmp_root();
    std::fs::create_dir_all(d.path().join("Games/SFC")).unwrap();
    // A SFC rom that happens to be called `.bin` — the extension every disc-based machine
    // shares — is still a Super Famicom cart.
    std::fs::write(d.path().join("Games/SFC/Chrono Trigger.bin"), [0u8; 8]).unwrap();
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 1, "a rom in Games/SFC did not reach the shelf");
    assert_eq!(
        carts[0].system(),
        slot_store::System::Sfc,
        "the extension overrode the folder"
    );
}

/// Every card written before the machine folders existed looks like this: the roms sit
/// directly in `Games/` and only the extension says which machine they are for. The folder is
/// the authority now, but dropping the extension fallback would empty those cards in one
/// upgrade, so it stays.
#[test]
fn a_rom_filed_directly_in_games_still_reads_off_its_extension() {
    let d = tmp_root();
    write_rom(&d, "Pokemon Emerald.gba", "POKEMON EMER");
    write_rom(&d, "Metroid Fusion.gba", "METROID");
    std::fs::write(d.path().join("Games/Zelda.smc"), [0u8; 8]).unwrap();
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 3);
    let by_stem: Vec<(&str, slot_store::System)> = carts
        .iter()
        .map(|c| (c.stem.as_str(), c.system()))
        .collect();
    assert!(by_stem.contains(&("Zelda", slot_store::System::Sfc)));
    assert!(by_stem.contains(&("Pokemon Emerald", slot_store::System::Gba)));
}

/// `NES` and `SNES` are the names people actually type, so they are accepted and mean the same
/// machines as `FC` and `SFC`. This is about *folders*: the overlay art still has exactly one
/// name per machine, and a `NES-*.png` would still not be found.
#[test]
fn a_regional_folder_name_names_the_same_machine() {
    let d = tmp_root();
    std::fs::create_dir_all(d.path().join("Games/NES")).unwrap();
    std::fs::create_dir_all(d.path().join("Games/SNES")).unwrap();
    std::fs::write(d.path().join("Games/NES/SMB.nes"), [0u8; 8]).unwrap();
    std::fs::write(d.path().join("Games/SNES/Secret of Mana.smc"), [0u8; 8]).unwrap();
    let carts = scan(d.path()).unwrap();
    let by_stem: Vec<(&str, slot_store::System)> = carts
        .iter()
        .map(|c| (c.stem.as_str(), c.system()))
        .collect();
    assert!(by_stem.contains(&("SMB", slot_store::System::Fc)), "{by_stem:?}");
    assert!(
        by_stem.contains(&("Secret of Mana", slot_store::System::Sfc)),
        "{by_stem:?}"
    );
}

/// The folder decides the machine, but it cannot decide whether the file is a rom at all —
/// `Games/GBA/readme.txt` is in the Game Boy Advance folder and is still a readme. Showing it
/// would put an unloadable "game" on the shelf.
#[test]
fn a_note_in_a_machine_folder_is_not_a_cart() {
    let d = tmp_root();
    std::fs::create_dir_all(d.path().join("Games/GBA")).unwrap();
    write_rom(&d, "GBA/Pokemon Emerald.gba", "POKEMON EMER");
    std::fs::write(
        d.path().join("Games/GBA/where-i-got-these.txt"),
        b"from a forum",
    )
    .unwrap();
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 1, "a readme reached the shelf as a game");
    assert_eq!(carts[0].stem, "Pokemon Emerald");
}

/// An archive is a rom where a core reads it itself and a note everywhere else: `zip` and `7z`
/// are deliberately *not* on the never-a-rom list, because `NEOGEO/foo.zip` is FBNeo's own
/// romset. A zip somewhere the frontend cannot unpack yet is a load error, not a reason to
/// hide it from the shelf.
#[test]
fn an_archive_is_a_rom_even_outside_the_arcade_folders() {
    let d = tmp_root();
    std::fs::create_dir_all(d.path().join("Games/GBA")).unwrap();
    std::fs::write(d.path().join("Games/GBA/Mother 3.zip"), b"PK\x03\x00fan").unwrap();
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 1, "a zip in Games/GBA was hidden from the shelf");
    assert_eq!(carts[0].system(), slot_store::System::Gba);
}

/// ★★ **2026-10-25：`md` and `ps1` are also file extensions of other things.**
///
/// `.md` is Mega Drive and it is also a Markdown note; `.ps1` is PlayStation and it is also a
/// PowerShell script. `NOT_A_ROM` lists both as "never a cart", which would have made a
/// `Games/MD/Sonic.md` **invisible** — the card would simply have had no Mega Drive shelf, and
/// nothing would have said why. The rule that fixes it is the one the folder table already
/// makes: **inside a machine's own folder the extension is not consulted at all**, so the
/// never-a-rom list only applies outside those folders.
///
/// This test is the pair of assertions that would have caught that bug: the same file name is a
/// cart in `Games/MD/` and not a cart anywhere else.
#[test]
fn an_extension_that_is_also_a_document_is_a_cart_only_inside_its_machine_folder() {
    use slot_store::System;
    let d = tmp_root();
    std::fs::create_dir_all(d.path().join("Games/MD")).unwrap();
    std::fs::create_dir_all(d.path().join("Games/PS1")).unwrap();
    // Filed as a machine's own rom: shelved, and as that machine.
    std::fs::write(d.path().join("Games/MD/Sonic.md"), [0u8; 8]).unwrap();
    std::fs::write(d.path().join("Games/PS1/Ridge.cue"), b"FILE \"Ridge.bin\" BINARY\n").unwrap();
    // The same two extensions loose in `Games/`: documents, not carts. Without this half the
    // scan would shelve every Markdown note and every PowerShell script on the card.
    std::fs::write(d.path().join("Games/readme.md"), b"# notes\n").unwrap();
    std::fs::write(d.path().join("Games/build.ps1"), b"Write-Host hi\n").unwrap();

    let carts = scan(d.path()).unwrap();
    let by_stem: Vec<(&str, System)> =
        carts.iter().map(|c| (c.stem.as_str(), c.system())).collect();
    // ★ **Alphabetical, not the order the folders were written.** `order()` sorts by initial
    // then stem, and `Ridge` sorts before `Sonic` — this expectation used to be written in
    // write order (MD first, PS1 second) and failed the first time it ran on the device
    // (2026-10-25). What it pins is the *pair* and the *machines*, so compare that as a set
    // and let the shelf's own ordering be somebody else's test. Sorted by stem because
    // `System` is deliberately not `Ord` — it is a machine, not a value with an order.
    let mut expected: Vec<(&str, System)> = vec![("Sonic", System::Md), ("Ridge", System::Ps1)];
    let mut got = by_stem.clone();
    expected.sort_by(|a, b| a.0.cmp(b.0));
    got.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(
        got, expected,
        "the two machine-folder carts are not the two that reached the shelf"
    );
    assert_eq!(
        by_stem,
        vec![("Ridge", System::Ps1), ("Sonic", System::Md)],
        "the shelf's own order moved: it is alphabetical by initial, not by folder"
    );
}

/// `dir_name` and `from_dir` are two directions of one table, and this is what stops them
/// drifting: a machine whose folder cannot be read back is a machine whose labels, overlays
/// and saved settings all land in the wrong place.
#[test]
fn every_machines_folder_name_reads_back_as_itself() {
    use slot_store::System;
    for s in [
        System::Gba,
        System::Gb,
        System::Gbc,
        System::Sfc,
        System::Fc,
        System::Neogeo,
        System::Arcade,
        // ★ 2026-10-25：MD 与 PS1 的文件夹。**这一格必须跟着 `DIRS` 一起长** —— 漏掉一个
        //   机种，它的标签/遮罩/存档就会落到别的机种目录里，而且 `from_dir` 读不回来时
        //   卡里的 rom 会退到按扩展名判种，`.md` 那种撞车的文件名就直接判不出来了。
        System::Md,
        System::Ps1,
    ] {
        assert_eq!(
            System::from_dir(s.dir_name()),
            Some(s),
            "{} does not read back",
            s.dir_name()
        );
    }
    // An unknown folder names no machine, so a rom in one falls through to its extension.
    assert_eq!(System::from_dir("MyStuff"), None);
    // And the match is without case, because the card was written somewhere else.
    assert_eq!(System::from_dir("neogeo"), Some(System::Neogeo));
    assert_eq!(System::from_dir("gBa"), Some(System::Gba));
}
