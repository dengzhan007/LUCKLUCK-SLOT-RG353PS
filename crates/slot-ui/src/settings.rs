//! The settings screen behind MENU → Settings: what the *device* is set to, as opposed to what
//! the card holds.
//!
//! **A list, not a grid.** `menu.rs` is a grid because "where does this button go" is the only
//! question a grid answers well; every row here is the other kind — a name and the value it is
//! currently set to — and a grid of buttons would make the user open a screen to find out what
//! a setting already is.
//!
//! ★ **The width is the menu's**, deliberately: the same `MARGIN` of 30, the same 660-wide rows,
//! and the same hint bar pinned at the foot drawn by the caller. A settings screen that sat at a
//! different width from the screen that opens it would read as a different machine (2026-10-06
//! 六六：「行宽度应该对齐上面的关于页贴纸」—— that lesson is the menu's, and this screen inherits
//! it rather than re-learning it). **The sticker itself is not inherited** (2026-10-10, below):
//! the rows align to the sticker's edges; they do not need its picture above them.
//!
//! ★ **Every face is baked at boot, one pair per value.** A row's value changes under a keypress,
//! and rasterising a plate on a keypress is the one thing a plate is expensive at: the rounded
//! corners are a per-pixel pass on a 660-wide bar. Two sets of three gears, four lid values, one
//! word and two switches is fifteen pairs, ~80 KB each, which is cheaper than the shelf's own
//! furniture — and cheaper than the shelf's was, because `menu::BUTTON_H` came down 6 px on the
//! same day.
//!
//! ★ **2026-10-10: no sticker, and the list fills the page.** The menu's sticker was a second
//! copy of a label the user had just tapped through — 六六:「设置面板页就不用加『关于』贴纸了吧
//! 有点多余」— so the list starts at `TOP` = 26 (the shortcut card's own top, not the menu's
//! `PAD + STICKER_H + LABEL_GAP`) and five rows fill the page down to the bar:
//! `26 + 5·70 + 4·10 = 416` to the pixel. The window machinery below is kept as headroom rather
//! than deleted: it is inert at five rows (`scroll_max` = 0), and the first row of a sixth scrolls
//! the list by exactly one pitch — still **derived from which row is in hand** (`scroll_for`)
//! rather than a second piece of state the input has to keep in step.
//!
//! ★ **2026-10-16: the sixth row arrived** (「加速时声音」), and it is exactly the row the
//! machinery above was kept for: `scroll_max` is one pitch (80), and holding it walks the list
//! up by that much — the first row slides out under the window's top and the rest stay put.
//! Nothing about the window changed; only the list got longer than it.

use crate::draw::{Draw, TexId};
use crate::menu::MARGIN;
use crate::palette;
use crate::plate::UndoFace;
use crate::rounded;
use crate::slot_chrome::{edge, housing, opening};
use crate::text;

