mod common;

use common::tmp_root;
use slot_store::{
    atomic_write, read_slot_state, write_slot_state, ArtSource, Mode, SlotState,
    ART_SOURCE_DEFAULT, BRIGHTNESS_MAX, DOZE_MIN_CHOICES, DOZE_MIN_DEFAULT, FF_SOUND_DEFAULT,
    FF_SPEED_CHOICES, FF_SPEED_DEFAULT, LOOK_MAX, REWIND_SPEED_CHOICES, REWIND_SPEED_DEFAULT,
    SHELF_BADGE_DEFAULT,
};
use tempfile::tempdir;

#[test]
fn atomic_write_leaves_no_partial_file_and_no_temp_behind() {
    let d = tempdir().unwrap();
    let p = d.path().join("x.bin");
    atomic_write(&p, b"first").unwrap();
    atomic_write(&p, &vec![7u8; 4_000_000]).unwrap();
    assert_eq!(std::fs::read(&p).unwrap().len(), 4_000_000);
    let strays: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() != "x.bin")
        .collect();
    assert!(strays.is_empty(), "temp files left behind: {strays:?}");
}

#[test]
fn corrupt_slot_state_reads_as_default_rather_than_panicking() {
    let d = tmp_root();
    std::fs::write(d.path().join("System/slot.state"), b"\x00\xff not json").unwrap();
    assert_eq!(read_slot_state(d.path()), SlotState::default());
}

#[test]
fn slot_state_round_trips_including_a_stem_with_an_equals_sign() {
    let d = tmp_root();
    let s = SlotState {
        cart: Some("Cheats = On".into()),
        brightness: 3,
        blue_light: 9,
        volume: 71,
        muted: true,
        clock_set: true,
        utc_offset_min: 0,
        mode: Mode::Light,
        look: 4,
        doze_min: 10,
        // Off the defaults on purpose: a round trip that only ever carries the fallback value
        // cannot tell "the line survived" from "the line was missing".
        shelf_badge: false,
        ff_speed: 3,
        rewind_speed: 4,
        // Heard while speeding, which is not what this build has always done - the same
        // "off the defaults on purpose" the badge above is set to.
        ff_sound: true,
        favorites: Default::default(),
        keymap: Default::default(),
        keymaps: Default::default(),
        keymap_mode: false,
        turbo_hz: slot_store::TURBO_HZ_DEFAULT,
        ui_sfx: slot_store::UI_SFX_DEFAULT,
        art_source: slot_store::ART_SOURCE_DEFAULT,
    };
    write_slot_state(d.path(), &s).unwrap();
    assert!(
        d.path().join(".userdata/slot.state").is_file(),
        "mutable Slot state must be written below the directory-mounted userdata root"
    );
    assert!(
        !d.path().join("System/slot.state").exists(),
        "the legacy System/slot.state path must not be used for new atomic writes"
    );
    assert_eq!(read_slot_state(d.path()), s);
}

/// Existing cards still read their old System/slot.state, and the next write moves the
/// complete settings record into the profile-private userdata directory.
#[test]
fn legacy_system_slot_state_migrates_on_the_next_write() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\n",
    )
    .unwrap();

    let mut state = read_slot_state(d.path());
    assert_eq!(state.utc_offset_min, 120);
    state.volume = 73;
    write_slot_state(d.path(), &state).unwrap();

    let private = std::fs::read_to_string(d.path().join(".userdata/slot.state")).unwrap();
    assert!(private.contains("utc_offset_min=120"));
    assert!(private.contains("volume=73"));
    assert_eq!(read_slot_state(d.path()), state);
}

#[test]
fn profile_private_state_takes_precedence_over_legacy_shared_state() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=\nbrightness=7\nblue_light=2\nvolume=44\nmuted=0\nclock_set=1\nutc_offset_min=120\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.path().join(".userdata")).unwrap();
    std::fs::write(
        d.path().join(".userdata/slot.state"),
        "cart=\nbrightness=7\nblue_light=2\nvolume=44\nmuted=0\nclock_set=1\nutc_offset_min=-450\n",
    )
    .unwrap();

    assert_eq!(read_slot_state(d.path()).utc_offset_min, -450);
}

