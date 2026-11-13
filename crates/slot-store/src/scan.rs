use std::collections::HashSet;
use std::fmt;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::atomic::atomic_write;
use crate::gba::{sgb_enhanced};

/// Which machine a cart is for, and so which shelf it belongs on.
///
/// One shelf per machine the card files and a hand asks for: the Advance (Gba), the Game Boy /
/// Game Boy Color (Gb / Gbc, one frame size between them), the Super Famicom (Sfc) and the
/// Famicom / NES (Fc). Decided off the *extension* rather than by opening the rom, for the same
/// reason everything else here is: the fast path of `scan_cached` reads a name and nothing else,
/// and a property that costs a name to answer is free.
///
/// A zip deliberately gets no entry. Two reasons, and the second is the one that decides it:
/// the cores are handed the bytes of the file this frontend read off the card, so a zip would
/// reach the core as an archive rather than as a rom — there is no extraction stage here the
/// way RetroArch has one — and worse, an extension is the only thing that tells these three
/// apart for free. A zip throws that away and leaves the machine unknowable without opening the
/// file to look inside it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum System {
    Gba,
    Gb,
    Gbc,
    /// Super Famicom / SNES. Frame 256x224, lands on the 720x480 panel at integer 2x.
    Sfc,
    /// Famicom / NES. Frame 256x240, lands on the 720x480 panel at integer 2x.
    Fc,
    /// NeoGeo. Frame 320x224, lands on the 720x480 panel at integer 2x (640x448).
    /// Rom is a .zip identified by its parent folder `Games/NEOGEO/`.
    Neogeo,
    /// Other arcade games (CPS1/CPS2/CPS3 and the rest of FBNeo's set). Stretched to
    /// 640x480 (4:3) rather than integer-scaled. Rom is a .zip identified by its parent
    /// folder `Games/ARCADE/`.
    Arcade,
    /// Mega Drive / Genesis. Frame 320x224 — the same numbers as NeoGeo, so it lands on the
    /// 720x480 panel at integer 2x (640x448) rather than being stretched.
    ///
    /// ★ **Appended at the end, like every variant before it**: the discriminant is the shelf
    /// slot and the `Sfx::Bgm` index, and a card that has been used for months must not have a
    /// cart change machine under it because a row was inserted in the middle.
    ///
    /// `.bin` is MD's *and* PCE's *and* PS1's, so this can only ever be answered by the folder
    /// (`Games/MD/`); `from_extension` takes `.md` / `.gen` / `.smd`, which are MD's alone.
    Md,
    /// PlayStation. Frame 640x480, so `scale_for` gives 1 and the picture is exactly the panel's
    /// width — the one machine whose native size is the output size.
    ///
    /// Also appended at the end, and also folder-first: `.bin` and `.cue` are shared with MD and
    /// with PCE. PS1's own extensions (`.pbp`, `.chd`, `.iso`) are handled in `from_extension`.
    Ps1,
    /// PC Engine / TurboGrafx-16. Frame 256x240 straight out of `pce_fast`
    /// (`pce_fast_initial_scanline` defaults to 3 and `pce_fast_last_scanline` to 242, which is
    /// 240 rows), cropped to 224 the same way fceumm and genesis_plus_gx are — see
    /// `slot::core`'s `Core::PceFast` arm for the numbers and for what still needs the device.
    ///
    /// ★ Appended at the end like every variant before it: the discriminant is the shelf slot
    /// and the `Sfx::Bgm` index, and a card used for months must not have a cart change machine
    /// under it because a row was inserted in the middle.
    ///
    /// `.bin` is MD's *and* PCE's *and* PS1's and `.cue` is PCE's *and* PS1's, so this can only
    /// ever be answered by the folder (`Games/PCE/`). `from_extension` takes `.pce`, which is
    /// PCE's alone.
    Pce,
}

impl System {
    /// ★ 2026-10-27（六六）：这台机种的载体是**代币**。
    ///
    /// ARCADE 与 NEOGEO 在货架上是圆牌（`66SLOT_DISC_*.png`，300×300），进槽走**投币**动画：
    /// 代币整枚出屏 → 街机箱体从插卡口升起 → CRT 进游戏。判据只在这里写一次，别处一律
    /// 问它，免得"货架当盘、进槽当卡带"这种半套。
    pub const fn is_coin(self) -> bool {
        matches!(self, System::Neogeo | System::Arcade)
    }

    /// From whatever follows the last dot in a file name. Compared without case, because a
    /// card written on another machine arrives in whatever case that machine felt like.
    pub fn from_extension(ext: &str) -> Option<Self> {
        if ext.eq_ignore_ascii_case("gba") {
            Some(System::Gba)
        } else if ext.eq_ignore_ascii_case("gb") {
            Some(System::Gb)
        } else if ext.eq_ignore_ascii_case("gbc") {
            Some(System::Gbc)
        } else if ext.eq_ignore_ascii_case("smc")
            || ext.eq_ignore_ascii_case("sfc")
            || ext.eq_ignore_ascii_case("fig")
        {
            Some(System::Sfc)
        } else if ext.eq_ignore_ascii_case("nes") || ext.eq_ignore_ascii_case("fds") {
            Some(System::Fc)
        } else if ext.eq_ignore_ascii_case("md")
            || ext.eq_ignore_ascii_case("gen")
            || ext.eq_ignore_ascii_case("smd")
        {
            // ⚠️ `.md` is **also** markdown's, and `NOT_A_ROM` names it. That is why `is_rom`
            // now refuses a `NOT_A_ROM` extension before it ever consults this: a `readme.md`
            // beside a flat card's roms must not become a cart. `.bin` is deliberately absent —
            // MD, PCE and PS1 all use it, and no reading of the extension can separate them.
            Some(System::Md)
        } else if ext.eq_ignore_ascii_case("cue")
            || ext.eq_ignore_ascii_case("chd")
            || ext.eq_ignore_ascii_case("pbp")
            || ext.eq_ignore_ascii_case("iso")
        {
            Some(System::Ps1)
        } else if ext.eq_ignore_ascii_case("pce") {
            // `.pce` is PCE's alone, so it is the one spelling that survives outside a machine
            // folder. `.bin` and `.cue` are shared with MD and PS1 and are deliberately absent
            // here for the same reason they are for MD.
            Some(System::Pce)
        } else {
            None
        }
    }

    /// The machine a **folder** names, matched without case because a card written on another
    /// machine arrives in whatever case that machine felt like.
    ///
    /// This is the authoritative answer for a file that sits in one of the machine folders:
    /// the extension is not consulted at all. That is the whole point — `.bin` is MD's, PCE's
    /// and PS1's, `.zip` is FBNeo's, and no reading of the extension can separate them, while
    /// the folder separates all three for free.
    ///
    /// The canonical names are [`System::dir_name`]'s, read off the same table so the two can
    /// never disagree. `NES` / `SNES` are accepted as well and mean the same machines as
    /// `FC` / `SFC` — they are the names people actually type, and a card whose games silently
    /// fail to appear is a worse outcome than a second spelling. Note this is about *folders*:
    /// the overlay art still has exactly one name per machine (`fc-*.png`, never `nes-*.png`).
    pub fn from_dir(dir: &str) -> Option<Self> {
        DIRS.iter()
            .find(|(_, canonical, aliases)| {
                dir.eq_ignore_ascii_case(canonical)
                    || aliases.iter().any(|a| dir.eq_ignore_ascii_case(a))
            })
            .map(|(s, _, _)| *s)
    }

    /// The folder this machine's carts and faces go in, on a card that sorts them.
    ///
    /// Always the canonical name, never an alias — this is what gets written.
    pub fn dir_name(self) -> &'static str {
        DIRS.iter()
            .find(|(s, _, _)| *s == self)
            .map(|(_, c, _)| *c)
            .unwrap_or("GBA")
    }
}

