use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::atomic::atomic_write;

/// Twenty steps, and so nineteen of them lit: step 0 is the panel off, which is what the lid
/// closes with. Ten was too coarse at the bottom — step 1 sat well above what a dark room
/// wants, with nothing between it and black — so the ramp is finer rather than the top higher.
pub const BRIGHTNESS_MAX: u8 = 19;
pub const BLUE_LIGHT_MAX: u8 = 9;
pub const VOLUME_MAX: u8 = 100;

/// What a real zone can be, in minutes. The card keeps UTC because the base system's clock
/// and its ntp both assume it; this is the only thing that turns it into the time on the
/// shelf. Minutes rather than hours: several zones are offset by thirty and forty five.
pub const UTC_OFFSET_MIN: i16 = -720;
pub const UTC_OFFSET_MAX: i16 = 840;

/// How long the dark panel waits before the machine actually stops, in minutes — and the only
/// four values the card will take.
///
/// The dark is immediate: the lid or the POWER tap kills the backlight on the edge, and the
/// device is still running flat out behind it at 400–700 mA. What this is the length of is the
/// window in which the user might come straight back, not a power saving. It ends in a
/// `poweroff` rather than a sleep because this board cannot wake itself — the RTC alarm never
/// fires — so a standby would be a leak with no end.
///
/// **Four lengths of walk-away, not a number.** A minute value typed by hand would let a card
/// be set to five seconds and shut down under the user's thumb, and would be a fifth thing to
/// translate; the list is the setting.
///
/// ★ **2026-10-10: `0` joined the list, and it is the opposite of a wait.** The panel still
/// goes dark on a shut lid — that part is the lid's, not this setting's — but the machine
/// never stops by itself, which is what the screen calls 「仅息屏」 / "Screen Off Only". It
/// replaced the old `3 / 10 / 30 / 60`, so a card carrying one of those reads as the default:
/// the values *are* the setting, and a value this build no longer offers is the same thing as
/// any other word it cannot read (see `doze_choice`).
pub const DOZE_MIN_CHOICES: [u8; 4] = [0, 5, 10, 15];

/// The rates a turbo key repeats at, in Hz, in the order the operation keymap's foot row walks
/// them. GBA polling runs near 59.7 Hz, so the list stops at 15 rather than promising presses
/// the core cannot distinguish.
pub const TURBO_HZ_CHOICES: [u8; 5] = [2, 4, 8, 12, 15];

/// The rate a card that has never tuned the turbo rows repeats at: the middle of the list.
pub const TURBO_HZ_DEFAULT: u8 = 8;

/// What a card that has never been asked reads as, and what the setting opens on.
///
/// ★ 2026-10-10: five, not three. Three is no longer one of the choices, and five is the
/// shortest wait that still ends in a `poweroff` — a first boot must not sit awake behind a
/// shut lid for ever just because nobody has been to the settings screen yet.
pub const DOZE_MIN_DEFAULT: u8 = 5;

/// The multipliers the fast forward key runs at, in the order `←`/`→` walks them.
///
/// **Three, and four is the top: eight is not on this list on purpose.** Eight was measured on
/// the H700 and bought no speed at all — the worker is at 100% of one core either way, so a
/// step count it cannot serve only costs presents, and the picture fell from 60 Hz to about 30
/// at the same multiplier. A gear the hardware cannot serve is a gear that lies (see
/// `slot::emu::FAST_STEPS`).
///
/// ★ **2026-10-10 六六：「三档就好」, and the fast forward's set is one with the rewind's** —
/// same controls, same screen, decided in one pass rather than two.
pub const FF_SPEED_CHOICES: [u8; 3] = [2, 3, 4];

/// Four, which is what this build already did before the setting existed. A card that has never
/// been to the settings screen must fast forward exactly as it did, not slower.
pub const FF_SPEED_DEFAULT: u8 = 4;