/// The mode is the one field a card written before it existed can be missing, so it is the one
/// field whose absence has to read as something other than corruption. Everything else about
/// such a file is still strict — see `a_slot_state_missing_a_key_reads_as_default_not_half_populated`
/// for the other half of that rule.
#[test]
fn a_state_file_from_before_the_mode_reads_as_dark_and_keeps_the_rest() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(s.mode, Mode::Dark, "an older card did not come up dark");
    assert_eq!(
        s.cart.as_deref(),
        Some("Emerald"),
        "the cart was thrown away"
    );
    assert_eq!(s.brightness, 7);
    assert_eq!(s.volume, 44);
    assert_eq!(s.utc_offset_min, 120);
}

/// And a mode this build cannot read is corruption like any other word it cannot read: the
/// strictness is there to catch a file someone has been editing, and a third mode is exactly
/// that.
#[test]
fn a_mode_that_is_not_a_mode_reads_as_default() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=0\nmode=sepia\n",
    )
    .unwrap();
    assert_eq!(read_slot_state(d.path()), SlotState::default());
}

#[test]
fn a_slot_state_missing_a_key_reads_as_default_not_half_populated() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=3\nblue_light=1\n",
    )
    .unwrap();
    assert_eq!(read_slot_state(d.path()), SlotState::default());
}

#[test]
fn an_out_of_range_level_reads_as_default() {
    let d = tmp_root();
    for body in [
        "cart=\nbrightness=20\nblue_light=1\nvolume=50\n",
        "cart=\nbrightness=3\nblue_light=10\nvolume=50\n",
        "cart=\nbrightness=3\nblue_light=1\nvolume=101\n",
    ] {
        std::fs::write(d.path().join("System/slot.state"), body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted {body:?}"
        );
    }
}

#[test]
fn a_first_boot_is_neither_dark_nor_silent() {
    let d = tmp_root();
    let s = read_slot_state(d.path());
    assert!(s.cart.is_none());
    assert!(s.brightness > 0, "boots with the backlight off");
    assert!(s.volume > 0, "boots muted");
}

/// The top of the ramp is a setting someone can pick, not one past the end of it. Nine was
/// the top until the scale doubled, and a card written back then still has to read as the
/// level it named rather than as a corrupt file.
#[test]
fn the_top_brightness_step_is_in_range() {
    let d = tmp_root();
    for value in [9, BRIGHTNESS_MAX] {
        std::fs::write(
            d.path().join("System/slot.state"),
            format!(
                "cart=\nbrightness={value}\nblue_light=0\nvolume=60\nmuted=0\nclock_set=1\nutc_offset_min=0\n"
            ),
        )
        .unwrap();
        assert_eq!(
            read_slot_state(d.path()).brightness,
            value,
            "step {value} should be a valid level"
        );
    }
}

/// The offset is what turns the card's UTC into the time on the shelf, so it has to outlive
/// the session that chose it.
#[test]
fn slot_state_round_trips_a_negative_utc_offset() {
    let d = tmp_root();
    let s = SlotState {
        cart: None,
        brightness: 5,
        blue_light: 0,
        volume: 60,
        muted: false,
        clock_set: true,
        utc_offset_min: -450,
        mode: Mode::Dark,
        look: 0,
        doze_min: 15,
        shelf_badge: SHELF_BADGE_DEFAULT,
        ff_speed: FF_SPEED_DEFAULT,
        rewind_speed: REWIND_SPEED_DEFAULT,
        ff_sound: FF_SOUND_DEFAULT,
        favorites: Default::default(),
        keymap: Default::default(),
        keymaps: Default::default(),
        keymap_mode: false,
        turbo_hz: slot_store::TURBO_HZ_DEFAULT,
        ui_sfx: slot_store::UI_SFX_DEFAULT,
        art_source: slot_store::ART_SOURCE_DEFAULT,
    };
    write_slot_state(d.path(), &s).unwrap();
    assert_eq!(read_slot_state(d.path()).utc_offset_min, -450);
}