/// Every machine, its canonical folder name, and the spellings [`System::from_dir`] also takes.
///
/// One table for both directions so that `dir_name` and `from_dir` cannot drift: adding a
/// machine means adding a row, and a row is enough for both.
const DIRS: [(System, &str, &[&str]); 10] = [
    (System::Gba, "GBA", &[]),
    (System::Gb, "GB", &[]),
    (System::Gbc, "GBC", &[]),
    (System::Sfc, "SFC", &["SNES"]),
    (System::Fc, "FC", &["NES"]),
    (System::Neogeo, "NEOGEO", &[]),
    (System::Arcade, "ARCADE", &[]),
    // ★ Appended at the end, never inserted: the row order is nothing, but the *variants* are
    // discriminants, so the two new machines could only ever be added here.
    //
    // The aliases are the names people actually type. `GENESIS` is what a US card says and
    // `MEGADRIVE` is what a European one does; `PSX` is the old name the machine had while it
    // was in development and the one most rom sets still use for the folder. `TG16` is the US
    // name for the PC Engine and `TURBOGRAFX` the fuller one — both are on plenty of cards.
    (System::Md, "MD", &["GENESIS", "MEGADRIVE"]),
    (System::Ps1, "PS1", &["PSX"]),
    (System::Pce, "PCE", &["TG16", "TURBOGRAFX", "PCENGINE"]),
];

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    /// Filename stem, which is the key for labels, saves and states. Not a content hash.
    pub stem: String,
    pub rom: PathBuf,
    pub label: Option<PathBuf>,
    /// The cart's whole picture, when the card carries one: the finished png with its label
    /// already on it, resolved off `Cartridge/` by `cart_art_for`. A cart that has one is
    /// drawn from it and nothing else — no magenta slot, no minted label, no shell colour
    /// (see `slot_ui::cart::cart_face`) — **when the art source says it outranks the
    /// sticker** (see `slot_store::ArtSource`); the label-sticker composite is the *other*
    /// road to a dressed cart and stays exactly as it is. `None` for the normal card.
    pub art: Option<PathBuf>,
    pub title: String,
    /// The game's own identity: an Advance cart's four character header code, and — a Game
    /// Boy cart has no code field — its header title. Empty when the rom has none.
    pub code: String,
    /// ★ 2026-10-24（六六）：「GBC 有两种卡带，共通和专用，得画两份」——
    ///   卡带头 `0x143` 的原字节：`0x80` 双模（共通壳）、`0xC0` GBC 专用壳、`0x00` 初代 GB。
    ///   `0` 也表示"不知道"（非 Game Boy 卡、或读不到头）—— 两种情况下 `shape()` 都按机种
    ///   的默认壳走，与从前一致。
    pub cgb: u8,
    /// Which bucket the shelf's letter ring files this cart under: the pinyin initial of the
    /// first character of the name the shelf shows — see `bucket` for why it is the file name
    /// and not the header title. Derived here rather than stored, because it is one binary
    /// search over a table already in the binary and the cache does not need a column for it.
    pub initial: char,
}

impl Cart {
    /// Which machine this cart is for.
    ///
    /// **The folder decides, and it decides outright.** A rom filed in `Games/GBA/` is a Game
    /// Boy Advance cart whatever it is called, and its extension is not read at all. This is
    /// what keeps two machines that share an extension apart: `.bin` is MD's, PCE's and PS1's,
    /// `.zip` is FBNeo's, and no reading of the extension can separate them while the folder
    /// separates all three for free.
    ///
    /// The extension is the **fallback**, and only for a rom filed directly in `Games/` — which
    /// is what every card written before the machine folders existed looks like. Dropping it
    /// would empty those cards in one upgrade, so it stays until the folders are the only
    /// shape there is.
    pub fn system(&self) -> System {
        machine_of_path(&self.rom).unwrap_or(System::Gba)
    }

    /// Whether this cart's rom is Super Game Boy enhanced — mGBA would draw an SGB border
    /// for it unless SLOT turns that off. Reads the rom on demand, so it is not part of the
    /// scan fast path; calling it per cart at boot would cost a header read apiece.
    pub fn sgb_enhanced(&self) -> bool {
        sgb_enhanced(&self.rom)
    }

    /// The machine folder this rom is filed under, **spelled exactly as the card spells it** —
    /// `FC` or `NES`, `SFC` or `SNES`, not just the canonical name.
    ///
    /// ★ 2026-10-26（六六）：「FC/NES和SFC/SNES根据Games文件夹的命名决定用哪个」—— 缺省卡带
    ///   图是按**目录名**键的（FC 与 NES、SFC 与 SNES 各一张），而机种只能到 `FC`/`SFC`，
    ///   分不清两张；目录名是唯一能分清它们的东西。所以这里把原始目录名放出去，而不是复用
    ///   `System::dir_name`（那会把 NES 拍平回 FC）。
    ///
    /// `None` 当 rom 直接放在 `Games/` 下（或扫不出机种目录），那时 `system()` 靠扩展名兜底，
    /// 缺省图也自然落空（`default_art_for` 对非目录名返回 None），回退内建壳 —— 与从前一致。
    ///
    /// 这是 `machine_dir_name` 的公开壳：后者还服务 `system()`，而那个调用方要的是"能识别成
    /// 机种"的名字，这里要的是"卡上写的名字"，二者只在 `System::from_dir` 能识别时一致。
    pub fn dir_name(&self) -> Option<&str> {
        machine_dir_name(&self.rom)
    }
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// An unmounted card or a card with no library is an empty shelf, not a boot failure.
///
/// Reads every rom's header, so a shelf built off this pays for the whole card. `scan_cached`
/// is what a boot calls; this is the reference the cache has to agree with.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let games = list_names(&root.join("Games"), is_rom, true)?;
    let labels = labels_set(&root.join("Labels"))?;
    Ok(build(root, &games, &labels, None).0)
}