/// The multipliers the rewind key runs at, in the order `←`/`→` walks them.
///
/// **Powers of two, and that is not a coincidence**: the history is one snapshot every other
/// frame, so a multiplier *is* a whole number of snapshots popped per present. Three would be
/// one and a half snapshots, which is not a thing the ring can do — the honest gears are the
/// powers of two, and the user asked for three of them.
///
/// The top one is cheap rather than fast: popping is a `lz4` decode of one delta, about a
/// hundredth of a core frame, so eight costs four decodes and the same single frame the
/// two-times does.
pub const REWIND_SPEED_CHOICES: [u8; 3] = [2, 4, 8];

/// Two, which is exactly what "one snapshot popped per present" already was. The setting adds
/// gears above the one this build had; it does not change the default behaviour.
pub const REWIND_SPEED_DEFAULT: u8 = 2;

/// Whether the shelf's bottom-left badge is printed when nobody has said otherwise.
///
/// Since 2026-10-29 (六六) the **machine** logos (GB / GBC / GBA) are removed from the drawing
/// code entirely — `app.rs::draw_shelf_logo` and the `shelf_ind_*` fields / setters /
/// `SHELF_IND_W`/`SHELF_IND_H` constants are gone. What `shelf_badge` still gates is the **disc
/// row's own "Disc" badge** (`draw_disc_logo`), the one thing left in that corner. `false` keeps
/// it hidden, matching 2026-10-23 六六's "the corner should be clean". Reversible: a card may
/// still set `shelf_badge=1`. A named constant rather than a literal in `Default` so the tests
/// that spell a whole `SlotState` out can point at it.
pub const SHELF_BADGE_DEFAULT: bool = false;

/// Whether fast forward and rewind keep playing the game's audio when nobody has said otherwise.
///
/// **`false`, because that is what this build has always done** — the same rule every other
/// default here follows. The worker has dropped the core's audio while speeding since the
/// beginning (resampling it down to real time is pitch-shifted noise), so a card written before
/// the setting existed has to come up silent, exactly as it ran. A named constant rather than a
/// literal in `Default` so the tests that spell a whole `SlotState` out can point at it.
pub const FF_SOUND_DEFAULT: bool = false;
/// ★ 2026-10-28：「插卡续玩」默认值。`true` = 这一版之前一直有的行为：插入 / 开机卡槽有卡时回读
/// resume 续玩。缺行按此走。`SHELF_BADGE_DEFAULT` 同款命名约定。
pub const INSERT_RESUME_DEFAULT: bool = true;
/// ★ 2026-10-28：「拔卡续存」默认值。`true` = 旧行为：拔卡把当前画面写进 `resume.state`。
/// 原生 `.sav` 不受此开关影响，`lid` / 关机 / 自动存档那条也不受。
pub const EJECT_SAVE_DEFAULT: bool = true;

/// ★ 2026-10-22：UI 提示音开关的默认值。开着是这一版会采用的默认（一旦素材到位，
/// 提示音就该默认响），但**素材未到位时开着也静默**，所以旧卡升上来不会有意外声响。
/// 与 `FF_SOUND_DEFAULT` 一样取具名常量，方便测试整份写出 `SlotState` 时引用。
pub const UI_SFX_DEFAULT: bool = true;

/// How many looks a mode has: the device's own palette, then six. The table itself lives in
/// `slot_ui::palette` — it is a drawing fact — so this is only the bound the state file is held
/// to on the way in.
pub const LOOK_MAX: u8 = 6;

/// Which way round the frontend prints itself: light type on dark plastic, or dark type on
/// light plastic.
///
/// It lives here rather than in the ui because it is a setting the card remembers, like the
/// brightness and the volume, and the ui reads its palette off whatever is in the state file.
/// A `Mode` owned by the drawing code would have to be mirrored here anyway, and two enums for
/// one line of text is how a file and a screen come apart.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

impl Mode {
    /// The other one. What SELECT+START does, and the whole of what it does: there is no third
    /// mode to step through, so a toggle is the honest shape of the control.
    pub fn other(self) -> Mode {
        match self {
            Mode::Dark => Mode::Light,
            Mode::Light => Mode::Dark,
        }
    }

