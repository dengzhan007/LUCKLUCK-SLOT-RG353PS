mod arcade_names;
mod atomic;
mod core;
mod gba;
mod migrate;
mod name;
mod paging;
mod pinyin;
mod ring;
mod rom;
mod sav;
mod scan;
pub mod search;
mod slot_state;
mod slots;
mod wifi;
mod stamp;
mod theme;

pub use arcade_names::{
    init as init_arcade_names, len as arcade_name_count, lookup as arcade_name,
    shown as shown_name, OVERRIDE_FILE as ARCADE_NAMES_FILE,
};
pub use atomic::atomic_write;
pub use core::{core_for, read_selected_cores, write_selected_core, Core, SELECTED_CORE_FILE};
pub use gba::{
    header, header_code, header_title, read_header, sgb_enhanced, CGB_DUAL, CGB_ONLY,
};
pub use migrate::{migrate_states, MigrationReport};
pub use name::{clean_label, clean_label_stripped, init_label_config, label_tags, set_strip_tags};
pub use paging::{cycle_on_page, index_of, len_on, pages, step_cursor, step_page};
pub use pinyin::{initial, letter_of};
pub use ring::{StateEntry, StateRing, RING_MAX};
pub use rom::{is_zip, read_rom, read_rom_named, RomError};
pub use sav::{describe, is_rzip, save_plan, SavePlan, BLANK};
pub use slots::{Slot, SlotEntry, StateSlots, SLOT_COUNT, SLOT_MAX};
pub use wifi::{unescape_ssid, WifiConf, WIFI_FILE};
pub use scan::{is_hidden, rescan, scan, scan_cached, Cart, StoreError, System};
pub use slot_state::{
    read_slot_state, write_slot_state, ArtSource, Mode, SlotState, ART_SOURCE_DEFAULT,
    BLUE_LIGHT_MAX, BRIGHTNESS_MAX, DOZE_MIN_CHOICES, DOZE_MIN_DEFAULT, FF_SOUND_DEFAULT,
    FF_SPEED_CHOICES, FF_SPEED_DEFAULT, UI_SFX_DEFAULT, LOOK_MAX, REWIND_SPEED_CHOICES,
    REWIND_SPEED_DEFAULT, SHELF_BADGE_DEFAULT, TURBO_HZ_CHOICES, TURBO_HZ_DEFAULT,
    UTC_OFFSET_MAX, UTC_OFFSET_MIN, VOLUME_MAX, INSERT_RESUME_DEFAULT, EJECT_SAVE_DEFAULT,
};
pub use stamp::{
    civil_from_days, days_from_civil, days_in_month, format_stamp, parse_stamp, stamp_now,
};
pub use theme::{Theme, THEME_FILE};