/// What a row is for.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Row {
    /// ★ 2026-10-10: how many core frames one present runs while R2 is held. Four things, and
    /// the one that is **not** here is the point: eight was measured and bought nothing (see
    /// `slot_store::FF_SPEED_CHOICES`), so the list is the honest gears rather than every
    /// number the multiplier could be. **2026-10-28 六六：label 改「快进倍率」** —— 描述说的是
    /// 档位，不是动作。
    FastForward,
    /// ★ 2026-10-10: how many snapshots one present pops while L2 is held. Powers of two,
    /// because that is what the ring can do — see `slot_store::REWIND_SPEED_CHOICES`.
    /// **2026-10-28 六六：label 改「回溯倍率」** —— 同上一行，说的是档位。
    Rewind,
    /// The clock picker, reopened. It used to be reachable exactly once, on slot's own first
    /// launch, and never again; `A` here is the way back to it (2026-10-10, 六六).
    DateTime,
    /// How long the dark panel waits before the machine actually stops — `slot_store`'s
    /// `DOZE_MIN_CHOICES`, so the list is the card's and this screen only prints it.
    /// **2026-10-28 六六：label 改「息屏后关机」** —— 原来的「息屏关机时间」读起来像"息屏
    /// 关机的时长"，而这一行说的是"息屏之后多久才关机"，词序反了。
    Sleep,
    /// ★ 2026-10-16: whether fast forward and rewind keep playing the game's audio. **The second
    /// switch**, appended at the end like the badge was, and the first row about how the frontend
    /// *sounds*. Off is the old behaviour outright — the worker drops the core's audio while
    /// speeding (see `emu::Worker::run`), because resampling it down to real time is pitch-shifted
    /// noise; on time-compresses it with the pitch kept (`stretch`).
    Sound,
    /// ★ 2026-10-17 六六：「MENU页设置添加刷新本机ROM功能」. **Not a setting and not a switch** —
    /// a door, like `DateTime`, and the third of the three ways a row can be one (`opens`).
    /// Everything else on this screen says what the machine *is set to*; this one says "look at
    /// the card again". A card filled while the device was on — over the network, or by pulling
    /// it and copying games onto it — is otherwise invisible until the **next boot**, because the
    /// boot's own scan is the only one there is (`scan_cached` is called exactly once, there).
    /// **2026-10-28 六六：「选项去掉仅留描述名」** —— 这一行没有"当前值"可显示，`A` 就是做，
    /// 所以只留名字，值位留空。
    Refresh,
    /// ★ 2026-10-28 六六：插卡 / 开机卡槽有卡 → 自动读 `resume.state` 续玩。和游戏内菜单的「即时读档」
    /// 手动行分开：这一行是"自动触发"，前缀「插卡」把"自动"和"手动"区分开，不跟即时读档撞名。
    InsertResume,
    /// ★ 2026-10-28 六六：拔卡 → 把当前画面存进 `resume.state`（续玩快照）。原生 `.sav` 不受此开关影响，
    /// `lid` / 关机 / 自动存档那条也不受（它们走 `flush_resume`，不清槽）。和「即时存档」手动行区分。
    EjectSave,
    /// ★ 2026-10-20 六六：「开机画面切换」。**一张列表，不是一个开关**：值是卡上
    /// `Bootlogos/` 里那些图的名字 —— 图由用户自己放到卡上（或用工具箱转好），这里只负责挑。
    ///
    /// **2026-10-28 六六：方向键全给光标，`A` 循环改值**。原先是 `←`/`→` 浏览、`A` 替换、`B`
    /// 取消（本屏唯一这么走的一行）；现在 `←`/`→` 让给光标移动，所以这一行退成普通列表行：
    /// `A` 换下一张并写盘（`apply_boot`），失败不动、回当前那张。
    Boot,
    /// ★ 2026-10-21 六六：**Wi-Fi 配置**，一道门（`opens`），`A` 打开配置浮层。它的值也只有
    /// 两个词 —— 卡上有没有 `wifi.conf`。SSID 不打印在这一行：它长短不一，而这一行是个定宽
    /// 的盘子；名字在浮层里看，和键位表一个道理。
    Wifi,
}

impl Row {
    /// ★ **2026-10-28 六六定的行序**：双列后每行两个，用户按屏幕从上到下给的顺序 ——
    /// `快进倍率/回溯倍率`、`日期与时间/息屏后关机`、`加速时声音/刷新游戏库`、
    /// `插卡续玩/拔卡续存`、`开机画面/无线网络`。删了「操作键映射」（游戏中有入口）与
    /// 「美术资源」（功能已迭代掉），且把插卡/拔卡续玩档位提前、开机画面/无线网络压到末行。
    /// ★ **2026-10-28 第二轮：`日期与时间` 与 `加速时声音` 对调**（六六）。日期与时间去掉
    ///   「校准」描述项后成了"只留名字"的行（和刷新同类）；声音行是开关，放它上面读起来
    ///   更像"声音属于机器"的一组。
    /// ★ **2026-10-28 第三轮：第三行与第一行对调**（六六：「第三行…和第一行对调后推送」）。
    ///   第三行是 `日期与时间/刷新游戏库`（刚去掉校准的那行），第一行是 `快进倍率/回溯倍率`；
    ///   对调后两行"只留名字/开门"的行（日期/刷新）抬到顶上，倍速两行落到第三。
    ///   新序：`日期与时间/刷新游戏库`、`加速时声音/息屏后关机`、`快进倍率/回溯倍率`、
    ///   `插卡续玩/拔卡续存`、`开机画面/无线网络`。
    pub const ALL: [Row; 10] = [
        Row::DateTime,
        Row::Refresh,
        Row::Sound,
        Row::Sleep,
        Row::FastForward,
        Row::Rewind,
        Row::InsertResume,
        Row::EjectSave,
        Row::Boot,
        Row::Wifi,
    ];