    /// What the state file calls it. Spelled at the definition rather than at either end, so
    /// the reader and the writer cannot disagree about the word.
    pub fn word(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }

    /// Anything else is not a mode. A state file edited on a desktop must not be able to put
    /// the device into a third one.
    pub fn from_word(word: &str) -> Option<Mode> {
        match word.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Mode::Dark),
            "light" => Some(Mode::Light),
            _ => None,
        }
    }
}

/// Which of the two art sources wins when a cart carries **both**: the whole picture in
/// `Cartridge/`, or the sticker composited onto the shell from `Labels/`.
///
/// ★ 2026-10-23 六六：「美术资源 整体贴图/局部贴纸」—— the switch decides **priority, not
/// availability**. Both sources stay live whichever way it is set: a cart carrying only one
/// of the two is dressed from the one it has, and a cart carrying neither falls back to the
/// magenta-slot shell with the game name set in the slot, exactly as it always did. Only a
/// cart with both files asks the question this answers.
///
/// `Whole` is the default because `Cartridge/` is the new home and `Labels/` is what a cart
/// without one falls back to — and because a card that predates the folder is untouched
/// either way: an empty `Cartridge/` holds no picture to win with.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum ArtSource {
    #[default]
    Whole,
    Sticker,
}

impl ArtSource {
    /// What the state file calls it. Spelled at the definition rather than at either end, so
    /// the reader and the writer cannot disagree about the word — the rule `Mode::word`
    /// follows.
    pub fn word(self) -> &'static str {
        match self {
            ArtSource::Whole => "whole",
            ArtSource::Sticker => "sticker",
        }
    }

    /// Anything else is not a source. A state file edited on a desktop must not be able to
    /// put the device into a third one.
    pub fn from_word(word: &str) -> Option<ArtSource> {
        match word.trim().to_ascii_lowercase().as_str() {
            "whole" => Some(ArtSource::Whole),
            "sticker" => Some(ArtSource::Sticker),
            _ => None,
        }
    }
}