/// The shelf, re-read only where the card has changed.
///
/// A boot used to read the header of every cart on the card: three hundred games is three
/// hundred directory lookups and cluster chain walks on a card that is the slowest thing in
/// the machine, and it is the same answer every time until a game is added. This keeps the
/// answer on the card and checks whether it still applies before doing any of that work.
///
/// The check is the *shape* of the two trees the shelf is drawn from: the timestamp of every
/// directory in `Games/` and `Labels/`, and the sorted file names under each. Adding, removing,
/// renaming or re-labelling a game all move one of those, so the cache is thrown away and
/// the card is read again — and when it is, the headers of the games that did not change are
/// reused from the cache rather than read back, so even a card with one new game on it pays
/// for one game rather than for three hundred.
///
/// ★ **Since 2026-10-07 the timestamps alone can carry it.** When every directory's timestamp
/// matches the cache, the names cannot have changed either, so the walk that reads them is
/// skipped outright — measured at **1794 ms** on a 968-game card, which was the single largest
/// block of a boot after the fonts. The walk still runs whenever a timestamp moved, is
/// unusable, or there is no cache; see the two paths in the body.
///
/// ⚠️ **The stamp holds every directory in the tree, not the two the shelf is drawn from**
/// (since 2026-10-15, and it is what makes the check above sound). The walk is recursive, so a
/// card that files its games `Games/GBA/中文/` could have a rom removed from the deepest folder
/// without `Games/` moving at all: a stamp of two numbers called that "nothing changed" and left
/// the cart on the shelf with no rom behind it — `§36` of the memory's 排错 ⑩. One timestamp per
/// directory is what closes it, and a cache hit pays one `stat` each to prove it, which is a
/// handful on a card that files its games by machine.
///
/// What it does not catch is a rom rewritten in place under its own name with its own
/// timestamp untouched, which no directory listing can see. `.userdata/library.cache` is a plain
/// file: delete it to force the slow path, which is the answer for a card that has been
/// patched rather than added to.
pub fn scan_cached(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let games_dir = root.join("Games");
    let labels_dir = root.join("Labels");
    let cached = read_cache(root);

    // ★ **The fast path: every directory the cache names still carries the timestamp it had when
    //   the names under it were read.** Then nothing under any of them can have changed either,
    //   and the walk below — where a boot spends its second-longest block (`scan_cached` measured
    //   **1794 ms** on a 968-game card, 2026-10-07, out of a 2014 ms `library scan + shelf`) — is
    //   pure cost. Proving it costs one `metadata` per directory and lists nothing.
    //
    //   ⚠️ **`stamp.stands_alone` is half the guard and the stamp's *coverage* is the other half,
    //   and both are load-bearing.** A directory mtime is coarse (whole seconds here, and
    //   2-second steps on the VFAT a card is formatted with), so a stamp that merely *matches*
    //   can still be hiding a change made inside the same window — which is exactly what the name
    //   walk was there to catch, and what the three `…_after_the_first_scan_…` tests in
    //   `tests/scan.rs` failed on when this fast path first went in without it. The guard asks
    //   for the stamp to be **older than the coarseness**, which makes a hidden change impossible
    //   rather than unlikely; the proof is on the method. And the stamp has to name **every**
    //   directory, not the two at the top: `list_names` walks recursively, so the change that
    //   matters may be a rom deleted from `Games/GBA/中文/`, which moves that folder's timestamp
    //   and leaves `Games/`'s exactly where it was (`§36`). See `Stamp`.
    //
    //   In practice this means the **boot right after games are copied** still walks (the stamp
    //   is fresh), and every boot after that does not. That is the right way round: the slow one
    //   is the boot where the answer actually changed.
    if let Some(c) = &cached {
        if c.rows_agree()
            && restamp(root, &c.stamp) == c.stamp
            && c.stamp.stands_alone(mtime_of(&cache_path(root)))
        {
            return Ok(carts_from_rows(root, &c.rows, &label_keys(&c.labels)));
        }
    }

    // The slow path, which is also the *verifying* path: reached when there is no cache, when
    // the timestamps are unusable, or when they moved. The names are compared rather than
    // assumed, so a directory whose timestamp moved but whose contents did not is still
    // answered from the cache instead of being rebuilt. Both tree roots are stamped even when
    // the card does not have them (`ABSENT`) — that is what catches a `Games/` appearing later,
    // and the card root itself is deliberately **not** stamped: it is not an input to this scan,
    // and a file dropped at the top of the card is nothing for the shelf to react to.
    walk_and_build(root, &games_dir, &labels_dir, cached.as_ref(), true)
}

/// Read the whole card again, **without letting the cache answer for it** — the scan behind
/// the settings screen's refresh row (2026-10-17).
///
/// A boot is the only other caller of the walk, and a boot is happy to be told "nothing
/// changed". This is not: the user has just put games on the card and asked the machine to look
/// again, so "the cache vouches for it" is the one answer that must not be given — the fast
/// path would hand back the shelf as it was and the press would read as a key that did nothing.
/// Handing `trust_cache: false` down also drops the old `rows`, so every rom's header is read
/// afresh rather than only the ones whose directory moved. That is the expensive half, and it is
/// what makes this honest for a card whose roms were *replaced* rather than added to: a rom
/// rewritten in place under its own name and its own timestamp is invisible to any directory
/// listing, and this is the one path that can see it.
///
/// The cache is still **written** on the way out, so the next boot starts from the truth this
/// read established rather than from the stale one it replaced.
pub fn rescan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let games_dir = root.join("Games");
    let labels_dir = root.join("Labels");
    let cached = read_cache(root);
    walk_and_build(root, &games_dir, &labels_dir, cached.as_ref(), false)
}

/// The walk itself, shared by both callers so they cannot disagree about what a card is.
///
/// `trust_cache` is the whole of the difference between them, and it is one flag rather than
/// two functions because the *reading* is the part that has to stay identical — a second
/// copy of the two `list_names_tracked` calls is a second answer to "what is on this card".
fn walk_and_build(
    root: &Path,
    games_dir: &Path,
    labels_dir: &Path,
    cached: Option<&Cache>,
    trust_cache: bool,
) -> Result<Vec<Cart>, StoreError> {
    let mut dirs = Vec::new();
    let games = list_names_tracked(games_dir, is_rom, true, "Games", &mut dirs)?;
    let labels = list_names_tracked(labels_dir, is_png, true, "Labels", &mut dirs)?;
    let stamp = Stamp { dirs };
    let label_set = label_keys(&labels);

    if trust_cache {
        if let Some(c) = cached {
            if c.stamp == stamp && c.games == games && c.labels == labels && c.rows_agree() {
                return Ok(carts_from_rows(root, &c.rows, &label_set));
            }
        }
    }

    let (carts, rows) = build(
        root,
        &games,
        &label_set,
        if trust_cache {
            cached.map(|c| c.rows.as_slice())
        } else {
            None
        },
    );
    write_cache(
        root,
        &Cache {
            stamp,
            games,
            labels,
            rows,
        },
    );
    Ok(carts)
}

// ---------------------------------------------------------------------------------------
// The cache
// ---------------------------------------------------------------------------------------

/// Under `System/` with the rest of slot's own bookkeeping. It describes the card rather than
/// the shelf the user curates, so it does not live beside the roms.
const CACHE: &str = ".userdata/library.cache";
const LEGACY_CACHE: &str = "System/library.cache";

/// The first line, which is also the format's version. A stale stamp or a run of bytes that
/// only looks like one has to read as no cache at all rather than as a shelf with holes in
/// it, and the version is what makes that possible across a future change to the layout.
///
/// **2 since 2026-10-15**: the stamp went from two directory timestamps to one line per
/// directory (`dirs`), because the walk it vouches for is recursive. A card carrying a `1` file
/// reads as no cache, which costs the one full scan that rewrites it in the new shape.
///
/// **3 since 2026-10-20**: `code` gained a meaning for Game Boy and Game Boy Color carts —
/// their header title, there being no code field on one — where a `2` file carries the
/// bytes that sit at 0xAC of a Game Boy rom, which is the Nintendo logo and identifies
/// nothing. A card carrying a `2` file reads as no cache and pays the one full scan.
/// ★ 2026-10-24：4 = 多一列 `cgb`（壳型判据）。旧缓存没有这一列，直接不认、重扫一次 ——
/// 一次慢开机，比"猜一个壳型"诚实。
///
/// ★ 5 紧接着：4 那版读 `cgb` 时**没有先让校验和认可这个头**，于是卡上的 `.zip`
/// （压缩容器，不是游戏头）也被采信，缓存里写进了 `213` 这类随机值。5 是"这一列可信"
/// 的重新收集 —— 同一批 rom 的 mtime/size 都没变，不升版就永远复用那份脏数据。
const MAGIC: &str = "slot-library 5";

/// Every directory the names below were gathered from, with the timestamp each one carried at
/// the moment they were read, sorted by path.
///
/// **A list, not two numbers**, and that is the fix for the ghost entry (`§36` of the memory's
/// 排错 ⑩, 2026-10-15). `list_names` walks `Games/` recursively, so a card that files its games
/// `Games/GBA/中文/` can have a rom removed from the deepest folder without `Games/` moving at
/// all; a stamp holding only `Games/` and `Labels/` answered "nothing changed" and left the cart
/// on the shelf with no rom behind it. Holding every directory closes it, because a change —
/// a rom, or a folder, added, removed or renamed — moves the timestamp of the directory it
/// happened in, and that directory is in this list.
///
/// The two tree roots are in the list **whether or not the card has them** (`ABSENT` for one
/// that is not there), which is what catches a `Games/` folder created on a card that had none.
/// The card root itself is not: nothing this scan reads sits directly in it, so stamping it
/// could only send the next boot back over a change the shelf has no opinion about.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Stamp {
    dirs: Vec<DirStamp>,
}