    /// The row's place in `Row::ALL` — **not its discriminant**.
    ///
    /// ★ **2026-10-28 事故修复**：这个值一度是 `self as usize`（枚举判别值），而 `Row::ALL`
    ///   的顺序在行序重排后与枚举定义顺序**脱钩**（枚举仍是 `FastForward=0, Rewind=1,
    ///   DateTime=2, …`，`ALL` 却已排成 `DateTime, Refresh, Sound, Sleep, FastForward, …`）。
    ///   判别值 ≠ `ALL` 下标 ⇒ `draw_settings` 用 `index()` 取 `settings_rows`（按 `ALL` 顺序
    ///   构建）错位、`up/down/left/right` 的网格算术错位 ⇒ 「日期与时间」整格消失（实际是
    ///   FastForward 用判别值 0 取到了 DateTime 的 1 元素值列表，而它的当前档位是 2，越界
    ///   返回 `None`，连盘子都不画）+ 方向键导航乱套。第一轮 `ALL` 恰好按判别值顺序排，
    ///   掩盖了这个错位；第二轮起重排后暴露。
    ///   ⇒ 这个值必须是 **`ALL` 里的位置**，谁用它都按同一张表走。
    pub fn index(self) -> usize {
        Row::ALL.iter().position(|r| *r == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Row::FastForward => crate::lang::SETTINGS_FF,
            Row::Rewind => crate::lang::SETTINGS_REWIND,
            Row::DateTime => crate::lang::SETTINGS_DATETIME,
            Row::Sleep => crate::lang::SETTINGS_SLEEP,
            Row::Sound => crate::lang::SETTINGS_FF_SOUND,
            Row::Refresh => crate::lang::SETTINGS_REFRESH,
            Row::InsertResume => crate::lang::SETTINGS_INSERT_RESUME,
            Row::EjectSave => crate::lang::SETTINGS_EJECT_SAVE,
            Row::Boot => crate::lang::SETTINGS_BOOT,
            Row::Wifi => crate::lang::SETTINGS_WIFI,
        }
    }

    /// Whether `A` **opens or acts** rather than changing the value in place. A row that does
    /// the thing under `A` — the clock picker, the refresh, the Wi-Fi form — is the other kind
    /// of row from a switch or a list, and the screen's foot line says so.
    ///
    /// ★ **2026-10-28**: the direction keys are all for the caret now (`↑`/`↓` move a row,
    /// `←`/`→` move a column), so the only key a value change gets is `A`. A row that is not
    /// `opens` changes under `A`; a row that is does its thing under `A`. The boot row used to
    /// be a third kind (browse with `←`/`→`, apply with `A`) and is now an ordinary list — `A`
    /// steps it and writes, which is why it is no longer `opens`.
    pub fn opens(self) -> bool {
        matches!(self, Row::DateTime | Row::Refresh | Row::Wifi)
    }

    /// **Which of the foot lines this row wants.** An index rather than a `bool` since the
    /// refresh row arrived (2026-10-17), and an index rather than the string itself because the
    /// caller stores one baked face per line and picks between them by this number.
    ///
    /// ★ **2026-10-28**: `A` is the only key that does anything but move the caret, so the lines
    /// collapse to three — change under `A` (a list or a switch), open a screen under `A` (the
    /// clock, the Wi-Fi form), and act under `A` (the refresh). The boot row's own fourth line
    /// (browse / apply / cancel) is gone with its old keys.
    pub fn hint_index(self) -> usize {
        match self {
            Row::DateTime | Row::Wifi => HINT_INDEX_OPEN,
            Row::Refresh => HINT_INDEX_ACT,
            _ => HINT_INDEX_CHANGE,
        }
    }

    /// The line pinned at the foot while this row is in hand.
    pub fn hint(self) -> &'static str {
        match self.hint_index() {
            HINT_INDEX_OPEN => HINT_OPEN,
            HINT_INDEX_ACT => HINT_ACT,
            // Kept for the boot row's old four-line scheme; no row asks for it since 2026-10-28,
            // but the caller still uploads four lines and this keeps the constant referenced.
            HINT_INDEX_PICK => HINT_PICK,
            _ => HINT_CHANGE,
        }
    }

    /// ★ **2026-10-28: `↑` moves a whole row, not one entry** — the caret stays in its column
    /// and wraps at the top of the grid, so `A`/the two columns cannot swap the user's muscle
    /// memory for what "up" means. The grid is row-major (`col = i % COLS`), and a wrap keeps
    /// the column: from the top row the caret lands on the bottom row's same column.
    pub fn up(self) -> Row {
        let i = self.index();
        let col = i % COLS;
        let rows = (Row::ALL.len() + COLS - 1) / COLS;
        let row = i / COLS;
        let ni = ((row + rows - 1) % rows * COLS + col).min(Row::ALL.len() - 1);
        Row::ALL[ni]
    }

    /// ★ **2026-10-28: `↓` moves a whole row down, column kept** — the mirror of `up`.
    pub fn down(self) -> Row {
        let i = self.index();
        let col = i % COLS;
        let rows = (Row::ALL.len() + COLS - 1) / COLS;
        let row = i / COLS;
        let ni = (((row + 1) % rows) * COLS + col).min(Row::ALL.len() - 1);
        Row::ALL[ni]
    }

    /// ★ **2026-10-28: `←` moves to the left column.** Only a right-column row moves (a left-
    /// column row is already at the edge); the caret stays put otherwise. No wrap — a caret at
    /// the left edge has nowhere to go.
    pub fn left(self) -> Row {
        let i = self.index();
        if i % COLS == 0 {
            self
        } else {
            Row::ALL[i - 1]
        }
    }

    /// ★ **2026-10-28: `→` moves to the right column**, the mirror of `left`.
    pub fn right(self) -> Row {
        let i = self.index();
        if i % COLS == COLS - 1 {
            self
        } else {
            Row::ALL[i + 1]
        }
    }
}