/// Half hour zones are real and whole hour steps would put several countries permanently
/// thirty minutes out.
#[test]
fn an_offset_outside_the_range_of_real_zones_reads_as_default() {
    let d = tmp_root();
    for body in [
        "cart=\nbrightness=5\nblue_light=0\nvolume=60\nmuted=0\nclock_set=1\nutc_offset_min=900\n",
        "cart=\nbrightness=5\nblue_light=0\nvolume=60\nmuted=0\nclock_set=1\nutc_offset_min=-780\n",
    ] {
        std::fs::write(d.path().join("System/slot.state"), body).unwrap();
        assert_eq!(read_slot_state(d.path()), SlotState::default(), "{body}");
    }
}

/// ★ `doze_min` is the third exception in `parse` (see its doc), for the same reason as `mode`:
/// every card in the field carries a state file written before the lid timer could be set, and
/// reading that missing line as corruption would throw away the cart the user left selected and
/// the brightness they chose. It has to come up on `DOZE_MIN_DEFAULT` — the wait a card that has
/// never been asked uses — rather than on anything the file happens to say.
#[test]
fn a_state_file_from_before_the_doze_setting_waits_the_default_and_keeps_the_rest() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\nmode=light\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(
        s.doze_min, DOZE_MIN_DEFAULT,
        "an older card did not come up on the default wait"
    );
    assert_eq!(s.doze_min, 5, "the default wait moved");
    assert_eq!(
        s.cart.as_deref(),
        Some("Emerald"),
        "the cart was thrown away"
    );
    assert_eq!(s.brightness, 7);
    assert_eq!(s.mode, Mode::Light, "the mode line was ignored");
}

/// A value that is *present* and not one of the four is corruption, not "fall back to the
/// default". The difference is between a file that is merely older and one someone has been
/// editing, and a hand-typed number would end in a `poweroff` under the user's thumb — which is
/// the whole reason the setting is a list rather than a number.
///
/// ★ The retired `3 / 10 / 30 / 60` belong on this list as much as the nonsense does: 2026-10-10
/// replaced them with `0 / 5 / 10 / 15`, so a card still carrying an old value has to read as the
/// default. (`10` is *not* on the list — it survived the change.)
#[test]
fn a_doze_min_that_is_not_a_choice_reads_as_default() {
    let d = tmp_root();
    for value in ["1", "3", "4", "7", "30", "60", "180", "", "sixty"] {
        let body = format!(
            "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=0\nmode=dark\ndoze_min={value}\n"
        );
        std::fs::write(d.path().join("System/slot.state"), &body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted doze_min={value:?}"
        );
    }
}

/// All four lengths of walk-away have to survive the file, not just the one a round-trip test
/// happened to pick: the choice *is* the setting, so a choice that comes back as something
/// else is one the user cannot make.
#[test]
fn slot_state_round_trips_each_doze_choice() {
    let d = tmp_root();
    for minutes in DOZE_MIN_CHOICES {
        let s = SlotState {
            cart: None,
            brightness: 5,
            blue_light: 0,
            volume: 60,
            muted: false,
            clock_set: true,
            utc_offset_min: 0,
            mode: Mode::Dark,
            look: 0,
            doze_min: minutes,
            shelf_badge: SHELF_BADGE_DEFAULT,
            ff_speed: FF_SPEED_DEFAULT,
            rewind_speed: REWIND_SPEED_DEFAULT,
            ff_sound: FF_SOUND_DEFAULT,
            favorites: Default::default(),
            keymap: Default::default(),
            keymaps: Default::default(),
            keymap_mode: false,
            turbo_hz: slot_store::TURBO_HZ_DEFAULT,
            ui_sfx: slot_store::UI_SFX_DEFAULT,
            art_source: slot_store::ART_SOURCE_DEFAULT,
        };
        write_slot_state(d.path(), &s).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            s,
            "{minutes} minutes did not survive the round trip"
        );
    }
}