/// One directory's timestamp, as the cache holds it.
#[derive(Clone, PartialEq, Eq, Debug)]
struct DirStamp {
    /// Where the directory is, `/`-separated and **relative to the card root**: `Games`,
    /// `Games/GBA`, `Games/GBA/中文`, `Labels`. Relative rather than absolute because the cache
    /// outlives the place the card was mounted — it is written by a computer and read at
    /// `/mnt/sdcard`.
    path: String,
    /// Seconds since the epoch, `0` when the directory is there but will not say, `ABSENT` when
    /// it is not there at all. Two different facts, and the guard needs them apart: an absent
    /// directory has nothing under it to go stale, while one that is present and silent is
    /// exactly the case where the walk is the only guard there is.
    mtime: i64,
}

impl DirStamp {
    /// A directory that is not there. Negative so it cannot be mistaken for a time — in
    /// particular so that it never reads as "settled long ago" in the arithmetic below.
    const ABSENT: i64 = -1;

    /// Whether this directory's timestamp is old enough, measured against the moment the cache
    /// was written, that a change inside it could not be hiding in the same bucket.
    ///
    /// **A directory that is not there stands alone**, and that is not a loophole: `ABSENT` is
    /// the absence of anything to be stale rather than a stale time, and a card with no
    /// `Labels/` is a card rather than an unreadable one. Anything appearing inside it later has
    /// to create a directory first, which moves the timestamp of the directory the new one lands
    /// in — and that one is in the list. A directory that *is* there and answers `0` does not
    /// stand alone: nothing can be proved about it, so the walk runs, which is what the old
    /// two-number stamp did for a card whose directories would not say either.
    fn settles(&self, cache_written: i64) -> bool {
        self.mtime == Self::ABSENT
            || (self.mtime != 0 && self.mtime + Stamp::COARSENESS + Stamp::SLACK < cache_written)
    }
}

impl Stamp {
    /// **Whether these timestamps can stand in for the names under them.**
    ///
    /// A directory mtime is coarse. `mtime_secs` keeps whole seconds, and **VFAT — which is what
    /// a card is — writes a directory's mtime in 2-second units on top of that**, so a change
    /// made inside the same window as the previous one leaves the stamp exactly where it was.
    /// The name walk exists for precisely that case, and the only question here is when it may
    /// be skipped.
    ///
    /// It may be skipped when **every** directory is `settles` — older than the coarseness,
    /// measured against the moment the cache was written. With `M` a directory's stamp, `C` the
    /// cache file's own mtime (it is written at the end of the scan that produced it) and `G`
    /// the coarseness:
    ///
    /// * a change made **after this scan** has a bucket `≥ T_scan − G > C − G`, and the guard
    ///   says `M + G < C`, so that bucket cannot be `M` — the stamp will have moved by the next
    ///   boot and the walk will run. Nothing can hide behind it;
    /// * a change made **between `C` and this scan** would have moved the stamp off `M` for the
    ///   same reason, so a stamp that still reads `M` proves there was none;
    /// * a change made **before `C`** is already in the cache.
    ///
    /// The argument is about the directory the change happens *in*, which is why every one of
    /// them has to be held: the proof only covers the names under the directories this stamp
    /// names, and it is `list_names` — recursive — that decides which those are.
    ///
    /// Both numbers come off the same filesystem, which is what makes this safe on a device whose
    /// clock is wrong: an unset clock makes `C` older than `M` and the guard simply fails,
    /// falling back to the walk rather than to a shelf with a game missing from it. A zero
    /// timestamp — a directory that is there but could not be stat'd — fails the same way. An
    /// empty list is refused outright: a cache that names no directory at all vouches for
    /// nothing, and a stamp that promised otherwise would answer from the rows forever.
    fn stands_alone(&self, cache_written: i64) -> bool {
        !self.dirs.is_empty() && self.dirs.iter().all(|d| d.settles(cache_written))
    }

    /// How coarse a directory mtime may be and still be treated as an exact stand-in, in
    /// seconds. **Two, because that is what a card is**: VFAT's directory timestamps move in
    /// 2-second steps. A filesystem finer than this costs nothing — the guard is a lower bound,
    /// not an equality.
    const COARSENESS: i64 = 2;
    /// One more second on top of [`Self::COARSENESS`], so a bucket boundary landing exactly on
    /// the cache's own write cannot sit on the edge of the inequality.
    const SLACK: i64 = 1;
}

/// Where a stamped directory lives: a relative path is under the card root, and the empty one —
/// the root itself — is the root.
fn dir_path(root: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    }
}

/// The timestamp of one directory, in the shape `Stamp` holds it.
fn dir_stamp(dir: &Path, rel: &str) -> DirStamp {
    let mtime = if dir.is_dir() {
        mtime_of(dir)
    } else {
        DirStamp::ABSENT
    };
    DirStamp {
        path: rel.to_string(),
        mtime,
    }
}

/// The same stamp, re-read off the card **without listing anything**.
///
/// This is the entire cost of a cache hit: one `metadata` per directory the cache names — a
/// handful on a card that files its games by machine — in place of the walk that measured
/// 1794 ms over 968 roms. The directories are the cache's own list, so a folder that has been
/// deleted answers `ABSENT` and a folder that has been added moves the timestamp of the one it
/// was added to; either way the two stamps differ and the walk runs.
fn restamp(root: &Path, cached: &Stamp) -> Stamp {
    Stamp {
        dirs: cached
            .dirs
            .iter()
            .map(|d| dir_stamp(&dir_path(root, &d.path), &d.path))
            .collect(),
    }
}

struct Row {
    /// The file's own name in `Games/`, extension and all. The stem is derived from it
    /// rather than stored, so one name cannot disagree with the label it looks up.
    name: String,
    size: u64,
    mtime: i64,
    has_label: bool,
    title: String,
    code: String,
    /// `0x143`，见 `Cart::cgb`。缓存里多这一列 ⇒ 旧缓存整份作废重扫一次。
    cgb: u8,
}

struct Cache {
    stamp: Stamp,
    /// Sorted, as `list_names` returns them. Compared as a whole: the order is part of the
    /// identity, and a cache written in another order is one this code did not write.
    games: Vec<String>,
    labels: Vec<String>,
    rows: Vec<Row>,
}

impl Cache {
    /// Whether the rows account for every game the cache claims. A truncated or hand-edited
    /// file can pass the name comparison and still be missing a row, which would take a cart
    /// off the shelf until the next change happened to rewrite it.
    fn rows_agree(&self) -> bool {
        self.rows.len() == self.games.len()
            && self
                .rows
                .iter()
                .zip(&self.games)
                .all(|(row, name)| row.name == *name)
    }
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(CACHE)
}

fn read_cache(root: &Path) -> Option<Cache> {
    // Prefer the profile-private cache. Older packages kept it under System/;
    // accept that file for migration, but never write back to the old path.
    let text = std::fs::read_to_string(cache_path(root))
        .or_else(|_| std::fs::read_to_string(root.join(LEGACY_CACHE)))
        .ok()?;
    parse_cache(&text)
}

/// Best effort. A card that will not take the write still boots, it only boots slowly — which
/// is what it did before the cache existed, so there is nothing here worth failing over.
fn write_cache(root: &Path, cache: &Cache) {
    let path = cache_path(root);
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let _ = atomic_write(&path, render_cache(cache).as_bytes());
}

fn render_cache(cache: &Cache) -> String {
    let mut out = String::new();
    out.push_str(MAGIC);
    out.push('\n');
    out.push_str(&format!("dirs {}\n", cache.stamp.dirs.len()));
    for d in &cache.stamp.dirs {
        out.push_str(&format!("{}\t{}\n", escape(&d.path), d.mtime));
    }
    names_section(&mut out, "games", &cache.games);
    names_section(&mut out, "labels", &cache.labels);
    out.push_str(&format!("rows {}\n", cache.rows.len()));
    for row in &cache.rows {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            escape(&row.name),
            row.size,
            row.mtime,
            u8::from(row.has_label),
            escape(&row.title),
            escape(&row.code),
            row.cgb,
        ));
    }
    out
}