/// The foot lines. Here rather than in `lang.rs` for the reason `menu::HINT` is: they are a
/// description of *this* screen's controls, and they read wrong away from it.
///
/// Words rather than arrow glyphs — the same choice the menu's own foot line makes. The arrow
/// characters are not in the label face's reduced set, and a hint line missing two glyphs is a
/// hint line that says nothing.
///
/// ★ **An index each, since 2026-10-17**, because there are three lines now and the caller picks
/// between them by number: `settings::HINT_CHANGE` / `HINT_OPEN` / `HINT_ACT` are uploaded in
/// `hint_index` order and `Row::hint_index` is what chooses. A `bool` was enough while the only
/// question was "does this row open a screen". **Four since 2026-10-20**, when the boot row
/// arrived with keys of its own.
///
/// ⚠️ **A line is 660 px wide and it does not shrink** — `shortcuts::hint_face` renders with
/// `text::fit(..., f32::MAX, 1, HINT_PX, HINT_PX)`, i.e. no wrap and no smaller size, so a line
/// that is too long is *cut off* rather than squeezed. Measure before writing one:
/// `python _tools/_font_width.py "<the line>"` (the longest line there is today is 638 px of
/// the 660).
pub const HINT_INDEX_CHANGE: usize = 0;
pub const HINT_INDEX_OPEN: usize = 1;
pub const HINT_INDEX_ACT: usize = 2;
pub const HINT_INDEX_PICK: usize = 3;

/// ★ **2026-10-28: the direction keys all move the caret now** (`↑`/`↓` a row, `←`/`→` a column),
/// so `A` is the only key that does anything else. The line a list row or a switch wears says so:
/// change under `A`.
#[cfg(feature = "lang-en")]
pub const HINT_CHANGE: &str = "Arrows to choose      A to change      B to go back";
#[cfg(not(feature = "lang-en"))]
pub const HINT_CHANGE: &str = "上下左右选择    A 改值    B 返回";
#[cfg(feature = "lang-en")]
pub const HINT_OPEN: &str = "Arrows to choose      A to open      B to go back";
#[cfg(not(feature = "lang-en"))]
pub const HINT_OPEN: &str = "上下左右选择    A 进入    B 返回";
/// ★ **The third line** (2026-10-17): for a row that neither lists values nor opens a screen.
/// `A` does the thing on the spot, so the line says so — "进入" would promise a screen that does
/// not come, and "改值" a list that is not there.
#[cfg(feature = "lang-en")]
pub const HINT_ACT: &str = "Arrows to choose      A to do it      B to go back";
#[cfg(not(feature = "lang-en"))]
pub const HINT_ACT: &str = "上下左右选择    A 执行    B 返回";
/// ★ **The fourth line** (2026-10-20), and the boot row is the only row that wears it: `←`/`→`
/// **look** rather than set, `A` is what writes the picture into the machine, `B` throws the
/// looking away. Neither of the other three would say the truth about it — "左右改" promises a
/// change that only happens on `A`, and "A 进入/执行" hides that `←`/`→` is how you choose one.
///
/// **Measured, not guessed**: 487 px of the 660 there are (EN) — the four-part line
/// ("Up and down to choose" + "browse" + "A" + "B") is 744 px and would be cut off, so the
/// up-and-down part, which every other line already says, is the part that went.
#[cfg(feature = "lang-en")]
pub const HINT_PICK: &str = "Left and right to browse      A to set      B to go back";
#[cfg(not(feature = "lang-en"))]
pub const HINT_PICK: &str = "上下选择    左右浏览    A 替换    B 取消";