/// What a card that has never been asked reads as: the whole picture outranks the sticker.
/// A named constant rather than a literal in `Default` so the tests that spell a whole
/// `SlotState` out can point at it — the reason `SHELF_BADGE_DEFAULT` exists.
pub const ART_SOURCE_DEFAULT: ArtSource = ArtSource::Whole;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotState {
    /// Filename stem. `None` is an empty slot, which is the shelf.
    pub cart: Option<String>,
    pub brightness: u8,
    pub blue_light: u8,
    pub volume: u8,
    /// Silence on top of the level rather than instead of it, so unmuting gives back the
    /// number the user last chose.
    pub muted: bool,
    /// Whether anyone has ever confirmed the wall clock. The marker for slot's own first
    /// launch, and the one field a fresh card must read as false.
    pub clock_set: bool,
    /// Minutes to add to the card's UTC to get local time. Zero is a device that never left
    /// Greenwich, which is also what a card that has never been asked reads as.
    pub utc_offset_min: i16,
    /// Light or dark. The one field whose absence is not an error — see `parse`.
    pub mode: Mode,
    /// Which of the mode's twelve colouring ramps is in use — an index into that mode's row,
    /// zero being the device's own palette. **Bounded here rather than trusted**: a state file
    /// edited on a desktop must not be able to put the device past the end of the row.
    ///
    /// Like `mode` and for the same reason, absent on every card written before it existed — see
    /// `parse`. Its fallback is zero, which is the device every card in the field already is.
    pub look: u8,
    /// Minutes of dark panel before the machine powers off, from `DOZE_MIN_CHOICES`. Like
    /// `mode`, absent on every card written before it existed — see `parse`. **Zero is not a
    /// wait: it is the timer switched off**, and every reader has to treat it as "no timer"
    /// rather than as "a zero-second timer", which would end in an immediate `poweroff`.
    pub doze_min: u8,
    /// Whether the shelf prints a badge in its bottom-left corner — the machine's own mark on a
    /// game row, and the row's own word (`Disc` / `Book`) on the other two.
    ///
    /// **One widget, so one setting.** The badge is the *shelf's* furniture rather than the
    /// machine's — which is why a disc row never borrowed a machine's face for its corner — so a
    /// user who wants that corner empty wants it empty on all three rows, and a row-by-row switch
    /// would be three settings for one piece of furniture.
    ///
    /// `true` is what this build has always done, and the line is absent on every card written
    /// before it existed — see `parse`.
    pub shelf_badge: bool,
    /// The fast forward multiplier, from `FF_SPEED_CHOICES`. Like `mode` and `doze_min`,
    /// absent on every card written before it existed — see `parse`.
    pub ff_speed: u8,
    /// The rewind multiplier, from `REWIND_SPEED_CHOICES`. Read by the emu worker, which turns
    /// it into "how many snapshots one present pops" — see `slot::emu`.
    pub rewind_speed: u8,
    /// Whether fast forward and rewind keep playing the game's audio. **One switch for both
    /// speeds**, the same bargain the badge makes for the shelf's corner: what the player wants
    /// to hear while speeding is one question, not one per key. Off is the shipped behaviour —
    /// see `slot::emu::Worker::run` for what the worker does either way.
    ///
    /// `false` is what this build has always done, and the line is absent on every card written
    /// before it existed — see `parse`.
    pub ff_sound: bool,
    /// The carts the user has starred, as filename stems. A `BTreeSet` rather than a `Vec`
    /// because it is a membership question asked once per cart per frame on the shelf, and
    /// because the order is never read: the shelf is ordered by the library, not by when a
    /// star was given. Written one `favorites=` line per stem, so a stem carrying `=`, a space
    /// or a comma survives the round trip (see `parse`).
    pub favorites: BTreeSet<String>,
    /// Operation-key bindings for the six physical keys. The value is a target name in
    /// `slot_ui::keys::TARGETS`; empty means the card has not customised that key.
    pub keymap: BTreeMap<String, String>,
    /// ★ 2026-10-26：**每台机种各一份操作键表。** 外层键是机种的文件夹名（`System::dir_name`，
    /// 也就是 `GBA` / `GB` / `PS1` …），内层和上面那张表一样是「物理键 → 目标名」。
    ///
    /// **为什么不是一张表**：十台机种的手柄不是同一个（GBA 只有 A/B，MD 有 A/B/C + X/Y/Z，
    /// PS1 是 △○×□ + L1/R1），同一把物理键在不同机种上该默认成什么是**机种的事**，不是卡的事。
    /// 一张表只能表达“所有机种一样”，而这正是旧行为 —— 于是 D-pad 之上那两颗键在所有机种上
    /// 都只能空着。
    ///
    /// **和上面那张表的关系**：`keymap` 是**旧卡的全局兜底**，先铺底；本表里有的机种再逐键覆盖。
    /// 一张从 10-21 用到现在的卡（`keymap_Y=GAME_A`）读出来仍是“每个机种都是 Y=A”，直到某个
    /// 机种被单独改过为止 —— 升级二进制不该把玩家的键位翻掉。
    pub keymaps: BTreeMap<String, BTreeMap<String, String>>,
    /// Whether the operation keymap has been customised. Kept separately from an empty table
    /// so “default” remains a chosen state rather than an inferred one.
    pub keymap_mode: bool,
    /// Turbo repeat rate in Hz, clamped to `TURBO_HZ_CHOICES` on read.
    pub turbo_hz: u8,
    /// ★ 2026-10-22：UI 提示音（方向移动 / 步进 / 确认）的总开关。**不影响插/拔两条
    /// 动作音** —— 那是机器自己的声响，不是界面提示。`true` 是这一版会采用的默认；
    /// 素材未到位时即使开着也是静默（见 `slot::audio::Sfx::render`）。
    pub ui_sfx: bool,
    /// Which art source wins when a cart carries both — see `ArtSource`. Absent on every
    /// card written before it existed — see `parse`.
    pub art_source: ArtSource,
    /// ★ 2026-10-28：插卡 / 开机卡槽有卡时，是否自动读 `resume.state` 续玩。
    /// `true` 是这一版之前一直有的行为（`session` 在插入 / 开机时回读 resume），缺行按
    /// `INSERT_RESUME_DEFAULT` 走——见 `parse`。
    pub insert_resume: bool,
    /// ★ 2026-10-28：拔卡时是否把当前画面写进 `resume.state`（即时存档快照）。
    /// **不影响原生电池存档 `.sav` 的写入**——那一条拔卡必写，丢档不行。
    /// `lid` / 关机 / 自动存档那条走 `persist::flush`（不清槽），不在本开关管辖内。
    /// `true` 是旧行为；缺行按 `EJECT_SAVE_DEFAULT`。
    pub eject_save: bool,
}