/// The default has to be one of the four, or a card that has never been asked would open on a
/// value the file then refuses to take back — the first write of an untouched card would turn
/// it into a corrupt one.
#[test]
fn the_doze_default_is_one_of_the_choices() {
    assert!(
        DOZE_MIN_CHOICES.contains(&DOZE_MIN_DEFAULT),
        "the default {DOZE_MIN_DEFAULT} is not among {DOZE_MIN_CHOICES:?}"
    );
}

/// The same rule for the two speed rows, and it carries more than the doze one does: their
/// defaults are **what this build already did** (four and two), so a card that has never been
/// to the settings screen must fast forward and rewind exactly as it did before the rows
/// existed. A default off the list would be a gear the row cannot print and the file will not
/// take back.
#[test]
fn the_speed_defaults_are_choices_and_are_the_old_behaviour() {
    for (default, choices) in [
        (FF_SPEED_DEFAULT, &FF_SPEED_CHOICES[..]),
        (REWIND_SPEED_DEFAULT, &REWIND_SPEED_CHOICES[..]),
    ] {
        assert!(
            choices.contains(&default),
            "the default {default} is not among {choices:?}"
        );
    }
    assert_eq!(
        FF_SPEED_DEFAULT, 4,
        "the fast forward default is the one this build already ran at"
    );
    assert_eq!(
        REWIND_SPEED_DEFAULT, 2,
        "the rewind default is one snapshot popped per present, which is 2x"
    );
}

/// ★ `ff_speed` and `rewind_speed` are the fourth and fifth exceptions in `parse` (see its
/// doc). A card written before the speed rows existed carries neither line, and reading that
/// as corruption would throw away the cart the user left selected, the brightness they chose
/// and the wait they set — all to protect two values that have exactly one honest reading.
#[test]
fn a_state_file_from_before_the_speed_rows_keeps_the_old_behaviour() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\nmode=light\ndoze_min=10\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(
        s.ff_speed, FF_SPEED_DEFAULT,
        "an older card did not come up on the fast forward it already had"
    );
    assert_eq!(
        s.rewind_speed, REWIND_SPEED_DEFAULT,
        "an older card did not come up on the rewind it already had"
    );
    assert_eq!(s.cart.as_deref(), Some("Emerald"), "the cart was thrown away");
    assert_eq!(s.brightness, 7);
    assert_eq!(s.doze_min, 10, "the wait they set was ignored");
}

/// A speed *present* and off its list is corruption, not a fall back to the default — the same
/// rule the wait and the mode are held to. A hand-typed multiplier would otherwise hand the
/// emu worker a step count nothing measured and nothing drew.
#[test]
fn a_speed_that_is_not_a_choice_reads_as_default() {
    let d = tmp_root();
    for (key, value) in [
        ("ff_speed", "1"),
        ("ff_speed", "8"),
        ("ff_speed", "0"),
        ("ff_speed", ""),
        ("ff_speed", "four"),
        ("rewind_speed", "1"),
        ("rewind_speed", "3"),
        ("rewind_speed", "16"),
        ("rewind_speed", ""),
    ] {
        let body = format!(
            "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=0\nmode=dark\ndoze_min=10\n{key}={value}\n"
        );
        std::fs::write(d.path().join("System/slot.state"), &body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted {key}={value:?}"
        );
    }
}

/// Every gear has to survive the file, not just the one a round trip happened to pick: the
/// choice *is* the setting, so a gear that comes back as another one is a gear the user cannot
/// choose.
#[test]
fn slot_state_round_trips_each_speed_choice() {
    let d = tmp_root();
    for (ff, rewind) in FF_SPEED_CHOICES
        .iter()
        .flat_map(|ff| REWIND_SPEED_CHOICES.iter().map(move |rw| (*ff, *rw)))
    {
        let s = SlotState {
            cart: None,
            brightness: 5,
            blue_light: 0,
            volume: 60,
            muted: false,
            clock_set: true,
            utc_offset_min: 0,
            mode: Mode::Dark,
            look: 0,
            doze_min: DOZE_MIN_DEFAULT,
            shelf_badge: SHELF_BADGE_DEFAULT,
            ff_speed: ff,
            rewind_speed: rewind,
            ff_sound: FF_SOUND_DEFAULT,
            favorites: Default::default(),
            keymap: Default::default(),
            keymaps: Default::default(),
            keymap_mode: false,
            turbo_hz: slot_store::TURBO_HZ_DEFAULT,
            ui_sfx: slot_store::UI_SFX_DEFAULT,
            art_source: slot_store::ART_SOURCE_DEFAULT,
        };
        write_slot_state(d.path(), &s).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            s,
            "{ff}x fast forward / {rewind}x rewind did not survive the round trip"
        );
    }
}