/// Every value a row can show, in the order `←`/`→` walks them.
///
/// **Every list is handed in rather than read here**: the minutes and the two sets of gears are
/// the card's (`slot_store`'s three `*_CHOICES`), and this crate has never heard of a card. A
/// row that opens a screen has exactly one value, which is the word for opening it.
///
/// ★ The badge and sound rows are the two whose values are **words rather than a list handed
/// in**, and they are still two values in the same order the card reads them (`on`, then `off`)
/// — the screen prints a list either way, and `settings_value` is what knows which end the card
/// is at.
/// **`logos` is the card's too**: the boot screen's list is whatever `.bmp` files are sitting
/// in `Bootlogos/`, and this crate has no more heard of a card than it has of a gear list.
pub fn values(
    row: Row,
    doze: &[u8],
    ff: &[u8],
    rewind: &[u8],
    logos: &[String],
    baseos_sleep: bool,
) -> Vec<String> {
    match row {
        Row::FastForward => ff.iter().map(|n| crate::lang::speed(*n)).collect(),
        Row::Rewind => rewind.iter().map(|n| crate::lang::speed(*n)).collect(),
        Row::Sleep if baseos_sleep => vec![crate::lang::SETTINGS_SLEEP_SYSTEM.to_string()],
        Row::Sleep => doze.iter().map(|m| crate::lang::minutes(*m)).collect(),
        // ★ **2026-10-28 第二轮（六六）：日期与时间去掉「校准」描述项** —— 和刷新一样成了
        //   "只留名字"的行，`A` 开门（`opens`），值位空、`face` 只画居中的名字。原先那一个
        //   词（「校准」）是给"按 A 进去"的暗示，而底行的 `HINT_OPEN`（「A 进入」）已经说了。
        Row::DateTime => vec![String::new()],
        Row::Sound => [true, false]
            .iter()
            .map(|on| crate::lang::switch_word(*on).to_string())
            .collect(),
        // ★ **2026-10-28 六六：「选项去掉仅留描述名」**。这一行没有"当前值"—— `A` 就是做，
        //   值位留空，`face` 只画名字（一个空字符串就是"没有值"）。原先的两个词（刷新 /
        //   已刷新）是给瞬时回执用的，方向键全给光标后 `A` 就是动作，回执没了意义。
        Row::Refresh => vec![String::new()],
        // The generated BaseOS wordmark is always available as index 0.
        // Custom BMP choices follow it, even when the card contains no images.
        Row::Boot => std::iter::once(crate::lang::SETTINGS_BOOT_DEFAULT.to_string())
            .chain(logos.iter().cloned())
            .collect(),
        // ★ 2026-10-21：两个词 —— 「未配置」/「已配置」，判据是卡上有没有 wifi.conf。
        //   SSID 本身不在这里：这一行是定宽盘子，名字在浮层里。
        Row::Wifi => [crate::lang::SETTINGS_WIFI_UNSET, crate::lang::SETTINGS_WIFI_SET]
            .iter()
            .map(|w| w.to_string())
            .collect(),
        // ★ 2026-10-28：两个开关，和加速声音同一套走法（`[true, false]` → 开 / 关）。
        Row::InsertResume => [true, false]
            .iter()
            .map(|on| crate::lang::switch_word(*on).to_string())
            .collect(),
        Row::EjectSave => [true, false]
            .iter()
            .map(|on| crate::lang::switch_word(*on).to_string())
            .collect(),
    }
}

/// One row, one value: the plate with the name against its left edge and the value against its
/// right, idle and in hand.
pub struct RowFace {
    pub idle: UndoFace,
    pub chosen: UndoFace,
}

pub fn value_face(row: Row, value: &str) -> RowFace {
    RowFace {
        idle: face(Some((row.label(), value)), false),
        chosen: face(Some((row.label(), value)), true),
    }
}