/// Not derived. `read_slot_state` falls back here on a first boot, and all zeroes would
/// be a device with the backlight off and the mixer muted. The brightness default is the
/// middle of the ramp about to be written, not five of it: on a twenty step scale the old
/// five would come up at a quarter, and a card that has never been touched should look like
/// a card someone set at a sensible level.
impl Default for SlotState {
    fn default() -> Self {
        SlotState {
            cart: None,
            brightness: 10,
            blue_light: 0,
            volume: 60,
            muted: false,
            clock_set: false,
            utc_offset_min: 0,
            mode: Mode::Dark,
            look: 0,
            doze_min: DOZE_MIN_DEFAULT,
            // The corner has worn a badge for as long as the shelf has existed, so a card that
            // has never been asked reads as "shown" rather than as "hidden".
            shelf_badge: SHELF_BADGE_DEFAULT,
            ff_speed: FF_SPEED_DEFAULT,
            rewind_speed: REWIND_SPEED_DEFAULT,
            // Speeding has always been silent here, and a card that has never been asked has to
            // stay that way — see `FF_SOUND_DEFAULT`.
            ff_sound: FF_SOUND_DEFAULT,
            favorites: BTreeSet::new(),
            keymap: BTreeMap::new(),
            keymaps: BTreeMap::new(),
            keymap_mode: false,
            turbo_hz: TURBO_HZ_DEFAULT,
            ui_sfx: UI_SFX_DEFAULT,
            art_source: ART_SOURCE_DEFAULT,
            insert_resume: INSERT_RESUME_DEFAULT,
            eject_save: EJECT_SAVE_DEFAULT,
        }
    }
}

/// Mutable frontend state belongs in the writable per-instance user-data
/// directory, not in System/. Managed BaseOS profiles bind-mount .userdata as
/// a directory; atomic rename is safe there. Binding a single file at
/// System/slot.state is not safe because atomic_write replaces the file by
/// renaming a sibling temporary file over the mount point (Linux rejects that
/// with EBUSY).
fn state_path(root: &Path) -> PathBuf {
    root.join(".userdata").join("slot.state")
}

/// Pre-profile releases stored this file below System/. Read it as a one-way
/// migration fallback so existing cards keep their settings on first launch.
fn legacy_state_path(root: &Path) -> PathBuf {
    root.join("System").join("slot.state")
}

pub fn read_slot_state(root: &Path) -> SlotState {
    let bytes = match std::fs::read(state_path(root)) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::read(legacy_state_path(root)).ok()
        }
        Err(_) => None,
    };
    bytes
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| parse(&s))
        .unwrap_or_default()
}