/// `<key> <count>` followed by `count` escaped names, the shape `take_names` reads.
fn names_section(out: &mut String, key: &str, names: &[String]) {
    out.push_str(&format!("{key} {}\n", names.len()));
    for name in names {
        out.push_str(&escape(name));
        out.push('\n');
    }
}

fn parse_cache(text: &str) -> Option<Cache> {
    let mut lines = text.lines();
    if lines.next()? != MAGIC {
        return None;
    }
    let dir_count: usize = lines.next()?.strip_prefix("dirs ")?.parse().ok()?;
    let mut dirs = Vec::with_capacity(dir_count);
    for _ in 0..dir_count {
        // `<escaped path>\t<mtime>`. The split is on the **first** tab and an escaped path
        // never carries a tab, so the path may be empty — the card root — and still read.
        let (path, mtime) = lines.next()?.split_once('\t')?;
        dirs.push(DirStamp {
            path: unescape(path),
            mtime: mtime.parse().ok()?,
        });
    }
    let games = take_names(&mut lines, "games")?;
    let labels = take_names(&mut lines, "labels")?;
    let count = lines.next()?.strip_prefix("rows ")?.parse().ok()?;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let mut fields = lines.next()?.split('\t');
        let row = Row {
            name: unescape(fields.next()?),
            size: fields.next()?.parse().ok()?,
            mtime: fields.next()?.parse().ok()?,
            has_label: fields.next()? == "1",
            title: unescape(fields.next()?),
            code: unescape(fields.next()?),
            cgb: fields.next()?.parse().ok()?,
        };
        if fields.next().is_some() {
            return None;
        }
        rows.push(row);
    }
    Some(Cache {
        stamp: Stamp { dirs },
        games,
        labels,
        rows,
    })
}

/// `<key> <count>` followed by `count` escaped names.
fn take_names(lines: &mut std::str::Lines<'_>, key: &str) -> Option<Vec<String>> {
    let count: usize = lines
        .next()?
        .strip_prefix(key)?
        .strip_prefix(' ')?
        .parse()
        .ok()?;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(unescape(lines.next()?));
    }
    Some(names)
}

/// The three bytes that would break the line format written out of the way. A card is the
/// user's and a rom's header text is not ours to assume printable, so neither a file name nor
/// a title is trusted to be free of them. Escaping rather than rejecting keeps a cart on the
/// shelf instead of quietly dropping it from the cache.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            // An escape this code did not write. Kept verbatim rather than dropped, so a
            // hand-edited file loses nothing it can be read back for.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// Reading the card
// ---------------------------------------------------------------------------------------

/// Names, sorted. Read as names rather than as entries because on the fast path that is all
/// the shelf needs: whether a game was added or removed is a question about the names, and
/// the answer must not cost a stat per file.
/// `recursive` walks `dir`'s subdirectories too, and the returned strings are then **paths
/// relative to `dir`** (`GBA/GBA中文/x.gba`). `Games/` is walked recursively so a card may file its
/// games per machine (or per machine and language) and still be one shelf; `Labels/` is walked
/// flat, so a card that files labels the same way is not found unless the lookup is told about
/// the subdirectory — which is why the label lookup uses `stem_of` (the *file* name's stem) either
/// way. On a flat card a relative path is just the name, so an existing card's cache still agrees.
fn list_names(dir: &Path, keep: fn(&Path) -> bool, recursive: bool) -> Result<Vec<String>, StoreError> {
    list_names_tracked(dir, keep, recursive, "", &mut Vec::new())
}

/// [`list_names`], and the timestamp of every directory the walk looked in, appended to `dirs`.
///
/// `prefix` is what those directories are called **under the card root** (`Games`, `Labels`),
/// which is what lets `scan_cached` re-stat the whole tree a boot later without listing any of
/// it. A caller that only wants the names — `scan`, the reference implementation — passes an
/// empty prefix and a list it throws away.
fn list_names_tracked(
    dir: &Path,
    keep: fn(&Path) -> bool,
    recursive: bool,
    prefix: &str,
    dirs: &mut Vec<DirStamp>,
) -> Result<Vec<String>, StoreError> {
    let mut names = Vec::new();
    collect_names(dir, Path::new(""), prefix, keep, recursive, &mut names, dirs)?;
    names.sort();
    // Sorted, because `read_dir` order is the card's business rather than ours and a stamp is
    // compared as a whole: an order that wandered between boots would miss the fast path for no
    // reason at all. Sorting here rather than at the call site keeps that a property of the walk.
    dirs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(names)
}

/// The path a subdirectory is stamped under: the parent's, a slash, and the name.
fn under(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn collect_names(
    dir: &Path,
    rel: &Path,
    dir_rel: &str,
    keep: fn(&Path) -> bool,
    recursive: bool,
    out: &mut Vec<String>,
    dirs: &mut Vec<DirStamp>,
) -> Result<(), StoreError> {
    // **Every directory the walk enters is stamped, and this is the whole point of the fix**
    // (2026-10-15): a rom or a folder added, removed or renamed moves the timestamp of the
    // directory it happened in, so a stamp that held only the two at the top could not see a
    // change one level down. Taken here, at the same moment the names below it are read, so the
    // stamp and the names cannot describe two different cards.
    dirs.push(dir_stamp(dir, dir_rel));
    let entries = match std::fs::read_dir(dir) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            // A card's own bookkeeping (System Volume Information, and a Mac's ._ sidecars) is not
            // a machine folder; walking it would only ever turn up junk.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if recursive && !name.starts_with('.') && name != "System Volume Information" {
                collect_names(
                    &path,
                    &rel.join(name),
                    &under(dir_rel, name),
                    keep,
                    recursive,
                    out,
                    dirs,
                )?;
            }
            continue;
        }
        if !keep(&path) {
            continue;
        }
        // A name that is not UTF-8 cannot be keyed by stem, labelled, or written to the cache.
        // Skipped rather than failing the shelf over it.
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let joined = rel.join(name);
        if let Some(s) = joined.to_str() {
            out.push(s.replace('\\', "/"));
        }
    }
    Ok(())
}

/// The stems of everything in `Labels/`, so a cart can be asked whether it has a face without
/// a stat of its own.
fn labels_set(dir: &Path) -> Result<HashSet<String>, StoreError> {
    Ok(label_keys(&list_names(dir, is_png, true)?))
}

/// The label files as keys: the path under `Labels/` with its `.png` taken off — `<machine>/<stem>`
/// for a card that sorts its art the way it sorts its games, `<stem>` for a flat one. Both
/// spellings live in one set, so a card that mirrors some of its art and not the rest still finds
/// every file it has, and `has_label` stays a set lookup rather than a stat per cart.
fn label_keys(names: &[String]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for n in names {
        if n.len() > 4 && n.to_ascii_lowercase().ends_with(".png") {
            keys.insert(n[..n.len() - 4].to_string());
        }
    }
    keys
}

/// The machine a path under `Games/` names — **the folder first, the extension second**.
///
/// One function for both shapes a path arrives in, because there are two and they must not
/// answer differently:
///
/// * a name straight out of the scan, relative to `Games/` (`GBA/Emerald.gba`), where the
///   machine folder is the **first** component;
/// * a cart's `rom`, which is that name joined onto the absolute `Games` directory, where the
///   machine folder is whatever **follows** `Games`.
///
/// A rom filed directly in `Games/` — the shape of every card written before the machine
/// folders existed — has no machine folder in either shape, so it falls through to the
/// extension and lands where it always did.
fn machine_of_path(p: &Path) -> Option<System> {
    if let Some(dir) = machine_dir_name(p) {
        if let Some(s) = System::from_dir(dir) {
            return Some(s);
        }
    }
    p.extension()
        .and_then(|e| e.to_str())
        .and_then(System::from_extension)
}