/// The row's size. `ROW_W` is the sticker's width rather than a number of this file's, so the
/// rows and the label the menu draws above its own grid cannot drift apart.
pub const ROW_W: f32 = crate::sticker::STICKER_W as f32;
/// ★ **2026-10-28 六六：单行改双列**。一屏 12 项、每行两个 —— 单行那一版每行 70 px 只有一行字，
/// 屏上大半是空气；双列把 12 项收进 6 行，每格 60 px 放**两行字**（功能名在上、当前值在下），
/// 一屏全收、不再滚动。
pub const COLS: usize = 2;
/// The air between the two fields of a row. The menu's gutter, for the same reason the
/// menu's `MARGIN` is this screen's: a gutter that read as a gap rather than as a split is the
/// one the menu already settled on.
const GUTTER: f32 = 14.0;
/// How wide one field is. **Derived from the sticker width, not a taste**: two fields plus the
/// gutter span the same 660 the single row did, so the double column still reads as one machine
/// with the menu — the 2026-10-06 lesson (row width should match the about-page sticker) applies
/// to the row as a whole, not to each half of it.
pub const CELL_W: f32 = (ROW_W - GUTTER) / COLS as f32;
/// Where the list starts: **not the menu's `TOP` any more**.
///
/// The menu's `TOP` is `PAD + STICKER_H + LABEL_GAP` = 274, i.e. "under the 228 px sticker". A
/// page of parameters does not want a second copy of the label the user tapped through to reach it
/// (2026-10-10 六六:「设置面板页就不用加『关于』贴纸了吧有点多余」), so with the sticker gone the
/// list takes its place and starts at the shortcut card's own top, 26.
pub const TOP: f32 = 26.0;
/// ★ **66, and the number is the window's, not a taste** (2026-10-28). Five rows fill the page
/// from `TOP` down to the pinned bar, and the bar's top is `OUT_H - HINT_BAR_H` = 480 − 64 = **416**
/// (the foot line is a `shortcut_hint_face`, which is `shortcuts::ROW_H` tall). `5·ROW_H + 4·GAP`
/// = 390 under the 26 px above it lands on 416 to the pixel, so at this `GAP` the rows come out at
/// 66. **One line of type in a cell** — the two-line experiment (name over value) read as
/// wasteful on a 60 px cell (六六 2026-10-28:「空间有点浪费」), so each cell is back to one line,
/// name left / value right, and the extra height is air rather than a second line.
pub const ROW_H: f32 = 66.0;
/// Wide enough to read as a gap between two rows rather than as one tall cell split in two.
pub const GAP: f32 = 15.0;
/// How far below the last row the next one starts.
pub const PITCH: f32 = ROW_H + GAP;
/// The bottom of the window: the top of the bar pinned at the foot.
const WINDOW_BOTTOM: f32 = 480.0 - HINT_BAR_H;
/// How many whole rows the window holds. **Derived rather than written**: `ROW_H` is the number
/// the layout is decided by, and this has to follow it rather than be kept in step by hand.
/// **Five today, of the five grid rows there are** — the whole list fits, so nothing scrolls.
/// The window machinery below is kept as headroom, the way it was at five single rows before the
/// sixth arrived: it is inert at a list that fills the page, and a sixth grid row revives it.
pub const VISIBLE: usize = ((WINDOW_BOTTOM - TOP + GAP) / PITCH) as usize;
/// The height of whatever is pinned at the foot of this screen, which is what the list must
/// clear. It is `shortcuts::ROW_H` because the settings screen's foot line is built by
/// `shortcut_hint_face`, the same call the shortcut card and the menu use.
const HINT_BAR_H: f32 = crate::shortcuts::ROW_H as f32;
/// How round. One step rounder than a menu button: these are fields, not keys.
const RADIUS: f32 = 10.0;
/// The rim, in pixels. Lighter than a button's, because a row is a field and a heavy outline
/// would make five of them read as five keys.
const RIM_PX: f32 = 1.5;
/// The air between the plate's edge and the type.
const PAD: f32 = 20.0;
/// The name's type, the bigger of the two in a cell.
const TEXT_PX: f32 = 24.0;
/// The value's type, a step down from the name's.
const VALUE_PX: f32 = 16.0;
const TEXT_MIN_PX: f32 = 13.0;

/// Where a row sits, in offscreen pixels, as its top left corner, with the list scrolled by
/// `scroll`. Scroll is a property of the *window* and never of the row: a row does not know
/// where it is on the screen, which is what lets `scroll_for` be the only thing that decides.
///
/// ★ **2026-10-28: a column, not a single file.** `i` is the row's index in `Row::ALL`, which is
/// also the order `↑`/`↓` walks them; the grid position is derived from it (`col = i % COLS`,
/// `row = i / COLS`), so the linear order and the visual order cannot come apart. A row that is
/// `COLS` places further on in the list is exactly one row lower on the screen.
pub fn row_at(i: usize, scroll: f32) -> (f32, f32) {
    let col = i % COLS;
    let row = i / COLS;
    (
        MARGIN + col as f32 * (CELL_W + GUTTER),
        TOP + row as f32 * PITCH - scroll,
    )
}

/// Whether a row is inside the window at this scroll.
///
/// **A row that has slid up past `TOP` is not drawn at all** rather than drawn and covered. The
/// sticker lives directly above the window and the rows are opaque plates with rounded corners,
/// so a row painted under it would show through the corners as two square ones.
pub fn visible(i: usize, scroll: f32) -> bool {
    let (_, y) = row_at(i, scroll);
    y + ROW_H > TOP && y < WINDOW_BOTTOM
}

/// How many grid rows the list needs. **The window counts rows, not entries**: `VISIBLE` is a
/// number of grid rows, and a double column makes two entries per row, so the list's length has
/// to be converted before the window arithmetic can run.
fn grid_rows() -> usize {
    (Row::ALL.len() + COLS - 1) / COLS
}

/// How far down the list can be scrolled. Zero when everything fits — six grid rows, the page
/// this screen fills.
pub fn scroll_max() -> f32 {
    let n = grid_rows();
    if n <= VISIBLE {
        0.0
    } else {
        (n - VISIBLE) as f32 * PITCH
    }
}