pub fn write_slot_state(root: &Path, s: &SlotState) -> std::io::Result<()> {
    let mut text = format!(
        "cart={}\nbrightness={}\nblue_light={}\nvolume={}\nmuted={}\nclock_set={}\nutc_offset_min={}\nmode={}\nlook={}\ndoze_min={}\nshelf_badge={}\nff_speed={}\nrewind_speed={}\nff_sound={}\nkeymap_mode={}\nturbo_hz={}\nui_sfx={}\nart_source={}\ninsert_resume={}\neject_save={}\n",
        s.cart.as_deref().unwrap_or(""),
        s.brightness,
        s.blue_light,
        s.volume,
        s.muted as u8,
        s.clock_set as u8,
        s.utc_offset_min,
        s.mode.word(),
        s.look,
        s.doze_min,
        s.shelf_badge as u8,
        s.ff_speed,
        s.rewind_speed,
        s.ff_sound as u8,
        s.keymap_mode as u8,
        s.turbo_hz,
        s.ui_sfx as u8,
        s.art_source.word(),
        s.insert_resume as u8,
        s.eject_save as u8,
    );
    // One line per favourite rather than a delimited list, so a stem that happens to contain
    // the delimiter is not two carts in disguise: `split_once('=')` on the way back splits at
    // the first `=` only, so everything after it — `=`, space, comma and all — is the stem.
    for stem in &s.favorites {
        if !stem.is_empty() {
            text.push_str("favorites=");
            text.push_str(stem);
            text.push('\n');
        }
    }
    for (phys, target) in &s.keymap {
        if !phys.is_empty() && !target.is_empty() {
            text.push_str("keymap_");
            text.push_str(phys);
            text.push('=');
            text.push_str(target);
            text.push('\n');
        }
    }
    // ★ 2026-10-26：每台机种那份。**写成 `keymap_<SYS>_<PHYS>`**（`keymap_PS1_X=GAME_X`）——
    // 前缀仍是 `keymap_`，于是老二进制读新卡会把它当成一把叫 `PS1_X` 的物理键（`keys::name`
    // 认不出，落回 stock），而不是把整份文件当损坏。反过来新二进制读老卡走上面那条全局表。
    for (system, map) in &s.keymaps {
        for (phys, target) in map {
            if system.is_empty() || phys.is_empty() || target.is_empty() {
                continue;
            }
            text.push_str("keymap_");
            text.push_str(system);
            text.push('_');
            text.push_str(phys);
            text.push('=');
            text.push_str(target);
            text.push('\n');
        }
    }
    let path = state_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&path, text.as_bytes())
}