/// The name of the folder a rom sits in, as described on [`machine_of_path`].
fn machine_dir_name(p: &Path) -> Option<&str> {
    let parts: Vec<&str> = p
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    // `Games` is matched without case because the card may have been written anywhere, and
    // `position` rather than `rposition` because the machine folder is the one *directly*
    // under it — a folder called Games deeper in the tree is not this tree's root.
    let start = parts
        .iter()
        .position(|c| c.eq_ignore_ascii_case("Games"))
        .map(|i| i + 1)
        .unwrap_or(0);
    parts.get(start).copied()
}

/// Extensions that are never a rom, not even inside a machine folder.
///
/// The folder decides the *machine*, but it cannot decide whether the file is a rom at all —
/// `Games/GBA/readme.txt` is in the Game Boy Advance folder and is still a readme. So a file
/// whose machine comes from its folder is still checked against this list, and a file whose
/// machine comes from its extension never needed it (an extension that maps to a machine is
/// by definition not on it).
///
/// Notably **absent**: `zip`, `7z`, `cue`, `chd` and the rest of the archive and disc formats.
/// A zip in `NEOGEO/` is FBNeo's own romset and the core reads it itself; a zip anywhere else
/// is a rom the frontend has not learned to unpack yet, which is a load error rather than a
/// reason to hide it from the shelf.
const NOT_A_ROM: &[&str] = &[
    // notes and links a user drops in beside the roms
    "txt", "md", "nfo", "pdf", "url", "lnk", "diz",
    // metadata and configuration
    "db", "dat", "ini", "cfg", "conf", "json", "xml", "yml", "yaml", "toml",
    // images — art lives in `Labels/`, not beside the rom
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico",
    // saves and states, which have their own two folders
    "sav", "srm", "state", "rtc", "eep", "fla", "mpk",
    // scripts
    "exe", "bat", "cmd", "sh", "ps1", "py",
    // ★ 2026-10-25：**一张卡自己的 BIOS。** PS1 汉化/改版常常要配一版特定 BIOS，约定是把它
    // 放在游戏旁边、与 rom **同名**（`Ridge.cue` ↔ `Ridge.bios`），见 `slot::core`。它不是
    // 一张卡：落在 `Games/PS1/` 里必须是"这张卡带的附件"，而不是"多出来的一张卡"。
    // **不进 `COLLIDING_ROM_EXTS`** —— 那个名单是"在某些地方也是 rom"的豁免表，`bios`
    // 两个地方都不是 rom。
    "bios",
];

/// Whether this file is a cart at all.
///
/// A file is a cart when its machine can be determined, and its machine can be determined
/// either way: by the folder it sits in (any extension but the ones on [`NOT_A_ROM`]) or, for
/// a rom filed directly in `Games/`, by its extension. What used to be a plain extension test
/// is now the second half of that, kept exactly for the cards that still need it.
fn is_rom(p: &Path) -> bool {
    if is_hidden(p) {
        return false;
    }
    let ext = p.extension().and_then(|e| e.to_str());
    let in_machine_folder = machine_dir_name(p).and_then(System::from_dir).is_some();
    // ★ **`.md` and `.ps1` are roms here and a readme and a shell script there** (2026-10-25,
    // the day MD and PS1 arrived). Both spellings are on `NOT_A_ROM`, and both are also a
    // Mega Drive extension and the PowerShell one, so one rule cannot answer for both places:
    //
    // * **inside `Games/MD/` or `Games/PS1/`** the folder is an explicit statement of what the
    //   file is, so the extension collision is ignored and the file is taken. (A stray
    //   `readme.md` filed in `Games/MD/` becomes a cart. That is the price, and it is a
    //   deliberately small one: nobody files notes *inside* a machine folder, they file them
    //   beside the roms at the top of `Games/`.)
    // * **directly in `Games/`** there is no folder to say anything, so the extension is the
    //   only evidence there is — and for these two the honest reading of a bare `.md` beside
    //   roms is a readme, not a Mega Drive cart. They stay refused.
    let collides = ext.is_some_and(|e| COLLIDING_ROM_EXTS.iter().any(|n| e.eq_ignore_ascii_case(n)));
    let not_a_rom =
        ext.is_some_and(|e| NOT_A_ROM.iter().any(|n| e.eq_ignore_ascii_case(n)))
            && !(in_machine_folder && collides);
    if in_machine_folder {
        return !not_a_rom;
    }
    !not_a_rom && ext.is_some_and(|e| System::from_extension(e).is_some())
}

/// Extensions that are a machine's rom **and** something else entirely, and so can only be
/// taken when a machine folder vouches for them. See [`is_rom`].
const COLLIDING_ROM_EXTS: &[&str] = &["md", "ps1"];

/// The machine folder a rom name belongs in — the canonical name, for a label lookup.
///
/// Same decision as [`Cart::system`], spelled as the folder a label is filed under, and read
/// off the same [`machine_of_path`] so a cart and its art can never disagree about which
/// machine they belong to.
fn machine_of(name: &str) -> Option<&'static str> {
    machine_of_path(Path::new(name)).map(System::dir_name)
}

/// Where a cart's art is, if the card carries it: `Labels/<machine>/<stem>.png` on a sorted card,
/// else the flat `Labels/<stem>.png`. `keys` is what `label_keys` built from one directory walk.
fn label_for(
    labels_dir: &Path,
    machine: Option<&str>,
    stem: &str,
    keys: &HashSet<String>,
) -> Option<PathBuf> {
    if let Some(machine) = machine {
        if keys.contains(&format!("{machine}/{stem}")) {
            return Some(labels_dir.join(machine).join(format!("{stem}.png")));
        }
    }
    keys.contains(stem)
        .then(|| labels_dir.join(format!("{stem}.png")))
}

/// Where a cart's whole-cart art is, if the card carries it: `Cartridge/<machine>/<stem>.png`
/// on a sorted card, else the flat `Cartridge/<stem>.png` — the same two-level rule
/// `label_for` reads a sticker by, because it is the same kind of file: one picture per game,
/// named after the rom's stem.
///
/// This is the **pure** cart (2026-10-23, 六六): the finished picture, label and all, with no
/// magenta in it and nothing pasted at run time. The frontend draws a cart that has one from
/// the file alone — when the art source says it outranks the sticker; a cart without one is
/// the composite, unchanged. See `slot_store::ArtSource`.
///
/// **Its own folder, not `System/Carts/`**: that one holds the fallback shell templates the
/// composite path draws, and a game's picture filed among them would sit beside the
/// machine's own. `Cartridge/` is made by `root::ensure` — per machine, the way `Games/` is —
/// and a flat file directly under it is read too, for a card that sorts nothing.
///
/// Stat'ed rather than walked. The two candidates are the whole search, so a `metadata` each
/// is cheaper than the directory walk a third names list would need — and it stays true on a
/// cache hit, where a names list would have to be carried through the cache format to say
/// the same thing. That is also why the cache gains no column for this.
fn cart_art_for(carts_dir: &Path, machine: Option<&str>, stem: &str) -> Option<PathBuf> {
    if let Some(machine) = machine {
        let in_machine = carts_dir.join(machine).join(format!("{stem}.png"));
        if in_machine.is_file() {
            return Some(in_machine);
        }
    }
    // The flat names the machine's own art and its templates use are not a game's: a rom
    // called `cart_gba` must not be handed the template as its own finished picture.
    if stem.starts_with("cart_") || stem.starts_with("tpl_") || stem.starts_with("board_") {
        return None;
    }
    let flat = carts_dir.join(format!("{stem}.png"));
    flat.is_file().then_some(flat)
}