/// Where the list stands when the row at `index` is in hand: **as little scroll as it takes to
/// bring that row inside the window**, and never past the end of the list.
///
/// This is the whole of "the selected row is always on screen", and it is *derived* rather than
/// tracked: `↑`/`↓` move the row and the offset follows from it, so the two cannot come apart
/// after a wrap at either end of the list.
pub fn scroll_for(index: usize) -> f32 {
    let n = grid_rows();
    if n <= VISIBLE {
        return 0.0;
    }
    let row = index / COLS;
    (row.saturating_sub(VISIBLE - 1).min(n - VISIBLE) as f32) * PITCH
}

/// The foot of the window — the lowest pixel a row may occupy, which is where the pinned bar
/// begins. **The window's foot, not the list's**: the list is longer than the window on purpose,
/// and a caller asking "does this clear the bar" is asking about the window.
pub fn bottom() -> f32 {
    TOP + VISIBLE as f32 * PITCH - GAP
}

/// One row on the panel, at its slot's place.
pub fn draw_row(face: Option<(TexId, u32, u32)>, i: usize, scroll: f32, out: &mut Vec<Draw>) {
    let Some((tex, w, h)) = face else {
        return;
    };
    if !visible(i, scroll) {
        return;
    }
    let (x, y) = row_at(i, scroll);
    out.push(Draw::Tex {
        x,
        y,
        w: w as f32,
        h: h as f32,
        tex,
        alpha: 1.0,
    });
}