/// ★ `shelf_badge` is the sixth exception in `parse` (see its doc), and it is the one whose
/// fallback is a *behaviour* rather than a number.
///
/// ⚠️ **The fallback is `false`, not `true` — reversed on 2026-10-23 (六六).** This test used to
/// assert the badge came up *shown*, on the reasoning that the corner had worn one since the
/// shelf existed. That was the old design; the badge was meant to come off, the drawing code was
/// kept rather than orphaned, and `SHELF_BADGE_DEFAULT` was flipped to `false` (see its own doc).
/// What has to hold is the *shape* of the rule, not the old value: a card written before the
/// setting comes up exactly the way a fresh one does, and everything else it says is kept.
#[test]
fn a_state_file_from_before_the_badge_setting_keeps_the_rest() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\nmode=light\ndoze_min=10\nff_speed=3\nrewind_speed=4\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(
        s.shelf_badge, SHELF_BADGE_DEFAULT,
        "an older card did not come up the way a fresh one does"
    );
    assert!(
        !s.shelf_badge,
        "the badge's default is *hidden* since 2026-10-23 — the drawing code is kept but off"
    );
    assert_eq!(s.cart.as_deref(), Some("Emerald"), "the cart was thrown away");
    assert_eq!(s.brightness, 7);
    assert_eq!(s.ff_speed, 3, "the speed rows they set were ignored");
    assert_eq!(s.mode, Mode::Light, "the mode line was ignored");
}

/// Both ends of the switch have to survive the file. A one-way round trip — whichever end the
/// test happened to pick — cannot tell "hidden survived" from "hidden was never written".
#[test]
fn slot_state_round_trips_the_badge_both_ways() {
    let d = tmp_root();
    for wanted in [true, false] {
        let s = SlotState {
            shelf_badge: wanted,
            ..SlotState::default()
        };
        write_slot_state(d.path(), &s).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            s,
            "a badge set to {wanted} did not survive the round trip"
        );
    }
}

/// The look is the seventh field a card written before it existed can be missing, and its
/// fallback is zero for the same reason `mode`'s is dark: zero is the device every card in the
/// field already is, so an older card comes up on the palette it was left on rather than on a
/// ramp it never chose.
#[test]
fn a_state_file_from_before_the_look_reads_as_the_device_and_keeps_the_rest() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\nmode=light\ndoze_min=10\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(s.look, 0, "an older card did not come up on the device's own colours");
    assert_eq!(s.cart.as_deref(), Some("Emerald"), "the cart was thrown away");
    assert_eq!(s.brightness, 7);
    assert_eq!(s.mode, Mode::Light, "the mode line was ignored");
}

/// Every arm of the row has to survive the file, not just the one a round trip happened to
/// pick: the choice *is* the setting, so an arm that comes back as another one is an arm the
/// user cannot choose.
#[test]
fn slot_state_round_trips_every_look() {
    let d = tmp_root();
    for wanted in 0..=LOOK_MAX {
        let s = SlotState {
            look: wanted,
            ..SlotState::default()
        };
        write_slot_state(d.path(), &s).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            s,
            "look {wanted} did not survive the round trip"
        );
    }
}

/// A look that is present and past the end of the row is corruption, the same rule the mode's
/// word and the badge's flag are held to. It is not "fall back to the default": a file someone
/// has been editing is not a file we wrote.
#[test]
fn a_look_past_the_end_of_the_row_reads_as_default() {
    let d = tmp_root();
    for value in ["7", "8", "-1", "yes", ""] {
        let body = format!(
            "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=0\nmode=dark\nlook={value}\n"
        );
        std::fs::write(d.path().join("System/slot.state"), &body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted look={value:?}"
        );
    }
}