/// All or nothing. A file we only half recognise is not one we wrote, and inheriting the
/// missing fields from the defaults would hide the corruption behind plausible values.
///
/// `mode` is the one exception, and deliberately: every card in the field has a state file
/// written before the mode existed, and under the rule above the first boot of a binary that
/// knows about it would throw the whole file away — the cart the user left selected, the
/// brightness they set, the timezone they confirmed. Strictness here exists to catch a file
/// that has been corrupted, not to punish one that is merely older, and a missing mode has
/// exactly one sensible reading: dark, which is what the device did before it could be
/// anything else. Every *other* key is still all-or-nothing, including one this build has
/// never heard of.
///
/// `favorites` is the second exception, for the same reason and one more: a card that has
/// never been starred carries no `favorites` line at all, and reading that as corruption
/// would throw the file away the first time this build booted on it. A key that repeats is
/// not a duplicate either — one `favorites=` line per stem is the format — so this arm adds
/// to a set rather than assigning, which is the one place a later line does not replace an
/// earlier one.
///
/// `doze_min` is the third, and for exactly `mode`'s reason: every card in the field has a
/// state file written before the lid timer could be set, and a missing line has one honest
/// reading — the default wait, `DOZE_MIN_DEFAULT`. A value that is *present* and not one of
/// `DOZE_MIN_CHOICES` is still corruption, the same way a misspelt mode is: the difference is
/// between a file that is older and a file someone has been editing.
///
/// `ff_speed` and `rewind_speed` are the fourth and fifth, for that same reason again and with
/// one more thing riding on it: their fallbacks are **what this build already did** (four and
/// two), so a card written before the speed rows existed keeps the exact fast forward and the
/// exact rewind it had. Getting that wrong would be a setting the user never chose changing
/// under them on the first boot of a new binary.
///
/// `shelf_badge` is the sixth, and its fallback is whatever `SHELF_BADGE_DEFAULT` says — which
/// is **`false`, per 2026-10-23 六六**. It used to be `true`, on the reasoning that the corner
/// had worn a badge since the shelf existed; that decision was reversed when the badge was
/// meant to come off (the drawing code is kept but defaulted off, see the constant's own doc),
/// so "the line is missing" now has to read as "hidden". A card written before the setting
/// comes up the way a fresh one does, and a card that says `shelf_badge=1` still gets it —
/// `durability.rs::slot_state_round_trips_the_badge_both_ways` pins both ends of that.
///
/// `ff_sound` is the seventh, and its fallback is `false` for the same reason once more: the
/// worker has dropped the core's audio while speeding since the beginning, so "the line is
/// missing" has to read as "silent" — a card that came up making noise while fast forwarding
/// because it predates the setting would be this build answering a question nobody asked it.
///
/// `art_source` is the eighth, and its fallback is `ART_SOURCE_DEFAULT` for the same reason
/// once more: a card that predates the folder has no whole-cart picture to win with, so the
/// line being missing has to read as the default — and a word this build has never heard of
/// is still corruption, the same way a misspelt mode is.
fn parse(text: &str) -> Option<SlotState> {
    let mut cart = None;
    let mut brightness = None;
    let mut blue_light = None;
    let mut volume = None;
    let mut muted = None;
    let mut clock_set = None;
    let mut utc_offset_min = None;
    let mut mode = None;
    let mut look = None;
    let mut doze_min = None;
    let mut shelf_badge = None;
    let mut ff_speed = None;
    let mut rewind_speed = None;
    let mut ff_sound = None;
    let mut keymap_mode = None;
    let mut turbo_hz = None;
    let mut ui_sfx = None;
    let mut art_source = None;
    let mut insert_resume = None;
    let mut eject_save = None;
    let mut favorites = BTreeSet::new();
    let mut keymap = BTreeMap::new();
    let mut keymaps: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once('=')?;
        match key {
            "cart" => cart = Some(value.to_string()),
            "brightness" => brightness = Some(level(value, BRIGHTNESS_MAX)?),
            "blue_light" => blue_light = Some(level(value, BLUE_LIGHT_MAX)?),
            "volume" => volume = Some(level(value, VOLUME_MAX)?),
            "muted" => muted = Some(level(value, 1)? == 1),
            "clock_set" => clock_set = Some(level(value, 1)? == 1),
            "utc_offset_min" => utc_offset_min = Some(offset(value)?),
            // Written by this build and unreadable in it is still corruption: a misspelt word
            // here means someone has been editing the file, and the defaults are the safe
            // answer for the rest of it too.
            "mode" => mode = Some(Mode::from_word(value)?),
            // A look is a number held to its row, and one this build has never heard of is
            // corruption the same way a misspelt mode is — see the note on `mode` above.
            "look" => look = Some(level(value, LOOK_MAX)?),
            "doze_min" => doze_min = Some(doze_choice(value)?),
            "shelf_badge" => shelf_badge = Some(level(value, 1)? == 1),
            "ff_speed" => ff_speed = Some(multiplier(value, &FF_SPEED_CHOICES)?),
            "rewind_speed" => rewind_speed = Some(multiplier(value, &REWIND_SPEED_CHOICES)?),
            "ff_sound" => ff_sound = Some(level(value, 1)? == 1),
            "keymap_mode" => keymap_mode = Some(level(value, 1)? == 1),
            "turbo_hz" => turbo_hz = Some(value.parse().ok().unwrap_or(TURBO_HZ_DEFAULT)),
            // ★ 2026-10-22：UI 提示音开关。缺行 → `UI_SFX_DEFAULT`（见 `parse` 尾部）。
            "ui_sfx" => ui_sfx = Some(level(value, 1)? == 1),
            // ★ 2026-10-23：美术资源开关。缺行 → `ART_SOURCE_DEFAULT`（见 `parse` 尾部）。
            "art_source" => art_source = Some(ArtSource::from_word(value)?),
            "insert_resume" => insert_resume = Some(level(value, 1)? == 1),
            "eject_save" => eject_save = Some(level(value, 1)? == 1),
            "favorites" => {
                if !value.is_empty() {
                    favorites.insert(value.to_string());
                }
            }
            // ★ 2026-10-26：两种拼法共用这个前缀。`keymap_<PHYS>` 是旧卡的全局表；
            // `keymap_<SYS>_<PHYS>` 是每台机种那份。**先认机种**：`PS1` 是 `System::from_dir`
            // 认得的目录名，而六把物理键里没有一个叫 `PS1` 的，所以两者不会撞。
            _ if key.starts_with("keymap_") => {
                let rest = key["keymap_".len()..].trim();
                let (system, phys) = match rest.split_once('_') {
                    Some((dir, tail)) if crate::System::from_dir(dir).is_some() => {
                        (crate::System::from_dir(dir).map(|s| s.dir_name()), tail)
                    }
                    _ => (None, rest),
                };
                if !phys.is_empty() && !value.trim().is_empty() {
                    let target = value.trim().to_ascii_uppercase();
                    let phys = phys.to_ascii_uppercase();
                    match system {
                        Some(sys) => {
                            keymaps.entry(sys.to_string()).or_default().insert(phys, target);
                        }
                        None => {
                            keymap.insert(phys, target);
                        }
                    }
                }
            }
            _ => return None,
        }
    }
    let cart = cart?;
    let fallback = SlotState::default();
    Some(SlotState {
        cart: (!cart.is_empty()).then_some(cart),
        brightness: brightness?,
        blue_light: blue_light?,
        volume: volume?,
        muted: muted?,
        clock_set: clock_set?,
        utc_offset_min: utc_offset_min?,
        mode: mode.unwrap_or_default(),
        look: look.unwrap_or(fallback.look),
        doze_min: doze_min.unwrap_or(fallback.doze_min),
        shelf_badge: shelf_badge.unwrap_or(fallback.shelf_badge),
        ff_speed: ff_speed.unwrap_or(fallback.ff_speed),
        rewind_speed: rewind_speed.unwrap_or(fallback.rewind_speed),
        ff_sound: ff_sound.unwrap_or(fallback.ff_sound),
        keymap_mode: keymap_mode.unwrap_or(fallback.keymap_mode),
        turbo_hz: turbo_hz
            .map(|hz| if TURBO_HZ_CHOICES.contains(&hz) { hz } else { TURBO_HZ_DEFAULT })
            .unwrap_or(fallback.turbo_hz),
        favorites,
        keymap,
        keymaps,
        ui_sfx: ui_sfx.unwrap_or(fallback.ui_sfx),
            art_source: art_source.unwrap_or(fallback.art_source),
            insert_resume: insert_resume.unwrap_or(fallback.insert_resume),
            eject_save: eject_save.unwrap_or(fallback.eject_save),
    })
}

/// The only place a minute count from a file becomes one: outside the list is not a setting,
/// so it is corruption.
fn doze_choice(value: &str) -> Option<u8> {
    value.parse().ok().filter(|n| DOZE_MIN_CHOICES.contains(n))
}

/// A multiplier from a file, held to the list its row prints. Two rows use this and they are
/// deliberately different lists — the honest gears are not the same for both — so the list is
/// handed in rather than assumed.
fn multiplier(value: &str, choices: &[u8]) -> Option<u8> {
    value.parse().ok().filter(|n| choices.contains(n))
}

fn offset(value: &str) -> Option<i16> {
    value
        .parse()
        .ok()
        .filter(|n| (UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(n))
}

fn level(value: &str, max: u8) -> Option<u8> {
    value.parse().ok().filter(|n| *n <= max)
}