/// The two faces one row is drawn with. `None` is a row with no words on it, which nothing asks
/// for yet — it exists so the empty recess the menu draws has an equivalent here and the two
/// screens cannot disagree about what an empty slot looks like.
fn face(texts: Option<(&str, &str)>, chosen: bool) -> UndoFace {
    let (w, h) = (CELL_W.round() as u32, ROW_H.round() as u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    // The menu's own three greys in the menu's own roles: a row is `housing` on the panel,
    // `opening` when it is a recess, and `edge` — the lightest value in the theme — when it is
    // the one in hand. Taken from the same functions the buttons take them from, so a theme that
    // flips one flips both.
    let (fill, ink, value_ink) = match (texts, chosen) {
        (None, _) => (opening(), palette::dim_ink(), palette::dim_ink()),
        // The lit row's two inks: `chosen_ink` for the name (see `palette::chosen_ink` for why it
        // is chosen rather than derived), and the same for the value — a lit row has one voice, so
        // the value must not be the *dimmer* of the two the way it is on an unlit row.
        (Some(_), true) => (edge(), palette::chosen_ink(), palette::chosen_ink()),
        // ★ **未选中行：名字是该 look 的墨色，值退后一档 —— 但必须读得出**（2026-10-16 六六：
        //   「简单来说就是主次分明就行」）。原先值是 `dim_ink`，而 `dim` 从 ramp 的**地面**退后
        //   一档、这一行的底却是 ramp 的**塑料壳色** ⇒ RED 1.25 / AMBER 1.89 / GREEN 2.27，
        //   那一行的值基本看不见（当天评审列出的"第二处"，当时留了说明未改）。
        //   `idle_ink()` 问的是"对**实际被画在的那个面**够读且最钝"，与 `menu` 未选中键同一个函数 ——
        //   一屏之内"次"的标准只有一条，两处不能各有一套。
        (Some(_), false) => (housing(), palette::ink(), palette::idle_ink()),
    };
    // The lit row wears its own rim — see `palette::chosen_rim`. A rim in `edge` on a face filled
    // with `edge` is 1.00:1: the selected row had no outline at all, on either look.
    let rim = if chosen { palette::chosen_rim() } else { edge() };
    rounded::plate(&mut rgba, w, h, RADIUS, RIM_PX, fill, Some(rim));
    crate::menu::mould(&mut rgba, w, h, RADIUS, RIM_PX);

    let Some((label, value)) = texts else {
        return UndoFace { rgba, w, h };
    };
    let Some(font) = text::label_font() else {
        return UndoFace { rgba, w, h };
    };

    // ★ **2026-10-28: one line, name left / value right** (六六:「按钮内改单行显示吧」). The
    // two-line stack (name over value) read as wasteful on a 60 px cell, so a cell is one line
    // again — the layout the original single-column rows used, now inside a 323 px cell. The name
    // is the bigger of the two and the value a step down, the same name/value inks the single row
    // used, so the hierarchy is unchanged. A row with no value (the refresh row — 六六:「选项去掉
    // 仅留描述名」) draws only its name, centred, because a name pinned to the left edge with the
    // right half empty would read as a row that lost its value rather than one that never had one.
    let name = text::fit(font, label, CELL_W - 2.0 * PAD, 1, TEXT_PX, TEXT_MIN_PX);
    if value.is_empty() {
        stamp_centred(&mut rgba, w, h, font, &name, ink, 0, h);
        return UndoFace { rgba, w, h };
    }
    let value = text::fit(font, value, CELL_W - 2.0 * PAD, 1, VALUE_PX, TEXT_MIN_PX);
    let val_w = crate::menu::line_w(font, &value);
    let budget = (CELL_W - 2.0 * PAD - val_w - GAP).max(1.0);
    let name = text::fit(font, label, budget, 1, TEXT_PX, TEXT_MIN_PX);

    // Both lines vertically centred in the whole cell; the name's left edge at `PAD`, the value's
    // right edge at `CELL_W - PAD`. The value is measured first, which is what the name's budget
    // is measured against — the name may not run into the value, and the gap between them is the
    // same `GAP` the two columns sit apart by.
    stamp_line(&mut rgba, w, h, font, &name, ink, PAD);
    stamp_line(&mut rgba, w, h, font, &value, value_ink, CELL_W - PAD - val_w);
    UndoFace { rgba, w, h }
}

/// One line of type, set into `dst` with its left edge at `x`, centred in the cell's height.
///
/// ★ **2026-10-28: the left-anchored line is back** — a one-line cell has a left edge again (the
/// name against it), which is what the two-line stack removed. The value uses the same function
/// with its x derived from its own measured width, so the two ends cannot disagree about the
/// cell's middle line.
fn stamp_line(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    font: &fontdue::Font,
    layout: &text::Layout,
    colour: [u8; 3],
    x: f32,
) {
    if layout.lines.is_empty() {
        return;
    }
    let width = crate::menu::line_w(font, layout).ceil().max(1.0) as u32;
    let cov = text::coverage(width, dst_h, layout);
    let left = x.round() as i32;
    for y in 0..dst_h {
        for sx in 0..width {
            let a = cov[(y * width + sx) as usize] as u32;
            if a == 0 {
                continue;
            }
            let dx = left + sx as i32;
            if dx < 0 || dx as u32 >= dst_w {
                continue;
            }
            let at = ((y * dst_w + dx as u32) * 4) as usize;
            for c in 0..3 {
                let under = dst[at + c] as u32;
                dst[at + c] = ((colour[c] as u32 * a + under * (255 - a)) / 255) as u8;
            }
        }
    }
}

/// One line of type, set into `dst`, **centred horizontally and vertically in a band** that
/// starts at `band_top` and is `band_h` tall.
///
/// ★ **2026-10-28: kept for the one case a one-line cell still centres** — a row with no value
/// (the refresh row) draws its name centred rather than pinned to the left edge.
fn stamp_centred(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    font: &fontdue::Font,
    layout: &text::Layout,
    colour: [u8; 3],
    band_top: u32,
    band_h: u32,
) {
    if layout.lines.is_empty() || band_h == 0 {
        return;
    }
    let width = crate::menu::line_w(font, layout).ceil().max(1.0) as u32;
    let cov = text::coverage(width, band_h, layout);
    let left = (dst_w as i32 - width as i32) / 2;
    for y in 0..band_h {
        for sx in 0..width {
            let a = cov[(y * width + sx) as usize] as u32;
            if a == 0 {
                continue;
            }
            let dx = left + sx as i32;
            let dy = band_top as i32 + y as i32;
            if dx < 0 || dx as u32 >= dst_w || dy < 0 || dy as u32 >= dst_h {
                continue;
            }
            let at = ((dy as u32 * dst_w + dx as u32) * 4) as usize;
            for c in 0..3 {
                let under = dst[at + c] as u32;
                dst[at + c] = ((colour[c] as u32 * a + under * (255 - a)) / 255) as u8;
            }
        }
    }
}

// Kept honest by the compiler rather than by a comment. Two facts have to hold — the rows
// `VISIBLE` claims fit above the bar, and one more would run under it. That pair is what makes
// `scroll_max` and `VISIBLE` agree with the pixels without either being written by hand.
//
// ⚠️ **The bar is `HINT_BAR_H` (64), not `plate::HINT_H` (24)** — the settings screen's foot line
// is a `shortcut_hint_face`, and measuring against the 24 would leave the guard 16 px too
// permissive: a window that ran 10 px under the real bar would have built happily.
const _: () = assert!(TOP + VISIBLE as f32 * PITCH - GAP <= WINDOW_BOTTOM);
const _: () = assert!(TOP + (VISIBLE + 1) as f32 * PITCH - GAP > WINDOW_BOTTOM);
// A window that cannot hold a single whole row is not a window.
const _: () = assert!(VISIBLE >= 1);