/// And the end of the row survives, which is the boundary the "past the end" loop above is one
/// step outside of.
#[test]
fn the_last_look_is_a_choice_not_a_corruption() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=0\nmode=dark\nlook=6\n",
    )
    .unwrap();
    assert_eq!(read_slot_state(d.path()).look, 6);
}

/// A value that is *present* and is not 0 or 1 is corruption, not "fall back to the default" —
/// the same rule `muted` and `clock_set` are held to. The badge is a flag, and a file someone has
/// been editing is not a file we wrote.
#[test]
fn a_shelf_badge_that_is_not_a_level_reads_as_default() {
    let d = tmp_root();
    for value in ["2", "-1", "yes", "true", "on", ""] {
        let body = format!(
            "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=0\nmode=dark\ndoze_min=10\nshelf_badge={value}\n"
        );
        std::fs::write(d.path().join("System/slot.state"), &body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted shelf_badge={value:?}"
        );
    }
}

/// ★ `ff_sound` is the seventh exception in `parse` (see its doc), and its fallback is a
/// *behaviour* rather than a number: the worker has dropped the core's audio while speeding since
/// the beginning. A card written before the setting therefore has to come up **silent** —
/// coming up noisy would be this build quietly making a sound nobody asked it to make, the mirror
/// of the badge coming up hidden.
#[test]
fn a_state_file_from_before_the_ff_sound_setting_is_silent_and_keeps_the_rest() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\nmode=light\ndoze_min=10\nff_speed=3\nrewind_speed=4\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(
        s.ff_sound, FF_SOUND_DEFAULT,
        "an older card did not come up speeding the way it always has"
    );
    assert!(
        !s.ff_sound,
        "the default is *silent* — speeding has never made a sound in this build"
    );
    assert_eq!(s.cart.as_deref(), Some("Emerald"), "the cart was thrown away");
    assert_eq!(s.brightness, 7);
    assert_eq!(s.ff_speed, 3, "the speed rows they set were ignored");
    assert_eq!(s.mode, Mode::Light, "the mode line was ignored");
}

/// Both ends of the switch have to survive the file. A one-way round trip — whichever end
/// the test happened to pick — cannot tell "heard survived" from "heard was never written".
#[test]
fn slot_state_round_trips_the_ff_sound_both_ways() {
    let d = tmp_root();
    for wanted in [true, false] {
        let s = SlotState {
            ff_sound: wanted,
            ..SlotState::default()
        };
        write_slot_state(d.path(), &s).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            s,
            "fast forward sound set to {wanted} did not survive the round trip"
        );
    }
}

/// A value that is *present* and is not 0 or 1 is corruption, not "fall back to the default" —
/// the same rule the badge's flag is held to. The sound is a flag, and a file someone has been
/// editing is not a file we wrote.
#[test]
fn an_ff_sound_that_is_not_a_level_reads_as_default() {
    let d = tmp_root();
    for value in ["2", "-1", "yes", "true", "on", ""] {
        let body = format!(
            "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=0\nmode=dark\ndoze_min=10\nff_sound={value}\n"
        );
        std::fs::write(d.path().join("System/slot.state"), &body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted ff_sound={value:?}"
        );
    }
}

/// ★ 2026-10-23：美术资源开关。一张在这个设置存在之前写的卡没有 `art_source` 行 ——
/// 读作默认（整体贴图优先），其余字段一个不丢，和 mode / doze 同一条规矩。
#[test]
fn a_state_file_from_before_the_art_source_reads_as_whole_and_keeps_the_rest() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=1\nclock_set=1\nutc_offset_min=120\nmode=light\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(
        s.art_source,
        ART_SOURCE_DEFAULT,
        "an older card did not come up on the default source"
    );
    assert_eq!(s.art_source, ArtSource::Whole, "the default moved");
    assert_eq!(
        s.cart.as_deref(),
        Some("Emerald"),
        "the cart was thrown away"
    );
}