fn stem_of(name: &str) -> Option<String> {
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
}

/// Cart and cache row for every name, in one pass.
///
/// `cached` is the previous run's rows. A name whose size and timestamp are the ones the
/// cache already has keeps the header it came with, which is what makes a card with one new
/// game on it cost one header read rather than the whole shelf.
fn build(
    root: &Path,
    games: &[String],
    labels: &HashSet<String>,
    cached: Option<&[Row]>,
) -> (Vec<Cart>, Vec<Row>) {
    let games_dir = root.join("Games");
    let labels_dir = root.join("Labels");
    let carts_dir = root.join("Cartridge");
    let mut carts = Vec::with_capacity(games.len());
    let mut rows = Vec::with_capacity(games.len());

    for name in games {
        let rom = games_dir.join(name);
        let meta = std::fs::metadata(&rom).ok();
        let (size, mtime) = meta.as_ref().map_or((0, 0), |m| (m.len(), mtime_secs(m)));
        let known = cached.and_then(|rows| {
            rows.iter()
                .find(|r| r.name == *name && r.size == size && r.mtime == mtime)
        });
        // Which machine this rom is for, read the same way `Cart::system` reads it — the
        // folder first, the extension only for a rom filed directly in `Games/`. `header`
        // needs it to know which bytes of the file are the title, so it has to agree.
        let system = machine_of_path(&rom);
        let (title, code, cgb) = match known {
            Some(row) => (row.title.clone(), row.code.clone(), row.cgb),
            // `read_header` 一次给出 title / code / CGB 标志 —— 三件事一次 open。
            // `None`（读不到头）当 0：壳型退回机种默认，与从前一致。
            None => {
                let (t, c, g) = crate::gba::read_header(&rom, system);
                (t, c, g.unwrap_or(0))
            }
        };
        let stem = stem_of(name).unwrap_or_default();
        // `machine_of` is the same decision as the `system` above, spelled as the folder a
        // label is filed under.
        let machine = machine_of(name);
        let label = label_for(&labels_dir, machine, &stem, labels);
        // The whole-cart picture, if the card carries one for this game. `None` for the
        // normal card, and then the frontend dresses this cart exactly as it always has.
        let art = cart_art_for(&carts_dir, machine, &stem);
        rows.push(Row {
            name: name.clone(),
            size,
            mtime,
            has_label: label.is_some(),
            title: title.clone(),
            code: code.clone(),
            cgb,
        });
        carts.push(Cart {
            initial: bucket(&stem),
            stem,
            rom,
            label,
            art,
            cgb,
            title,
            code,
        });
    }
    order(&mut carts);
    (carts, rows)
}

/// Which letter ring bucket a cart belongs in.
///
/// From the name the shelf *shows*, which is the file name with its tags taken off — not from
/// the header title. The header title is twelve characters of the publisher's language, so a
/// card of Chinese file names filed under it puts POKEMON EMER and ROCKMANZERO on the ring and
/// jumps to a letter that matches nothing anybody can see. The label, the line of type under the
/// row and this all read the same string, so they cannot disagree.
fn bucket(stem: &str) -> char {
    crate::pinyin::initial(&crate::name::clean_label_stripped(stem))
}

/// The carts a cache holds, rebuilt without touching a rom.
fn carts_from_rows(root: &Path, rows: &[Row], labels: &HashSet<String>) -> Vec<Cart> {
    let games_dir = root.join("Games");
    let labels_dir = root.join("Labels");
    let carts_dir = root.join("Cartridge");
    let mut carts: Vec<Cart> = rows
        .iter()
        .map(|row| {
            let stem = stem_of(&row.name).unwrap_or_default();
            let label = row
                .has_label
                .then(|| label_for(&labels_dir, machine_of(&row.name), &stem, labels))
                .flatten();
            // Stat'ed live rather than carried in the cache: the two candidates are the
            // whole search, so a cache hit answers as truly as a fresh walk does.
            let art = cart_art_for(&carts_dir, machine_of(&row.name), &stem);
            Cart {
                initial: bucket(&stem),
                stem,
                rom: games_dir.join(&row.name),
                label,
                art,
                cgb: row.cgb,
                title: row.title.clone(),
                code: row.code.clone(),
            }
        })
        .collect();
    order(&mut carts);
    carts
}

/// By letter, then by stem, then by file name.
///
/// The letter comes first because the ring above the shelf is an index, and an index is only an
/// index if its entries are in order. File names are hanzi, and hanzi in code point order have
/// nothing to do with the order they are read in — `宝` sorts before `恶` and `洛` comes after
/// both, but the pinyin runs B, E, L — so a shelf sorted by file name has its B carts scattered
/// down the row and stepping the dial to C lands *behind* where the user is standing. Grouped,
/// the row reads A, B, C left to right, down is always forwards and up is always back, and a
/// letter is a run of carts rather than a hop.
///
/// Within a letter it is the file name, which is the order the shelf had before — the grouping
/// is the only thing that changed. That order is a code point one and not a pinyin one, and it
/// is the honest place to stop: sorting `宝可梦` before `白色` needs every character's full
/// pinyin, and the card carries only the first letter of each.
///
/// The file name breaks the tie after the stem so a card holding both `Foo.gba` and `Foo.GBA`
/// comes out the same way on every boot rather than in whatever order the directory handed them
/// over.
///
/// `char` order is the ring's order — `#` is below `A` — so the shelf and the dial agree on
/// which letter comes first without either of them being told.
fn order(carts: &mut [Cart]) {
    carts.sort_by(|a, b| {
        a.initial
            .cmp(&b.initial)
            .then_with(|| a.stem.cmp(&b.stem))
            .then_with(|| a.rom.cmp(&b.rom))
    });
}

/// The modification time of anything — a directory for the stamp, the cache file for the moment
/// the stamp was taken. `0` for a path that will not stat, which every caller treats as "cannot
/// be trusted" rather than as a time.
fn mtime_of(p: &Path) -> i64 {
    std::fs::metadata(p).map_or(0, |m| mtime_secs(&m))
}