/// 两个词都要过得了文件（大小写与空白不计），而一个不认识的词是 corruption 而不是
/// "读作默认" —— 和 mode 的同一条规矩：文件旧和文件被人改过是两件事。
#[test]
fn the_art_source_round_trips_both_words_and_rejects_a_third() {
    let d = tmp_root();
    for word in ["whole", "sticker", "WHOLE", " sticker "] {
        let body = format!(
            "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=0\nclock_set=1\nutc_offset_min=0\nmode=dark\nart_source={word}\n"
        );
        std::fs::write(d.path().join("System/slot.state"), &body).unwrap();
        let want = if word.trim().to_ascii_lowercase() == "whole" {
            ArtSource::Whole
        } else {
            ArtSource::Sticker
        };
        assert_eq!(
            read_slot_state(d.path()).art_source,
            want,
            "art_source={word:?} did not read back"
        );
    }
    for word in ["both", "label", "", "0", "1"] {
        let body = format!(
            "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=0\nclock_set=1\nutc_offset_min=0\nmode=dark\nart_source={word}\n"
        );
        std::fs::write(d.path().join("System/slot.state"), &body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted art_source={word:?}"
        );
    }
}

/// ★★ 2026-10-26：**操作键表是每台机种一份，而老卡那张全局表照旧读得出来。**
///
/// 两件事一次验：新拼法 `keymap_<SYS>_<PHYS>` 能回圈，而 `keymap_<PHYS>`（本轮之前每一个二进制
/// 写的那种）仍然当全局兜底读 —— 升级不能把玩家的键位弄丢。
#[test]
fn the_keymap_is_per_machine_and_the_old_global_table_still_reads() {
    let d = tmp_root();
    let mut s = SlotState { cart: Some("Emerald".to_string()), ..SlotState::default() };
    // The old card: one global line, Y = A, written by every build up to this one.
    s.keymap.insert("Y".to_string(), "GAME_A".to_string());
    // And the new one: a PlayStation with two rows of its own.
    let mut ps1 = std::collections::BTreeMap::new();
    ps1.insert("X".to_string(), "GAME_A".to_string());
    ps1.insert("Y".to_string(), "GAME_X".to_string());
    s.keymaps.insert("PS1".to_string(), ps1);
    write_slot_state(d.path(), &s).unwrap();

    let back = read_slot_state(d.path());
    assert_eq!(back.keymap.get("Y").map(String::as_str), Some("GAME_A"));
    let ps1 = back.keymaps.get("PS1").expect("PS1 rows did not survive the round trip");
    assert_eq!(ps1.get("X").map(String::as_str), Some("GAME_A"));
    assert_eq!(ps1.get("Y").map(String::as_str), Some("GAME_X"));
    assert!(back.keymaps.get("GBA").is_none(), "a machine nobody touched was invented");
}

/// 手写的卡也一样：两种拼法写在同一份文件里互不干扰，而一个认不出的机种名不会把
/// 整份状态当成损坏（它只是一行读不到的物理键名）。
#[test]
fn a_hand_written_state_file_carries_both_keymap_spellings() {
    let d = tmp_root();
    let body = "cart=Emerald\nbrightness=7\nblue_light=2\nvolume=44\nmuted=0\nclock_set=1\nutc_offset_min=0\nmode=dark\nkeymap_Y=GAME_A\nkeymap_PS1_X=GAME_X\nkeymap_ZZ_A=GAME_B\n";
    std::fs::write(d.path().join("System/slot.state"), body).unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(s.keymap.get("Y").map(String::as_str), Some("GAME_A"));
    assert_eq!(
        s.keymaps.get("PS1").and_then(|m| m.get("X")).map(String::as_str),
        Some("GAME_X")
    );
    // `ZZ` is not a folder this build ships, so that line stays a physical-key name nobody
    // asks about rather than becoming a machine: the file still reads.
    assert!(s.keymaps.get("ZZ").is_none());
    assert_eq!(s.keymap.get("ZZ_A").map(String::as_str), Some("GAME_B"));
}