/// Seconds, or 0 for a filesystem that will not say. Zero is a value the stamp can hold and
/// compare like any other: a card whose directory timestamps are unreadable simply never
/// matches, which is a slow boot rather than a wrong shelf.
///
/// ⚠️ **Whole seconds, so this is a coarse clock on purpose and the walk is what makes it safe.**
/// Two changes inside one second are one number here, and VFAT's own 2-second steps make it
/// coarser still — see [`Stamp::stands_alone`], which is where that is allowed for and where the
/// only fast path that trusts it proves it may.
fn mtime_secs(m: &Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

fn is_png(p: &Path) -> bool {
    !is_hidden(p)
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// A leading dot is card metadata rather than content, and every folder on the card is read
/// through this. macOS writes `._<name>` beside each file it copies onto a FAT volume, which
/// carries the extension of the file it shadows, so the extension alone cannot tell them
/// apart. It also sorts first, which is why the sidecar rather than the file is what a picker
/// walking the folder in order tends to land on.
pub fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

#[cfg(test)]
mod stamp_tests {
    use super::*;

    /// A stamp of one directory, which is all any of these need: the guard is per directory and
    /// the cases below are about one timestamp at a time.
    fn stamp(t: i64) -> Stamp {
        Stamp {
            dirs: vec![DirStamp {
                path: "Games".to_string(),
                mtime: t,
            }],
        }
    }

    /// **The guard as arithmetic**, which is the boundary the three `…_after_the_first_scan_…`
    /// cases in `tests/scan.rs` exercise from the outside: a stamp that is not at least the
    /// coarseness older than the cache is not allowed to stand in for the names under it.
    #[test]
    fn a_fresh_stamp_never_stands_alone() {
        let cache = 1_000_000;
        for age in 0..=Stamp::COARSENESS + Stamp::SLACK {
            let t = cache - age;
            assert!(
                !stamp(t).stands_alone(cache),
                "a stamp {age}s old must not stand alone"
            );
        }
        let t = cache - (Stamp::COARSENESS + Stamp::SLACK + 1);
        assert!(stamp(t).stands_alone(cache));
    }

    /// **Every** directory has to be settled, which is the fix for the ghost entry: the walk is
    /// recursive, so a cart's rom can be removed from a folder the top of the tree never hears
    /// about. A stamp that let one fresh directory through with the rest would be trusting a
    /// name it did not check.
    #[test]
    fn every_directory_has_to_be_settled() {
        let cache = 1_000_000;
        let old = cache - 100;
        let dir = |path: &str, mtime: i64| DirStamp {
            path: path.to_string(),
            mtime,
        };
        let both = |a: i64, b: i64| Stamp {
            dirs: vec![dir("Games", a), dir("Games/GBA/中文", b)],
        };
        assert!(!both(old, cache).stands_alone(cache));
        assert!(!both(cache, old).stands_alone(cache));
        assert!(both(old, old).stands_alone(cache));
    }

    /// A path that would not stat answers zero, and zero must never read as "very old" — that is
    /// the card whose timestamps are unreadable, where the walk is the only guard there is.
    #[test]
    fn an_unreadable_timestamp_never_stands_alone() {
        assert!(!stamp(0).stands_alone(1_000_000));
        assert!(!stamp(0).stands_alone(0));
        assert!(!Stamp { dirs: Vec::new() }.stands_alone(1_000_000));
    }

    /// A directory that is **not there** is a different fact from one that will not say its
    /// time, and it does stand alone: a card with no `Labels/` is a card. Anything that appears
    /// inside it later has to create the directory first, which moves the timestamp of the
    /// directory above — so this cannot be used to hide a change.
    #[test]
    fn an_absent_directory_stands_alone() {
        let cache = 1_000_000;
        assert!(stamp(DirStamp::ABSENT).stands_alone(cache));
        // But only alongside a real one: a stamp that names nothing vouches for nothing.
        let mut s = stamp(DirStamp::ABSENT);
        s.dirs.push(DirStamp {
            path: "Games".to_string(),
            mtime: 0,
        });
        assert!(!s.stands_alone(cache));
    }

    /// The stamp the walk writes and the stamp a cache hit re-reads have to be the same shape,
    /// or every boot would look like a changed card. The directories are namespaced under the
    /// card root and sorted, so `read_dir`'s order cannot leak into what the cache compares.
    #[test]
    fn a_restamp_of_an_unchanged_tree_is_the_same_stamp() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Games/GBA")).unwrap();
        let mut dirs = Vec::new();
        let names = list_names_tracked(&d.path().join("Games"), is_rom, true, "Games", &mut dirs)
            .expect("walk");
        assert!(names.is_empty());
        let stamp = Stamp { dirs };
        let paths: Vec<&str> = stamp.dirs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, vec!["Games", "Games/GBA"]);
        assert_eq!(
            restamp(d.path(), &stamp),
            stamp,
            "a card that did not change re-read as a different stamp"
        );
    }

    /// The whole-cart picture is found the way a sticker is: the machine's own folder first
    /// on a sorted card, the flat `Carts/` root otherwise — and the machine's own art and
    /// its templates are never mistaken for a game's, however the rom happens to be named.
    #[test]
    fn a_whole_cart_picture_is_found_by_stem() {
        let d = tempfile::tempdir().unwrap();
        let carts = d.path().join("Cartridge");
        std::fs::create_dir_all(carts.join("GBA")).unwrap();
        // The machine's own art and a template, which no game may be handed.
        std::fs::write(carts.join("cart_gba.png"), b"machine").unwrap();
        std::fs::write(carts.join("tpl_metal_gba.png"), b"template").unwrap();
        // A game's own, both spellings.
        std::fs::write(carts.join("GBA/POKEMON EMER.png"), b"in-machine").unwrap();
        std::fs::write(carts.join("ROCKMAN ZERO.png"), b"flat").unwrap();

        assert_eq!(
            cart_art_for(&carts, Some("GBA"), "POKEMON EMER"),
            Some(carts.join("GBA/POKEMON EMER.png")),
            "the machine's own folder wins on a sorted card"
        );
        assert_eq!(
            cart_art_for(&carts, None, "ROCKMAN ZERO"),
            Some(carts.join("ROCKMAN ZERO.png")),
            "the flat root answers for a rom filed outside a machine folder"
        );
        assert_eq!(
            cart_art_for(&carts, Some("GBA"), "NO SUCH GAME"),
            None,
            "a cart without the file is the normal card"
        );
        assert_eq!(
            cart_art_for(&carts, None, "cart_gba"),
            None,
            "the flat names the machine art uses are not a game's"
        );
        assert_eq!(cart_art_for(&carts, None, "tpl_metal_gba"), None);
        assert_eq!(cart_art_for(&carts, None, "board_gba"), None);
    }
}

/// Whether a path is a cart, decided on the name alone — **no filesystem, on purpose.**
///
/// ★ 2026-10-25: `bios` joined `NOT_A_ROM` so a PlayStation cart's own `.bios` stops being a
/// second cart on the shelf. These cases are the whole of that rule, and they are written
/// without a `tempdir` because `is_rom` never touches one: the decision is the extension and
/// the folder, and neither needs the file to exist. (The scanning path itself is covered in
/// `crates/slot/tests/ps1_bios.rs`, which needs a real card.)
#[cfg(test)]
mod is_rom_tests {
    use super::*;

    #[test]
    fn a_bios_is_never_a_cart() {
        for p in [
            "Games/PS1/Ridge Racer.bios",
            "Games/PS1/scph5501.bios",
            "Games/GBA/Emerald.bios",
            // Directly in `Games/`, where only the extension can say anything — and it says no.
            "Games/scph5501.bios",
            // Upper case too: a card written on a desktop is still a card.
            "Games/PS1/RIDGE.BIOS",
        ] {
            assert!(!is_rom(Path::new(p)), "{p} was taken as a cart");
        }
    }

    /// And the file it belongs to still is one — the rule is about the `.bios`, not about the
    /// machine folder having gone quiet.
    #[test]
    fn a_cue_beside_a_bios_is_still_the_cart() {
        assert!(is_rom(Path::new("Games/PS1/Ridge Racer.cue")));
        assert!(is_rom(Path::new("Games/PS1/track01.bin")));
    }

    /// `bios` is **not** on the collision list: that list is for extensions that are a rom in
    /// some places (`md` is a Mega Drive cart and markdown, `ps1` a Mega Drive cart too and a
    /// shell script). A `.bios` is not a rom anywhere, so machine-folder membership must not
    /// excuse it.
    #[test]
    fn the_collision_rule_does_not_excuse_a_bios() {
        assert!(!COLLIDING_ROM_EXTS.contains(&"bios"));
        // The proof that the rule is what stands between us and a phantom cart: inside a
        // machine folder everything not on `NOT_A_ROM` is a cart.
        assert!(is_rom(Path::new("Games/PS1/whatever.xyz")));
    }

    /// ★ 2026-10-25：`.pce` 是 PCE 独有的扩展名，所以它是唯一一个在机种文件夹之外
    ///   也认得的 PC Engine 证据。`.bin` 和 `.cue` 与 MD / PS1 撞车，一律不进这里 ——
    ///   那是 `System::from_extension` 的判断，不是 `is_rom` 的。
    #[test]
    fn a_pce_is_a_cart_wherever_it_is_filed() {
        assert!(is_rom(Path::new("Games/PCE/Blazing Champions.pce")));
        assert!(is_rom(Path::new("Games/Blazing Champions.pce")));
        assert_eq!(
            System::from_extension("pce"),
            Some(System::Pce),
            "a .pce outside a machine folder has to name its own machine"
        );
    }
}
