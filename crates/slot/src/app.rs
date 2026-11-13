use std::path::{Path, PathBuf};

use slot_gfx::{System as ScreenSystem, OUT_H, OUT_W};
use slot_input::{Action, Btn, MUTE_CHORD_MS};
use slot_power::{Battery, Charge, LedState, LidPolicy, Power};
use slot_retro::LinkChannel;
use slot_store::{
    format_stamp, read_slot_state, rescan, scan_cached, write_slot_state, Cart, Core, Slot,
    SlotState, StateEntry, StateRing, StateSlots, System, Theme, BLUE_LIGHT_MAX, BRIGHTNESS_MAX,
    RING_MAX, SLOT_COUNT, VOLUME_MAX,
};
use slot_ui::lang;
use slot_ui::{
    board_at, board_zoom, draw_backdrop, draw_edge_glow,
    draw_shortcut_row, draw_slot_inside, draw_slot_shell, draw_status, draw_sticker_at,
    draw_top_band, ease, grown, label_colour, label_text, letters,
    lid_at, lift_of, on_board, size_for_dir, ClockPicker, Draw, FfState, Hud, HudKind, Icon, Letters,
    Millis, Placed, Polaroids, PowerChoice, Refusal, Seat, Shelf, SlotChrome, TexId, Toast,
    BOARD_W, BOARD_X, CART_W, CHIP_H, CHIP_U, CHIP_V, CHIP_W, HINT_EDGE, HINT_H, HOP_LIFT,
    MOUTH_H, PLATE_Y, SHADOW_H, SHADOW_W, SHELF_TITLE_H, SOCKET_H, SOCKET_U, SOCKET_V, SOCKET_W,
    TURN_PAD, DISC_DIA_DEFAULT, DISC_DROP_S, DISC_SPIN_S,
};

use crate::audio::{Profile, Sfx};
use crate::core_picker::{Chip, CorePicker, Outcome, Press};
use crate::link_radio::LinkRole;
use crate::link_start::{link_port, LinkFail, LinkProgress, LinkStarter, LinkStep};
use crate::persist::{self, Snapshot};
// ★ 2026-10-26（六六）：长按 Y 打开的机种面板。`Outcome` 与 `core_picker` 的那个同名，
//   所以这一路带前缀 —— 两个面板都在 `App` 上，读代码时不带前缀就分不清是谁的结论。
use crate::sys_picker::{Outcome as SysOutcome, SysPicker};

/// 一张已经上传到 GPU 的贴图：句柄，以及它上传时的像素宽高。
///
/// 这个三元组在这棵树里出现过一百多次（每个面、每块板、每张快照都是它），写全了
/// clippy 就会嫌弃「类型太复杂」。给它一个名字既是为了让 `type_complexity` 闭嘴，
/// 也是因为它本来就该有个名字：`(TexId, u32, u32)` 里那两个 `u32` 是**贴图自己的**
/// 尺寸，不是画上去的矩形尺寸 —— 画多大是 `Draw::Tex` 的 `w`/`h` 决定的。
pub type Upload = (TexId, u32, u32);

/// A floor, not a delay. The animation is where the core load hides, so a slow load
/// extends it and a load that is already done still waits it out.
pub const INSERT_S: f32 = 0.73;
/// The tail of the insert, spent on a cart that has already landed. The game arriving on the
/// frame the cart seats reads as a cut; a beat of nothing first says the cart caused it.
///
/// Long enough to cover the rest of the sound of it landing and leave a little air after it,
/// which `the_game_waits_for_the_cart_to_finish_landing` holds against the clip.
const INSERT_HOLD_S: f32 = 0.28;

/// **PS1 光盘插入：总时长。** 下落 + 旋转 = 2.75s，是普通插卡的 3.8 倍。
pub const DISC_INSERT_S: f32 = DISC_DROP_S + DISC_SPIN_S;
/// **PS1 光盘弹出时长。** 加速反转 → 托盘弹出 → 减速收回，总 1.4s。
/// ★ 2026-10-26：改为 1.23 s（吸入式 `draw_disc_eject` 五拍：
/// 停止旋转 0.40 + 插卡位发光 0.30 + 直接弹射回原位 0.30 + 光线熄灭 0.23 = 1.23 s）。
pub const DISC_EJECT_S: f32 = 1.23;

/// **光盘弹出前那一小拍静止。** 普通卡带是 `EJECT_HOLD_S`（0.35 s），光盘不用那么长：
/// CRT 关机（`POWER_OFF_S` = 0.16 s）自己就是一拍，屏黑了之后盘还在洞里缩着，再等 0.35 s
/// 就成了"卡住"。0.10 s 只够音效落地，接着就进五拍第一拍（停转）。
///
/// ★ 2026-10-26：旧值 `-DISC_EJECT_S`（-1.23）是个 bug —— 相位只在 `screen <= 0` 之后才
/// 开始走，于是屏黑之后还要干等 1.23 s 光盘才动，整段退出拖到 2.46 s。
const DISC_EJECT_HOLD_S: f32 = 0.10;

/// ★ 2026-10-26：货架区「GAME START LOADING…」那行呼吸发光字（`draw_loading`）。
///
/// 图是 `System/Carts/slot_loading.png`，**720×240 的成品**：自带留白，笔画落在
/// x 47..668、y 55..183 ⇒ **笔画中心在 y = 119**（不是 120，图不是严格的上下对称）。
/// 六六：「应该居中」—— 所以把笔画中心摆到屏中心 240，图上沿就落在 `240 - 119 = 121`。
/// （货架里中间卡带、光盘、插卡位背板全是对 240 居中的，字跟着它们才是一条轴。）
const LOADING_Y: f32 = OUT_H as f32 / 2.0 - 119.0;
/// 呼吸一个来回。1.5 s 比心跳慢一点 —— 它说的是"在做事"，不是"在等"。
const LOADING_PERIOD_S: f32 = 1.5;
/// 呼吸最暗到几成。**不能到 0**：字整个消失再出现读作闪烁，不是呼吸。
const LOADING_LO: f32 = 0.34;
/// 起手那一小段淡入，免得按下 A 的瞬间字"啪"地出现。
const LOADING_FADE_IN_S: f32 = 0.12;

/// When the travel ends and the cart is against the contacts, which is what it clicks on.
pub const SEATED_AT: f32 = INSERT_S - INSERT_HOLD_S;

/// ★★ 2026-10-27（六六）：「MD 插卡没动作，卡顿后就在卡槽里了」。
///
/// 这条动画的时钟（`Phase::Inserting.t` / `Ejecting.t`）**每帧最多走这么多**。
/// 根因是插卡的第一帧里同步跑 `open_core`（dlopen + `retro_init` + 读 ROM），而 `dt` 是真实
/// 经过时间 —— 一次 300~800 ms 的卡顿会让 `t` 一帧跨过 `SEATED_AT`(0.45)，整段行程被吃掉。
/// 封顶之后卡顿只吃掉帧数，行程一定走满。1/20 s：60 fps 的 0.0167 远在下面，正常帧零影响。
const INSERT_STEP_MAX: f32 = 0.05;

/// ★ 2026-10-27（六六）：「**箱体升起的时候伴随屏幕震动效果更好**」。
///
/// 振幅（离屏像素）从箱体开始升的那一刻算起、线性掉到 0；频率 16 Hz，60 fps 下约 4 帧一个来回。
const COIN_SHAKE_PX: f32 = 7.0;
const COIN_SHAKE_HZ: f32 = 16.0;

/// 投币的 CRT 什么时候开。
///
/// ★ **2026-10-28（六六）**：「感觉现在提前 CRT 动画播不全，还是改回载入完毕再进入 CRT
/// 特效然后进游戏画面」。
///
/// 原先（`7c1da855` 起）提前 `POWER_ON_S` 的 2/3（0.15 s）开 —— 六六想的是"进度读不到、
/// 核心就绪就是最早信号，提前一点让 CRT 和箱体最后几帧叠在一起"。**但这样 CRT 长满整屏
/// （0.22 s）之前箱体还没升到位，CRT 动画本身被截断了**（它从中间一点辉光长开，还没长满
/// 整屏就急着显示游戏画面）。
///
/// 六六 2026-10-28 改回：**箱体完全升到位（`coin_total_s()`）且核心就绪，才切 `Playing`、
/// CRT 才开** —— CRT 长满的 0.22 s 是它自己的完整一段，不再和箱体最后 27% 行程重叠。
/// `Playing{screen<1}` 的投币支继续用**同一条插入时钟**推箱体的升起，所以交接不跳。
/// 原 `COIN_CRT_LEAD_S` 常量随之删除。

/// The eject is the insert played backwards, so it is the same length. It used to be shorter,
/// on the grounds that pulling something out is a quicker movement than pushing it in, but
/// every part of the screen is driven off one progress and two lengths make every one of them
/// come back faster than it left.
pub const EJECT_S: f32 = SEATED_AT;

/// Between the picture going out and the cart starting to move. Long enough for the game to
/// have actually stopped: the core is paused the moment the button is held, but what it has
/// already handed the device is up to a ring's worth of audio, and the cart must not start
/// coming out over the last of it.
const EJECT_HOLD_S: f32 = 0.35;

/// The panel striking, once the cart is home. Long enough to read as a screen coming up,
/// short enough that it is not something to sit through.
const POWER_ON_S: f32 = 0.22;

/// **←/→ 一次跳多少秒。** 十秒是「听不清一句」的长度：一个四分半的前奏按一下就过去了，
/// 而一秒一步又太细，滚一屏都到不了一处变化。
const SEEK_STEP_SEC: f32 = 10.0;
/// **按住 ←/→ 时每隔多久再跳一步。** 220ms 是"还能数得清自己按了几下"的上限；
/// 再快就变成连续扫带，而那时用户其实想要的是上一首/下一首（那是 ↑/↓ 的活）。
const SEEK_REPEAT_MS: u64 = 220;


/// Going out is quicker than coming up, the way a panel dies faster than it strikes.
const POWER_OFF_S: f32 = 0.16;

/// Volume has ten times the range of the other two, so it moves ten times as far. Twenty
/// presses end to end is close enough to their ten that the three feel like one control.
const VOLUME_STEP: u8 = 5;

/// Crash insurance, and the only durable write that happens with the game still running.
const AUTOSAVE_MS: Millis = 60_000;

/// A dead battery is a far likelier hard cutoff than anyone holding POWER for eight
/// seconds, so the last of the charge goes on the state and then on stopping.
const BATTERY_CRITICAL: u8 = 5;

/// The gauge moves by a percent over minutes and on the device it is a sysfs read, so it
/// is not worth a look every frame.
const BATTERY_POLL_MS: Millis = 10_000;

/// The gauge moves over minutes but the charge state is a step change: it flips the instant
/// a cable goes in. Ten seconds of a stale bolt on screen, and a stale colour on the LED, is
/// worse than the read costs — `status` is a short string, far cheaper than the pair.
const CHARGE_POLL_MS: Millis = 1_000;

/// Below this the LED goes red. Well clear of `BATTERY_CRITICAL`, since it is a warning with
/// time to act on it rather than a cutoff.
const BATTERY_LOW: u8 = 20;

/// Long enough to notice the wrong state loading, short enough that the offer is gone by the
/// time the switcher is opened for any other reason.
pub const UNDO_GRACE_MS: Millis = 30_000;

/// How long A has to be down on the shelf before it means "start this cart clean". Past the
/// point a press could be a tap, and short enough to hold without wondering whether the
/// device is still listening.
const PLAY_HOLD_MS: Millis = 500;

/// How far apart the menu's rows sit, and what marks the one in hand. The pitch clears the
/// 40 px face with a little air; the bar is drawn to the face's own width, padding included,
/// so it wraps the words rather than the panel.
/// How long the shutdown screen is on the panel before the machine is allowed to stop. Only
/// needs to outlast a couple of frames — it exists so the ordinary loop presents the screen,
/// rather than the binary rendering one out of band on a GPU that is about to go away.
const SHUTDOWN_SHOW_MS: Millis = 250;

const POWER_MENU_PITCH: f32 = 44.0;

/// ★ 2026-10-21：**游戏菜单**的行距。比 `POWER_MENU_PITCH` 大一档，理由见
/// `power_menu::GAME_ROW_PX` —— 两处都是"小气"那次反馈一起调上去的：
/// 字 30→34、行高 40→46、行距 44→58、板余量 28/20→40/26。
///
/// **三样一起动**，因为只动一样会被另外两样吃回去：字放大了行距不变，行会挤在一起；
/// 行距放大了板的余量不变，板还是原来那么小。
/// ★★ 2026-10-21 傍晚第二版：**58 → 66。** 六六：「七个选项字号再小一档并拉开点行距间距」。
/// 字从 34 收到 26，行距反而放大 —— 每一格是一枚 44 高的按钮，66 的行距意味着格与格之间
/// 有 22 像素的空气，读起来是一排按钮而不是一叠字。四行 = 264，加标题与板边仍在一屏里。
const GAME_MENU_PITCH: f32 = 66.0;
/// ★ 2026-10-21：**游戏菜单是两列，不是一列。** 规格的六行 + 重开 + 退出 + 联机 = 九行，
/// 58 px 的行距单列要 522 px —— 超过 480 的屏高，第一行被顶出屏外、标题还压在它身上（真机
/// 截图就是这么翻车的）。排成两列之后九行是五行，`5·58 = 290`，一屏放得下，也才配得上
/// "跟色板浏览器一样是块板" 这句话 —— 它是网格，这个也该是。
const GAME_MENU_COLS: usize = 2;
/// 板左右各留的边。和 `draw_menu_panel` 的 `PAD_X` 同一个数。
const GAME_MENU_PAD_X: f32 = 40.0;
/// 两列之间的缝。够把两块行分开，又不至于让板读成两半。
const GAME_MENU_GAP_X: f32 = 14.0;
/// 标题和第一行之间。
const GAME_MENU_TITLE_GAP: f32 = 8.0;
/// 板上下各留的边。
const GAME_MENU_PAD_Y: f32 = 22.0;

/// 一个格子有多宽：板宽（屏宽减两边 `GAME_MENU_PAD_X` 再减两边的板内边）去掉缝之后，两列
/// 平分。**行面是各自量出来的宽度**（`menu_face_at` 按字数伸缩），所以这个数只决定"格子在
/// 哪"，不决定"行有多宽" —— 行在格子里居中。
pub(crate) fn game_menu_cell_w() -> f32 {
    let panel_w = OUT_W as f32 - 2.0 * GAME_MENU_PAD_X;
    (panel_w - 2.0 * GAME_MENU_PAD_X * 0.6 - GAME_MENU_GAP_X) / GAME_MENU_COLS as f32
}
/// How much shorter the bar is than the row it marks, top and bottom. Enough that the rows
/// stay separate things rather than one continuous block when the selection moves.
const POWER_MENU_BAR_INSET: f32 = 4.0;
/// The cheat table's row spacing and the most rows it shows before it scrolls. Smaller than the
/// power menu's pitch because codes are short and there are often many of them.
const CHEAT_PITCH: f32 = 44.0;
const CHEAT_VISIBLE: usize = 12;
/// How far the row makes way while a cart is open, as `Shelf::draw_row` counts `recede`. It is
/// set by where the neighbours stand: here they come to rest at -41 and 574, where the mockup
/// frames the open cart with them. Parted far enough for the recede alone to dim them to a
/// quarter, they left the open cart alone in the frame.
const CORE_PICKER_RECEDE: f32 = 0.26;
/// How much further the neighbours' faces darken while a cart is open, since the recede that
/// stands them in place dims them only part of the way. Just the recede leaves a side cart's
/// face at `SIDE_ALPHA * (1 - CORE_PICKER_RECEDE)`, and the mockup has the neighbours at a
/// quarter, so this is what is left to take off.
///
/// Derived from the shelf's own `SIDE_ALPHA` rather than written down as a number. It used to
/// be a literal, computed against the 0.55 `SIDE_ALPHA` of the day, and when the look was
/// re-cut to 0.7 the literal stayed put: the neighbours came to rest at a third instead of a
/// quarter, which the test for it caught.
const CORE_PICKER_DIM: f32 = 0.25 / (slot_ui::SIDE_ALPHA * (1.0 - CORE_PICKER_RECEDE));
/// The legend's line, under the open cart and clear of the case band.
const CORE_LEGEND_Y: f32 = 386.0;
/// The soft oval under the resting lid, as the mockup draws it: its size, how far below the
/// lid's bottom edge its centre falls, and how dark it is. Scaled with the lid as it lifts.
const LID_SHADOW_W: f32 = 168.0;
const LID_SHADOW_H: f32 = 18.0;
const LID_SHADOW_DROP: f32 = 29.0;
const LID_SHADOW_ALPHA: f32 = 0.8;
/// The longest the cart stands on the shelf waiting for its faces before it opens anyway, so a
/// face that never comes cannot freeze the picker. A fast scroll can leave the worker still
/// finishing the cart it was already building before it starts on this one, so the cap has to
/// cover that wait too, not just this cart's own build.
const FACES_WAIT_MS: Millis = 1500;

/// How far through a refused cart's exit the alert holds at full, and where it has finished
/// going. Fractions of that exit rather than seconds, because a cart refused early has a
/// short way to come back and the symbol has to fit inside it either way. It is gone before
/// the end: an alert still lit on the frame the shelf returns reads as a thing to dismiss.
const ALERT_HOLD: f32 = 0.45;
const ALERT_GONE: f32 = 0.9;

/// Any clock reading earlier than this was never set. An RTC that has lost power reports a
/// fault rather than a time, the kernel then starts at the epoch, and nothing that reaches
/// this frontend can legitimately be older than the frontend itself.
const CLOCK_FLOOR: i64 = 1_577_836_800;

/// The most recent undoable action. There is exactly one slot for it and a new save or load
/// replaces it: a stack of undos would be a knob.
pub enum PendingUndo {
    Save {
        stamp: String,
        /// Read out of the ring before the push that dropped it, which is what makes the
        /// undo a restore rather than a reconstruction.
        evicted: Option<(String, Vec<u8>, Vec<u8>)>,
    },
    Load {
        prior: Vec<u8>,
    },
}

/// One side of a live netpacket session. Nothing about the transport or the packets lives
/// here — only what a session being live at all means for the rest of `App`, and which of
/// libretro's two client ids this device is, for whatever the UI ends up showing while one
/// is open.
struct LinkSession {
    client_id: u16,
}

/// A link being started: the worker doing the slow parts, and which of libretro's two client
/// ids this device becomes if it succeeds. The id is decided by the row that was picked and
/// has to outlive the pick, because it is `Ready`, frames later, that needs it.
struct LinkStarting {
    starter: LinkStarter,
    client_id: u16,
}

/// ★ 2026-10-21：两张**发光件**的纹理，开机烘一次。
///
/// 独立的类型而不是两个字段，是因为它们同生同死：`palette` 管印刷（跟 look 走），
/// 这两张管发光（不跟 look 走，见 `slot_ui::light`），一个结构体才说得清这件事。
/// 尺寸跟着 id 一起存 —— `Draw::Tex` 会把面缩放到给的 quad，不知道原始尺寸就只能猜。
///
/// **只有两张，都是静态的。** 一颗会呼吸的电源灯很好看，代价是货架从"有输入才重画"
/// 变成一直重画 —— 一台掌机上那是实打实的电，而这两处光要说的都是**静止的状态**
/// （这是哪一行、手里是哪张），本来就不需要动。`light` 里那几个动效原语留给真需要
/// 动效的屏（插卡、处理中），这里一个都不用。
#[derive(Copy, Clone, Default)]
pub struct LightFaces {
    /// 面板顶边的通电渐隐线（左亮右灭）。游戏菜单和槽位表的抬头用它。
    pub power: Option<(TexId, u32, u32)>,
    /// 居中标题的对称背光。货架标题用它。
    pub glow: Option<(TexId, u32, u32)>,
    /// 选中卡带脚下的柔光池。三行货架共用一张，宽度跟着卡带走。
    pub pool: Option<TexId>,
}

/// The in-game menu, over a paused game rather than instead of it. `Phase::Playing` carries
/// the session; leaving it to show a menu would mean rebuilding it to come back, and "cancel
/// returns you to your game" is the entire requirement.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GameMenu {
    /// The top level, holding whichever row is in hand.
    Rows(usize),
    /// ★ 2026-10-20：**即时存档**，手里拿着的是槽位（0 = `Auto`，1..99 = `01`..`99`）。
    ///   槽位是这一级在管的，不是上一级的行号 —— 一行进去就是一个槽位表。
    Save(usize),
    /// 即时读档，同一个槽位表的另一半。
    Load(usize),
    /// ★ 2026-10-21：**保存设置**的确认弹窗。四个键：A 按机种全局保存 / X 按游戏保存 /
    ///   Y 恢复默认 / B 取消 —— 弹窗而不是直接存，因为"全局还是按游戏"是两条不同的路，
    ///   按错一条比多按一键难回头。
    SaveSettings,
    /// ★ 2026-10-21：**操作键映射**，菜单里那一级 —— `shortcuts.rs` 的同一张 ROWS 表，
    ///   上下滚动。两个数和货架那页的 `Phase::Shortcuts` 同名同义（`scroll` 是画到哪、
    ///   `want` 是滚向哪），缓动也复用同一个 settle。
    Shortcuts(f32, f32),
    /// Host or Join.
    Link(usize),
    /// The worker is running, and `LinkStep` is what the screen says while it does.
    Working(LinkStep),
    /// It did not work, and this is which one. B returns to the game.
    Failed(LinkFail),
}

/// A row of the in-game menu. One today — the menu exists for it — and an enum rather than a
/// bare index so a second row is a variant and a face rather than a set of numbers to keep
/// in agreement.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameRow {
    /// ★ 2026-10-20：即时存档 / 即时读档，各开一张 `Auto` + `01`–`99` 的槽位表。
    ///   两条都在最前面，因为菜单的第一件事是"把这一局存下来/取回来"。
    Save,
    Load,
    /// ★ 2026-10-21：**操作键映射**（货架设置页里那份键位一览，`shortcuts.rs` 的 ROWS）。
    ///   六六的规格原话「操作键映射（即上几轮做的放货架设置页里的那份）」—— 同一张表
    ///   复用，不另做一份薄版：两份迟早只改一份。
    Shortcuts,
    /// ★ 2026-10-21：**色板浏览器**。原入口是长按 SELECT+Y（`ColorHold`），六六的规格是
    ///   把它挪进菜单：「色板浏览器（取消组合键中长按 SELECT+Y 唤出的功能）」。
    Palette,
    /// ★ 2026-10-21：**屏幕档位** —— GBA 是网格档位（`MASK_STATES` 的 LCD3X/扫描线），
    ///   GB/GBC 是遮罩艺术环（`overlay_next`）。六六的规格写的是「网格档位（GBA）/
    ///   遮罩档位（GB/GBC）」，一行两义，跟着机种走。
    Grid,
    /// ★ 2026-10-21：**保存设置**，最下面一行（规格原话），按 A 弹窗确认。
    SaveSettings,
    /// ★ 2026-10-20：**重开本局**。六六问「其它的功能呢？」—— 一个菜单只有
    ///   存和读，"这一局废了重来"就得先退到货架再重新插一次卡。
    Restart,
    /// ★ 2026-10-21 六六：「所有游戏内菜单里的『退出到货架』删掉」。原来的 `Quit` 就长在
    /// 这个位置，走的是 `eject()` —— 拔卡回货架。删它不是因为做不到，是因为**这个菜单里
    /// 每一样都是"对这一局做的事"**，而"离开这一局"已经有一把专门的键（MENU 长按＝弹仓），
    /// 摆在菜单里只是给同一个动作开了第二个入口，还占着格子。
    /// 联机。gpSP 独占（`game_rows` 过滤掉其它核心），既有功能，排在所有规格行末尾。
    Link,
}

impl GameRow {
    /// Every row, once, in the order their faces are uploaded. Which of them a given cart
    /// actually shows is `App::game_rows`. Link 在末尾 —— 规格的六行在前（存、读、键位、
    /// 色板、档位、保存设置），联机是 gpSP 独占的既有功能，排最后不挡规格的顺序。
    pub const ALL: [GameRow; 8] = [
        GameRow::Save,
        GameRow::Load,
        GameRow::Shortcuts,
        GameRow::Palette,
        GameRow::Grid,
        GameRow::SaveSettings,
        GameRow::Restart,
        GameRow::Link,
    ];

    /// Position in `ALL`, which is the order the faces are in.
    pub fn index(self) -> usize {
        self as usize
    }

    /// ★★ 2026-10-21：**同一行、两个词，跟这一局的机种换。**
    ///
    /// `Grid` 这一格在两种机器上根本不是同一件事：GBA 有 LCD3x 网格与扫描线（那台机器没有
    /// 遮罩艺术环），GB/GBC 有遮罩艺术环（那台机器没有网格档位）。共用一个格子是因为**位置**
    /// 是同一个 —— 都是"这一屏看起来怎样" —— 但名字必须说实话，不然玩家会在 GB 上找一个
    /// 不存在的网格。
    pub fn text_for(self, system: System) -> &'static str {
        match self {
            GameRow::Save => lang::GAME_ROW_SAVE,
            GameRow::Load => lang::GAME_ROW_LOAD,
            GameRow::Shortcuts => lang::GAME_ROW_SHORTCUTS,
            GameRow::Palette => lang::GAME_ROW_PALETTE,
            GameRow::Grid if system == System::Gba => lang::GAME_ROW_GRID,
            GameRow::Grid => lang::GAME_ROW_OVERLAY,
            GameRow::SaveSettings => lang::GAME_ROW_SAVE_SETTINGS,
            GameRow::Restart => lang::GAME_ROW_RESTART,
            GameRow::Link => lang::LINK_ROW,
        }
    }
}

/// ★ 2026-10-21：**操作键映射弹窗** —— 六把物理键 + 最下面一行连发频率，浮在屏上。
///
/// 它不挂在任何一个相位上：货架设置页和游戏内菜单都能开它，而改键不该先把开它的那个屏
/// 拆掉。`B` 关掉之后，底下的屏原样还在 —— 这就是它做成 App 级浮层而不是 `Phase` 的全部
/// 理由（和色板浏览器同一条，只是那一份挂在 `pal_cursor` 上）。
///
/// `row` 是手里那一行：0..=5 是六把键，6 是最下面的频率行。`picking` 是目标清单打开着时
/// 指着的那一项。`confirm` 是「改完了，要保存吗」—— `A` 落笔，`B` 丢掉。
#[derive(Clone, Debug)]
pub struct KeymapEdit {
    /// Which row is in hand: 0..=5 are the six keys, 6 is the turbo-rate row.
    pub row: usize,
    /// When `Some`, the target picker is open on the row in hand, pointing at this target.
    pub picking: Option<usize>,
    /// The sheet is asking to save: `A` writes it into the card, `B` throws the edits away.
    pub confirm: bool,
    /// The map as it was when the sheet opened, so `B` can put it back exactly.
    pub original: std::collections::BTreeMap<String, String>,
    /// ★ 2026-10-26：**每台机种那份的快照**。`original` 只管旧卡那张全局表，这一份
    /// 是整份 `state.keymaps` —— `B`（取消）要把两者都放回去，不然取消会把一个没保存的
    /// 改动留在卡上，而且只留在当前机种那份里 —— 比从前更难查。
    pub original_systems:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// The rate as it was when the sheet opened.
    pub original_hz: u8,
}

/// The faces the keymap sheet is drawn with, minted by the frontend while it is open and
/// released when it closes. `sig` is the row values they were baked for, so an edit re-bakes the
/// rows and nothing else.
pub struct KeymapFaces {
    pub title: (TexId, u32, u32),
    /// One per row: the six keys, then the rate row, in that order.
    pub rows: Vec<(TexId, u32, u32)>,
    pub highlight: (TexId, u32, u32),
    pub foot: (TexId, u32, u32),
    /// One per `slot_ui::TARGETS`, in that order.
    pub targets: Vec<(TexId, u32, u32)>,
    /// The save-confirm plate: a title and one line per option.
    pub save_title: (TexId, u32, u32),
    pub save_lines: Vec<(TexId, u32, u32)>,
    /// The seven row values the `rows` were baked for.
    pub sig: Vec<String>,
}

/// One network in the air, as `slotnet.sh scan` reports it. `secured` is what the AP's
/// flags say, which is the only thing that decides whether a password is asked for.
#[derive(Clone, Debug)]
pub struct WifiNet {
    pub ssid: String,
    /// wpa_supplicant's signal level in dBm: −30 is beside the AP, −90 is nearly gone.
    pub signal: i32,
    pub secured: bool,
}

/// The Wi-Fi screen's layout: one margin, three vertical bands (title, body, foot), and
/// the grid's cell size. A line is 660 px wide at most — the same plate the settings
/// screen's foot uses — so every fixed line here was measured before it was written.
///
/// ★ 2026-11-03（六六）：「字体有点小」。这一页的整页文字从状态行那档（24px / 20 高的带子）
/// 换成了 `page_face`（32px / **44** 高的带子），行距跟着从 34 走到 48：34 是二十来像素的
/// 字才够用的行距，四十四高的字块塞进去会上下相压。整体因此重排了一遍 —
/// 标题 22..66、正文 84 起、脚注 432..476（屏高 480，留 4 的底边）。
const WIFI_MARGIN: f32 = 40.0;
const WIFI_TITLE_Y: f32 = 22.0;
const WIFI_BODY_Y: f32 = 84.0;
const WIFI_PITCH: f32 = 48.0;
const WIFI_FOOT_Y: f32 = 432.0;
const WIFI_GRID_Y: f32 = 200.0;
const WIFI_CELL: f32 = 44.0;
const WIFI_CELL_GAP: f32 = 6.0;
/// 打字页自己的一套纵位：标签、输进去的串、密码下面那句提示。**不借 `WIFI_PITCH`** ——
/// 那 48 是给整页四十四高的行距定的，而这一页在网格顶上就被截住了：网格 200 起、
/// 四行各 50 到 400，脚注 432。标签 76..120、串 124..144（串仍是二十高的小字，见
/// `wifi_glyph`）、提示 152..196，四段依次贴住网格上沿。
const WIFI_EDIT_Y: f32 = 76.0;
const WIFI_EDIT_TEXT_Y: f32 = 124.0;
const WIFI_EDIT_NOTE_Y: f32 = 152.0;
/// How many networks the list shows at once. Six lines at 48 px is the body band — eight
/// was what 34 px bought, and the type grew by half, so the count came down with it.
const WIFI_LIST_MAX: usize = 6;

/// `slotnet.sh scan` prints `ssid\tsignal\tsecured` per network. The name arrives as
/// wpa_cli prints it — printable ASCII, everything else `\xNN` — so it is unescaped here,
/// which is what lets a Chinese network show up as its own name. A line that does not
/// parse is dropped rather than guessed at: the list is what the air said, and a network
/// nobody can read is a network nobody can pick.
fn parse_wifi_scan(out: &str) -> Vec<WifiNet> {
    let mut nets = Vec::new();
    for line in out.lines() {
        let mut fields = line.split('\t');
        let (Some(ssid), Some(signal), Some(secured)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let ssid = slot_store::unescape_ssid(ssid);
        if ssid.is_empty() {
            continue;
        }
        nets.push(WifiNet {
            ssid,
            signal: signal.parse().unwrap_or(-100),
            secured: secured == "1",
        });
    }
    nets
}

/// Which of the screen's three views is up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WifiView {
    /// What the card is set to, and the three things that can be done to it.
    Status,
    /// The scan results, one in hand.
    List,
}

/// Which field the character grid is typing into. `Psk` after a picked network carries
/// that network's name already, so the grid only ever asks for the secret.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WifiField {
    Ssid,
    Psk,
}

/// One layer of the character grid. `chars` is exactly `cols * rows` long: a layer with
/// fewer real characters than cells simply has fewer cells, so there are no empty boxes
/// to wonder about.
#[derive(Clone, Copy)]
pub struct GridLayer {
    pub cols: usize,
    pub rows: usize,
    pub chars: &'static str,
}

/// The four layers, in the order `L`/`R` walks them: lower case, upper case, digits with
/// the sixteen common symbols, then the rest of the ASCII punctuation. Everything a
/// WPA passphrase can legally be is in here; a network whose name is not (a CJK SSID) is
/// still configured by editing `System/wifi.conf` on a desktop, which is the documented
/// way round.
pub const WIFI_LAYERS: [GridLayer; 4] = [
    GridLayer {
        cols: 7,
        rows: 4,
        chars: "abcdefghijklmnopqrstuvwxyz ",
    },
    GridLayer {
        cols: 7,
        rows: 4,
        chars: "ABCDEFGHIJKLMNOPQRSTUVWXYZ ",
    },
    GridLayer {
        cols: 7,
        rows: 4,
        chars: "0123456789.,!?@#$%&*()-_=+ ",
    },
    GridLayer {
        cols: 4,
        rows: 4,
        chars: ":;\"'/\\|<>[]{}~^`",
    },
];

/// The grid while a name or a password is being typed.
#[derive(Clone, Debug)]
pub struct WifiTyping {
    pub field: WifiField,
    /// The name being typed, or the picked network's when only the password is asked for.
    pub ssid: String,
    pub psk: String,
    /// Which layer, and where the caret sits in it.
    pub layer: usize,
    pub row: usize,
    pub col: usize,
}

/// ★ 2026-10-21：**Wi-Fi 配置浮层** —— 设置屏「无线网络」行那道门背后的东西。
///
/// 和键位映射弹窗同一条理由做成 App 级浮层：它在设置页里开，但扫列表/输密码都是它自己的
/// 事，不该把设置页拆掉。三个视图（当前状态 / 扫描列表 / 录入网格）在一张屏里轮转，
/// `B` 一路退回设置页。
pub struct WifiEdit {
    pub view: WifiView,
    /// The scan results, and which one is in hand.
    pub nets: Vec<WifiNet>,
    pub pick: usize,
    /// A scan thread is out; the status view says so instead of listing what to press.
    pub scanning: bool,
    /// The grid, when a name or a password is being typed.
    pub typing: Option<WifiTyping>,
    /// When the "已保存" line went up, so it can fade on its own. The words are a fixed
    /// string — the note is only ever the saved line, so there is nothing to bake.
    pub note: Option<Millis>,
}

/// The faces the Wi-Fi screen is drawn with. The fixed lines are baked once; `ssid` and
/// `nets` follow what the card and the air say, so they are re-baked whenever `wifi_dirty`
/// says something changed.
pub struct WifiFaces {
    pub title: (TexId, u32, u32),
    pub label_current: (TexId, u32, u32),
    pub label_ssid: (TexId, u32, u32),
    pub label_psk: (TexId, u32, u32),
    pub label_open: (TexId, u32, u32),
    pub act_scan: (TexId, u32, u32),
    pub act_manual: (TexId, u32, u32),
    pub act_del: (TexId, u32, u32),
    pub line_pick: (TexId, u32, u32),
    pub line_none: (TexId, u32, u32),
    pub line_scanning: (TexId, u32, u32),
    pub line_no_nets: (TexId, u32, u32),
    pub line_saved: (TexId, u32, u32),
    pub hint_status: (TexId, u32, u32),
    pub hint_list: (TexId, u32, u32),
    pub hint_edit: (TexId, u32, u32),
    /// The current network's name, or `None` when the card has no credentials.
    pub ssid: Option<(TexId, u32, u32)>,
    /// One face per scan result, in the order `nets` holds them.
    pub nets: Vec<(TexId, u32, u32)>,
}

/// ★ 2026-11-11：**货架首字母搜索浮层**（SELECT+Y）。
///
/// 键盘就是 Wi-Fi 配置浮层那张四层全键盘（`WIFI_LAYERS` + `wifi_glyphs`），一本不另烤：
/// a–z / A–Z / 0–9 / 符号四层，方向键走格、A 输入、B 退格、L/R 换层。
///
/// **它不碰 `Shelf` 的过滤维度**，只借 `set_view` 做实时预览：
/// `back_view` 是进来那一刻的整份快照，取消时 `set_view(back_view, back_stem)` 原样还回去，
/// 所以收藏夹 / 子文件夹 / 机种货架怎么叠着进来，就怎么叠着回去（§6d、§6g 两个坑）。
/// `hits` 是本次命中的索引表；空结果时**不换 view**（空 view 会触发顶层空兜底跳去
/// 子文件夹），只把提示换成「没有匹配的游戏」，货架停在上一批结果上。
pub struct SearchState {
    pub query: String,
    pub layer: usize,
    pub row: usize,
    pub col: usize,
    pub back_view: Vec<usize>,
    pub back_stem: Option<String>,
    pub hits: Vec<usize>,
}

/// 搜索浮层的四张定句脸：标题、网格键位、动作键位、空结果。输入串和键帽都用
/// `wifi_glyphs` 逐字拼，一个键都不用重烘。
pub struct SearchFaces {
    pub title: (TexId, u32, u32),
    pub hint_grid: (TexId, u32, u32),
    pub hint_act: (TexId, u32, u32),
    pub empty: (TexId, u32, u32),
}

/// Which end of a link this device is offering to be. The player picks; there is no
/// discovery on this network and nothing to negotiate it with.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkRow {
    Host,
    Join,
}

impl LinkRow {
    /// Host first: it is the end that has to exist before the other one can arrive.
    pub const ALL: [LinkRow; 2] = [LinkRow::Host, LinkRow::Join];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            LinkRow::Host => lang::LINK_HOST,
            LinkRow::Join => lang::LINK_JOIN,
        }
    }

    /// What the radio is asked to bring up: an access point, or an association to one.
    pub fn role(self) -> LinkRole {
        match self {
            LinkRow::Host => LinkRole::Host,
            LinkRow::Join => LinkRole::Join,
        }
    }

    /// libretro's own client id, not ours — 0 the host and 1 the joiner, the only two this
    /// product has. The two devices must never both think they are the same one, which is
    /// exactly what the role they picked decides.
    pub fn client_id(self) -> u16 {
        match self {
            LinkRow::Host => 0,
            LinkRow::Join => 1,
        }
    }
}

/// Which of the deck's two panels is showing.
///
/// **Two, and the second is the one that needs the first.** The name-and-spectrum face is the
/// deck's resting state: it says what is playing whether or not the card carries a single word
/// of lyrics. The scrolling-lyrics face is the one a person turns to when they want the words,
/// and a card with no `.lrc` has none to show — so on such a card the two faces are the same
/// face, and saying so here rather than in the drawer keeps the drawer from having an opinion
/// about whether lyrics exist.
/// **信息页要印的字，拥有所有权。**由 [`App::deck_info`] 造、由前端填进 `lcd::Info`
/// （那个是借用的）。
///
/// ★ 2026-10-06 重做。**为什么要这一层而不是让 `lcd::Info` 直接拥有**：
/// 这一页有两行是**算出来的**（`3/12 歌名`、`时长 3:45`），而合成只在**翻页或换碟**时发生，
/// 不在每帧。要把 `format!` 的结果喂给一个只借 `&str` 的结构，唯一的办法是 `Box::leak`——
/// 而那是一帧泄一次。**拥有在这一层，借用在合成那一层**，一个 `leak` 也不留。
///
/// `keys` 是 `&'static` 的一层间接：它来自 `lang::DECK_KEY_GUIDE`（两语言表择一编译），
/// **不随曲目变化**，所以它不需要拥有；而 `tags` 每首歌都不一样，所以它需要。
pub struct DeckInfo {
    pub disc: String,
    pub disc_line: String,
    pub track_line: String,
    pub artist: String,
    pub length_line: String,
    pub tags: Vec<(String, String)>,
    pub keys: &'static [(&'static str, &'static str)],
}

impl DeckInfo {
    /// 借给 `slot_ui::lcd::Info`。**一个字段对一个字段，顺序即语义。**
    ///
    /// 这个转换本身没有任何逻辑 —— 它存在的理由是**让"app 知道什么"和"slot-ui 怎么排"
    /// 分成两半**：这一页的每一句话都在这里，而它印在哪一档、写在第几行、
    /// 装不下时是跳过还是裁掉，全部在 `lcd` 那一侧。**排版规则不散落在 app 里。**
    pub fn as_ui(&self) -> slot_ui::lcd::Info<'_> {
        slot_ui::lcd::Info {
            disc: &self.disc,
            disc_line: &self.disc_line,
            track_line: &self.track_line,
            artist: &self.artist,
            length_line: &self.length_line,
            tags: &self.tags,
            keys: self.keys,
        }
    }

    /// 这一页有几页。**数据，不是常数** —— 标签多的盘详情页就多。
    pub fn pages(&self) -> usize {
        slot_ui::lcd::info_pages(&self.as_ui())
    }

    /// 详情一共有几行。**给宿主例程量版式用**（`info_check` 打印它）——
    /// 屏上不需要这个数，而把它做成公有方法比让例程去数 `details()` 更好：
    /// **那一层是私有的，例程也不该知道"一行"是怎么定义的。**
    pub fn rows(&self) -> usize {
        slot_ui::lcd::info_detail_rows_of(&self.as_ui())
    }

    /// 合成出来的那一帧，**按 `page` 挑第几页**。
    ///
    /// **`page` 被夹到有效范围**而不是 `panic` 或直接索引：它是 `App` 的一个 `usize`，
    /// 而这一页的全部输入都来自卡上的文件 —— 夹一下比"某张卡让机器崩"便宜。
    ///
    /// ★ **最后一页永远是键位指南**，而它不在 `detail_pages` 的算术里 ——
    ///   所以判据是"是不是最后一页"，不是"页码是不是某个常数"。
    ///   （常数 2 的那一版在标签多到要第三页时，键位页就被挤掉了。）
    pub fn frame(&self, page: usize) -> slot_ui::lcd::Frame {
        let c = self.as_ui();
        let last = self.pages() - 1;
        if page >= last {
            slot_ui::lcd::info_keys_frame(&c)
        } else {
            slot_ui::lcd::info_page_frame(&c, page)
        }
    }

    /// 烘焙门槛用的键：**这一页说的是不是同一件事**。
    ///
    /// ★ **为什么不给哈希**：要比较的输入一共五个字符串加一张标签表，
    /// **逐个比一遍和求一个哈希的成本一样**，而哈希有一个"极小概率撞了"的失效模式，
    /// 那个模式的症状是"换了碟还显示上一张的封面"——**屏幕上完全看不出是 bug**。
    /// 逐字段比的成本是几次 `memcmp`，撞不了。
    pub fn same_as(&self, other: &DeckInfo) -> bool {
        self.disc == other.disc
            && self.disc_line == other.disc_line
            && self.track_line == other.track_line
            && self.artist == other.artist
            && self.length_line == other.length_line
            && self.tags == other.tags
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Panel {
    /// Song name and the spectrum. The default, because it is the one that always has something.
    #[default]
    Name,
    /// Scrolling lyrics, falling back to the name alone when the track carries none.
    Lyrics,
}

impl Panel {
    /// The other one. A single function because the two call sites are a key press and a test,
    /// and two hand-written `match`es would be two places to forget a variant.
    pub fn toggled(self) -> Panel {
        match self {
            Panel::Name => Panel::Lyrics,
            Panel::Lyrics => Panel::Name,
        }
    }
}

/// 光驱里那颗**还在转**的光盘 —— 进 PS1 游戏、CRT 开机转场那一小段的状态。
///
/// 六六原话："加载完毕的时候光盘继续转，屏幕出现 CRT 电视开机的效果（画面从中间一点辉光
/// → 横向的一条辉光线 → 展开后是游戏画面 → 铺满屏幕，开始游玩）"。
///
/// 为什么要有这块状态：插入动画（`slot_ui::draw_disc_insert`）自己算角度，用的是插入的
/// 本地时间 `t`；一旦进了 `Playing`，插入的 `t` 就没了。而"继续转"必须**接着插入末帧的
/// 相位**走 —— 否则交接那一帧盘面会跳一个随机角。所以交接那一刻把三样东西抄下来：
///
/// * `angle0`：当时的光盘相位（`slot_ui::disc_spin_angle`，带 `None` 即"不刹"）；
/// * `at`：当时的 `clock`（毫秒），之后的相位由它往前推；
/// * `t0`：当时的**插入时间**（秒），呼吸光相位接着它走。
///
/// 清零的时机：`screen` 长满（画面 640×480 已经把插卡位那个洞一起盖住了）——
/// 见 `step_screen`。
#[derive(Clone, Copy, Debug)]
struct DiscLive {
    angle0: f32,
    at: f64,
    t0: f32,
}

/// ★ 2026-10-27（六六）：ARCADE / NEOGEO 的**投币素材**，开机烘一次（换主题重烘）。
///
/// 两张：`cab` = 从插卡口升起的街机箱体（`SLOT_LOAD_*.png`，洋红区已换成本主题亮色）；
/// `glow` = **只烘洋红区**的那圈柔光（比本体大一圈），画在本体下面、alpha 做呼吸。
///
/// **代币不在这里** —— 它直接复用货架烘出来的那张卡面（同一张图、同一个随机色）。
#[derive(Copy, Clone, Debug)]
pub struct Coin {
    pub cab: (TexId, u32, u32),
    pub glow: Option<(TexId, u32, u32)>,
}

impl DiscLive {
    /// 交接那一刻的样子：相位从插入动画的末帧续上，时钟从 `clock` 起算。
    fn handoff(t_insert: f32, clock: f64) -> Self {
        Self {
            angle0: slot_ui::disc_spin_angle((t_insert - slot_ui::DISC_SPIN_START).max(0.0), None),
            at: clock,
            t0: t_insert,
        }
    }

    /// `(相位, 时间轴上的 t)` —— 光盘相位接着交接帧满速往前推，`t` 给呼吸光取相位。
    fn phases(&self, clock: f64) -> (f32, f32) {
        let dt = ((clock - self.at) / 1000.0) as f32;
        (self.angle0 + dt * slot_ui::DISC_SPIN_RATE, self.t0 + dt)
    }
}

#[derive(Debug)]
pub enum Phase {
    /// Slot's own first launch, ahead of the shelf and ahead of a seated cart. Three things
    /// run off the wall clock and a cartridge RTC is the one that breaks silently.
    SetClock {
        picker: ClockPicker,
        /// Whether the picker was opened from the settings screen rather than being slot's own
        /// first launch. It decides *both* ways out: confirming it returns to the settings screen
        /// instead of starting the shelf, and `B` is a way back rather than a key with no meaning.
        /// A first launch has nowhere to go back to — that is the whole difference.
        from_menu: bool,
    },
    Shelf,
    Inserting {
        cart: String,
        t: f32,
        core_ready: bool,
        /// A cart already in the slot at boot. It is drawn seated from the first frame and
        /// the shelf is never drawn behind it, because a resume is not a movement the user
        /// made and there is nothing for the cart to have come from.
        resumed: bool,
        /// Start the cart from the beginning, ignoring whatever `resume.state` holds. The
        /// state is skipped rather than deleted, so a later tap still resumes it.
        clean: bool,
        /// PS1 光盘插入状态：`Some` 表示这是光盘插入，`None` 表示普通卡带插入。
        disc: Option<slot_ui::DiscState>,
    },
    Playing {
        cart: String,
    },
    Ejecting {
        cart: String,
        t: f32,
    },
    Polaroids {
        cart: String,
    },
    /// ★ 2026-10-06 六六要的**信息页**：SELECT 呼出的一张全屏纸，把碟片与曲目摊开给你看。
    ///   为什么单独一个 Phase 而不是 Listening 上的一个标志：它**要把屏整个占住**，
    ///   而 Listening 的屏是满屏液晶面板 —— 两个画面叠在一起只会互相压。
    DiscInfo,
    /// The menu: the label, and a grid of the places this frontend can go that are not a game.
    ///
    /// It was `About`, and it was one screen holding both: the label and thirty-nine rows of
    /// shortcut, where a reader who wanted the fourth row scrolled past three screens of the
    /// first. The card is a screen of its own now (`Shortcuts` below) and this is what MENU is
    /// for — a short list, laid out for four and filled by two.
    Menu {
        /// Which button is in hand, as an index into `menu::Item::GRID`. Never an empty slot:
        /// the navigation steps over those rather than landing on one.
        index: usize,
    },
    /// The shortcut card, as a screen of its own. Everything the old about screen did with a
    /// scroll belongs here and nothing else does — this is the only screen in the frontend that
    /// is taller than the panel.
    Shortcuts {
        scroll: f32,
        want: f32,
    },
    /// The settings screen: what the **device** is set to, as against what the card holds —
    /// the two speeds, the clock and the lid timer. Behind the menu's fourth button, and a list
    /// of rows rather than a grid of buttons: a button answers "where does this go" and a row
    /// answers "what is this set to", and every setting this frontend has is the second kind.
    ///
    /// ★ **2026-10-10: four rows, a window that holds three, so the list scrolls.** The same
    /// shape the shortcut card has — a press moves `row` and the offset eases after it — except
    /// that here the target is **derived from the row** (`settings::scroll_for`) rather than
    /// tracked beside it: one press is one row, so there is nothing for a second field to hold
    /// that the row does not already say, and nothing that can drift after a wrap.
    Settings {
        /// Which row is in hand, as one of `slot_ui::settings::Row::ALL`.
        row: slot_ui::settings::Row,
        /// How far the list is scrolled, in pixels. Never written by the input — it eases toward
        /// `settings::scroll_for(row.index())` and stops there.
        scroll: f32,
    },
    /// The deck: a disc, what is playing on it, and the transport keys under it. Reached from the
    /// menu's `Discs` button, and the screen the menu exists to have.
    Listening {
        /// Which key is in hand, as an index into `deck::Key::ALL`.
        key: usize,
        /// Whether the disc is running. Play and pause are one key with two glyphs, so this is
        /// what decides which glyph that key wears.
        playing: bool,
        /// Which disc is in the deck, as an index into `App::discs`. **Distinct from the key
        /// index on purpose**: a deck's transport is under the fingers and its disc is what the
        /// hand reaches past the transport to change, so L and R move the disc and Left and
        /// Right walk the keys. Folding the two into one number would make the arrows do two
        /// different jobs depending on what they landed on.
        disc: usize,
    },
    Doze {
        cart: Option<String>,
        /// ★ 2026-11-02（六六）：「放音乐时屏蔽超时关机；唤醒回播放器」。
        ///
        /// The deck keeps playing through a doze — its decode thread is its own and the
        /// device is shared, so a shut lid over a running disc is dark screen, loud
        /// music. What the lid takes away is the screen and nothing else, so the doze
        /// carries the deck's place rather than dropping it: which disc, which key, and
        /// whether the transport was running. `None` = the lid closed over something
        /// that is not the deck, and waking falls back to the shelf as before.
        deck: Option<(usize, usize, bool)>,
    },
}

/// Where the shelf's line of type sits: centred in the gap between the row's own foot and
/// the top of the case. Derived rather than picked, so moving either end — a taller cart, a
/// deeper bay — carries it along instead of leaving it behind.
///
/// How many carts fall under each slot of the letter ring, in `letters::SLOTS` order.
///
/// One pass at boot and never again: the card cannot change while slot is running. The strip
/// reads it to know which letters are worth stopping on and which are drawn dim, so it has to
/// be counted from the same list the row is drawn from rather than from anything on disk.
fn tally_letters(carts: &[Cart]) -> [usize; letters::N] {
    let mut counts = [0usize; letters::N];
    for cart in carts {
        counts[letters::slot_of(cart.initial)] += 1;
    }
    counts
}

/// The panel's ink at `a`, for the drawn readouts that are not type and so have no baked face to
/// take their colour from.
///
/// **A multiply on the ink rather than a grey of its own**, which is what makes one number work
/// on both themes: a fixed grey that reads as "dimmed ink" on the dark panel is a grey smear on
/// the light one, and a theme that has to be walked twice to check a bar is a theme that will be
/// walked wrong.
fn dim_ink(a: f32) -> [f32; 4] {
    let mut c = slot_ui::palette::ink_f();
    c[3] = a;
    c
}

/// Where the shortcut card starts on its own screen.
///
/// It used to hang under the label, so its first row was the label's foot plus a gap. The label
/// is on the menu screen now — `menu::TOP` is where the menu's grid begins, and it is derived
/// from the label's own height — so the card takes the panel's top padding instead. The two
/// numbers are the same, which is what keeps the two screens' type on one line when the user
/// goes from one to the other.
const CARD_TOP: f32 = 26.0;

/// The bar the pinned foot line is set on: the panel's full width, opaque, flush with the
/// bottom edge. Drawn here rather than baked into the hint's own face, because it belongs to
/// the screen and not to the card — the card is 660 across, this is all 720.
fn hint_bar_ink() -> [f32; 4] {
    // **Why this is a function and not a constant (2026-10-23).** It used to be one fixed
    // near-black, which is fine on the device's dark case and *wrong on every light look*. On PAPER
    // the bar measured **15.17:1** against the sheet — a black slab across a cream page — while
    // the hint's own `dim_ink` sat on it at **2.24:1** and its `halo` outline at **16.92:1**, so the
    // outline outshouted the letters by 7.6x and the words read as hollow engravings (the 8x crop
    // of the rendered light look shows exactly that). A foot line belongs to the case, so the bar is
    // now **the ground stepped 55% toward `housing`**: a quiet bezel that follows all fourteen
    // looks — 1.15:1 off the sheet instead of 15.17:1 — and puts the same letters at **5.87:1
    // light / 9.30:1 dark**, with the halo dropped to 1.29:1 so it stops competing with them.
    let ground = slot_ui::palette::ground();
    let housing = slot_ui::housing();
    let step = |g: u8, h: f32| g as f32 / 255.0 * 0.45 + h * 0.55;
    [
        step(ground[0], housing[0]),
        step(ground[1], housing[1]),
        step(ground[2], housing[2]),
        1.0,
    ]
}
/// The bar's lit top edge: a few hairlines fading upward, which is as close as a list of flat
/// rects comes to a glow. Faint on purpose — it is a rim light, not a light.
const HINT_GLOW_STEPS: u32 = 8;
const HINT_GLOW_H: f32 = 1.5;
const HINT_GLOW_A: f32 = 0.13;

/// How far one press takes the card. Six rows: less than that is a card the user has to walk to
/// the end of, and a whole screen is a card that jumps past the line it was being read at.
const CARD_PAGE: f32 = 180.0;

/// How fast the card closes on where the last press asked it to go, as a share of the way there
/// per second. Slower than the letter strip's spring on purpose: this is text being moved, and
/// there is nothing here that has to land on anything.
const CARD_EASE: f32 = 9.0;

/// The same, for the settings list — and **faster on purpose**. The card is a page of text the
/// user is reading and a page that arrives late is a page they have to find their place in; the
/// settings list moves one row (fifty pixels) and the press that moved it is still under the
/// finger. A field that took the card's time to travel read as a dropped press.
const SETTINGS_EASE: f32 = 16.0;

/// The footer band and the faint light along its top edge. Opaque and the panel's full width,
/// so the rows scrolling under it pass behind it rather than through it. `top` is where the
/// band starts; the glow is laid above it, brightest against the edge.
fn draw_hint_bar(top: f32, out: &mut Vec<Draw>) {
    out.push(Draw::Rect {
        x: 0.0,
        y: top,
        w: OUT_W as f32,
        h: (OUT_H as f32 - top).max(0.0),
        colour: hint_bar_ink(),
    });
    for i in 0..HINT_GLOW_STEPS {
        // Brightest next to the edge, dimming as it climbs away from it.
        let a = HINT_GLOW_A * (i as f32 + 1.0) / HINT_GLOW_STEPS as f32;
        let y = top - (HINT_GLOW_STEPS - i) as f32 * HINT_GLOW_H;
        out.push(Draw::Rect {
            x: 0.0,
            y,
            w: OUT_W as f32,
            h: HINT_GLOW_H,
            // `slot_ui::rim()` — the case's own simulated highlight — so the bar's lit edge
            // follows the same rule as every moulded face, instead of a warm grey hard-coded for
            // the dark look (it read as a stray cream line across the light look's black slab).
            colour: {
                let rim = slot_ui::rim();
                [rim[0], rim[1], rim[2], a]
            },
        });
    }
}

/// Where 灰度 sits in `GRADES` — the one grade that keeps reading the card's own `cc.txt`, so it
/// needs naming rather than being found by counting.
pub const GRADE_GRAY: u16 = 9;

/// One entry of the colour-machine grade list — a name for the browser and boot log, and what
/// the grade actually is.
pub struct Grade {
    pub name: &'static str,
    pub kind: GradeKind,
}

/// How a grade makes its look. Four mechanisms, because that is what the good ones need.
pub enum GradeKind {
    /// Straight through.
    Off,
    /// A 3×3 and the gamma it runs under, both in linear space. The matrix already carries its
    /// profile's luminance: `lum` is a scalar applied to the linear colour *before* the matrix,
    /// so folding it into the entries is exact arithmetic rather than an approximation.
    Matrix([[f32; 3]; 3], f32),
    /// Two stops of a 1D ramp — dark then light — walked by the picture's own luma.
    Ramp([u8; 3], [u8; 3]),
    /// A baked 32³ lookup, laid out exactly like `u_pix`: 1024×32 RGBA, `idx = (b*1024 + g*32 + r)*4`.
    Lut(&'static [u8]),
}

/// The colour grades a GBA or GBC cycles, **page one then page two, nine to a page**. The order
/// is the one the player asked for, so the two pages are two moods: the first is "what the panel
/// was", the second is "what the picture can be made to look like".
pub const GRADES: [Grade; 18] = [
    // ---- page 1: 实机系 ----
    Grade {
        name: "无矫正",
        kind: GradeKind::Off,
    },
    Grade {
        name: "AGB-001",
        kind: GradeKind::Matrix(DisplayFilter::AGB_001_CC, 2.2),
    },
    Grade {
        name: "libretro GBA",
        kind: GradeKind::Matrix(
            [
                [0.82355, 0.17745, -0.09100],
                [0.09100, 0.59150, 0.22750],
                [0.14333, 0.12968, 0.63700],
            ],
            2.2,
        ),
    },
    // VBA/No$GBA full colour: target 2.45 with a display gamma of 1.45, which one gamma cannot
    // hold exactly; 1.69 is the best single value and the look is a nostalgic one anyway.
    Grade {
        name: "VBA 全彩",
        kind: GradeKind::Matrix(
            [
                [0.73000, 0.27000, 0.00000],
                [0.08250, 0.67750, 0.24000],
                [0.08250, 0.24000, 0.67750],
            ],
            1.69,
        ),
    },
    Grade {
        name: "GBC 转置",
        kind: GradeKind::Matrix(
            [
                [0.88000, 0.05000, -0.03000],
                [0.07250, 0.60000, 0.35000],
                [0.02500, 0.05000, 1.20000],
            ],
            1.7,
        ),
    },
    Grade {
        name: "NDS",
        kind: GradeKind::Matrix(DisplayFilter::NDS_CC, 2.0),
    },
    Grade {
        name: "DB16",
        kind: GradeKind::Lut(include_bytes!("../assets/db16.bin")),
    },
    Grade {
        name: "DB32",
        kind: GradeKind::Lut(include_bytes!("../assets/db32.bin")),
    },
    Grade {
        name: "Commodore 64",
        kind: GradeKind::Lut(include_bytes!("../assets/c64.bin")),
    },
    // ---- page 2: 风格系 ----
    Grade {
        name: "灰度",
        kind: GradeKind::Matrix(DisplayFilter::DEFAULT_CC, 1.0),
    },
    Grade {
        name: "双色 深蓝米白",
        kind: GradeKind::Ramp([16, 24, 64], [246, 238, 214]),
    },
    Grade {
        name: "双色 紫青",
        kind: GradeKind::Ramp([48, 24, 80], [150, 245, 235]),
    },
    Grade {
        name: "双色 墨绿淡黄",
        kind: GradeKind::Ramp([18, 44, 32], [238, 232, 168]),
    },
    Grade {
        name: "双色 棕奶白",
        kind: GradeKind::Ramp([58, 34, 22], [250, 226, 190]),
    },
    Grade {
        name: "TealOrange",
        kind: GradeKind::Lut(include_bytes!("../assets/tealorange.bin")),
    },
    Grade {
        name: "HorrorBlue",
        kind: GradeKind::Lut(include_bytes!("../assets/horrorblue.bin")),
    },
    Grade {
        name: "Classic Chrome",
        kind: GradeKind::Lut(include_bytes!("../assets/classicchrome.bin")),
    },
    Grade {
        name: "BleachBypass",
        kind: GradeKind::Lut(include_bytes!("../assets/bleachbypass2.bin")),
    },
];

/// What a colour grade does to **one colour** — the same arithmetic the game pass does, run on a
/// single pixel so the browser can show what a grade is before it is picked.
///
/// A Game Boy's entries carry their own four shades, so a cell shows the palette itself; a grade
/// has no such list, so it is run instead. **It used to be run on the grey axis** — `[0, 85, 170,
/// 255]` and every step between — which answers "what does this grade do to white and black", and
/// most grades leave those alone: every entry came out looking alike and grey, and the player said
/// exactly that. A hue is what a picture is made of, so a hue is what is fed in (`SWEEP`), and
/// three of the four mechanisms below can only be told apart on one:
///
/// * a matrix moves colours, so a sweep shows it as a wash or a cast;
/// * a palette grade collapses hues onto its own colours, so the sweep becomes a stack of them;
/// * a baked cube is a *tone* map as well, so the tone strip under the sweep is where its split
///   shows — `TealOrange` leaves a saturated dark red red and puts its teal in the neutral
///   shadows, so a second row of hues would have shown nothing of it.
pub fn grade_rgb(g: &Grade, rgb: [u8; 3]) -> [u8; 3] {
    let mut out = [0u8; 3];
    match g.kind {
        GradeKind::Off => out = rgb,
        // In linear space, and as the real 3x3 rather than a row sum: with a grey input every
        // column is the same number and the sum was correct, but a hue has three different ones.
        GradeKind::Matrix(m, gamma) => {
            let lin = rgb.map(|c| (c as f32 / 255.0).powf(gamma));
            for row in 0..3 {
                let x = m[row][0] * lin[0] + m[row][1] * lin[1] + m[row][2] * lin[2];
                out[row] = (x.clamp(0.0, 1.0).powf(1.0 / gamma) * 255.0).round() as u8;
            }
        }
        // Walked by the picture's own luma, Rec.601 weights, exactly as the shader samples
        // `ramp_lut`'s 256 entries.
        GradeKind::Ramp(dark, light) => {
            let l = 0.299 * rgb[0] as f32 + 0.587 * rgb[1] as f32 + 0.114 * rgb[2] as f32;
            for c in 0..3 {
                let (a, b) = (dark[c] as f32, light[c] as f32);
                out[c] = (a + (b - a) * (l / 255.0)).clamp(0.0, 255.0).round() as u8;
            }
        }
        // The cube is 32 per channel, `idx = (b*1024 + g*32 + r)*4`, so a colour is quantised to
        // the cell of the cube it lands in — which is what the shader's own lookup does.
        GradeKind::Lut(bytes) => {
            let q = rgb.map(|c| (c as usize * 31 + 127) / 255);
            let at = (q[2] * 1024 + q[1] * 32 + q[0]) * 4;
            out = [bytes[at], bytes[at + 1], bytes[at + 2]];
        }
    }
    out
}

/// A colour grade's browser cell: **the grade, run over a picture's colours** — `SWEEP`'s eight
/// hues with `TONE`'s black-to-white strip under them, every stop through `grade_rgb`.
///
/// The cell is the grade rather than a drawing of it, which is what makes the browser worth
/// opening: a grade that washes colour out washes the cell out, a palette grade snaps it onto that
/// palette's own colours, and a split-tone pulls the two ends of the tone strip apart — teal at
/// one end and orange at the other, which is why it is called that.
pub fn grade_swatch(g: &Grade) -> slot_ui::PaletteSwatch {
    slot_ui::PaletteSwatch::chart(|rgb| grade_rgb(g, rgb))
}

/// The 1D ramp a duotone walks: 256 RGBA entries, dark at the picture's black and light at its
/// white, interpolated in the encoded space the shader samples with (see `GAME_FRAG`).
fn ramp_lut(dark: [u8; 3], light: [u8; 3]) -> [u8; 1024] {
    let mut lut = [0u8; 1024];
    for i in 0..256usize {
        for c in 0..3 {
            let (a, b) = (dark[c] as u32, light[c] as u32);
            lut[i * 4 + c] = ((a * (255 - i as u32) + b * i as u32 + 127) / 255) as u8;
        }
        lut[i * 4 + 3] = 255;
    }
    lut
}

/// The on-screen picture's two independent knobs.
///
/// Two ways to the same end — a screen the picture is seen *through* — and the machine picks:
///
/// **A mesh**, drawn in the game shader (`display_grid`), on the two machines with a screen
/// bezel: a Game Boy's is its palette's lightest shade and always on, a Game Boy Color's is
/// white or black and rides the overlay ring.
///
/// **An aperture table** (`lcd3x`, `applied_mask`), on the Advance: a 3x3 of per-subpixel
/// multipliers. It is the one road to a real LCD subpixel structure, which a mesh cannot be —
/// and it is a *multiply*, so it can only darken, which is why the bezel machines draw a mesh
/// instead (a white grid needs a mix). `SELECT+X` steps its `MASK_STATES` presets — off, the
/// LCD3x table at four strengths, and the scanline at two — and the position is the first
/// number of `display.txt`.
///
/// `cc` picks the colour correction. The *parameters* are shared — every colour machine (GBA,
/// GBC and the console cores SFC/FC/MD/PCE/PS1/FBNEO) walks one table of **`GRADES`**, and a Game
/// Boy walks its own palette table instead (see `gb_lut`), because a greyscale machine's "colour"
/// is a palette rather than a grade — but each machine remembers its own pick, so `System/display.txt`
/// keeps one line per machine (`mask`, then `gba`/`gb`/`gbc`/`sfc`/`fc`). Each is cycled on its
/// own chord — SELECT+X for that machine's screen look (the overlay ring where there is art, the
/// grid ring where there is none), SELECT+Y for the colour.
///
/// AGB-001 与 NDS 在**线性空间**里做（`CC_GAMMA`）：直接在编码空间乘会把画面压暗、冲淡；
/// 转线性、乘完再转回，观感才对（RetroArch 手持着色器同法）。灰度档走 gamma 1.0，行为与旧版一致。
struct DisplayFilter {
    /// The card's LCD3x table if it ships one, else the built-in. **Kept but unbound** with the
    /// aperture it feeds — see the struct doc.
    #[allow(dead_code)]
    lcd3x: [[[u8; 3]; 3]; 3],
    /// The card's colour matrix (from `cc.txt`) if it ships one, else the NOCOLOR (0% saturation)
    /// default. Only used when `cc_mode` is 2. The matrix the game pass multiplies the picture by,
    /// in the spirit of a colour-saturation shader but applied as a single 3x3 multiply so it
    /// costs nothing here.
    nocolor_cc: [[f32; 3]; 3],
    /// The Advance's aperture choice, 0..=4. Read by nothing else — see the struct doc.
    /// The Advance's aperture preset, `0..MASK_STATES` — what SELECT+X steps on the GBA.
    mask_mode: u8,
    /// ★ 2026-10-26 六六：**新加**。NEOGEO / ARCADE / MD / PS1 / PCE 的扫描线档，0/1/2 分别
    /// 是「无 / 50% / 100%」。这三个机种的 SELECT+X 走这条（见 `App::cycle_mask` 的机种
    /// 分派），跟 `mask_mode`（GBA 孔径）和 `overlay_index`（GB/GBC/FC/SFC 的遮罩环）
    /// 分开存，理由：一个卡上可能同时留着 GBA 的孔径设定和 PS1 的扫描线设定，两者不能
    /// 共享同一个 u8 —— 共享的话切机种就互相踩，用户上一次选的孔径下次开机就丢了。
    /// 走 `System/display.txt` 的 `scanline` 行持久化。
    scanline_state: u8,
    /// One colour-correction choice per *machine*, in `machine_slot` order
    /// (GBA, GB, GBC, SFC, FC). Every machine remembers its own pick.
    ///
    /// The *parameters* are shared — that is the only sharing there is. Every colour machine
    /// (Advance, Color, and the console cores SFC/FC/MD/PCE/PS1/FBNEO) walks the one colour-grade
    /// ring in `GRADES`; a Game Boy walks its own palette table instead (`gb_lut`), because a
    /// greyscale machine's "colour" is a palette rather than a grade. But the *index* each machine
    /// holds is its own: pick a grade on the Advance and the Game Boy Color does not move. The two
    /// are stored and cycled independently, and `System/display.txt` keeps one line per machine so
    /// adding a console core later is a one-arm addition here and a new line on disk, never a
    /// reshuffle of other machines' remembered picks.
    ///
    /// `u16` rather than `u8` because the Game Boy's slot is an index into the whole palette
    /// table, which is past 255 entries and will only grow. The colour machines' grades are
    /// bounded by `CC_COUNT`; the Game Boy's by `GB_CC_COUNT`.
    cc: [u16; 10],
}

impl DisplayFilter {
    const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    // GRAYSCALE (0% saturation): plain luma. The whole picture collapses to its luma
    // (Rec.601 weights 0.299/0.587/0.114), so colours keep their brightness but lose hue
    // entirely — the "black & white / 灰度" look. This is mode 0. Overridable per card through
    // System/cc.txt; this is the fallback. Uploaded row-major -> column-major by
    // `Fbo::set_color_correction`, so this multiplies as written.
    const DEFAULT_CC: [[f32; 3]; 3] = [
        [0.299, 0.587, 0.114],
        [0.299, 0.587, 0.114],
        [0.299, 0.587, 0.114],
    ];
    /// AGB-001 (mode 1): the authentic original Game Boy Advance screen. The AGB-001 / AGS-001
    /// panel had a small, non-sRGB gamut, so games — mastered oversaturated to fight the dim
    /// reflective screen — look washed out, faintly green, and dark on it. This is a desaturation
    /// to s = 0.6 (luma blend, Rec.601 weights) with a mild green cast (blue pulled down, green
    /// nudged up), run in linear space under `CC_GAMMA` (2.2) so the midtones darken as the real
    /// panel did. Values are a first pass from the colour science (see the Handheld Color Space
    /// Project); every entry here is the whole look, so tune to taste.
    const AGB_001_CC: [[f32; 3]; 3] = [
        [0.7052, 0.2301, 0.0447],
        [0.1220, 0.8515, 0.0465],
        [0.1100, 0.2160, 0.5939],
    ];
    /// NDS (mode 2): the Nintendo DS Phat screen — a softer, near-sRGB middle ground between the
    /// washed-out GBA and modern displays. Desaturation to s = 0.88 (mild), no tint, run under a
    /// slightly lower gamma (2.0) than AGB-001 so it reads brighter, matching the DS Phat's
    /// backlit panel. Pokefan531's `nds-color` shader is the reference.
    const NDS_CC: [[f32; 3]; 3] = [
        [0.91588, 0.07044, 0.01368],
        [0.03588, 0.95044, 0.01368],
        [0.03588, 0.07044, 0.89368],
    ];

    /// 线性空间里做色彩校正所用的 gamma（对应 `GAME_FRAG` 的 `u_cc_gamma`）。
    /// 2.2 = sRGB。AGB-001/NDS 两档在线性空间里做（游戏 ROM 是编码空间、直接乘会被压暗/冲淡）；
    /// 灰度(0) 传 1.0，仍是编码空间直乘，与旧版逐位一致。
    pub const CC_GAMMA: f32 = 2.2;
    /// Which slot a machine's colour choice lives on. The frontend shares the *palette
    /// parameters* across machines — one `GRADES` ring for every colour machine, one palette
    /// table for the Game Boy — but each machine keeps its own remembered pick, so every variant
    /// gets a distinct slot. Adding a console core later is a one-arm addition here and a new
    /// line in `display.txt`, and no other machine's remembered choice moves.
    const fn machine_slot(s: System) -> usize {
        match s {
            System::Gba => 0,
            System::Gb => 1,
            System::Gbc => 2,
            System::Sfc => 3,
            System::Fc => 4,
            System::Neogeo => 5,
            System::Arcade => 6,
            // ★ 2026-10-25：MD 与 PS1，同样追加在末尾，理由同上 —— 这个下标是
            //   `display.txt` 的键，插队会把每一张已发出的卡上的颜色选择静默指到别的机种。
            System::Md => 7,
            System::Ps1 => 8,
            // ★ 2026-10-25：PCE 追加在末尾，理由同上 —— 这个下标是 `display.txt` 的键。
            System::Pce => 9,
        }
    }

    /// How many colour-correction entries the colour machines (GBA, GBC) have: grayscale, AGB-001,
    /// NDS, pixel-art — four. The Game Boy has its own count (the whole palette table).
    pub const CC_COUNT: u16 = GRADES.len() as u16;
    /// How many the Game Boy has: the whole palette table, which is every PixelShift and TWB64
    /// entry the core ships plus the two house looks in front of them. See `crate::palettes`.
    pub const GB_CC_COUNT: u16 = crate::palettes::GB_PALETTES.len() as u16;

    /// The Game Boy's palettes — the whole table, as `(name, four shades)` — live in
    /// `crate::palettes`, generated from gambatte's own `gbcpalettes.h`.
    ///
    /// The table is four shades rather than a matrix, because four shades is what the machine
    /// has. A 3x3 multiply can only slope between one white point and one black point: it can
    /// tint, and it cannot follow a palette whose middles sit off that line. `Sunburst` runs pale
    /// green -> yellow -> orange -> red, and a two-point map draws a straight line from one end to
    /// the other and loses both of the middle colours. `gb_lut` turns a table like this into a
    /// per-shade map instead, and the shader looks it up.
    ///
    /// Entry 0 原生灰 is the core's own output, untouched — and it is deliberately *not* routed
    /// through the lookup, so the machine's neutral look costs nothing.
    ///
    /// The three Game Boy models — the screen art **and** the four shades behind it — live in
    /// `crate::palettes::GB_MODELS`.
    ///
    /// They are not entries in the palette table, and that is the whole of what "bound" means:
    /// each model's four shades were drawn as the other half of its screen art, so the choice is
    /// made once, by choosing the art. Letting them in as palettes as well would be two ways to
    /// land on one look.
    ///
    /// How many of a Game Boy's overlay choices are models: they come first, so a card that ships
    /// the three masks opens on the DMG.
    pub const GB_MODEL_COUNT: usize = 3;

    /// How much of the lattice colour is mixed into the dots: half.
    ///
    /// The geometry is the shader's — the right column and bottom row of every 3x3 source pixel,
    /// 5 of its 9 — and at that coverage the mesh is much heavier than the old aperture table
    /// ever was, so this is the one number that decides whether the picture survives it. Tried
    /// at 35% and set back to 50% on the device; it is a taste dial and nothing else depends
    /// on the value.
    pub const GB_GRID_MIX: f32 = 0.5;
    /// How strongly the Game Boy Color's lattice shows, in either colour. Three tenths: a half read
    /// as a veil over the picture rather than as the glass in front of it, and a quarter was a
    /// shade too faint.
    pub const GBC_GRID_MIX: f32 = 0.30;
    /// How strongly the consoles' scanlines show, at their half state and their full one.
    ///
    /// A Game Boy Color's lattice is a mesh seen through a plastic window, so it mixes at a
    /// third and reads as glass. A CRT's scanline is the picture's own row gap, so the
    /// consoles go from half to full — the three states their ring offers are off, half and
    /// full, and these are the last two.
    pub const CONSOLE_GRID_MIX_HALF: f32 = 0.5;
    pub const CONSOLE_GRID_MIX_FULL: f32 = 1.0;
    /// **扫描线隔几行画一条**（2026-10-24 六六：「FC/SFC 的扫描线太密了画面基本看不见，
    /// 能不能减少一半密度？」）。
    ///
    /// 原来每一条源像素行都画（`step = 1`）：FC/SFC 是 2 倍整数缩放，一条线占 2 屏像素、
    /// 间隔也只有 2 屏像素 —— 占屏面积 1/3，黑 50% 时等于给画面盖了层纱。
    /// `step = 2` 只在一半的行上画，间距翻倍到 4 屏像素、占屏 1/6。
    ///
    /// 这是**几何**不是浓度：`CONSOLE_GRID_MIX_*` 决定一条线多黑，这个决定一屏多少条。
    /// GBC 的格子（`GBC_GRID_STATES` 那三个态）不走这条路，它永远 `step = 1`。
    pub const CONSOLE_SCANLINE_STEP: u32 = 2;
    /// How many states each Game Boy Color overlay carries on the ring: **without the grid, with a
    /// white one, and with a black one**. The count is what ties `overlay_choices`,
    /// `ordinary_overlay_at` and `display_grid` together, so it lives in one place.
    pub const GBC_GRID_STATES: usize = 3;
    /// How many apertures the Advance's ring holds: OFF, four strengths of the LCD3x table, and the
    /// scanline at two. **The number itself lives in `root`,** because `display.txt`'s first field
    /// indexes it and that is the file that has to clamp against it; this is the ring agreeing with
    /// the card rather than the other way round.
    pub const MASK_STATES: usize = crate::root::MASK_STATES;

    /// The palette's own name is not a second array beside the table: each entry of
    /// `crate::palettes::GB_PALETTES` is `(name, four shades)`, so a name cannot drift from the
    /// colours it names. `display_palette_name` is the accessor.
    ///
    /// Where the four shades sit in the greys the core hands over. Not evenly spaced: mGBA's
    /// greyscale is its own four levels (RGB555 `0x7FFF / 0x56B5 / 0x294A / 0x0000`, which is
    /// 255 / 173 / 82 / 0 once widened), and the lookup has to put each shade's colour *on* its
    /// level rather than on an idealised third of the range.
    const GB_SHADE_LEVELS: [u8; 4] = [255, 173, 82, 0];

    /// Turn four shades into a 256-entry RGBA lookup, so the shader can be exact.
    ///
    /// Every grey the machine can hand over gets a colour, interpolated between the two shades
    /// either side of it. Interpolating matters even though a Game Boy only draws four levels:
    /// the reflection pass feeds this a *blurred* frame, which is continuous, and a lookup that
    /// only knew about four values would posterise the glow around the screen.
    ///
    /// Built in encoded space, the space the panels are described in, so a shade lands on its
    /// own colour exactly rather than a gamma away from it.
    ///
    /// Takes the shades themselves rather than an index into the table: the table's entries and
    /// the three models' four shades are the same kind of thing here, and only one of the two has
    /// an index at all.
    fn gb_lut(pal: [[u8; 3]; 4]) -> [u8; 1024] {
        let lv = Self::GB_SHADE_LEVELS;
        let mut lut = [0u8; 1024];
        for g in 0..256u32 {
            // Find the pair of shades this grey falls between. `lv` runs bright to dark and `g`
            // runs dark to bright, so the search walks the other way.
            let mut lo = 0;
            while lo < 3 && (g as u8) < lv[lo + 1] {
                lo += 1;
            }
            // Between lv[lo] (brighter) and lv[lo + 1] (darker).
            let (hi_lv, lo_lv) = (lv[lo] as u32, lv[(lo + 1).min(3)] as u32);
            let t = if hi_lv == lo_lv {
                0
            } else {
                ((hi_lv - g) * 255 / (hi_lv - lo_lv)).min(255)
            };
            let a = pal[lo];
            let b = pal[(lo + 1).min(3)];
            let i = (g * 4) as usize;
            for c in 0..3 {
                let (x, y) = (a[c] as u32, b[c] as u32);
                lut[i + c] = ((x * (255 - t) + y * t + 127) / 255) as u8;
            }
            lut[i + 3] = 255;
        }
        lut
    }

    /// The neutral aperture the LCD3x modes sit between: an everywhere-white table, so OFF is a
    /// clean framebuffer and the two LCD3x steps are the grid at half and full strength.
    const FLAT_MASK: [[[u8; 3]; 3]; 3] = [[[255u8; 3]; 3]; 3];
    /// A horizontal scanline overlay: the top row of each 3-line cell is the dark gap, the other
    /// two are lit. Used at full strength for mode 4; mode 3 is this lerped halfway from FLAT.
    const SCANLINE_MASK: [[[u8; 3]; 3]; 3] = [[[0u8; 3]; 3], [[255u8; 3]; 3], [[255u8; 3]; 3]];
    fn new(lcd3x: [[[u8; 3]; 3]; 3], nocolor_cc: [[f32; 3]; 3], mode: u8, cc: [u16; 10], scanline: u8) -> Self {
        Self {
            lcd3x,
            nocolor_cc,
            // One stored index, one ring: the aperture preset whose position `SELECT+X` steps.
            mask_mode: mode.min(Self::MASK_STATES as u8 - 1),
            // ★ 2026-10-26 六六：扫描线档三态，手改过的卡写 `scanline 999` 也夹回 0。
            scanline_state: scanline.min(2),
            cc: [
                // GBA: the colour-grade ring.
                cc[0].min(Self::CC_COUNT - 1),
                // Game Boy: the whole palette table.
                cc[1].min(Self::GB_CC_COUNT - 1),
                // Game Boy Color: the colour-grade ring.
                cc[2].min(Self::CC_COUNT - 1),
                // Super Famicom: the colour-grade ring.
                cc[3].min(Self::CC_COUNT - 1),
                // Famicom: the colour-grade ring.
                cc[4].min(Self::CC_COUNT - 1),
                // NeoGeo: the colour-grade ring.
                cc[5].min(Self::CC_COUNT - 1),
                // Other arcade: the colour-grade ring.
                cc[6].min(Self::CC_COUNT - 1),
                // ★ 2026-10-25：MD 与 PS1 —— 都是彩色机种，都是 `GRADES` 环。
                //   两个新槽位**必须**在这里夹一遍，否则一张手改过的卡写着 `md 999`
                //   会让 `GRADES` 的索引越界（`grades_per_page` / `applied_cc` 都直接下标）。
                cc[7].min(Self::CC_COUNT - 1),
                cc[8].min(Self::CC_COUNT - 1),
                // ★ 2026-10-25：PCE，同样必须夹一遍（理由同 MD/PS1：手改过的卡
                //   写个 `pce 999` 会让 `GRADES` 下标越界）。
                cc[9].min(Self::CC_COUNT - 1),
            ],
        }
    }
    /// The aperture table for the machine in hand. The two machines with a screen bezel answer
    /// `flat` — they draw their mesh in the shader instead — and the Advance reads the table its
    /// preset picks.
    fn applied_mask(&self, flat: bool) -> [[[u8; 3]; 3]; 3] {
        // A Game Boy's lattice is not a table any more: it is drawn from the palette in the game
        // shader (see `GB_GRID_MIX`), which is what makes it follow the four shades. The aperture
        // here is therefore neutral, and the mesh and the picture are made of the same colours.
        if flat {
            return Self::FLAT_MASK;
        }
        match self.mask_mode {
            0 => Self::FLAT_MASK,
            1 => lerp_mask(&Self::FLAT_MASK, &self.lcd3x, 0.25),
            2 => lerp_mask(&Self::FLAT_MASK, &self.lcd3x, 0.50),
            3 => lerp_mask(&Self::FLAT_MASK, &self.lcd3x, 0.75),
            4 => self.lcd3x,
            5 => lerp_mask(&Self::FLAT_MASK, &Self::SCANLINE_MASK, 0.50),
            _ => Self::SCANLINE_MASK,
        }
    }
    /// Colour correction rides under the mask, and what it *is* depends on the machine.
    ///
    /// A Game Boy gets a palette: see `gb_cc`, whose black point is the reason this returns a
    /// bias at all. The colour machines keep a short list of screen grades — 0 the grayscale luma,
    /// 1 the AGB-001 look, 2 the NDS look — all anchored on black, so their bias is the zero
    /// vector and their picture is bit for bit what it was before this pair existed.
    fn applied_cc(&self, machine: System) -> ([[f32; 3]; 3], [f32; 3]) {
        let mode = self.cc[Self::machine_slot(machine)];
        if machine == System::Gb {
            // A Game Boy is drawn by its palette lookup, not by a matrix: see `gb_lut`. The
            // matrix path stays identity so nothing double-applies when the lookup is on.
            return (Self::IDENTITY, [0.0, 0.0, 0.0]);
        }
        let grade = &GRADES[(mode as usize).min(GRADES.len() - 1)];
        let m = match grade.kind {
            // 灰度 keeps the card's own `cc.txt` matrix when it ships one, so that one grade stays
            // overridable per card; every other entry reads its own numbers.
            GradeKind::Matrix(_, _) if mode == GRADE_GRAY => self.nocolor_cc,
            GradeKind::Matrix(m, _) => m,
            _ => Self::IDENTITY,
        };
        (m, [0.0, 0.0, 0.0])
    }
    /// None for every machine now: the colour grades (grayscale / AGB-001 / NDS) have no single
    /// signature colour to fill the letterbox with, and a Game Boy palette leaves it black too.
    /// The surround stays black, which is what the real panels looked like.
    fn cc_border(&self, _machine: System) -> Option<[f32; 3]> {
        None
    }
    /// Step the aperture preset, wrapping at the end of the ring.
    fn cycle_mask(&mut self) {
        self.mask_mode = (self.mask_mode + 1) % Self::MASK_STATES as u8;
    }

    /// SELECT+X on a console that has no bezel art of its own (MD / NeoGeo / Arcade / PS1 / PCE):
    /// step the console scanlines 无 → 50% → 100% and back. Three states, a separate field from
    /// `mask_mode` (the Advance's aperture) and `overlay_index` (the art ring), so each machine's
    /// own dial survives a machine switch (see `scanline_state`).
    fn cycle_scanline(&mut self) {
        self.scanline_state = (self.scanline_state + 1) % 3;
    }
}

/// Linearly blend two 3x3 aperture tables, element by element, at `t` in [0,1]. Used to soften
/// the LCD3x grid to a 50% strength for the middle mask step.
fn lerp_mask(a: &[[[u8; 3]; 3]; 3], b: &[[[u8; 3]; 3]; 3], t: f32) -> [[[u8; 3]; 3]; 3] {
    let mut out = [[[0u8; 3]; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            for k in 0..3 {
                let av = a[i][j][k] as f32;
                let bv = b[i][j][k] as f32;
                out[i][j][k] = (av + (bv - av) * t).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// One cheat code for the seated cart, plus whether it is live. `enabled` is per-code; the
/// master `cheats_on` in `App` gates the lot — a code switched on but under a master that is
/// off does nothing, exactly as a global toggle over a picker of switches should.
pub(crate) struct CheatItem {
    pub(crate) code: String,
    pub(crate) desc: String,
    pub(crate) enabled: bool,
}

/// Inset of the shelf's star indicator from the corner of the case. Clear of the cart bay,
/// which owns the true bottom edge of the screen.
///
/// How far in from the panel's left edge the disc row's badge sits. Flush was too loud for a
/// mark this small: it read as a cut-off rather than as a badge.
const SHELF_IND_LEFT: f32 = 20.0;
/// How tall the bottom-right favourites indicator is drawn. Enlarged on request, to match the
/// mark a favourite cart wears.
const FAV_IND_DRAW: f32 = 36.0;
/// The top band's own margin: the distance the clock and the battery reading are printed in from
/// their edges. Repeated here so the two bottom corner marks can stand in the same columns as the
/// two readings above them.
const STATUS_READ_MARGIN: f32 = 24.0;

/// ★ 2026-10-22（六六）：UI 提示音的去抖窗口。长按连发的 repeat 间隔是 110~130ms，
/// 50ms 只吃掉**同一帧附近的重复**（一次点击被两条路径读到），不会吃掉真的连续两次移动。
const UI_SFX_DEBOUNCE_MS: Millis = 50;

/// The shell that brings the card's own WiFi up and starts SSH, on demand rather than at
/// boot: the radio is a real drain on a standby budget, so it is the user who asks for it.
/// `System/slotnet.sh` raises `wlan0`, joins from `System/wifi.conf`, takes a DHCP address
/// and starts dropbear unless the base system already runs one; the address is left in
/// `/tmp/slot-ip` for the dialog to show, since DHCP is what decides it. Dock-agnostic by
/// design — the same card, the same file, on AGS-102 and on BaseOS alike.
const DIRECT_ENTER: &str = "sync; export PATH=/usr/sbin:/usr/bin:/sbin:/bin:$PATH; \
/bin/sh \"${SLOT_ROOT:-/mnt/sdcard}/System/slotnet.sh\" up; sync";

/// The shell that takes the radio back down. Only what this card's own script started is
/// killed — a base system running its own SSH keeps it — and the card is never touched:
/// the whole point of doing this over the network.
const DIRECT_EXIT: &str = "sync; export PATH=/usr/sbin:/usr/bin:/sbin:/bin:$PATH; \
/bin/sh \"${SLOT_ROOT:-/mnt/sdcard}/System/slotnet.sh\" down; sync";

/// A system-shelf swap in flight: which shelf is arriving, how far through the shutter's beats
/// it is, and whether the view has been swapped yet. While `App::shelf_switch` holds one,
/// the row stays exactly where it is — the shutter covers the swap (see `slot_ui::shutter`),
/// so the carts no longer move for it.
///
/// ★★ 2026-10-26（六六）：**满闸之后停住，等用户指一台。**
///
/// 原来 `t` 一路上涨走完 P1→P4，"满闸"只是 P3 那固定 0.40 秒（`HOLD_S`），换装在那儿偷偷
/// 发生。现在那 0.40 秒改成**一段无限长的等待**：帘子落满就冻在 `t = SWAP_AT_S`，机种
/// 十格浮在帘子上；用户按 A 或 B 之后才开始回卷（`up`）。所以这条时间轴有两截，
/// 中间那一截不是时间，是人。
struct ShelfSwitch {
    /// The system whose shelf is leaving. Held for the length of the swap so the corner logo can
    /// cross-dissolve from it rather than cut.
    from: System,
    /// 帘子卷起时要露出谁。**在满闸、用户按下 A/B 的那一刻才定** —— 一路上它都是 `from`
    /// （"不换"），所以无论怎么打断，卷起来的都不会是半台别的机器。
    to: System,
    /// 落下段：0 → `SHUTTER_SWAP_AT_S`，到那儿就冻结。
    t: f32,
    /// 回卷段：0 → `SHUTTER_UP_S`。`None` = 还在落下，或者满闸等着用户指。
    up: Option<f32>,
}

/// One uploaded menu button: the face it wears at rest and the one it wears in hand.
///
/// Two textures rather than one plus a highlight bar behind it — which is what the power menu
/// does and what every other list on this device does — because these buttons are rounded, and
/// a bar behind a rounded button is a bar with square corners sticking out of it.
#[derive(Copy, Clone)]
pub struct MenuButton {
    pub idle: (TexId, u32, u32),
    pub chosen: (TexId, u32, u32),
}

/// Which shelf row is up: the games (cartridges) or the discs (music).
///
/// **Every reader goes through `App::row` / `App::row_mut`**, which is the whole trick: the
/// keyboard, the drawing and the frontend all ask "the row" rather than reaching for `shelf`,
/// so adding a row is one assignment rather than a search for every caller that forgot.
///
/// ★ 2026-10-21：书行没了（六六「删除像素阅读器暂时归档于工作区」）—— 剩下的两行是
///   游戏货架与碟片货架，仍然是同一份 `Shelf` 按各自尺寸铺出来。
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ShelfRow {
    Games,
    Discs,
}

pub struct App {
    phase: Phase,
    shelf: Shelf,
    /// The card's music, on a row that is **the same `Shelf`** laid out at a disc's size.
    ///
    /// A second row rather than a second `System`: `System` answers "which machine", and
    /// everything downstream of it assumes the answer implies a core, a save folder and a
    /// `States/` path. A disc has none of those. `Shelf::set_size` exists for exactly this case
    /// — its own comment says a shelf of something that is not a cartridge is a shelf with a
    /// different size and nothing else — so the ring, the spring, the glide and the favourites
    /// view all carry over untouched, and the two rows are the same code.
    disc_shelf: Shelf,
    /// Which row is up. An enum, and the rows are peers — neither is "the main one".
    row: ShelfRow,
    /// **←/→ 快退快进的连跳**：`dir` 是按住的方向（−1/0/+1），`at` 是下一次跳的时刻。
    /// 见 `deck_seek_hold`。
    deck_seek_dir: f32,
    deck_seek_at: Millis,
    /// 信息页那一整帧（**720×480，与面板同一张 1-bit 帧合成出来**）。
    ///
    /// ★ 2026-10-06 重做：它不再是"每行一张 `word_face` 纹理"（那是货架/菜单那一套渲染，
    ///   抗锯齿矢量字 + 没有格网），而是**面板同一条路的一张整帧** —— 与 `lcd_panel`
    ///   同一个 `PANEL_W × PANEL_H`、同一套四阶色板、同一张格网。见 `sync_info_panel`。
    info_panel: Option<(TexId, u32, u32)>,
    /// 信息页现在在第几页。**0 = 碟片与曲目，1 = 键位指南**（`lcd::INFO_PAGES` = 2）。
    info_page: usize,
    /// 信息页被呼出时它压住的那块屏，存成 `Listening` 的三个字段 `(key, playing, disc)`。
    ///
    /// **存元组而不是 `Phase`**：`Phase` 不派生 `Clone`/`Copy`（它带着 `ClockPicker` 这类
    /// 持有型字段，派生出来就等于承诺整个前端都能复制），而这里要的只是"回去时那三个数"。
    /// `None` = 兜底回货架。真要信息页从别的屏呼出，得把这一格换成 `Option<Phase>` 加 `Clone`。
    info_back: Option<(usize, bool, usize)>,
    /// The letter strip over the shelf: where the ring stands, and how far it still has to
    /// travel.
    letters: Letters,
    /// How many carts sit under each of the ring's slots, in `letters::SLOTS` order. Built
    /// once from the scan and never again — the card does not change while slot is running.
    /// The strip needs it to know which letters are worth stopping on, and which are drawn dim.
    letter_counts: [usize; letters::N],
    /// One small face per slot, in that order with the size each was rasterised at. Uploaded at
    /// boot: 27 letters of a fixed alphabet is less work than one cart face.
    ///
    /// The band the letters stand in is not here at all: it is the machine's own, and
    /// `slot_chrome::draw_top_band` draws it from the cart bay's numbers every frame.
    letter_faces: Vec<(TexId, u32, u32)>,
    /// The one piece of metal the strip repeats between two letters, with its own size.
    /// Uploaded beside the letters because it is the same kind of thing: fixed, tiny, and
    /// needed on the shelf from the first frame.
    letter_ridge_face: Option<(TexId, u32, u32)>,
    /// ★ 2026-10-24（六六）：字母轮下面那块**胶囊背板**。
    letter_pill_face: Option<(TexId, u32, u32)>,
    /// When A went down on the shelf, and `None` the rest of the time. The hold lives here
    /// rather than in the gesture layer because A is the GBA's A button everywhere else, and
    /// `Gestures` is deliberately blind to which screen is up.
    play_held: Option<Millis>,
    /// When X went down on the shelf, and `None` the rest of the time — the same shape as
    /// `play_held`, and for the same reason. X carries two shelf gestures, a short press that
    /// stars the cart and a hold that swaps the shelf, and which one a press is cannot be known
    /// until it either comes up or passes `PLAY_HOLD_MS`.
    fav_held: Option<Millis>,
    /// When Y went down on the shelf: a hold cycles which machine's shelf is up. Same shape as
    /// `fav_held`, and spent the same way.
    sys_held: Option<Millis>,
    /// When B went down on the shelf: a hold opens the direct-connect dialog. A tap does nothing.
    mtp_held: Option<Millis>,
    /// The mark a favourite cart wears, with the size it was rasterised at. One colour whatever
    /// shelf is up. Its own upload rather than one of the HUD's, because it is the one glyph on the
    /// shelf that is a colour of its own instead of the case's ink.
    fav_star: Option<(TexId, u32, u32)>,
    /// The shelf's indicator, unlit and lit: the same star hollow while the ordinary shelf is
    /// up and solid while the favourites are.
    fav_ind: Option<(Upload, Upload)>,
    /// The emoji atlas the `badge` column indexes, with the size it was uploaded at. `None`
    /// when the card carries no atlas, or when no row on it asks for a badge — a few
    /// megabytes of RGBA is not worth decoding for a rule nobody wrote.
    badge_atlas: Option<(TexId, u32, u32)>,
    /// The last refused action, and the only thing that tells an eject apart from a cart
    /// that would not seat: both leave down the same path.
    refusal: Option<Refusal>,
    /// The `t` a refused cart's exit started from, which is also how long there is left to
    /// say so. `None` for an eject the user asked for: nothing was refused.
    refused_from: Option<f32>,
    alert_face: Option<TexId>,
    /// The game under the eye on the shelf, rasterised by the frontend whenever the selection
    /// moves — one line under the row naming what the row is showing. The cart carries a
    /// printed label of its own, but that label is a 240 px cart's: at the distance a handheld
    /// is held it says which of three carts is which, not which game this is, and a long title
    /// on it is three small lines rather than a name.
    shelf_title_face: Option<(TexId, u32, u32)>,
    /// ★ 2026-10-24（六六）：长名字不缩号，改成**循环滚动**。
    /// 这个时钟是那条滚动的起点：换卡就重置到字头。
    title_scroll: Option<std::time::Instant>,
    /// ★ 2026-10-21：**发光件**，开机烘一次、之后只是一次 `Draw::Tex`。
    ///
    /// 三张：`power` 是面板顶边那条通电渐隐线（游戏菜单用），`glow` 是货架标题的对称背光，
    /// `dot` 是一颗点光源。放在 App 而不是 `Frontend`，是因为画它们在 `draw_row` 之后、
    /// 属于货架自己的图层；`Frontend` 只负责烘和传（见 `frontend::upload_fixed`）。
    light_faces: LightFaces,
    /// What the shutdown says, one per `PowerChoice::ALL` in that order and rastered at the
    /// menu's own size. "Powering down" under a restart was the screen contradicting the row
    /// the user had just chosen.
    shutdown_faces: Vec<(TexId, u32, u32)>,
    /// Open when a held POWER raised the menu, holding the highlighted row. An overlay
    /// rather than a phase, so cancelling returns to whatever was underneath without the
    /// phase having to be remembered anywhere.
    power_menu: Option<usize>,
    /// One per `PowerChoice::ALL`, in that order, with the size each was rastered at.
    power_menu_faces: Vec<(TexId, u32, u32)>,
    /// ★ 2026-10-29（六六）：电源菜单的背景图，整屏 720×480。恒定的霓虹隧道图，不随主题/
    /// 明暗 —— `None` 时回退主题底（图还没上传，或图不是本面板尺寸）。
    power_bg_face: Option<TexId>,
    /// The direct-connect (USB mass-storage) dialog: which stage it is showing, or `None` when it
    /// is not up. `0` is the confirm prompt, `1` the "connect the cable" beat, `2` the live mode.
    direct_link: Option<u8>,
    /// The dialog's lines, baked once at boot: [confirm title, confirm keys, connecting, active
    /// title, active hint].
    direct_faces: Vec<(TexId, u32, u32)>,
    /// When the "connect the cable" beat went up, so it can hand over to the live screen on its own.
    direct_at: Millis,
    /// The address the WiFi link came up on, once the connect script has left one.
    direct_ip: Option<String>,
    /// The baked face for that address, and whether the address has changed since it was baked.
    direct_ip_face: Option<(TexId, u32, u32)>,
    direct_ip_dirty: bool,
    /// ★ 2026-10-21：**Wi-Fi 配置浮层**（设置屏「无线网络」行那道门）。`None` 是关着。
    /// 扫描在worker线程里跑（`slotnet.sh scan` 要几秒），结果从 `wifi_rx` 收回来。
    wifi: Option<WifiEdit>,
    /// 卡上此刻的凭证（开机读一次，保存/删除时更新）。设置屏那一行的两个词读它。
    wifi_conf: Option<slot_store::WifiConf>,
    /// 扫描线程的回信信箱。`Some` 而 `wifi.scanning` 为真 = 扫描还在外面跑。
    wifi_rx: Option<std::sync::mpsc::Receiver<Vec<WifiNet>>>,
    /// 字符网格的字形（开机烤好，永不变）：网格里每个字符一张脸，输进去的串也拿它画。
    /// PSK 的掩码圆点也在里面（`'•'`）。
    wifi_glyphs: Option<Vec<(char, TexId, u32, u32)>>,
    /// 这一屏的文字脸：标题、标签、动作、提示线（都是定句），加当前 SSID 和扫描结果
    /// （这两样会变，变了就置脏让前端重烤）。
    wifi_faces: Option<WifiFaces>,
    /// 上面那些会变的脸要不要重烤。
    wifi_dirty: bool,
    /// Search overlay; the application phase remains Shelf while it is open.
    search: Option<SearchState>,
    /// View active before the first search in a filter chain.
    search_origin: Option<(Vec<usize>, Option<String>)>,
    /// Four fixed labels drawn by the search overlay.
    search_faces: Option<SearchFaces>,
    /// The picker while the cart is open, and while its lid is going back on. The cart it acts
    /// on is whichever the shelf has, read when it opens rather than held here: the shelf cannot
    /// move while it is up, so there is only ever one answer.
    core_picker: Option<CorePicker>,
    /// ★ 2026-10-26（六六）：长按 Y 打开的**机种选择面板**。开着的时候方向键与 A/B 都是
    /// 它的（`sys_picker_input`），货架那一行一步都不许动 —— 否则按 A 会插一张不是用户
    /// 指的那台的卡。
    sys_picker: Option<SysPicker>,
    /// 十台机种的标识贴图，按 `root::MACHINES` 的序。每台**两份**：卡上那张白图给深色主题，
    /// 同一个形状染成墨色给亮色主题（见 `slot_ui::sys_picker_ink_icon`）。卡上没有哪一张
    /// 就是 `None`：格子还在，只是画成一块空的圆角板 —— **十台是机器的事实，不是这张卡的目录**。
    ///
    /// 烘在 `upload_late_furniture`（开机后第一帧），不占那 4.4 秒里的任何一格。
    sys_icons: Vec<Option<(Upload, Upload)>>,
    /// 那一块屏的家具：两张按钮底、一圈光、标题、脚下那一排键位。
    sys_faces: Option<slot_ui::SysPickerFaces>,
    /// The open cart under the highlight, and its lid: the shelf face with a transparent border
    /// so it can be turned. Rebuilt by the frontend when the highlighted cart changes.
    core_board_face: Option<TexId>,
    core_lid_face: Option<TexId>,
    /// The cart the uploaded board and lid were built for.
    core_faces_stem: Option<String>,
    /// In `Core::ALL` order: each socket empty, and the chip seated and named in each. Uploaded
    /// at boot, since none of them ever changes.
    core_socket_faces: Vec<TexId>,
    core_chip_faces: Vec<TexId>,
    /// The chip in flight, blank, and the shadow under it.
    core_blank_chip_face: Option<TexId>,
    core_chip_shadow_face: Option<TexId>,
    /// `B` Cancel, the two arrows, `A` Choose, each with the width it was rastered at, laid out
    /// by role in that order: Cancel under the cart's left edge, Swap on the panel's centre,
    /// Choose under its right edge.
    core_legend_faces: Vec<(TexId, u32)>,
    /// Open when SELECT+MENU raised the in-game menu over a running game. An overlay rather
    /// than a phase, and for a stronger reason than the power menu's: `Phase::Playing` is
    /// what holds the seated cart, and a menu that left it would have to rebuild the session
    /// to come back from cancelling.
    game_menu: Option<GameMenu>,
    /// One per `GameRow::ALL`, in that order. Every row is uploaded whether or not this
    /// cart shows it — the faces never change, and which rows exist does.
    ///
    /// ★★ 2026-10-21：**两套** —— 一格是一枚圆角按钮（`slot_ui::game_row_face`），手里那一格
    /// 是亮底深字（和电源菜单选中行同一套），其余是机壳色底。两套一次烘好，选中只换一张面，
    /// 不在每帧叠半透明层 —— 叠层在圆角上会露边。
    game_menu_faces: Vec<(TexId, u32, u32)>,
    /// The same rows, one per `GameRow::ALL`, in the "in hand" state.
    game_menu_hot: Vec<(TexId, u32, u32)>,
    /// ★ 2026-10-21：菜单标题行 —— 游戏名（`selected_stem` 的清洗版），前端烘好交进来。
    /// `None` 在没有卡或者还没烘出来的时候；跟着 stem 走，换一张卡重烘一次。
    game_title_face: Option<(TexId, u32, u32)>,
    /// ★ 2026-10-21：`game_title_face` 的归属 —— 哪个 stem 烘出来的（缓存键）。
    /// 和 `cheat_face_stem` 同一款先例：`selected_stem` 变了前端就重烘，没变零成本。
    game_title_built_for: Option<String>,
    /// ★ 2026-10-26：PS1 光盘封面贴图 —— 从 `Cart.label`（游戏封面 PNG）加载后
    ///   上传 GPU。`None` 在没有 PS1 卡或贴图还没加载出来的时候。
    disc_face: Option<(TexId, u32, u32)>,
    /// ★ 2026-10-26：`disc_face` 的归属 —— 哪个 stem 烘出来的（缓存键）。
    disc_face_built_for: Option<String>,
    /// ★ 2026-10-26：**进游戏时那颗还在转的光盘**。`Some` 只在「核心刚就绪、`screen` 正在
    ///   从 0 长到 1」的这段里，也就是 CRT 正在开机、画面还没铺满的时候。见 [`DiscLive`]。
    disc_live: Option<DiscLive>,
    /// ★ 2026-10-27（六六）：ARCADE / NEOGEO 的投币素材，按机种各一套（见 [`Coin`]）。
    ///   开机在 `frontend::rebake` 里烘好交进来 —— 投币动画每帧都要用，不能在按下 A 时才去读卡。
    coins: Vec<(slot_store::System, Coin)>,
    /// ★ 2026-10-21：**保存设置弹窗**的三族面 —— 标题、四个键帽、四行字，前端在弹窗
    ///   开着的那一帧上传（`sync_save_settings`），关了放走。`None` 在没开的时候。
    save_settings_faces: Option<(Upload, Vec<Upload>, Vec<Upload>)>,
    /// ★ 2026-10-20：槽位表那一行字（"03 · 10-20 22:31"）和那张快照的贴图。二者都跟着
    ///   `slot_index()` 走：换一个槽位，前端就重烘一次字、重传一次图。
    slot_caption_face: Option<(TexId, u32, u32)>,
    slot_thumb_face: Option<(TexId, u32, u32)>,
    /// One per `LinkRow::ALL`, in that order.
    link_menu_faces: Vec<(TexId, u32, u32)>,
    /// One per `LinkStep::ALL`, and one per `LinkFail::SHOWN`, in those orders. A sentence
    /// each rather than a list, so nothing is ever in hand on either.
    link_step_faces: Vec<(TexId, u32, u32)>,
    link_fail_faces: Vec<(TexId, u32, u32)>,
    /// The worker behind `GameMenu::Working`, and `None` the rest of the time. It has no
    /// `Drop` of its own, so `close_game_menu` is what stops it: see there.
    starting: Option<LinkStarting>,
    /// The wire a finished starter handed over, waiting for whoever owns the emulator thread
    /// to collect it. `App` holds a session's own bookkeeping and never a transport (see
    /// `link`), and this is the one hop between the two — `Session::update` drains it into
    /// `EmuHandle::begin_link`, mirroring the hop `Session::bridge_link` already makes for
    /// an ending.
    link_transport: Option<(u16, Box<dyn LinkChannel>)>,
    /// Set when the menu's Restart is chosen. The binary acts on it, like `powering_off`.
    restarting: bool,
    /// ★★ 2026-10-21 傍晚改：**重开本局 = 核心自己的复位**（`retro_reset`），不是拔卡重插。
    ///
    /// 先前那条路（`reinsert`）是把卡拔出来再插回去，走原来那条 insert 路径；可那条路径
    /// **会先接上拔卡那一刻的即时存档**，于是"重开"看上去就是原地不动 —— 六六实测
    /// 「插拔后又回到拔卡时的即时存档」说的正是这个。
    ///
    /// 现在这里只是一个**请求位**：`restart_game` 置上，`Session::update` 每帧取走并交给
    /// `EmuHandle::reset`（核心在自己的线程上跑 `retro_reset`，同时清掉倒带历史）。相位
    /// 一帧都不动 —— 停机动画、插拔动画都没有，画面就是从开机那一刻重新出来，这是"在游戏里
    /// 重启"该有的样子。
    restart_core: bool,
    /// When the binary is allowed to act. The screen is drawn from the instant the choice is
    /// made, but the shutdown itself waits a few frames so the ordinary loop has drawn and
    /// presented it. Rendering out of band instead — one extra draw and swap between the
    /// choice and `poweroff` — hung the device: the swap can block on a GPU about to be torn
    /// down, and slot then never reached `poweroff` at all, leaving a machine that needed
    /// the PMIC held to recover.
    act_at: Millis,
    /// `None` outside the binary, where there is no content root and nothing persists.
    root: Option<PathBuf>,
    state: SlotState,
    /// The volume and the silence as they stood before each of the last two volume presses,
    /// oldest first. The mute chord is delivered behind the two presses that make it, so
    /// toggling has to give back what they already moved.
    vol_before: Vec<(u8, bool, Millis)>,
    /// `None` until a cart is in the slot. There is nothing to flush without a core.
    snapshot: Option<Box<dyn Snapshot>>,
    /// The seated cart's `Core`, resolved once by whoever spawned `snapshot` and handed here
    /// through `set_core` rather than re-read. `ring`, `flush_resume` and the eject path all
    /// take this instead of calling `core_for` themselves, which is what makes it structurally
    /// impossible for a later read or write to disagree with the dylib actually running: there
    /// is nowhere left in this file to derive a second opinion from. Stale between carts in
    /// exactly the way `snapshot` is — both are set together and neither is cleared on eject —
    /// which is safe because every reader of either is gated on a cart actually being seated.
    core: Core,
    /// `Some` for as long as a netpacket session is live. `App` never touches the transport
    /// or the core itself — those live on the emulator thread, wherever `EmuHandle::begin_link`
    /// was called from the same gesture this answers — this is only what the interlocks below
    /// need: that one is live at all, and which side of it this device is.
    link: Option<LinkSession>,
    /// What the slot itself is about to sound like, drained by whoever owns the device. One
    /// slot: two of these in a frame is not a movement the cart can make.
    sfx: Option<Sfx>,
    /// ★ 2026-10-22（六六）：上一次 UI 提示音的时刻，用于 **~50ms 去抖** —— 长按连发时
    /// ←→/字母轮每帧都动，不该是一串机枪。插/拔两条动作音不走这里（它们是精确卡在
    /// 触点上的，见 `Phase::Inserting` 的 `at`）。
    ui_sfx_at: Option<Millis>,
    /// `Some` exactly while the switcher is showing. It holds the ring as it was when it
    /// opened, so a save behind it cannot renumber what the user is looking at.
    polaroids: Option<Polaroids>,
    /// The one undoable action and the moment it happened. Belongs to the cart in the slot,
    /// so it leaves with it.
    pending: Option<(PendingUndo, Millis)>,
    /// The key caps on the switcher's bottom plate. The three fixed ones never change what
    /// they say and are uploaded once; the undo says which action it will take back, so it is
    /// rasterised on the way into the switcher. All of them outlive any one opening.
    legend_faces: Vec<TexId>,
    undo_face: Option<TexId>,
    /// The clock screen's line of type and its one instruction. Rasterised by the binary,
    /// and gone for the rest of the session once the clock is confirmed.
    clock_faces: Option<(TexId, TexId)>,
    /// The label, rasterised whole. Re-uploaded when the gauge moves.
    sticker_face: Option<TexId>,
    /// The menu's buttons, one entry per slot of `menu::Item::GRID`: the face at rest and the
    /// face in hand for a slot that has a screen, and `None` for one that does not. Two per
    /// button rather than one plus a bar behind it, because a bar cannot be rounded.
    menu_buttons: [Option<MenuButton>; slot_ui::menu::SLOTS],
    /// The recess an empty slot is drawn as, once for the whole grid.
    menu_empty: Option<(TexId, u32, u32)>,
    /// The menu's own foot line. Its own rather than the card's: the two screens say different
    /// things and only one of them scrolls.
    menu_hint: Option<(TexId, u32, u32)>,
    /// The settings screen's rows, one entry per `settings::Row::ALL` and per value that row can
    /// take: the face at rest and the face in hand.
    ///
    /// **Every value, baked at boot**, not the current one: the value changes under a keypress
    /// and a row is a 660-wide rounded plate, which is a per-pixel pass with no business running
    /// while the user is holding a direction. Four lid values and one "open" is five pairs.
    settings_rows: Vec<Vec<[(TexId, u32, u32); 2]>>,
    /// The foot lines, **one per `settings::Row::hint_index`** — "change the value", "open a
    /// screen", "do it now", "browse, then A" — uploaded in that order and picked between by
    /// that index. Three since the refresh row arrived (2026-10-17); two before that, and the
    /// second is still the clock's. **Four since the boot row arrived** (2026-10-20), which is
    /// the one row on this screen whose keys are not the screen's usual keys.
    settings_hint: [Option<(TexId, u32, u32)>; 4],
    /// The deck's faces: the disc, the name of what is on it, and the plate every transport key
    /// is drawn on. Uploaded at boot with the rest of the fixed furniture — a disc is 480 by 304
    /// and a session that never opens the deck should not have paid for it.
    deck_face: Option<(TexId, u32, u32)>,
    deck_name: Option<(TexId, u32, u32)>,
    deck_key: Option<(TexId, u32, u32)>,
    /// Which disc of the card's is in the deck, as `3/12`. `None` until the deck has more than
    /// one disc to count between, which is also the only time it is drawn.
    deck_count: Option<(TexId, u32, u32)>,
    /// Which track of that disc is on the deck, as `2/9`. Its own slot beside the disc counter
    /// rather than a second line under it: the two numbers answer different questions — which
    /// cassette, and where in it — and a reader who has to subtract one from the other to tell
    /// them apart is doing arithmetic on a panel.
    ///
    /// `None` for a disc of one track, which is the same rule the disc counter follows.
    deck_track_count: Option<(TexId, u32, u32)>,
    /// The line of lyrics under the deck, as the track's own `.lrc` has it. `None` for the
    /// majority of cards, and a deck without a line is a normal deck rather than a broken one.
    deck_lyric_face: Option<(TexId, u32, u32)>,
    /// The retro-LCD panel, as one baked texture: the logical frame masked and blown up ×3.
    ///
    /// **A texture and not a draw list, and that is the whole reason this panel can exist on
    /// this device.** A 240×120 frame in `Draw::Rect` would be 28 800 rects a frame, which is
    /// more than the shelf puts up in total; baked once and drawn as one quad it costs the same
    /// as any other face. The price is that it is re-baked whenever anything in it changes —
    /// the panel, the ramp, the track, the lyric line, or the spectrum — so `frontend` keys the
    /// bake on all of them and a still panel mints nothing.
    lcd_panel: Option<(TexId, u32, u32)>,
    /// The discs the card holds, in shelf order. Scanned once, the first time the deck is opened
    /// rather than at boot: a card with no `Music/` must not pay for a directory walk it will
    /// never use, and the walk opens every file to read its length.
    ///
    /// Held here rather than in the deck phase because the disc in the deck is an index into
    /// this, and an index into nothing is a bug waiting for the next screen to need it.
    discs: Vec<slot_media::Disc>,
    /// The disc in the deck, as an index into `discs`. Meaningless while `discs` is empty, and
    /// the deck says so rather than showing a blank one.
    disc: usize,
    /// Which track of that disc is on the deck, as an index into `discs[disc].tracks`.
    ///
    /// **Separate from `disc` because the transport walks within a disc and L/R walks between
    /// them.** A deck's rewind and fast-forward are inside the cassette; changing the cassette is
    /// the hand reaching past the transport. The frame needs both numbers and cannot tell one
    /// from the other.
    track: usize,
    /// The deck's playback thread, and the state it shares with the frame.
    ///
    /// **`Player` is owned here rather than by the phase** because the thread must outlive a
    /// visit: leaving the deck pauses rather than kills, so coming back resumes the same track
    /// rather than decoding it again. `Player::Drop` stops the thread when the `App` goes, so
    /// nothing is left running at quit.
    player: crate::music::Player,
    /// The same `Deck` the player writes, kept for the frame to read: elapsed time, the tap the
    /// visualiser draws from, and whether the transport is really running.
    ///
    /// Held beside the player rather than reached through it because the frame reads it on every
    /// frame and `&self` is all it needs — the player is only ever asked to change something.
    deck: std::sync::Arc<crate::music::Deck>,
    /// The current track's lyrics, when the card has an `.lrc` under `Music/Lyrics/` for it.
    /// `None` is the answer for most cards and is not a failure: a disc with no lyrics still
    /// plays and still draws its spectrum.
    ///
    /// **Re-read on every track change**, because the file is named after the track — and looked
    /// up through the index rather than by a guessed path, because the music and the lyrics on a
    /// real card are not always spelled the same way (see `lrc::Index`).
    lines: Option<Vec<slot_media::lrc::Line>>,
    /// The card's lyric files, found once when the discs were walked.
    ///
    /// **Beside the discs rather than inside the walk** because the two are separate questions
    /// with separate costs: the disc walk opens every audio file to read its length, and this one
    /// only reads directory names. An empty index is the ordinary case and costs one `read_dir`
    /// that fails.
    lyrics: slot_media::lrc::Index,
    /// The card's album art, from `Music/Covers/`. Same walk, same moment, same reasons.
    covers: slot_media::Covers,
    /// Which of the deck's two panels is up.
    ///
    /// **Held here rather than in the phase** for the same reason `player` is: the panel is a
    /// property of the deck, not of a visit to it. Coming back to the deck shows the panel that
    /// was up, and a deck that forgot which of its two faces you were looking at is a deck you
    /// have to set up again every time you check something.
    panel: Panel,
    /// Which of the LCD panel's colour ramps is up, as an index into `palettes::deck_ramps()`
    /// （= `pixelshift()` 的全量，四阶齐全的那 40 套；六六 2026-10-06「色板全量推上来」）。
    ///
    /// **An index and not a colour**, because the ramp is the thing the user is choosing and the
    /// four numbers that come out of it are a rendering of that choice. A card's own ramp would
    /// be a different source rather than a different index.
    ramp: usize,
    /// What the transport is doing, and how the list repeats.
    ///
    /// **Three transport states and not two**, because a deck that can only play and pause cannot
    /// stop, and stopping is what you want the moment a shuffled track ends. The play key's glyph
    /// is a function of this, which is the kit's own rule: *the box never changes, only whether
    /// the thing inside it is solid*.
    transport: slot_ui::lcd::Transport,
    /// One song, the whole list, or the list shuffled. **Three stops on one key**, which the
    /// kit's ON/OFF switch becomes when it has three positions.
    mode: slot_ui::lcd::Mode,
    /// The equaliser preset, printed at the right end of the artist's row. **One of 18** — the
    /// Winamp set VLC ships (see `slot_ui::eq`). `NULL` is the default and the honest "nothing
    /// applied"; cycling it pushes the new preset down to the deck's audio thread.
    eq: slot_ui::eq::Eq,
    /// When the MODE key was last pressed, in milliseconds. **A second press within
    /// [`EQ_DOUBLE_TAP_MS`] cycles the EQ instead of the play mode** — one key, two settings,
    /// and the tap-vs-double-tap split is the retro way to do it without a second button.
    last_mode_tap_ms: Millis,
    /// Which of the five keys the caret is on.
    ///
    /// **Kept beside `phase` and not inside it** for the same reason the panel is: the bar is the
    /// deck's, and a deck that forgot which key you were holding when you left it and came back
    /// is a deck you have to re-aim every time.
    key_focus: usize,
    /// The card's music has not been walked yet. Cleared by the walk itself.
    ///
    /// **A flag rather than a scan at boot** because the walk opens every file under `Music/`
    /// to read its length: a card with no `Music/` must not pay for it, and most cards have no
    /// `Music/`.
    ///
    /// It is deliberately "not walked yet" and **not** "someone asked for it". The deck can be
    /// entered two ways — the menu's button, and `open.txt` naming it for a debug boot — and a
    /// flag that only the first of those raises leaves the second showing an empty deck. One
    /// flag, true until the walk happens, is the only version of this that cannot be wrong
    /// about how the deck was reached.
    discs_dirty: bool,
    /// The shortcut card's rows, in the order they are written and each with the height it was
    /// rasterised at. Uploaded at boot with the rest of the fixed furniture. The heights are as
    /// much the point as the textures: the card is exactly as tall as its rows make it.
    shortcut_rows: Vec<(TexId, u32, u32)>,
    /// The one line of the card that does not move.
    shortcut_hint: Option<(TexId, u32, u32)>,
    /// One picture from `Wallpapers`, behind everything the shelf draws. `None` on a card
    /// that carries none, which is the common case.
    wallpaper: Option<TexId>,
    /// ★ 2026-10-28（六六）：「默认壁纸……洋红区加呼吸光效，跟主题色板走」。
    ///
    /// 只在内置默认壁纸（`slot_ui::default_wallpaper`）铺着时有值 —— 卡上自己的图不发光：
    /// 那是用户的照片，没有"洋红区"这回事。纹理是开机（和每次换主题）按主题亮色烘的一整张
    /// 柔光，每帧只多画一个 quad，`alpha` 走 [`App::breath_alpha`] 那条呼吸曲线。
    wall_glow: Option<(TexId, u32, u32)>,
    /// ★ 2026-10-23（六六）：货架页桌面图，替代程序化顶条/底条。
    ///   放在壁纸之上、卡带之下。中心透明区让卡带和标题透出来，四边灰框/黑条盖住壁纸。
    desktop: Option<TexId>,
    /// What is printed on the case: the battery's percent, and the time as it stands.
    battery_percent: slot_ui::Printed,
    /// The charging glyph, uploaded once at boot with the other icons rather than whenever
    /// the percent changes: unlike the percent, its face never varies.
    bolt: Option<TexId>,
    shelf_clock: slot_ui::Printed,
    hud: Hud,
    /// Whether the cart's cheat list (from `System/Cheats/<stem>.cht`) is currently applied to
    /// the core. Toggled by SELECT+A in game; persists across eject/reinsert within a session.
    pub(crate) cheats_on: bool,
    /// The seated cart's cheat codes, each with its own on/off. Loaded from
    /// `System/Cheats/<stem>.cht` when its core spawns; `set_cheats` rebuilds it and shuts the
    /// table. Browsable and switchable one by one in the cheat table (SELECT+A).
    cheats: Vec<CheatItem>,
    /// `Some` while the cheat table overlay is up, holding the row in hand. `None` otherwise.
    cheat_menu: Option<usize>,
    /// One rasterised row face per cheat, in cheat order, (re)built by the frontend when the
    /// seated cart changes. The ON/OFF state is drawn live beside each, so these never change
    /// on a toggle.
    cheat_faces: Vec<(TexId, u32, u32)>,
    /// The stem whose faces `cheat_faces` currently holds, so the frontend re-rasterises only
    /// when the cart under the highlight actually changes.
    cheat_face_stem: Option<String>,
    /// The "no cheats for this cart" panel, rasterised once at boot and shown when the table
    /// opens on a cart that carries none.
    cheat_empty_face: Option<(TexId, u32, u32)>,
    /// Set when a row is toggled inside the table, so `Session` knows to re-push the list to
    /// the core after `apply` returns (the table itself cannot reach the emulator).
    cheats_dirty: bool,
    /// Set when the light/dark mode has moved under the binary's feet. Everything on this side
    /// of the palette — the case, the scrim, the gauge, the bar — follows the mode by itself,
    /// because it is drawn as filled quads. Everything *baked* does not, and this flag is how
    /// the app says so: `Frontend` takes it and re-rasterises.
    mode_dirty: bool,
    /// A machine shelf has been swapped in, so the row's geometry has moved under the cart art,
    /// the shadow and the stand-in — all three were minted for the machine the device came up
    /// on. The frontend takes this and re-cuts them, the way `mode_dirty` re-cuts the furniture.
    shelf_dirty: bool,
    /// ★ 2026-10-17: the settings screen's refresh row rebuilt the library, so every face on the
    /// row is a picture of a cart list that no longer exists. Taken once by the frontend, which
    /// owns the resident window and the compositor both — the same handover `shelf_dirty` makes
    /// and for the same reason: a texture cannot be minted or released without one.
    library_dirty: bool,
    /// ★ 2026-10-17: the face handles the refresh row retired, waiting for the next frame to
    /// release them. **Gathered here rather than released on the spot** because `App` never holds
    /// a compositor (see `App::root`'s neighbour fields for the same rule): the shelf hands them
    /// over, they sit in this vector, and the frontend drains it before it rebuilds anything.
    stale_faces: Vec<TexId>,
    /// ★ 2026-10-28：刷新行的「已刷新」回执词随设置页改版一起删了（六六：「选项去掉仅留
    ///   描述名」—— `Refresh` 不再有值，`A` 就是做）。`library_note` 是那份回执的载体，
    ///   已无值可指，整条移除。
    /// ★ 2026-10-20：卡上 `Bootlogos/` 里的开机图，boot 时扫一次（`crate::bootlogo`）。
    /// **不进 `SlotState`**：它不是"机器被设成了什么"，是"这张卡上此刻有什么" ——
    /// 换一张卡、往目录里丢一张图，都是开机那一次扫描才认得的东西，
    /// 存进持久状态只会让两张卡互相说错对方的事。
    bootlogos: Vec<crate::bootlogo::Entry>,
    /// **机器现在用的选择索引**：0 is the generated BaseOS wordmark; custom BMPs are 1..=N.
    /// If the persisted filename no longer exists, the effective selection falls back to 0.
    boot_index: usize,
    /// The value currently browsed in the settings row. Index 0 is the generated BaseOS
    /// wordmark; index 1 and later are the user's BMP candidates. A successful apply/reset
    /// moves both indices together; a failure restores the effective index.
    boot_pick: usize,
    /// How far up the game layer's own screen is. Not a phase: it outlives the insert, since
    /// the cart is home and the chrome is still on screen while the picture arrives.
    screen: f32,
    /// Whether the core behind the slot has published anything yet. Pushed in, because only
    /// whoever owns the emulator knows: the compositor still holds the last cart's frame.
    game_ready: bool,
    /// Accumulated from `update`, which is the only clock the app has. Milliseconds, since
    /// that is what the HUD fade is stated in.
    clock: f64,
    /// `None` in unit tests, where there is no panel to darken and no battery to run out.
    power: Option<Power>,
    /// BaseOS runtime deep-suspend hook. Absent on host builds and on systems that do not provide it.
    baseos_sleep: Option<PathBuf>,
    /// Whether the current doze handed backlight blank/restore to the BaseOS sleep helper.
    /// False for the music display-off path, even when BASEOS_SLEEP is available.
    native_suspend_active: bool,
    dozed_at: Millis,
    /// When the state next has to be on the card. Moved by every resume write, not only by
    /// the autosave itself.
    autosave_at: Millis,
    battery_at: Millis,
    charge_at: Millis,
    /// The last full reading, with its charge half kept current by the fast tick. One
    /// snapshot rather than two values, so nothing on screen can show a percent and a bolt
    /// that never coexisted.
    battery: Option<Battery>,
    /// What the platform was last told to show. The fast tick recomputes `led_state()` every
    /// second whether or not anything moved, and `set_led` is a real write on a real device —
    /// `motor_change` two crates over exists for exactly the same reason, translating a strength
    /// asked for every frame into a write only on the edge between still and moving. This is
    /// that same edge kept here rather than behind the platform boundary: unlike the motor, the
    /// LED has no protocol-specific state of its own to translate through (`LedState` is
    /// already the discrete value the tick computes), and `App` is where the state it is
    /// computed from already lives, so every `Platform` gets the deduplication for free instead
    /// of each one having to grow its own copy of it.
    last_led: Option<LedState>,
    /// In-game display filter (panel mask + colour correction), cycled with SELECT+X — which serves
    /// the art on machines that ship it and the aperture grid on the one that does not.
    display: DisplayFilter,
    /// Which page of `palettes::GB_PALETTES` the palette key is walking, and which cell of that
    /// page the browser has under its caret (`None` while the browser is closed).
    ///
    /// The page outlives the browser on purpose: a hold opens the browser, the user turns to the
    /// page they want and lets go, and the tap walks *that* page. Keeping the two in one place is
    /// what makes "the page you were last looking at" true — a second copy of it on the browser
    /// would be a second answer to which fifteen the key is cycling.
    pal_page: usize,
    pal_cursor: Option<usize>,
    /// ★ 2026-10-21：色板浏览器是不是从**游戏菜单**里进的。B/A 退出时看这个旗子：
    /// 从组合键（原 SELECT+Y 长按）进的退回游戏，从菜单进的退回"色板"那一行。
    pal_from_menu: bool,
    /// The names of the entries on the page that is up, one face per cell in table order, as the
    /// frontend rasterised them — **on the machines whose names fit a cell, and empty on the one
    /// whose do not** (see `palette_names`). One list rather than two, because a panel either
    /// names its cells or names the one under the caret, and which of those it is can be read off
    /// this being empty.
    pal_names: Vec<(TexId, u32, u32)>,
    /// The entry under the caret, for the tables whose cells carry no names of their own. The
    /// Game Boy's reading, and the only one it has ever had.
    pal_name: Option<(TexId, u32, u32)>,
    /// ★ 2026-10-21：**操作键映射弹窗**打开着的时候是 `Some`。App 级浮层，不是相位 —— 它从
    ///   货架设置页和游戏菜单两处都能开，而 `B` 关掉之后底下的屏原样还在。
    keymap_edit: Option<KeymapEdit>,
    /// ★ 2026-10-21：这张弹窗的面。前端在开着的那一帧烘好交进来（`sync_keymap`），关了就
    ///   放走；`None` 是还没烘出来，此时不画而不是画一个洞。
    keymap_faces: Option<KeymapFaces>,
    /// The audio latency profile, chosen on the shelf with SELECT+VOL. Read from
    /// `System/audio.txt` at boot and written back whenever it changes.
    audio: Profile,
    /// A per-system screen overlay PNG from the card — `Overlay/GB/GB.png` (plain Game Boy)
    /// or `Overlay/GBC/GBC.png` (Game Boy Color) — drawn over the live game with its own alpha
    /// channel, so the user can theme the screen (bezel, dot-matrix grid, tint) without a
    /// rebuild. `None` = the built-in look. The RGBA is decoded once per insert in
    /// `Session::spawn_core`; the GL texture is minted lazily by the compositor when
    /// `overlay_dirty` is set, and released when the next cart has none.
    overlay_pixels: Option<(Vec<u8>, u32, u32)>,
    /// The compositor's texture handle for the decoded overlay, or `None` when there is no
    /// overlay on screen. Only the compositor can mint or drop the GL resource this names.
    overlay_tex: Option<TexId>,
    /// Set when `overlay_pixels` changed and the texture must be (re)built or released on the
    /// next frame. Pushed by `set_overlay`; cleared by the frontend after `sync_overlay`.
    overlay_dirty: bool,
    /// Every overlay PNG the seated cart's system offers, in rotation order, and which one is
    /// up. Built on insert (`Session::spawn_core`) from `Overlay/`; `overlay_next` cycles it.
    /// Empty for GBA, which has no overlay set.
    overlay_list: Vec<crate::root::OverlayFile>,
    /// Which system `overlay_index` counts in: 0 for GB, 1 for GBC. Kept because a Game Boy's
    /// index counts its three models before the card's own overlays and a Game Boy Color's does
    /// not, so the same number means different things on the two shelves.
    overlay_slot: usize,
    overlay_index: usize,
    /// Which overlay each system last showed, GB then GBC, persisted to `System/overlay.txt` so
    /// the choice survives a boot. Clamped against the current list length when it is applied.
    overlay_indices: [usize; crate::root::OVERLAY_RINGS],
    /// Which machine's shelf is up. The row holds only this system's carts; SELECT+L2 cycles it.
    shelf_system: System,
    /// Whether the favourites sub-shelf is up within the current system shelf. The whole of what
    /// the bottom-right star reads, and what the letter strip and letter jumps stand down for —
    /// a system shelf is still a library with letters, a favourites shelf is not.
    fav_view: bool,
    /// The disc row's badge — one face, not three, because a disc is not a machine and the row
    /// has no per-machine variant to show.
    disc_ind_faces: Vec<(TexId, u32, u32)>,
    disc_ind_light: Vec<(TexId, u32, u32)>,
    /// ★★ 2026-10-24（六六）：「**常驻插卡位的各机种背板**」。插卡位那个洞后面不再只有塑料：
    /// 每个机种一张 380×44 背板（`System/Carts/slot_back_*.png`），常驻在洞里，卡带插进来
    /// 才盖住它。按机种存而不是按机种枚举存数组，是因为只有三个机种有素材（见
    /// `slot_back_for`），没素材的机种就什么都不画 —— 和机种 logo 同一套处理。
    slot_back: Vec<(System, (TexId, u32, u32))>,
    /// 卷闸合上时浮在帘子上的那行 CHANGE，洋红高光已由前端换成本主题的发光色。
    change_face: Option<(TexId, u32, u32)>,
    /// 同一行字**预烘的柔光**（比字大 `2*GLOW_PAD`，画在字下面）。开机烘一次。
    change_glow: Option<(TexId, u32, u32)>,
    /// ★ 2026-10-26（六六）：「给所有机种的插卡/插光盘动画加上货架区的呼吸发光文字
    /// （颜色随主题）」。
    ///
    /// `System/Carts/slot_loading.png` 是一张 **720×240 的成品图**（两行 "GAME START /
    /// LOADING..." 的空心字，纯洋红 #FF00FF），洋红像素在 `rebake` 里换成本主题的亮色，
    /// 所以换主题字色跟着走 —— 和卷闸上那行 CHANGE 同一套。
    ///
    /// **不按机种分**：所有机种的插卡、PS1 的插光盘,共用这一张。
    loading_face: Option<(TexId, u32, u32)>,
    /// 那张字的柔光。比字大 `2·LOADING_GLOW_PAD`（14px 一圈），画在字下面。
    loading_glow: Option<(TexId, u32, u32)>,
    /// 这轮插入是从哪一刻开始的（`clock`，毫秒）。呼吸相位从它起算，所以字能跨过
    /// `Inserting → Playing` 继续呼吸，不跳相位 —— 见 `draw_loading`。
    loading_at: f64,
    /// The system-shelf swap in flight, if any. While it is `Some` the app takes no input.
    shelf_switch: Option<ShelfSwitch>,
    powering_off: bool,
}

impl App {
    pub fn new(carts: Vec<Cart>) -> Self {
        let letter_counts = tally_letters(&carts);
        // The deck's transport, built before the struct so the player can hold the same handle
        // the frame reads. No thread starts here: `Player::new` only makes the channel, and the
        // decode thread is `open`'s to start — a frontend that boots and never opens the deck
        // must not have spawned one.
        let deck = std::sync::Arc::new(crate::music::Deck::default());
        let player = crate::music::Player::new(std::sync::Arc::clone(&deck));
        // ★ 2026-10-23（六六）：货架页桌面图改为**代码层 rasterize**（`slot_ui::desktop_rgba`），
        //   它自带底条与 360×40 插卡位（透明缺口让卡带插进去）。旧的程序化卡座
        //   （`Shelf::draw` 里的 `draw_slot_inside`/`draw_slot_shell`：bay/拇指弧/缝）会从那道
        //   缺口里透出来跟桌面框打架 —— 碟片行早就是用 `set_mouth(false)` 把程序化卡座关掉
        //   的（见 `disc_row`），Games 行现在同样关掉，让卡带只插在桌面框自己的缺口里。
        let mut shelf = Shelf::new(carts);
        shelf.set_mouth(false);
        App {
            phase: Phase::Shelf,
            shelf,
            // Empty until the first entry into the disc row; `disc_row` fills both halves at once
            // so the row can never show a disc the list does not have.
            disc_shelf: Shelf::new(Vec::new()),
            row: ShelfRow::Games,
            deck_seek_dir: 0.0,
            deck_seek_at: 0,
            info_panel: None,
            info_page: 0,
            info_back: None,
            letters: Letters::new(),
            letter_counts,
            letter_faces: Vec::new(),
            letter_ridge_face: None,
            letter_pill_face: None,
            play_held: None,
            fav_held: None,
            sys_held: None,
            mtp_held: None,
            fav_star: None,
            fav_ind: None,
            badge_atlas: None,
            refusal: None,
            refused_from: None,
            alert_face: None,
            shelf_title_face: None,
            title_scroll: None,
            light_faces: LightFaces::default(),
            shutdown_faces: Vec::new(),
            power_menu: None,
            power_menu_faces: Vec::new(),
            power_bg_face: None,
            direct_link: None,
            direct_faces: Vec::new(),
            direct_at: 0,
            direct_ip: None,
            direct_ip_face: None,
            direct_ip_dirty: false,
            core_picker: None,
            sys_picker: None,
            sys_icons: Vec::new(),
            sys_faces: None,
            core_board_face: None,
            core_lid_face: None,
            core_faces_stem: None,
            core_socket_faces: Vec::new(),
            core_chip_faces: Vec::new(),
            core_blank_chip_face: None,
            core_chip_shadow_face: None,
            core_legend_faces: Vec::new(),
            game_menu: None,
            game_menu_faces: Vec::new(),
            game_menu_hot: Vec::new(),
            game_title_face: None,
            game_title_built_for: None,
            disc_face: None,
            disc_face_built_for: None,
            disc_live: None,
            coins: Vec::new(),
            save_settings_faces: None,
            slot_caption_face: None,
            slot_thumb_face: None,
            link_menu_faces: Vec::new(),
            link_step_faces: Vec::new(),
            link_fail_faces: Vec::new(),
            starting: None,
            link_transport: None,
            restarting: false,
            restart_core: false,
            act_at: 0,
            root: None,
            state: SlotState::default(),
            vol_before: Vec::new(),
            snapshot: None,
            core: Core::default(),
            link: None,
            sfx: None,
            ui_sfx_at: None,
            polaroids: None,
            pending: None,
            legend_faces: Vec::new(),
            undo_face: None,
            clock_faces: None,
            sticker_face: None,
            menu_buttons: [None; slot_ui::menu::SLOTS],
            menu_empty: None,
            menu_hint: None,
            settings_rows: Vec::new(),
            settings_hint: [None, None, None, None],
            deck_face: None,
            deck_name: None,
            deck_key: None,
            deck_count: None,
            deck_track_count: None,
            deck_lyric_face: None,
            lcd_panel: None,
            discs: Vec::new(),
            disc: 0,
            track: 0,
            player,
            deck,
            lines: None,
            lyrics: slot_media::lrc::Index::default(),
            covers: slot_media::Covers::default(),
            panel: Panel::default(),
            ramp: 0,
            transport: slot_ui::lcd::Transport::default(),
            mode: slot_ui::lcd::Mode::default(),
            eq: slot_ui::eq::Eq::default(),
            last_mode_tap_ms: 0,
            key_focus: 2, // 起来就在 PLAY 上：一台开着的机器指向它在做的事
            // **True from birth.** "Not walked yet" is the state a card's music starts in, and
            // it is what makes the walk happen on whichever frame the deck first appears —
            // whether it got there through the menu or through `open.txt`. Starting it at
            // `false` would mean only the menu route ever walked anything, and a card that was
            // never opened from the menu would show an empty deck forever.
            discs_dirty: true,
            shortcut_rows: Vec::new(),
            shortcut_hint: None,
            wallpaper: None,
            wall_glow: None,
            desktop: None,
            battery_percent: slot_ui::Printed::default(),
            bolt: None,
            shelf_clock: slot_ui::Printed::default(),
            hud: Hud::new(),
            cheats_on: true,
            cheats: Vec::new(),
            cheat_menu: None,
            cheat_faces: Vec::new(),
            cheat_face_stem: None,
            cheat_empty_face: None,
            cheats_dirty: false,
            mode_dirty: false,
            shelf_dirty: false,
            library_dirty: false,
            stale_faces: Vec::new(),
            bootlogos: Vec::new(),
            boot_index: 0,
            boot_pick: 0,
            screen: 0.0,
            game_ready: false,
            clock: 0.0,
            power: None,
            baseos_sleep: None,
            native_suspend_active: false,
            dozed_at: 0,
            autosave_at: AUTOSAVE_MS,
            battery_at: BATTERY_POLL_MS,
            charge_at: CHARGE_POLL_MS,
            battery: None,
            last_led: None,
            powering_off: false,
            audio: Profile::default(),
            overlay_pixels: None,
            overlay_tex: None,
            overlay_dirty: false,
            overlay_list: Vec::new(),
            overlay_slot: 0,
            overlay_index: 0,
            overlay_indices: [0; crate::root::OVERLAY_RINGS],
            shelf_system: System::Gba,
            fav_view: false,
            disc_ind_faces: Vec::new(),
            disc_ind_light: Vec::new(),
            slot_back: Vec::new(),
            change_face: None,
            change_glow: None,
            loading_face: None,
            loading_glow: None,
            loading_at: 0.0,
            shelf_switch: None,
            display: DisplayFilter::new(
                slot_gfx::builtin_panel_mask(),
                DisplayFilter::DEFAULT_CC,
                2,
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                0,
            ),
            // Page one, browser shut. `App::boot` moves the page onto whatever the card had
            // chosen, so the first tap of the palette key walks the fifteen around it.
            pal_page: 0,
            pal_cursor: None,
            pal_from_menu: false,
            pal_names: Vec::new(),
            pal_name: None,
            keymap_edit: None,
            keymap_faces: None,
            wifi: None,
            wifi_conf: None,
            wifi_rx: None,
            wifi_glyphs: None,
            wifi_faces: None,
            wifi_dirty: false,
            search: None,
            search_origin: None,
            search_faces: None,
        }
    }

    /// A seated cart goes back in through the insert animation rather than appearing
    /// already playing, so a boot and a resume are the same movement. A card with no
    /// `Games` directory scans empty, which is a shelf, not a boot failure.
    pub fn boot(root: &Path) -> Self {
        // ★ **分段计时**（2026-10-07 实测：`library scan + shelf` 占 **2023 ms**，是 slot 自身
        //   第二大的开销，仅次于字体那 3194 ms）。分三段打到 `slot.log`，看时间落在哪一段：
        //   目录遍历（**`scan_cached` 即使命中缓存也要走一遍 `Games/` 与 `Labels/`** ——
        //   它要用名字列表去核缓存）、卡带美术解码、还是别的。
        //   见 `slot-开机慢-诊断与最小加载方案-2026-10-07.md`。
        let t_boot = std::time::Instant::now();
        crate::root::ensure(root);
        crate::root::migrate(root);
        // Read the card's label preference (strip_tags) before the first scan, so the displayed
        // and filed names both honour it. A missing file leaves the long-standing default.
        slot_store::init_label_config(root);
        // ★ 2026-10-24：街机 / NeoGeo 的中文名表。内置一份（`assets/arcade_names.tsv`，
        //   9083 条，来自 MamePlus 遗产 `mame_cn.lst`），卡上若有
        //   `System/arcade_names.tsv` 就**整份替换**内置表 —— 六六自己的整合包对上不上的
        //   romset，往那个文件里补一行就行，不必重编二进制。
        //   在扫描之前调：名字只在**画的时候**查（`shown_name`），扫描与索引仍认 stem。
        slot_store::init_arcade_names(root);
        // Before anything is drawn. The card's palette cannot change while the device is on,
        // so it is read once and never asked for again.
        slot_ui::set_theme(Theme::read(root));
        // The card's own cart art, if it has any, also read once before the first frame and
        // before the face builder thread starts. Read once because a cart face is built on that
        // thread and the art has to be there, whole, by then; and said out loud because a card
        // whose art was refused — an index-coloured PNG, say — has nothing else to say so.
        // The card's per-cart rules — the editable half of the shell table, and the reason
        // a cart's colour is no longer only a compile-time decision. Read before the art is
        // installed because the art loader reads the same table for its template names, and
        // before the face builder starts because a face is built with the rule applied.
        let rules = slot_ui::load_cart_rules(root);
        slot_ui::install_cart_rules(rules);
        eprintln!("slot: cart rules: {}", slot_ui::cart_rules_summary());
        slot_ui::install_cart_art(slot_ui::load_cart_art(root));
        eprintln!("slot: cart art: {}", slot_ui::cart_art_summary());
        // ★ 2026-10-26（六六）：缺省卡带图，`System/Carts/66SLOT_*.png`，每机种一张（FC/NES、
        //   SFC/SNES 各一张）。开机加载、常驻共享 —— 和机器模板同一条规矩：在 face 构建线程
        //   启动前装好，缺的机种回退内建壳。
        slot_ui::install_default_art(slot_ui::load_default_art(root));
        eprintln!("slot: default art: {}", slot_ui::cart_defaults_summary());
        // The sleeve's art, loaded and said the same way. Separate from the carts' three because a
        // sleeve is not a machine and has no slot to key it by — there is one file, and a card
        // without it gets the built-in sleeve exactly as the cassette used to.
        //
        // **The cassette's own art is still loaded, below this.** The tree knows both shapes and
        // the row picks one; keeping both installed costs one `read_dir` and a `Vec` at boot, and
        // it means a card that ships `disc.png` is not silently ignored by a build that draws
        // sleeves.
        slot_ui::install_tape_art(slot_ui::load_tape_art(root));
        eprintln!("slot: disc art: {}", slot_ui::tape_art_summary());
        slot_ui::install_cd_art(slot_ui::load_cd_art(root));
        eprintln!("slot: cd art: {}", slot_ui::cd_art_summary());
        // 街机名表进了多少条 —— 「街机还是显示 romset 名」时的第一句要问的话。
        // 卡上有覆盖文件就是覆盖表的条数，没有就是内置表的（9083）。
        eprintln!("slot: arcade names: {}", slot_store::arcade_name_count());
        eprintln!(
            "slot: boot: prep+art {} ms",
            t_boot.elapsed().as_millis()
        );
        let t_scan = std::time::Instant::now();
        // The cached scan, not the plain one: a boot re-reads only the carts that changed
        // since the last one, which on a card of a few hundred games is the difference
        // between a shelf and a wait. See `slot_store::scan_cached`.
        let mut app = App::new(scan_cached(root).unwrap_or_default());
        eprintln!("slot: boot: scan_cached {} ms", t_scan.elapsed().as_millis());
        app.root = Some(root.to_path_buf());
        // BaseOS exports this adapter hook. Unlike the generic frontend timer, it enters the
        // board's real deep-suspend path and returns only after the physical power key wakes it.
        app.baseos_sleep = std::env::var_os("BASEOS_SLEEP")
            .map(PathBuf::from)
            .filter(|path| path.is_file());
        if let Some(path) = app.baseos_sleep.as_ref() {
            eprintln!("slot: BaseOS suspend hook: {}", path.display());
        }
        // The display filter (panel mask + colour correction) is read from the card so the
        // shipping look can be a per-card choice, then cycled live during play. The mask falls
        // back to the built-in LCD3x table; the colour correction falls back to the grayscale (0%
        // default; the modes fall back to (2, 0) (LCD3X + OFF, the shipped look).
        let lcd3x = crate::root::panel_mask(root).unwrap_or_else(slot_gfx::builtin_panel_mask);
        let nocolor_cc = crate::root::color_correction(root).unwrap_or(DisplayFilter::DEFAULT_CC);
        // Before the modes are read, not after: a card written before the Game Boy's palette list
        // was the table it is now has an index into the *old* one, and half of that range means
        // something else here. `migrate_palette` puts the look back where it was and leaves a
        // marker so it happens exactly once per card.
        crate::root::migrate_palette(root);
        crate::root::migrate_display_modes(root);
        let (mode, cc, scanline) = crate::root::display_modes(root);
        app.display = DisplayFilter::new(lcd3x, nocolor_cc, mode, cc, scanline);
        // The page the palette key walks starts on the entry the card left the Game Boy at, so
        // the first tap continues from that palette rather than from page one.
        app.pal_page = app.gb_palette() / slot_ui::PALETTE_PER_PAGE;
        app.audio = crate::root::audio_profile(root);
        // The card's Wi-Fi credentials, for the settings row's two words and the config
        // screen's first view. Read once here and kept honest by the screen itself, which
        // is the only thing that ever writes the file.
        app.wifi_conf = slot_store::WifiConf::read(root);
        app.state = read_slot_state(root);
        // Custom boot pictures are card content; the active choice is the shared BaseOS
        // selection record. Index 0 remains BaseOS's generated default, even if the card
        // contains no custom BMPs.
        app.bootlogos = crate::bootlogo::list(root);
        // Index 0 is the generated BaseOS wordmark. Candidate BMPs occupy 1..=N.
        // Missing selection is therefore explicit, never confused with the first image.
        app.boot_index = crate::bootlogo::selected(root, &app.bootlogos)
            .map_or(0, |index| index + 1);
        // ★ **这一行开机时就停在"机器在用的那一张"上**（2026-10-20）：浏览是 `boot_pick`
        // 的事，一个刚开的屏没有"浏览过但没应用"这种状态 —— 那只会让第一眼看到的和
        // 机器里生效的不是同一张。
        app.boot_pick = app.boot_index;
        // The card can ask for the network at boot: an empty `System/network.on` is the
        // switch. It is honoured here, in the frontend, rather than in the dock's session
        // so the same card asks the same way on AGS-102 and on BaseOS — and so a dock that
        // never had the hook (upstream BaseOS) gains it for free. A worker, never the boot
        // path: bringing the radio up takes seconds and the shelf must not wait for it.
        if root.join("System/network.on").is_file() {
            let net_root = root.to_path_buf();
            std::thread::spawn(move || {
                let _ = std::process::Command::new("/bin/sh")
                    .arg(net_root.join("System/slotnet.sh"))
                    .arg("up")
                    .status();
            });
        }
        // The restored level, pushed to the deck once now. The deck's atomic starts at its own
        // default (80) because nothing else ever set it, so a card that boots muted or at any
        // other level would play music at 80 until the first press of a volume key — the same
        // bug as `adjust`'s, on the other side of it.
        app.deck.set_volume(app.output_volume());
        // Which machine's shelf opens, and which overlay each system was last showing, both
        // off the card. A cart still in the slot outranks the stored shelf — a resume has to
        // open the shelf that holds it — and a stored shelf the card has no carts for is
        // skipped, so a card whose only games are, say, GB never opens on an empty GBA shelf.
        app.overlay_indices = crate::root::overlay_indices(root);
        // ★ 2026-11-09：同名跨机种时，**不能拿全库第一条**。库里不去重（`Games/FC/魂斗罗.nes`
        //   与 `Games/MD/魂斗罗.md` 是两条，stem 都是 `魂斗罗`），而 `order` 以路径兜底 ⇒
        //   `Games/FC/…` 恒在前（`F` < `M`）。从前这里就是全库 `find`，于是上次玩的是 MD 的
        //   魂斗罗，开机却打开 FC 的货架、续玩的也是 FC 那张卡 —— 与玩家报的
        //   「加载 MD 游戏时显示的是 FC 卡带且进去也是 FC」同一条根因。
        //   消歧用卡上的 `System/shelf.txt`（上次浏览的机种）：那张卡就是从那个货架插进去的。
        //   它只在"有几条同名"时才被问到，唯一匹配与从前完全一样。
        let stored = crate::root::shelf_system(root);
        let last_system = app
            .state
            .cart
            .as_deref()
            .and_then(|stem| {
                let same: Vec<&_> = app.shelf.carts.iter().filter(|c| c.stem == stem).collect();
                match same.len() {
                    0 => None,
                    1 => same.first().copied(),
                    _ => same
                        .iter()
                        .copied()
                        .find(|c| c.system() == stored)
                        .or_else(|| same.first().copied()),
                }
            })
            .map(|c| c.system());
        let mut system = last_system.unwrap_or(stored);
        if !app.shelf.carts.iter().any(|c| c.system() == system) {
            // ★ 2026-10-25：**改用 `root::MACHINES`，不再手抄。** 这一格原来是一份手写的
            //   七机种列表，加 MD/PS1 时很容易漏 —— 漏了不会编译失败，只会让「上一张卡的
            //   机种在这张卡上没有卡」时回退到一个 md/ps1 之前的老机种，看起来像开机没识别。
            //   `MACHINES` 是 `ensure()` 建目录用的同一份，两处一份，漏不掉。
            system = crate::root::MACHINES
                .into_iter()
                .find(|s| app.shelf.carts.iter().any(|c| c.system() == *s))
                .unwrap_or(System::Gba);
        }
        app.shelf_system = system;
        app.shelf.set_system(system);
        let view = app.system_view(system);
        app.shelf.set_view(view, None);
        app.retally_letters();
        // The palette, before anything has been rasterised: every face on the device is baked
        // with the ink burned into it, so the mode has to be in place before the first one, not
        // after. Anything later than this line is a light-mode device drawn in dark type.
        slot_ui::palette::set_mode(app.state.mode);
        // The look rides the mode: it is an index into that mode's row, so it is set right
        // after the mode is, and for the same reason — the first baked face would otherwise be
        // the device's own palette, not the one the card was left on.
        slot_ui::palette::set_look(app.state.look as usize);
        // The art source, before anything is rasterised for the same reason the mode is: a
        // cart face bakes in which of the two sources dressed it, so the first one built has
        // to be built under the source the card was left on.
        slot_ui::set_art_source(app.state.art_source);
        if app.state.clock_set {
            app.start();
        } else {
            // Seeded from the system clock and re-seeded by `set_power`, which is the first
            // moment there is a platform whose clock is the device's rather than the host's.
            app.phase = Phase::SetClock {
                picker: ClockPicker::from_secs(system_secs()),
                from_menu: false,
            };
        }
        // After `start`, so a screen opened this way opens onto a shelf that is already up
        // behind it rather than onto one that has not been built yet.
        if let Some(phase) = Self::open_at_boot(root) {
            app.phase = phase;
        }
        // `discs` is not a phase, it is a row, so it cannot come back from `open_at_boot` — which
        // returns one. Set it here, after the walk has a root to read and after the phase is
        // settled, so `open.txt: discs` shows the disc row rather than the game row with a
        // missing phase. Same development aid, same reasoning (no key injection on this board).
        if std::fs::read_to_string(root.join("System/open.txt"))
            .map(|t| t.trim() == "discs")
            .unwrap_or(false)
        {
            app.show_row(ShelfRow::Discs);
        }
        app
    }

    /// A card carrying `System/open.txt` naming a screen boots straight into it: `menu`, or
    /// `shortcuts`.
    ///
    /// A development aid in the same family as `SLOT_NO_CORE`, and for the same reason. The menu
    /// is behind a tap of MENU; the handheld has no console; and remote key injection does not
    /// work on this board — so a screen behind a keypress cannot be looked at without hands
    /// unless something opens it for you.
    ///
    /// **A file rather than an environment variable** because the frontend is started by the
    /// base OS's session script and there is no way to put a variable in front of that from
    /// here, while a file on the card is something the device's own boot reads without anybody's
    /// help. Nothing on the shipping path reads it: a card without the file boots to the shelf
    /// exactly as it always did, and a card with an unrecognised one is the same card.
    fn open_at_boot(root: &Path) -> Option<Phase> {
        let text = std::fs::read_to_string(root.join("System/open.txt")).ok()?;
        match text.trim() {
            "menu" => Some(Phase::Menu {
                index: slot_ui::menu::first_slot(),
            }),
            "shortcuts" => Some(Phase::Shortcuts {
                scroll: 0.0,
                want: 0.0,
            }),
            "deck" => Some(Phase::Listening {
                key: 0,
                playing: false,
                disc: 0,
            }),
            _ => None,
        }
    }

    /// The mask the game pass multiplies by, resolved through the current aperture preset.
    ///
    /// **Only the Advance wears it.** Every other machine is neutral: the Game Boy and the
    /// Color draw their lattice in the game shader (`display_grid`), and the consoles draw
    /// scanlines there, so none of them want a multiply table on top.
    ///
    /// ★ 2026-11-03（玩家反馈「FC/街机的横纹变成了方格或竖纹」）：这里原来是
    /// `matches!(Gb | Gbc)` —— 也就是**除了那两台，全都穿 GBA 那张孔径表**。3× 的 GBA 上它
    /// 是一张干净的 RGB 点阵；换到 2× 的 FC（512×448）就变成**欠采样**：
    /// `mask_scale = 纹素宽度`只在 3× 时等于 1 纹素/屏像素，2× 时 3 列里只采到第 0、2 列
    /// （实测取列 0,2,0,2…；GBA 3× 是 0,1,2,0,1,2）⇒ LCD3x 的 RGB 三列结构塌成 2px 周期的
    /// 亮度差加色偏，看起来就是一层方格/竖纹。逐像素复现见 `_tools/_scanline_vs_mask_sim.py`。
    /// 而 `mask` 的出厂默认值是 **2**（半强度 LCD3x，`root::display_modes`），所以这层方格
    /// 是**开箱就在**的 —— 玩家一旦把扫描线关掉，画面就只剩它。
    pub fn display_mask(&self) -> [[[u8; 3]; 3]; 3] {
        self.display
            .applied_mask(!matches!(self.current_machine(), System::Gba))
    }

    /// The Game Boy's four shades right now, whichever of the two tables they came from: a model's
    /// own set while a model's screen art is up, and the entry `cc` holds otherwise.
    ///
    /// This is the one place that decides which of the two wins, so the palette, the lattice and
    /// the boot log cannot disagree about it.
    fn gb_shades(&self) -> Option<[[u8; 3]; 4]> {
        if self.current_machine() != System::Gb {
            return None;
        }
        if let Some(m) = App::model_of_index(self.overlay_slot, self.overlay_index) {
            let m = m.min(crate::palettes::GB_MODELS.len() - 1);
            return Some(crate::palettes::GB_MODELS[m].1);
        }
        Some(crate::palettes::GB_PALETTES[self.gb_palette()].1)
    }

    /// The Game Boy's entry in the palette table, clamped to it. Out of range can only come from
    /// a `display.txt` edited by hand, and a card that names entry 9000 should show the last
    /// palette rather than have the app fall over.
    fn gb_palette(&self) -> usize {
        (self.display.cc[DisplayFilter::machine_slot(System::Gb)] as usize)
            .min(crate::palettes::GB_PALETTES.len() - 1)
    }

    /// The panel lattice for the machine that is up, as its colour and how much of it to mix.
    ///
    /// A Game Boy's colour is the current palette's **lightest shade**, which is the background:
    /// a DMG's mesh is olive, a Light's is cyan, and neither is written down anywhere — change the
    /// palette and the lattice follows.
    ///
    /// A Game Boy Color has no four shades to take a colour from, so its lattice is plain — white
    /// or black — and unlike the Game Boy's, which is always on, the Color's is **switched by the
    /// overlay ring** rather than by a key of its own: every overlay appears once per grid state,
    /// and the state is the overlay index modulo `GBC_GRID_STATES` (see `overlay_next`). This is
    /// why the colour machines need no aperture table: the mesh and the bezel are one choice.
    ///
    /// An Advance is on the aperture table instead (`display_mask`), so it draws no mesh at all:
    /// this is the two bezel machines' lattice and nothing else.
    ///
    /// The third element is the scanline flag: `true` keeps the bottom row alone. The fourth
    /// is **how many source rows apart the wires are** — `1` paints one on every row, `2`
    /// paints on every other one. Only the scanline form reads it; a lattice spans every row
    /// by definition and passes `1`.
    pub fn display_grid(&self) -> Option<([f32; 3], f32, bool, u32)> {
        match self.current_machine() {
            System::Gb => {
                let light = self.gb_shades()?[0];
                Some((
                    [
                        light[0] as f32 / 255.0,
                        light[1] as f32 / 255.0,
                        light[2] as f32 / 255.0,
                    ],
                    DisplayFilter::GB_GRID_MIX,
                    false,
                    1,
                ))
            }
            System::Gbc => {
                if self.overlay_slot != 1 {
                    return None;
                }
                let colour = match self.overlay_index % DisplayFilter::GBC_GRID_STATES {
                    1 => [1.0, 1.0, 1.0],
                    2 => [0.0, 0.0, 0.0],
                    _ => return None,
                };
                Some((colour, DisplayFilter::GBC_GRID_MIX, false, 1))
            }
            // ★ 2026-10-22: the consoles' scanlines, switched by the overlay ring the way a
            // Game Boy Color's mesh is: every overlay appears once per state and the state
            // is the overlay index modulo `GBC_GRID_STATES` — off, half, full. The lines are
            // **black** here, unlike the Color's white one: a CRT's scanline is the row gap
            // between lit lines, not a lit wire drawn on top of them. The `true` is the
            // scanline form of the shader — the bottom row of every 3x3 source pixel, which
            // is the panel's row gap — rather than the whole 5/9 lattice.
            // ★ 2026-10-25：MD 与 PS1 也走 CRT 扫描线 —— 两者都是**接电视的机种**，而且
            //   扫描线在这里是由遮罩环（`overlay_ring`）开的，没有 `md` / `ps1` 的遮罩
            //   素材就永远是关闭状态（`overlay_ring` 那一槽还空着，见 `root.rs`）。
            //   所以这一臂现在不会画任何东西，但**逻辑上和 SFC/FC 同属一类**：
            //   素材一到就该有扫描线，而不是到时候再想起"这两个机种忘了加"。
            System::Sfc
            | System::Fc
            | System::Neogeo
            | System::Arcade
            | System::Md
            | System::Ps1
            | System::Pce => {
                // The ring the *current* machine owns, so the state cannot be read off an
                // overlay set that belongs to a different machine.
                let (slot, _) = crate::root::overlay_ring(self.current_machine())?;
                //
                // ★★ 2026-11-03：**环里有素材才走环，没素材走 `scanline_state`。**
                //
                //   原来这里只有一条路：`overlay_slot == slot` 且 `overlay_index % 3`
                //   = 1/2，否则 `None`。可是 `overlay_choices` 对主机种是
                //   `素材数 × 3` —— **卡上没有 `Overlay/fc*.png` / `all_4-3*.png` 时它是 0**，
                //   索引被锁死在 0，这个分支就永远返回 `None`：那台机器**根本没有扫描线**，
                //   而 `overlay_next()` 里 `choices <= 1` 直接 return，SELECT+X 也按不动。
                //   640 档的发布包整个没有主机遮罩（只带 GB/GBC/gba-00），所以 640 机器上
                //   每一台主机都处在这个状态；720 包是 11-01 才补上那几张图的，补上之前也一样。
                //
                //   而现在 `ScanlineCycle`（SELECT+X 在无遮罩艺术的主机上走的那条）改的是
                //   `DisplayFilter::scanline_state` —— 那个字段**从来没有人读过**，写盘、读盘、
                //   循环都对，屏幕上纹丝不动。这里把它接上：没素材时它就是扫描线档。
                let from_ring = self.overlay_slot == slot && !self.overlay_list.is_empty();
                let state = if from_ring {
                    self.overlay_index % DisplayFilter::GBC_GRID_STATES
                } else {
                    self.display.scanline_state as usize
                };
                let mix = match state {
                    1 => DisplayFilter::CONSOLE_GRID_MIX_HALF,
                    2 => DisplayFilter::CONSOLE_GRID_MIX_FULL,
                    _ => return None,
                };
                Some((
                    [0.0, 0.0, 0.0],
                    mix,
                    true,
                    DisplayFilter::CONSOLE_SCANLINE_STEP,
                ))
            }
            System::Gba => None,
        }
    }

    /// Which machine the display filter is being asked about: the cart under the caret, which
    /// during play is the cart in the slot, and the shelf's own machine when there is no caret
    /// (an empty card). GB and GBC are two machines with two palettes even though they share one
    /// screen geometry, so this cannot be `screen_system`.
    pub fn current_machine(&self) -> System {
        self.shelf
            .current_cart()
            .map(|c| c.system())
            .unwrap_or(self.shelf_system)
    }

    /// Whether the machine now under the caret draws **CRT scanlines** rather than a bezel
    /// mesh: every console. One predicate for all three places that have to agree — which
    /// readings `display_grid` takes (`overlay_index` vs `scanline_state`), what `SELECT+X`
    /// cycles, and (through `display_mask`) that no console wears the Advance's aperture table.
    ///
    /// ★ 2026-11-03: 这三个地方原来各写各的匹配，漏掉一处就出现"按了没反应"——
    ///   `display_mask` 漏了这半边（主机种全穿 GBA 孔径），`scanline_state` 漏了读取端。
    ///   所以判据收成一处，谁要问"这是不是主机种"都走它。
    pub fn console_scanlines(&self) -> bool {
        matches!(
            self.current_machine(),
            System::Sfc
                | System::Fc
                | System::Neogeo
                | System::Arcade
                | System::Md
                | System::Ps1
                | System::Pce
        )
    }

    /// The colour-correction matrix for the game pass right now, identity when off.
    pub fn display_cc(&self) -> [[f32; 3]; 3] {
        self.display.applied_cc(self.current_machine()).0
    }

    /// ...and its black point, which is the whole difference between a saturation grade and a
    /// hardware palette. Zero now that the Game Boy is drawn by a lookup and not a matrix; the
    /// colour machines never had one.
    pub fn display_cc_bias(&self) -> [f32; 3] {
        self.display.applied_cc(self.current_machine()).1
    }

    /// The Game Boy's palette as a 256-entry RGBA lookup, or `None` when this frame is not a
    /// Game Boy palette at all — 原生灰, where the core's own greys are the look, and every
    /// colour machine, which uses the matrix above.
    ///
    /// Worth being clear about the cost, because it looks like the expensive one and is not:
    /// the matrix path is two `pow`s and nine multiplies per pixel, and this is one extra
    /// nearest-neighbour sample replacing all of it. `pow` is the instruction to avoid on this
    /// class of GPU.
    pub fn display_palette(&self) -> Option<[u8; 1024]> {
        let machine = self.current_machine();
        if machine != System::Gb {
            // A colour machine looks through a ramp only when its grade is a duotone: everything
            // else down the list is a matrix or a baked cube, which have their own paths.
            let mode = self.display.cc[DisplayFilter::machine_slot(machine)] as usize;
            return match GRADES[mode.min(GRADES.len() - 1)].kind {
                GradeKind::Ramp(dark, light) => Some(ramp_lut(dark, light)),
                _ => None,
            };
        }
        // 原生灰 is the core's own greys, so there is nothing to look up — and only the *table's*
        // own entry means that. A model is always a palette: its four shades are the other half of
        // the art, not a preference layered over it.
        if self.display.cc[DisplayFilter::machine_slot(machine)] as usize
            == crate::palettes::GB_NEUTRAL
            && App::model_of_index(self.overlay_slot, self.overlay_index).is_none()
        {
            return None;
        }
        Some(DisplayFilter::gb_lut(self.gb_shades()?))
    }

    /// What the Game Boy's current palette is called, for the boot log. One table, names and
    /// colours together, so a name cannot drift from what it names.
    pub fn display_palette_name(&self) -> &'static str {
        if self.current_machine() == System::Gb {
            if let Some(m) = App::model_of_index(self.overlay_slot, self.overlay_index) {
                return crate::palettes::GB_MODEL_NAMES
                    [m.min(crate::palettes::GB_MODEL_NAMES.len() - 1)];
            }
        }
        crate::palettes::GB_PALETTES[self.gb_palette()].0
    }

    /// 当前色彩校正该用的 gamma：灰度(0) 保持 1.0（编码空间直乘，黑白不压暗）；
    /// AGB-001(1)/NDS(2) 走线性空间，AGB-001 用 2.2（更暗）、NDS 用 2.0（稍亮）。
    /// GB 的色板同理：原生灰走 1.0（本来就是核心给的灰），其余色板都走 `CC_GAMMA`
    /// —— LUT 的插值要在线性空间里做，否则中间两档会被压暗。
    pub fn display_cc_gamma(&self) -> f32 {
        let machine = self.current_machine();
        let mode = self.display.cc[DisplayFilter::machine_slot(machine)] as usize;
        if machine == System::Gb {
            if mode == crate::palettes::GB_NEUTRAL
                && App::model_of_index(self.overlay_slot, self.overlay_index).is_none()
            {
                return 1.0;
            }
            return DisplayFilter::CC_GAMMA;
        }
        match GRADES[mode.min(GRADES.len() - 1)].kind {
            GradeKind::Matrix(_, g) => g,
            // A ramp or a baked cube is drawn by a lookup, and the lookup is built in the space
            // the shader samples it in — so there is no linearisation to ask for.
            _ => 1.0,
        }
    }

    /// The pixel-art lookup when the pixel grade is in force on a colour machine, else `None`.
    /// `frontend` pushes it every frame, the same way it pushes the Game Boy's palette.
    pub fn display_pixel_lut(&self) -> Option<&[u8]> {
        let machine = self.current_machine();
        if machine == System::Gb {
            return None;
        }
        let mode = self.display.cc[DisplayFilter::machine_slot(machine)] as usize;
        match GRADES[mode.min(GRADES.len() - 1)].kind {
            GradeKind::Lut(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// The audio profile the shelf has settled on. `Session::sync_audio_profile` is what
    /// notices a change and reopens the device for it.
    pub fn audio_profile(&self) -> Profile {
        self.audio
    }

    /// SELECT+VOL+ / SELECT+VOL- on the shelf: step the audio profile and persist it to
    /// `System/audio.txt`. Nothing is reopened here — the sink belongs to the session, and
    /// this only decides what it should open for.
    fn cycle_audio(&mut self, forward: bool) {
        self.audio = if forward {
            self.audio.next()
        } else {
            self.audio.prev()
        };
        if let Some(root) = &self.root {
            crate::root::write_audio_profile(root, self.audio);
        }
        // The profile moves no pixel of the picture, so this line is the only thing that tells
        // the player the press landed at all.
        let said = match self.audio {
            Profile::Stable => Toast::AudioStable,
            Profile::Balanced => Toast::AudioBalanced,
            Profile::Strict => Toast::AudioStrict,
        };
        self.hud.toast(said, self.now());
    }

    /// SELECT+X on an Advance: step the aperture table through its `MASK_STATES` presets (off ->
    /// LCD3X at 25, 50, 75 and 100% -> SCANLINE at 50 and 100%) and persist the choice to
    /// `System/display.txt`.
    fn cycle_mask(&mut self) {
        self.display.cycle_mask();
        if let Some(root) = &self.root {
            crate::root::write_display_modes(root, self.display.mask_mode, self.display.cc, self.display.scanline_state);
        }
    }

    /// SELECT+X on a console whose card carries no bezel art for it (every console on the 640
    /// build — that package ships no console overlays at all — and MD / NeoGeo / Arcade / PS1 /
    /// PCE anywhere): 无 → 50% → 100% scanlines and back, persisted to `System/display.txt`
    /// like the other two dials. Read by `display_grid` whenever the ring has nothing to show.
    fn cycle_scanline(&mut self) {
        self.display.cycle_scanline();
        if let Some(root) = &self.root {
            crate::root::write_display_modes(root, self.display.mask_mode, self.display.cc, self.display.scanline_state);
        }
    }

    /// SELECT+Y: advance the colour correction of the machine that is up, and persist all of
    /// them to `System/display.txt`. On a colour machine that is grayscale -> AGB-001 -> NDS;
    /// on a Game Boy it is the next palette on the page the browser last showed.
    ///
    /// **One page, not the whole table.** The Game Boy's list is every palette gambatte ships —
    /// hundreds of them — and a key that walked all of them would be a key nobody could aim. The
    /// page is the unit: hold the key to open the browser and turn to the page you want, let go,
    /// and this walks the fifteen on it. That is also what makes the ring's length something the
    /// user can see, rather than a number that changes with a palette pack.
    ///
    /// **Refused while a model is up**, and that is the whole of what "bound" means. A model's
    /// four shades are not a preference layered over its art, they are the other half of it: the
    /// DMG screen was drawn against the DMG ramp, and letting this key recolour the picture
    /// inside that case would leave the two disagreeing with nothing on screen to say so. The
    /// press is refused rather than dropped, so it reads as answered — the same as every other
    /// "nothing doing" action in this file.
    fn cycle_cc(&mut self) {
        if self.current_machine() == System::Gb {
            if App::model_of_index(self.overlay_slot, self.overlay_index).is_some() {
                self.refuse();
                return;
            }
            self.step_palette();
        } else {
            // A colour machine's list is two pages of nine, and the tap walks **the page the
            // grade in hand is on** — so `pal_page` is not consulted at all. It counts pages of
            // the Game Boy's hundreds-long palette table (`boot` seeds it from the GB slot), and
            // reading it here put the walk's base far past the end of an eighteen-entry list:
            // `cycle_on_page` then answered the last entry for ever, which is exactly the "the
            // key works once" the user saw. Deriving the page from the index keeps the two
            // tables apart and needs no second piece of state.
            let machine = self.current_machine();
            let slot = DisplayFilter::machine_slot(machine);
            let per = App::grades_per_page(machine);
            let at = self.display.cc[slot] as usize;
            let page = (at / per).min(GRADES.len().div_ceil(per).saturating_sub(1));
            let next = slot_store::cycle_on_page(page, at, GRADES.len(), per);
            self.display.cc[slot] = next as u16;
        }
        if let Some(root) = &self.root {
            crate::root::write_display_modes(root, self.display.mask_mode, self.display.cc, self.display.scanline_state);
        }
    }

    /// How many grades one page of the colour browser shows. The Game Boy's table is fifteen to a
    /// page (three columns of five); a colour machine's list is two pages of nine, three by three.
    pub fn grades_per_page(machine: System) -> usize {
        if machine == System::Gb {
            slot_ui::PALETTE_PER_PAGE
        } else {
            9
        }
    }

    /// The Game Boy's palette one along the page that is up, wrapping at the end of that page.
    /// `self.pal_page` is the page the browser was last showing, so a palette picked on page 7
    /// keeps being cycled among page 7's fifteen until the browser is opened again.
    fn step_palette(&mut self) {
        let total = crate::palettes::GB_PALETTES.len();
        let next = slot_store::cycle_on_page(
            self.pal_page,
            self.gb_palette(),
            total,
            slot_ui::PALETTE_PER_PAGE,
        );
        self.display.cc[DisplayFilter::machine_slot(System::Gb)] = next as u16;
    }

    /// Into the slot or onto the shelf. Reached on boot once the clock is known, and from
    /// the clock screen when it becomes known.
    fn start(&mut self) {
        // One cart is a dedicated device. There is nothing to choose between, so whatever
        // `slot.state` remembers, including a cart that is no longer on the card, names the
        // only thing it could have meant.
        // A position in the *view*, not the library: the shelf is one machine's carts, and a
        // library index would seat the caret on the wrong cart — or past the end of a shorter
        // view — the moment the shelf is filtered. `index_of_stem` reads the view.
        let seated = if self.single_cart() {
            Some(0)
        } else {
            self.state
                .cart
                .as_deref()
                .and_then(|stem| self.shelf.index_of_stem(stem))
        };
        self.phase = Phase::Shelf;
        match seated {
            Some(i) => {
                // The shelf sits on the resumed cart so ejecting it lands where it left.
                self.row_mut().index = i;
                self.row_mut().scroll = i as f32;
                // Never clean: a resume is the whole point of the cart still being in there.
                self.insert(false);
                if let Phase::Inserting { resumed, t, .. } = &mut self.phase {
                    *resumed = true;
                    // Seated already. The floor still runs, so the core has the same time to
                    // load; the cart simply does not travel to get there.
                    *t = INSERT_S;
                }
            }
            // A cart the library no longer has is an empty slot. Left uncorrected on disk:
            // the next seat rewrites it, and a boot is the worst moment to need a write.
            None => self.state.cart = None,
        }
        // The strip starts on the letter of whatever the row is showing. Snapped rather than
        // travelled to: there is no previous position at a boot, and a strip arriving from `#`
        // would be the shelf's first movement being one nobody made.
        if let Some(cart) = self.row().current_cart() {
            self.letters.snap_to(cart.initial);
        }
    }

    /// Confirms whatever is on the clock screen.
    ///
    /// Two ways off it, and `from_menu` is the whole of the difference. On slot's own first
    /// launch there is no way back and no second chance: confirming it starts the shelf, which is
    /// what the screen exists for. Opened from the settings screen it is a **setting** like any
    /// other, so confirming it returns there and `B` abandons it — a screen the user asked for
    /// that could only be left by changing something would be a trap.
    pub fn confirm_clock(&mut self) {
        let Phase::SetClock { picker, from_menu } = &self.phase else {
            return;
        };
        // Both read off before the borrow ends. The platform is given utc, because that is
        // what the base system's clock and its ntp both assume the card holds; the offset is
        // kept beside it as the only thing that turns it back into the time on the wall.
        let (secs, offset, from_menu) = (picker.secs(), picker.offset_min(), *from_menu);
        if let Some(power) = &mut self.power {
            power.set_clock(secs);
        }
        self.state.utc_offset_min = offset as i16;
        self.state.clock_set = true;
        self.persist();
        if from_menu {
            // Back to the row that opened it, not to the top of the screen: the user came for
            // one setting and going back to it is what "back" means everywhere else here.
            self.show_settings(slot_ui::settings::Row::DateTime);
        } else {
            self.start();
        }
    }

    /// Seconds since the epoch, from the platform once there is one. The shelf clock and the
    /// polaroid captions both read it, so neither can disagree with the cartridge RTC.
    /// Where the card is mounted. `None` only in the tests that never touch one.
    /// Which machine's shelf is up. The cart's own size on screen follows from it, so anything
    /// drawing a row of carts has to ask rather than assume the Advance cart's.
    pub fn shelf_system(&self) -> slot_store::System {
        self.shelf_system
    }

    /// ★ 2026-10-27 23:1x：这一行真正的盒子（含按目录的 `NES` / `SNES` 覆写）。占位卡、
    /// 空卡、脸部纹理都按它做 —— 只有它和 `slot_rects` 是同一个来源，才不会有"货架上是
    /// 竖版、占位卡是横版"这种一帧换形。
    pub fn shelf_size(&self) -> slot_ui::CartSize {
        self.shelf.size()
    }

    /// Where the shelf's line of type sits: centred in the gap between the row's own foot and
    /// the top of the case. Derived rather than picked, so moving either end — a taller cart, a
    /// deeper bay — carries it along instead of leaving it behind.
    fn shelf_title_y(&self) -> f32 {
        // ★ **2026-10-22（六六）：标题从"卡带行与底条之间的缝"移到**顶条居中**。** 机器货架的
        //   名字读作"这一台机器"，就该站在机器自己的那条顶边上，而不是挂在屏幕中间一段空隙里。
        //   初值取顶条垂直居中；**上机实测再微调**（本函数就是微调点）。
        //
        //   **碟片行没有顶条**（2026-10-07 六六：CD 顶条底条都撤掉），所以它仍按"整屏减底条"
        //   的老规矩量——那是整块屏，不是一条顶条。
        match self.row {
            ShelfRow::Games => MOUTH_H / 2.0 - SHELF_TITLE_H as f32 / 2.0,
            ShelfRow::Discs => {
                let case = OUT_H as f32 - MOUTH_H;
                (slot_ui::cd::SIZE.foot_y() + case) / 2.0 - SHELF_TITLE_H as f32 / 2.0
            }
        }
    }

    /// 卡上开机图的名字，**去掉扩展名**（一列全是 bmp，后缀只是噪音）。
    /// 前端在 boot 时把这一列烘成面，所以这里给的是字符串而不是 `Entry`。
    pub fn boot_labels(&self) -> Vec<String> {
        self.bootlogos.iter().map(|e| e.label().to_string()).collect()
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Local, not utc. The shelf clock, the polaroid captions and the stamps the states are
    /// named by all come through here, so the offset is applied once rather than at each of
    /// them, and none of them can disagree with the others about what time it is.
    pub fn wall_secs(&self) -> i64 {
        let utc = self.power.as_ref().map_or_else(system_secs, |p| p.now());
        utc + i64::from(self.state.utc_offset_min) * 60
    }

    /// What the clock screen is showing, or `None` off it. The binary rasterises from it and
    /// watches its text to know when to do so again.
    pub fn picker(&self) -> Option<&ClockPicker> {
        match &self.phase {
            Phase::SetClock { picker, .. } => Some(picker),
            _ => None,
        }
    }

    pub fn set_sticker_face(&mut self, face: TexId) {
        self.sticker_face = Some(face);
    }

    /// The shortcut card: one face per row and the line that stays at its foot. Fixed strings,
    /// so they are built once at boot and never asked for again.
    pub fn set_shortcut_faces(
        &mut self,
        rows: Vec<(TexId, u32, u32)>,
        hint: Option<(TexId, u32, u32)>,
    ) {
        self.shortcut_rows = rows;
        self.shortcut_hint = hint;
    }

    /// The menu's buttons, in `menu::Item::GRID` order, plus the recess an empty slot wears and
    /// the line at the foot of that screen. Baked rather than drawn from rects because a
    /// rounded corner is not a rectangle; re-baked when the theme flips, because both the plate
    /// and the type on it come out of the palette.
    pub fn set_menu_faces(
        &mut self,
        buttons: [Option<MenuButton>; slot_ui::menu::SLOTS],
        empty: Option<(TexId, u32, u32)>,
        hint: Option<(TexId, u32, u32)>,
    ) {
        self.menu_buttons = buttons;
        self.menu_empty = empty;
        self.menu_hint = hint;
    }

    /// The settings screen's rows and its foot lines.
    ///
    /// **Every value a row can take, not the one it has**: the value changes under a keypress and
    /// a row is a 660-wide rounded plate, so the whole list is rasterised at boot and picking one
    /// is a lookup. Re-baked when the theme flips, for the menu's reason — the plate and the type
    /// on it both come out of the palette.
    ///
    /// ★ **Four foot lines since 2026-10-20** (`pick`), one per `settings::Row::hint_index`.
    pub fn set_settings_faces(
        &mut self,
        rows: Vec<Vec<[(TexId, u32, u32); 2]>>,
        change: Option<(TexId, u32, u32)>,
        open: Option<(TexId, u32, u32)>,
        act: Option<(TexId, u32, u32)>,
        pick: Option<(TexId, u32, u32)>,
    ) {
        self.settings_rows = rows;
        // In `hint_index` order: the constants are declared in that order beside the lines
        // themselves, so this array literal and `Row::hint_index` cannot drift apart silently.
        self.settings_hint = [change, open, act, pick];
    }

    /// The deck's furniture: the disc, the name of what is on it, and the plate under every key.
    /// Re-baked when the theme flips, because the disc's shell and the keys' rim both come out of
    /// the palette.
    pub fn set_deck_faces(
        &mut self,
        disc: (TexId, u32, u32),
        name: Option<(TexId, u32, u32)>,
        key: (TexId, u32, u32),
    ) {
        self.deck_face = Some(disc);
        self.deck_name = name;
        self.deck_key = Some(key);
    }

    /// The deck's disc counter, uploaded separately from the furniture because it changes as the
    /// disc changes while the furniture does not.
    pub fn set_deck_count(&mut self, count: Option<(TexId, u32, u32)>) {
        self.deck_count = count;
    }

    /// The deck's track counter, uploaded apart from the disc's because it changes on a different
    /// key — a shoulder changes the disc, the transport changes the track.
    pub fn set_deck_track_count(&mut self, count: Option<(TexId, u32, u32)>) {
        self.deck_track_count = count;
    }

    /// The lyric line, replaced as the song's words change. `None` when the track has no `.lrc`.
    pub fn set_deck_lyric_face(&mut self, lyric: Option<(TexId, u32, u32)>) {
        self.deck_lyric_face = lyric;
    }

    /// The baked LCD panel, or `None` before the first bake. **Set by `frontend`, drawn by the
    /// deck**, and nothing in between: the frame is composed in `slot-ui` and uploaded here, so
    /// the app never holds a pixel of it.
    pub fn set_lcd_panel(&mut self, panel: Option<(TexId, u32, u32)>) {
        self.lcd_panel = panel;
    }

    /// The deck's disc face, replaced whenever the disc changes — the sticker is the disc's own.
    pub fn set_deck_face(&mut self, tex: TexId, w: u32, h: u32) {
        self.deck_face = Some((tex, w, h));
    }

    /// The name in the deck's top window, replaced as the disc changes. Separate from
    /// `set_deck_faces` because this one changes while a session runs and the furniture does not.
    pub fn set_deck_name_face(&mut self, name: Option<(TexId, u32, u32)>) {
        self.deck_name = name;
    }

    /// The disc in the deck, if the card has any. `None` is the ordinary state of a card with no
    /// `Music/`, and the deck draws its empty rather than a blank cassette.
    pub fn deck_disc(&self) -> Option<&slot_media::Disc> {
        self.discs.get(self.disc)
    }

    /// The name to print in the deck's top window: the disc's own name, and the menu's word for
    /// the screen when there is no disc yet. Returned rather than baked so the caller can see
    /// whether it changed and skip the rasterisation when it did not.
    pub fn deck_title(&self) -> String {
        match self.deck_disc() {
            Some(t) => t.name.clone(),
            None => slot_ui::lang::MENU_DISCS.to_string(),
        }
    }

    /// How many discs the card holds, and which one is in the deck. For the deck's own line.
    pub fn deck_disc_count(&self) -> (usize, usize) {
        (self.discs.len(), self.disc)
    }

    /// Put a disc in the deck. Clamped rather than trusted, because the index is the one number
    /// in the deck that a shorter list can make wrong.
    pub fn set_disc(&mut self, disc: usize) {
        self.disc = disc.min(self.discs.len().saturating_sub(1));
    }

    /// The track the transport is on, as `(track, tracks)`, for the deck's own line.
    pub fn deck_track(&self) -> (usize, usize) {
        let len = self.discs.get(self.disc).map_or(0, |t| t.tracks.len());
        (self.track.min(len.saturating_sub(1)), len)
    }

    /// 当前曲目的**容器时长**（秒）；拿不到就是 0.0。给 `deck_seek` 的上界用。
    pub fn deck_track_duration(&self) -> f32 {
        let (_, len) = self.deck_track();
        self.discs
            .get(self.disc)
            .and_then(|d| d.tracks.get(self.track.min(len)))
            .map_or(0.0, |t| t.duration)
    }

    /// How far through the current track the deck is, as `(elapsed, length)`. The length is the
    /// container's own, and 0.0 when it would not say — which reads as an empty bar rather than
    /// as a wrong one.
    pub fn deck_position(&self) -> (f32, f32) {
        let len = self
            .discs
            .get(self.disc)
            .and_then(|t| t.tracks.get(self.track))
            .map_or(0.0, |t| t.duration);
        (self.deck.elapsed(), len)
    }

    /// The lyrics of the track on the deck, and where in them the deck is. `None` for a card
    /// with no `.lrc`, which is most cards and is not a fault.
    pub fn deck_lyric(&self) -> Option<&str> {
        let lines = self.lines.as_ref()?;
        slot_media::lrc::at(lines, self.deck.elapsed())
    }

    /// The waveform the frame is drawing, read out of the deck's tap. Empty when nothing has
    /// been decoded yet, which the drawer reads as "no signal" rather than as silence.
    pub fn deck_tap(&self, dst: &mut [i16]) -> usize {
        self.deck.read_tap(dst)
    }

    /// Whether the track on the deck has run out (decoder thread parked on its last frame).
    ///
    /// ★ 2026-11-02：`deck_advance` 的合盖续播靠这个门控，测试也读它——相位里看不见解码
    ///   线程，只有这里能问到它的真实状态。
    pub fn deck_finished(&self) -> bool {
        self.deck.finished()
    }

    /// Put `track` of the current disc on the deck and start it.
    ///
    /// **The one way a track is ever put on**, so "open, then load its lyrics, then play" cannot
    /// be half-done by a caller that forgot a step. A track that will not decode leaves the deck
    /// where it was and says so on stderr rather than throwing the disc out: a card with one bad
    /// file in a folder is a card whose other files still play.
    fn play_track(&mut self, track: usize) -> bool {
        let Some(tr) = self
            .discs
            .get(self.disc)
            .and_then(|t| t.tracks.get(track))
        else {
            return false;
        };
        if self.player.open(tr.path.clone()).is_err() {
            return false;
        }
        self.track = track;
        // Through the index, not a guessed path: the music says `Artist - Title(Original).mp3`
        // and the lyric file says `Title.lrc`, and the lookup is what knows both spellings.
        self.lines = self.lyrics.load(tr);
        self.player.play();
        true
    }

    /// Step the transport within the disc: `-1` is rewind, `+1` is fast-forward.
    ///
    /// **Track steps, not seconds.** The decoder has no seek, so a rewind that moved time inside
    /// one track would have to reopen the file and would land on the same place. What a deck can
    /// honestly do without seek is change cassette, and this is the key that says so.
    ///
    /// A step off either end of the disc is refused rather than wrapped: a rewind at the first
    /// track on a real deck stops at the start, and a fast-forward at the last one runs off the
    /// end into the stop.
    fn step_track(&mut self, dir: i32) -> bool {
        let len = self.discs.get(self.disc).map_or(0, |t| t.tracks.len());
        if len == 0 {
            return false;
        }
        let next = self.track as i32 + dir;
        if next < 0 || next >= len as i32 {
            return false;
        }
        let ok = self.play_track(next as usize);
        // ★ 2026-10-27：换曲 = 走带（真机上换曲是按同一组琴键，不是把带子取出来）。
        if ok {
            self.sfx = Some(Sfx::TapeTransport);
        }
        ok
    }

    /// The disc in the deck, changed by L or R. Stops what was on and starts the new disc's
    /// first track: a deck playing album A does not carry on playing it while album B goes in.
    ///
    /// `None` when the step was refused — the shoulder is already against that end. `Some` of
    /// whether it started, which is **not** always true: the disc changed either way, because it
    /// is physically in the deck now, but a first track that will not decode leaves the deck
    /// silent and honest rather than refusing the whole change.
    fn change_disc(&mut self, dir: isize) -> Option<bool> {
        let next = slot_media::Disc::step(self.disc, self.discs.len(), dir);
        if next == self.disc {
            return None;
        }
        self.disc = next;
        self.track = 0;
        let started = self.play_track(0);
        // ★ 2026-10-27：换碟 = 一条新带进仓（不是"走带"那颗键）。
        self.sfx = Some(Sfx::TapeIn);
        Some(started)
    }

    /// Leave the deck: the transport stops, the thread does not. Coming back resumes this disc
    /// where it stopped rather than decoding it from the top, which is what a deck does when you
    /// walk away from it and come back.
    fn leave_deck(&mut self) {
        self.player.pause();
    }

    /// What the deck does on its own: put the next track on when this one has run out.
    ///
    /// **On the frame loop because nothing else is awake.** The decoder stops itself at the end
    /// of a track and there is no callback to call — the thread is not allowed to touch the app,
    /// and an app reached from a decoder thread would be a data race on the whole screen. So the
    /// flag is polled, and this is the only reader.
    ///
    /// Gated on the deck being the phase on screen, which is not politeness: a paused deck also
    /// reads as finished, and without the gate walking away from the deck would start the next
    /// track on the frame the user left.
    ///
    /// The end of the disc stops rather than wrapping. A disc that has run out is a disc that
    /// has run out, and silently starting the first track again is a loop nobody asked for.
    fn deck_advance(&mut self) {
        // ★ 2026-11-02（六六）：「放音乐时屏蔽超时关机」的另一半——合盖后专辑也要一曲接一曲。
        //   合盖场景下相位是 `Doze` 而不是 `Listening`，光认 `is_listening()` 会让 lid 底下
        //   播完第一首就哑住（后台播放变成"只有第一首"）。Doze 里存着随身听位置且当时在放，
        //   才算"用户正在听这张碟"；否则（暂停/停止时合盖）不续播。
        let deck_was_playing = matches!(
            self.phase,
            Phase::Doze { deck: Some((_, _, true)), .. }
        );
        if (!self.is_listening() && !deck_was_playing) || !self.deck.finished() {
            return;
        }
        // Nothing behind the fast-forward key is not a fault: a one-track disc is the degenerate
        // case of the same rule, and it has played all there is.
        let started = self.step_track(1);
        // ★ 切轨成功 ⇒ 传输状态回到「在放」；磁带末尾切不动 ⇒ 停。
        //   **状态在图标上**（`lcd::bar`），所以这里错了屏上就错，不必等下一帧。
        //   而单曲循环是 `Mode::One` 的事，不在这里——这里只管「这一盘还有没有下一首」。
        self.transport = if started {
            slot_ui::lcd::Transport::Playing
        } else {
            slot_ui::lcd::Transport::Stopped
        };
        // The phase's own copy follows the transport, so waking into it is not a lie about
        // whether sound is coming out. The doze carries one too (`deck` in `Phase::Doze`).
        if let Phase::Listening { key, playing, disc } = self.phase {
            if started != playing {
                self.phase = Phase::Listening {
                    key,
                    playing: started,
                    disc,
                };
            }
        } else if let Phase::Doze {
            cart,
            deck: Some((key, disc, playing)),
        } = &mut self.phase
        {
            // ⚠️ 只有**带着随身听**的 doze 才改：纯游戏 doze（cart 那条路）的相位里没有
            //   deck，写成 `Some(..)` 就是凭空造一张碟出来。模式本身已把这条排除在外。
            if *playing != started {
                let cart = cart.take();
                self.phase = Phase::Doze {
                    cart,
                    deck: Some((*key, *disc, started)),
                };
            }
        }
    }

    /// The readouts in the band between the disc and the transport: how far through the track
    /// it is, and what the decoder is hearing right now.
    ///
    /// **Both are rects, not textures, and that is the point.** A bar is a filled rectangle at
    /// one x and two widths; baking it would mean a rasterise and an upload for a number that
    /// changes every frame, which on this device is the difference between free and a visible
    /// hitch. The waveform is the same argument with more rects — 64 columns of them, which is
    /// still nothing next to the ~1500 textured quads the shelf puts up.
    ///
    /// The spectrum proper (Goertzel bands, a baked LCD panel) is **not** here: it needs a
    /// logical frame and a palette, which is its own piece of work. What is here needs none of
    /// that plumbing and is true the moment the first packet decodes.
    fn draw_deck_meters(&self, out: &mut Vec<Draw>) {
        // The band is 28 px tall and it is the only space on this screen that belongs to no
        // fixture: `FACE_Y + FACE_H` is the disc's own foot, `deck::key_y()` is where the
        // transport starts. Two rows of three and four pixels fit in it; anything taller runs
        // under the keys, which is the one thing on this panel that must stay legible.
        //
        // **Laid out from the two ends rather than centred**, because the counters already own
        // the outer thirds at the text's own height and a centred bar would run under both.
        let foot = slot_ui::deck::FACE_Y + slot_ui::deck::FACE_H;
        let w = 300.0;
        let x0 = (OUT_W as f32 - w) / 2.0;
        let (elapsed, len) = self.deck_position();
        if len > 0.0 {
            let at = (elapsed / len).clamp(0.0, 1.0) * w;
            // The whole length, then the part of it that has gone. Two rects rather than a
            // gradient, so the bar reads as a position and not as a level.
            out.push(Draw::Rect {
                x: x0,
                y: foot + 4.0,
                w,
                h: 3.0,
                colour: dim_ink(0.22),
            });
            if at > 0.0 {
                out.push(Draw::Rect {
                    x: x0,
                    y: foot + 4.0,
                    w: at,
                    h: 3.0,
                    colour: slot_ui::palette::ink_f(),
                });
            }
        }
        // The waveform, under the bar. Peak per column rather than every sample: 64 columns over
        // a 2048-sample tap is 32 samples each, and one rect per sample would be a line the
        // panel cannot resolve anyway.
        const COLS: usize = 48;
        let mut buf = [0i16; crate::music::TAP_LEN];
        let n = self.deck_tap(&mut buf);
        if n < COLS {
            return;
        }
        let per = n / COLS;
        let cw = w / COLS as f32;
        let hmax = 15.0;
        let base = foot + 9.0;
        for c in 0..COLS {
            let mut peak = 0u32;
            for s in &buf[c * per..(c + 1) * per] {
                peak = peak.max(s.unsigned_abs() as u32);
            }
            // Full scale at about -3 dBFS rather than at the clip: a column that only ever
            // touched the top of its range would make every quiet passage look like a drawing.
            let level = (peak as f32 / 24_000.0).clamp(0.0, 1.0);
            let h = level * hmax;
            if h < 1.0 {
                continue;
            }
            out.push(Draw::Rect {
                x: x0 + c as f32 * cw,
                y: base + (hmax - h),
                w: (cw - 1.0).max(1.0),
                h,
                colour: dim_ink(0.30 + 0.55 * level),
            });
        }
    }

    /// Whether the disc is running, as the phase has it: **what the transport was last asked to
    /// do.**
    ///
    /// **The phase and not the decoder's own flag, and the reason is a millisecond.** `play()`
    /// hands a command to a thread that drains it between packets, so the decoder's `playing`
    /// answers a frame or two after the press — long enough that the key's glyph would come back
    /// wearing a play triangle over a deck that is already making sound, and long enough that a
    /// test asserting on the press would fail against a correct implementation. What the user
    /// asked for is the answer; whether the song has *run out* is the other half of that question
    /// and `deck_advance` is where it is asked.
    pub fn deck_playing(&self) -> bool {
        match self.phase {
            Phase::Listening { playing, .. } => playing,
            _ => false,
        }
    }

    /// Whether the card's music still needs walking. True until somebody walks it, which the
    /// frontend does the first time the deck is on screen — by either route into it.
    pub fn discs_dirty(&self) -> bool {
        self.discs_dirty
    }

    /// Hand the walked discs over. The index is kept inside the list: a card whose music was
    /// swapped under a running session must not leave the deck pointing past its end.
    /// Hand over the card's lyric files, found by the caller walking `Music/Lyrics/`.
    ///
    /// **Separate from `set_discs` because the two walks answer different questions and have
    /// different costs.** The disc walk opens every audio file to read its length; this one only
    /// reads directory names. A caller that has a root can hand both over in the same frame.
    pub fn set_lyrics(&mut self, lyrics: slot_media::lrc::Index) {
        self.lyrics = lyrics;
    }

    /// Hand over the card's album art, found by the caller walking `Music/Covers/`.
    ///
    /// **The third of the same walk** as the discs and the lyrics, and for the same reason: each
    /// answers "what is on this card" about a different folder, and a row that could see the
    /// music but not the art would draw a sleeve with a blank face and nothing saying why.
    pub fn set_covers(&mut self, covers: slot_media::Covers) {
        self.covers = covers;
    }

    /// The cover file for the disc in the deck, if the card has one.
    pub fn deck_cover(&self) -> Option<PathBuf> {
        self.discs.get(self.disc).and_then(|t| self.covers.find(t))
    }

    /// The lines of lyrics around the one being sung, newest last — **the three the panel
    /// draws**, and the only ones it ever needs.
    ///
    /// **A window rather than the whole lyric**, for two reasons that point the same way: a
    /// 120-row panel shows four lines, and copying four `String`s is cheaper than handing the
    /// panel a `&Vec` it has to index into sixty times a second. The current line is the middle
    /// one, which is where `slot_ui::lcd` puts it.
    ///
    /// Empty for a card with no lyrics, and the panel says the song's name instead.
    pub fn deck_lyric_window(&self) -> Vec<String> {
        let Some(lines) = self.lines.as_ref() else {
            return Vec::new();
        };
        let Some(at) = slot_media::lrc::index_at(lines, self.deck.elapsed()) else {
            return Vec::new();
        };
        // **The sung line is pinned to slot 2 of a fixed five-slot window** (odd count, so the
        // middle is the active one). The lyric panel is a *framed* box narrower than the window,
        // so the top and bottom slots run past the frame and are clipped — the "scrolling inside
        // the frame" look. Three slots would all fit and read as a static list; five makes the
        // neighbours partial.
        //
        // ★ **Always exactly five slots, active at index 2.** The panel centres and inks
        // `lines[len()/2]`, so the active line *must* sit at index 2 for every song position. A
        // `skip(at-2).take(5)` window plus a `.filter` on empty lines would shift that index
        // whenever a blank LRC line fell inside the window — highlighting the wrong row — and
        // would also misplace the active line near the song's first/last two lines. Padding the
        // start and end with empty strings (the panel skips empty lines, so they simply do not
        // draw) keeps the active line locked at slot 2 from the first line to the last.
        let mut window: Vec<String> = Vec::with_capacity(5);
        for k in 0..5 {
            // k runs 0..5; the active line is at k == 2; its original index is `at + (k - 2)`.
            let idx = at as i32 + (k - 2);
            if idx >= 0 && (idx as usize) < lines.len() {
                // The LRC's per-character spaces are a karaoke convention for highlighting one
                // character at a time. Nothing here highlights, so keeping them would space a
                // twelve-character line across half the panel for no reason a reader could see.
                window.push(lines[idx as usize].text.split_whitespace().collect::<Vec<_>>().join(" "));
            } else {
                window.push(String::new());
            }
        }
        window
    }

    /// The song's name, as the panel shows it: the track's own title, or the disc's name for a
    /// loose file that carries none.
    pub fn deck_title_now(&self) -> String {
        self.discs
            .get(self.disc)
            .and_then(|t| t.tracks.get(self.track))
            .map(|t| {
                let title = t.title.trim();
                if title.is_empty() {
                    t.stem.clone()
                } else {
                    title.to_string()
                }
            })
            .unwrap_or_default()
    }

    /// The artist, as the panel shows it. Empty is normal — the music this was written against
    /// carries no artist tag between the files.
    pub fn deck_artist_now(&self) -> String {
        self.discs
            .get(self.disc)
            .and_then(|t| t.tracks.get(self.track))
            .map(|t| t.artist.trim().to_string())
            .unwrap_or_default()
    }

    /// The tap's rate, for the Goertzel. Zero before anything has decoded, which the spectrum
    /// reads as "no signal" rather than as a division by zero.
    pub fn deck_rate(&self) -> u32 {
        self.deck.rate()
    }

    /// Which panel the deck is showing. Read by the drawer and by the key handler's own test.
    pub fn deck_panel(&self) -> Panel {
        self.panel
    }

    /// Put the other panel up. **Returns what is now up**, because a key handler that has to read
    /// the field back to decide what to draw is a key handler that can be a frame stale.
    pub fn toggle_panel(&mut self) -> Panel {
        self.panel = self.panel.toggled();
        self.panel
    }

    /// The LCD panel's current ramp: its name and its four shades, brightest first.
    ///
    /// **`ramp` is an index into the panel's own list, and it is clamped on the way out** rather than trusted.
    /// The count is a property of a generated table, and a table that grew or lost an entry
    /// would otherwise leave a deck asking for a palette that is not there.
    pub fn deck_ramp(&self) -> (&'static str, [[u8; 3]; 4]) {
        let ramps = crate::palettes::deck_ramps();
        let at = self.ramp.min(ramps.len().saturating_sub(1));
        ramps[at]
    }

    /// Which ramp is up, as an index. For the frontend's bake gate, which compares numbers.
    pub fn deck_ramp_index(&self) -> usize {
        self.ramp
    }

    /// The panel's clock, in the same milliseconds the rest of the frontend uses. **Borrowed from
    /// `App` rather than from `Instant`** so that a test can advance it and a bake can be timed
    /// without a real clock.
    pub fn clock_ms(&self) -> Millis {
        self.clock as Millis
    }

    /// How many ramps the panel has. For the drawer, and for a test that walks all of them.
    pub fn deck_ramp_count(&self) -> usize {
        crate::palettes::deck_ramps().len()
    }

    /// Step to the next ramp, wrapping at the end. **Returns the new index**, for the same reason
    /// `toggle_panel` returns the panel.
    pub fn cycle_ramp(&mut self) -> usize {
        let n = self.deck_ramp_count();
        self.ramp = if n == 0 { 0 } else { (self.ramp + 1) % n };
        self.ramp
    }

    // ── The five-key transport bar ─────────────────────────────────────────────────────────

    /// What the bar draws, gathered in one place so the drawer cannot read half from the app
    /// and half from itself.
    ///
    /// ⚠️ **第九轮取消了操作区**：键排整条不再画，`focus`/`focused` 与曲目序号随之取消 ——
    /// 屏上不再有"光标在哪颗键上"这件事可画。按键本身照旧可用（`key_focus` 还在 app 里，
    /// 它管的是输入，不是画面）。
    pub fn deck_bar(&self) -> slot_ui::lcd::Bar {
        let (elapsed, length) = self.deck_position();
        slot_ui::lcd::Bar {
            transport: self.transport,
            mode: self.mode,
            // The numbers, not a formatted string: the strip draws "0:37" and "4:00" on the two
            // sides and owns that format (see `slot_ui::lcd::clock`).
            elapsed,
            length,
        }
    }

    /// Move the caret along the bar, stopping at both ends.
    ///
    /// **Not wrapping**, because a transport's keys are a row you point at, not a ring you travel.
    pub fn move_key_focus(&mut self, dir: i32) -> usize {
        let n = slot_ui::lcd::Key::ALL.len() as i32;
        let next = self.key_focus as i32 + dir;
        self.key_focus = next.clamp(0, n - 1) as usize;
        self.key_focus
    }

    /// Press the key the caret is on. **Returns what it did**, so a caller can decide whether
    /// the phase needs rewriting without asking again.
    ///
    /// ⚠️ **光标已经没有画出来的地方了**（键排在第九轮撤掉、2026-10-06 那三个播放键也撤了）。
    /// 这是给"还有别的东西在推光标"的调用者留的入口；**实体键走 [`deck_action`] 直连**。
    pub fn press_key(&mut self) -> Option<slot_ui::lcd::Key> {
        let key = slot_ui::lcd::Key::ALL[self.key_focus.min(4)];
        self.deck_action(key);
        Some(key)
    }

    /// **走带条上的一个动作，直接调。**
    ///
    /// ★ 2026-10-06 六六：「屏幕被往上顶了因为初版你生成的三个播放按钮没拿掉」——
    ///   键排撤掉之后，`Phase::Listening` 的输入就是这里，**不再经过"光标在哪颗键上"**。
    ///   光标那套机制留着（`key_focus` / `move_key_focus` / `press_key`），因为它牵着 `Phase`
    ///   的形状与好几处构造，删它要动一圈，而画面上已经没有任何东西能显示它。
    pub fn deck_action(&mut self, key: slot_ui::lcd::Key) {
        match key {
            // MENU leaves the deck — it is the way *out* of playing and into the list, which is
            // the menu. B does the same thing, so this is the same door with a name on it.
            slot_ui::lcd::Key::Menu => self.leave_deck(),
            slot_ui::lcd::Key::Prev => {
                if self.deck.elapsed() > 3.0 {
                    // 三秒之内再按「上一首」= 回到本曲开头。**磁带没有 seek，但"从头"有**：
                    // 重新 open 同一轨就是从头。
                    let track = self.track;
                    self.play_track(track);
                } else {
                    self.step_track(-1);
                }
            }
            slot_ui::lcd::Key::Play => {
                // ★ 播放中按 = 停；停了或暂停按 = 放。
                //   这是"播放键三态"能落在**一颗**键上的原因。
                match self.transport {
                    slot_ui::lcd::Transport::Playing => {
                        self.player.pause();
                        self.transport = slot_ui::lcd::Transport::Paused;
                    }
                    _ => {
                        if !self.player.is_open() {
                            // 停在最前面且没轨可放 ⇒ 什么都不做，而不是"假装在放"。
                            return;
                        }
                        self.player.play();
                        self.transport = slot_ui::lcd::Transport::Playing;
                    }
                }
                // ★ 2026-10-27：走带键。播放和暂停是同一颗琴键的两面，声音不分正反。
                self.sfx = Some(Sfx::TapeTransport);
                self.sync_phase_playing();
            }
            slot_ui::lcd::Key::Next => {
                self.step_track(1);
            }
            slot_ui::lcd::Key::Mode => {
                // ★ **接播放模式与 EQ 两种设置于同一颗 MODE 键。** 单独点按 = 切换播放模式
                //   （ONE/ONCE/REPEAT/SHUFFLE，照旧，正中走带区显示）；**600ms 内的第二次快速
                //   点按 = 切换 EQ 预设**，并把第一次点按顺带推进的播放模式撤回 —— 于是双点落
                //   在"EQ 动了、播放模式没动"上，而不是两个都动。这是老设备"单击/双点"共用一颗
                //   键的惯例，不需要再加键。
                const EQ_DOUBLE_TAP_MS: Millis = 600;
                let now = self.clock_ms();
                if now.saturating_sub(self.last_mode_tap_ms) < EQ_DOUBLE_TAP_MS {
                    self.mode = self.mode.prev(); // 撤回第一次点按推进的播放模式
                    self.cycle_eq();
                    self.last_mode_tap_ms = 0; // 复位：下一次单独点按从播放模式开始
                } else {
                    self.deck_mode_next();
                    self.last_mode_tap_ms = now;
                }
            }
        }
    }

    /// **←/→ 快退快进：一次跳 `SEEK_STEP_SEC` 秒。**
    ///
    /// ★ 2026-10-06：这是播放器第一次有定位能力（见 `music::Cmd::Seek` 与
    ///   `slot_media::Source::seek_to`）—— 在此之前"快退"只能是重新 open 整个文件
    ///   （= 回到开头），那不叫快退。
    fn deck_seek(&mut self, dir: f32) {
        let len = self.deck_track_duration();
        if len <= 0.0 {
            // 不知道多长就不跳：跳到一个没有的位置比不跳更糟。
            return;
        }
        let at = (self.deck.elapsed() + dir * SEEK_STEP_SEC).clamp(0.0, (len - 0.05).max(0.0));
        self.player.seek(at);
        // 跳完若本来是停着的，就让它停在新位置 —— 停着的盘不该因为快进而开始放。
        if self.transport != slot_ui::lcd::Transport::Playing {
            self.player.pause();
        }
    }

    /// 按住 ←/→ 时每隔 `SEEK_REPEAT_MS` 再跳一步。由 `timers` 驱动（每帧跑）。
    fn deck_seek_hold(&mut self) {
        if self.deck_seek_dir == 0.0 || !self.is_listening() {
            return;
        }
        // 头一段不连跳：按下的那一下已经跳过了，等到时刻才继续。
        let now = self.now();
        if now < self.deck_seek_at {
            return;
        }
        self.deck_seek_at = now + SEEK_REPEAT_MS;
        self.deck_seek(self.deck_seek_dir);
    }

    /// 松开 ←/→：停连跳。**不做回弹** —— 真实播放器没有那种行为，就不给。
    fn deck_seek_end(&mut self) {
        self.deck_seek_dir = 0.0;
    }

    /// 确保货架开在指定那一行（而不是"切一下"）。给 MENU 短按用。
    ///
    /// 进碟片行那一步要先把 `disc_shelf` 建出来（`build_disc_row` 里做），
    /// 直接写 `row` 字段会留下一行空货架。
    fn show_row(&mut self, row: ShelfRow) {
        match row {
            ShelfRow::Games => self.row = ShelfRow::Games,
            ShelfRow::Discs => {
                if self.row != ShelfRow::Discs {
                    self.build_disc_row();
                    if self.disc_shelf.view_len() == 0 {
                        return; // 空碟片行：留在游戏货架，和没音乐时一样
                    }
                }
                self.row = ShelfRow::Discs;
            }
        }
        self.shelf_dirty = true;
    }

    /// 让货架**停在**指定那一行。`show_row` 的另一半 —— 关关于页时要的是
    /// "回游戏货架"，而"游戏货架"这件事在 `row` 这个粘性字段里说不清，
    /// 所以给它一个名字。
    fn set_row(&mut self, row: ShelfRow) {
        self.show_row(row);
    }

    /// **停止**：回到曲首并停下 —— 磁带的 ■ 是"退出来"，不是"暂停在原地"。
    ///
    /// ⚠️ 播放器**没有 stop**（`music::Player` 有 `open` / `play` / `pause` / `seek`，没有 stop），
    /// 所以"回曲首"只能是**重新 open 同一轨**，再把刚起的播放按停 —— `play_track` 就是
    /// 干"重新 open"这件事的，复用它比在别处再写一遍 open 稳。
    fn deck_stop(&mut self) {
        let track = self.track;
        if self.play_track(track) {
            self.player.pause();
            self.transport = slot_ui::lcd::Transport::Stopped;
            // ★ 2026-10-27：停止也是那颗琴键（磁带 ■ 是"退出来"，同样一声）。
            self.sfx = Some(Sfx::TapeTransport);
            self.sync_phase_playing();
        }
    }

    /// **播放模式轮换**（ONE / ONCE / REPEAT / SHUFFLE）—— 屏上走带区正中那个 `[词]` 就是它。
    ///
    /// 它原先长在 MODE 键的**单击**上（双击切 EQ）；键排撤掉之后单击走这里，
    /// 双击那半交给 `START`（见 `apply` 的 `Phase::Listening`），比"一颗键两种点法"好记。
    fn deck_mode_next(&mut self) {
        self.mode = self.mode.next();
    }

    /// **信息页要印的字，拥有所有权。**由 [`Self::deck_info`] 造、由前端填进 `lcd::Info`
    ///（那个是借用的）。
    ///
    /// ★ 2026-10-06 重做：这一页**不再走 `plate::word_face`**（界面的 Plix 矢量字 +
    ///   `opening()` 底色 + **没有格网**），而是走**面板的每一个零件**：同一张 1-bit 帧、
    ///   同一圈 2px 墨框与左上浅边、同一套四阶色板、同一张格网、同一套点阵字体与同样两条
    ///   plan。详见 `slot_ui::lcd` 里"信息页"那一节。
    ///
    /// ⚠️ **"其它标签项有就显示、没有就一行都不占"是真的一行都不留**：标签表是容器给的，
    ///   而一张没有某项的盘留一行空白，只会让人以为"这一项是空的"而不是"这张盘没这个标签"。
    ///
    /// **空碟片（卡上没有 `Music/`）**：抬头给 `MENU_DISCS`（这地方的名字）、
    /// 读数给那句"这张卡上没有专辑"，**详情三行全空** —— 那时候屏上只有两行字和一条分隔线，
    /// 而这是真相（这张卡就没有专辑），不是一个没做出来的页面。
    pub fn deck_info(&self) -> DeckInfo {
        use slot_ui::lang::*;
        let keys = DECK_KEY_GUIDE;
        let Some(d) = self.deck_disc() else {
            return DeckInfo {
                disc: MENU_DISCS.to_string(),
                disc_line: INFO_NO_DISC.to_string(),
                track_line: String::new(),
                artist: String::new(),
                length_line: String::new(),
                tags: Vec::new(),
                keys,
            };
        };
        let (at, of) = self.deck_track();
        let t = d.tracks.get(at);
        // 曲名：容器说的就是它说的，**空则退回文件名** —— 与面板上歌名的取法同一个规则
        // （`deck_title_now`）；两处用不同的规则就会有一处显示另一个的文件名。
        let title = t.map(|t| {
            let s = t.title.trim();
            if s.is_empty() {
                t.stem.clone()
            } else {
                s.to_string()
            }
        });
        DeckInfo {
            disc: d.name.clone(),
            disc_line: format!(
                "{} {} · {}",
                INFO_TRACKS,
                d.tracks.len(),
                slot_ui::lang::disc_clock(d.duration)
            ),
            // 一碟一首歌时不印 `1/1` —— 那是在说"这张盘怎么被归档的"，
            // 而这一页要说的是"现在放的是哪一首"。与 `deck_track` 那两处的
            // `(of > 1)` 同一个判据。
            track_line: match (&title, of) {
                (Some(t), n) if n > 1 => format!("{}/{}  {}", at, n, t),
                (Some(t), _) => t.clone(),
                (None, _) => String::new(),
            },
            artist: t.map(|t| t.artist.trim().to_string()).unwrap_or_default(),
            length_line: match t.filter(|t| t.duration > 0.0) {
                Some(t) => format!("{} {}", INFO_LENGTH, slot_ui::lang::disc_clock(t.duration)),
                None => String::new(),
            },
            // ★ **只留有值的**：容器里那个键存在而值为空白的项，在这里就没有位置 ——
            //   合成那一侧也就不会为一个空值占一行。
            tags: t
                .map(|t| {
                    t.tags
                        .iter()
                        .filter(|(_, v)| !v.trim().is_empty())
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            keys,
        }
    }

    /// The equaliser preset the panel should print, and the audio thread should play. Read by
    /// `sync_lcd_panel` to set `Content::eq`, and by `set_eq` to push the change to the deck.
    pub fn deck_eq(&self) -> slot_ui::eq::Eq {
        self.eq
    }

    /// Cycle to the next equaliser preset and push it to the deck's audio thread. Called by the
    /// key handler on a double-tap of MODE; the deck picks it up on the next decoded block.
    pub fn cycle_eq(&mut self) {
        self.eq = self.eq.next();
        self.deck.set_eq(self.eq);
    }

    /// The transport stopped by itself — the track ran out. **Stops, it does not advance**:
    /// whether it goes on to the next track is the mode's business and `deck_advance`'s, and
    /// deciding it here as well would advance twice.
    pub fn on_track_ended(&mut self) {
        self.transport = match self.mode {
            slot_ui::lcd::Mode::One => slot_ui::lcd::Transport::Stopped,
            _ => slot_ui::lcd::Transport::Paused,
        };
    }

    /// The transport state to show, derived from the player so a state that cannot happen is
    /// never drawn: `playing` is read off the deck, everything else is ours.
    pub fn transport_now(&self) -> slot_ui::lcd::Transport {
        if self.deck.playing() {
            slot_ui::lcd::Transport::Playing
        } else if self.player.is_open() && self.transport == slot_ui::lcd::Transport::Playing {
            slot_ui::lcd::Transport::Paused
        } else {
            self.transport
        }
    }

    /// Mirror the transport into the phase's own `playing`, so the two cannot disagree.
    fn sync_phase_playing(&mut self) {
        // ★ 2026-11-02：修 OR bug。判据 = transport 在放 **或** 解码线程在放（两源取或，
        //   **不带相位旧值**）。假音频测试线程起不来时只有 transport 够快；换轨/自然放完
        //   靠 deck.playing 兜住。暂停后相位旧值挂着的 true 会让图标继续显示播放三角。
        let playing = self.transport == slot_ui::lcd::Transport::Playing
            || self.deck.playing();
        if let Phase::Listening { key, disc, .. } = self.phase {
            self.phase = Phase::Listening { key, playing, disc };
        }
    }

    pub fn set_discs(&mut self, discs: Vec<slot_media::Disc>) {
        self.discs = discs;
        if self.disc >= self.discs.len() {
            self.disc = 0;
        }
        // The track index is clamped by the same argument as the disc's, and for the same
        // reason: a card whose music was swapped under a running session must not leave the
        // transport pointing past the end of the cassette that is now in the deck.
        let len = self.discs.get(self.disc).map_or(0, |t| t.tracks.len());
        if self.track >= len {
            self.track = 0;
        }
        self.discs_dirty = false;
    }

    /// How far the shortcut card can be scrolled: how much taller than the panel its rows make
    /// it. Zero while the rows are not up yet, which is a card that does not move rather than
    /// one that scrolls into nothing.
    fn card_scroll_max(&self) -> f32 {
        let rows: f32 = self.shortcut_rows.iter().map(|(_, _, h)| *h as f32).sum();
        // The last row has to come to rest clear of the pinned hint's band, not under it, so the
        // room below the rows is the band's whole height. Falls back to `CARD_TOP` before the
        // hint is up, which is a card that does not move rather than one that scrolls into
        // nothing.
        let foot = self
            .shortcut_hint
            .map(|(_, _, h)| h as f32)
            .unwrap_or(CARD_TOP);
        let end = CARD_TOP + rows + foot;
        (end - OUT_H as f32).max(0.0)
    }

    /// Move the menu's caret by `(dx, dy)` slots. Landing on an empty slot is `menu::step`'s
    /// business, not this one's: the grid's shape belongs to the crate that draws it.
    fn menu_to(&mut self, index: usize, dx: isize, dy: isize) {
        self.phase = Phase::Menu {
            index: slot_ui::menu::step(index, dx, dy),
        };
    }

    /// Open what the button in hand is for.
    fn menu_open(&mut self, index: usize) {
        match slot_ui::menu::Item::GRID.get(index).copied().flatten() {
            Some(slot_ui::menu::Item::Shortcuts) => {
                self.phase = Phase::Shortcuts {
                    scroll: 0.0,
                    want: 0.0,
                }
            }
            // The disc deck. Reached, and drawn — see `Listening` above. **This is the only
            // button in the menu that opens a screen that is not built:** the deck shows a disc
            // and its transport, and pressing play does not yet start anything. Left here rather
            // than removed because the menu is what it is for, and an absent `Discs` would read
            // as the feature having been dropped rather than as it not having arrived.
            //
            // The walk is not asked for here: `discs_dirty` is already true until it happens, so
            // this route and the `open.txt` one are the same route as far as the walk is
            // concerned.
            Some(slot_ui::menu::Item::Discs) => {
                // **The player opens on the disc row, not the deck.** The row is the shelf of
                // albums and the deck is one album out of it playing, so the menu walks in where
                // the deck's own MENU key walks out to — see `Phase::Listening`'s `OpenMenu`,
                // which drops the user back on this same row. Picking an album there opens the
                // deck; that is what A means on this row (`insert`).
                //
                // A card with no `Music/` leaves the game row up rather than opening an empty
                // one: `show_disc_row` is a no-op with nothing to show, which is the honest
                // answer rather than a screen with no sleeves on it.
                //
                // ★ 2026-11-02（六六：「不能直接在按按钮的时候震动不离开 MENU 页吗？」）：
                //   没碟就**原地抖一下**（菜单页走 `screen_shake`，见那里），留在菜单——
                //   按的这颗按钮没成，画面就该停在原地告诉用户，而不是把他退回货架。
                // ★★ 2026-11-09（玩家反馈「播放器进不去，卡上明明放了音乐」）：**先扫，再判空。**
                //   详见主树同一处注释 —— `self.discs` 开机是空的，只有 `build_disc_row`
                //   （要 `show_row`）和 `sync_deck`（要 `is_listening()`）会填；2026-11-02 的
                //   `return` 让判空跑在扫描前面，于是第一次进菜单永远"没碟"，而那条能填
                //   `discs` 的路在 `return` 后面 ⇒ 播放器从此打不开。
                //   ⚠️ 直接写 `self.discs`、不走 `set_discs`：后者会清 `discs_dirty`，而歌词 /
                //   封面那两趟要等进卡座时 `sync_deck` 再走一遍。与 `build_disc_row` 同写法。
                if self.discs.is_empty() {
                    if let Some(root) = self.root.clone() {
                        self.discs = slot_media::discs(&root);
                    }
                }
                if self.discs.is_empty() {
                    self.refuse();
                    return;
                }
                self.show_row(ShelfRow::Discs);
                self.phase = Phase::Shelf;
            }
            // ★ **设置**（2026-10-10 六六）。它原本是四格版式里最后填上的一格；2026-10-21
            //   阅读器下线后菜单是三颗（上排两颗 + 播放器整行），这一项仍在上排右侧。
            //   这一项是**填格子**而不是加格子（见 `menu::Item::GRID` 的注释）。
            //   不落在货架上——它开的是一张自己的屏，和 `Shortcuts` 同一种走法。
            Some(slot_ui::menu::Item::Settings) => {
                self.show_settings(slot_ui::settings::Row::ALL[0]);
            }
            None => {}
        }
    }

    // ── The settings screen ─────────────────────────────────────────────────────────────────
    //
    // A list of rows with values on them rather than a grid of buttons: a button answers "where
    // does this go" and a row answers "what is this set to". Four rows today — the two speeds,
    // the clock and the lid timer — and it is a **list** rather than a redesign when a fifth
    // arrives, which is exactly why 六六 asked for the scroll on the day the third was written.
    //
    // ★ **Four rows, a window that holds three: the list scrolls** (2026-10-10). The offset is
    // *derived* from the row in hand — `settings::scroll_for` — rather than kept beside it, and
    // the phase carries only the settled value so the ease has somewhere to start. The
    // arithmetic, the window and the two compiler guards are in `slot_ui::settings`.
    //
    // Unlike the menu, this screen has no accessor for "which row is in hand": the menu needs one
    // because its buttons are baked from a background pump, while these rows are baked with the
    // rest of the fixed furniture in `upload_late_furniture`, under the same no-gate rule the
    // shortcut card is. `Phase::Settings { row, scroll }` is matched where it is drawn, where it
    // is driven and where it is ticked, and that is the whole of it.

    /// Open the settings screen on `row`, with the window **already** where that row is.
    ///
    /// ★ **Every way in goes through here** (2026-10-10). A phase built by hand would have to
    /// remember to seed `scroll`, and the one site that forgot would be a list that visibly slid
    /// into place from wherever the offset happened to be left the last time — a jump with no
    /// press behind it, which reads as a dropped frame rather than as a screen opening.
    fn show_settings(&mut self, row: slot_ui::settings::Row) {
        self.phase = Phase::Settings {
            row,
            scroll: slot_ui::settings::scroll_for(row.index()),
        };
    }

    /// Which value a row is showing, as an index into `settings::values` — the screen prints the
    /// list and this says where in it the card currently is.
    fn settings_value(&self, row: slot_ui::settings::Row) -> usize {
        match row {
            slot_ui::settings::Row::FastForward => slot_store::FF_SPEED_CHOICES
                .iter()
                .position(|n| *n == self.state.ff_speed)
                .unwrap_or(0),
            slot_ui::settings::Row::Rewind => slot_store::REWIND_SPEED_CHOICES
                .iter()
                .position(|n| *n == self.state.rewind_speed)
                .unwrap_or(0),
            slot_ui::settings::Row::Sleep if self.baseos_sleep.is_some() => 0,
            slot_ui::settings::Row::Sleep => slot_store::DOZE_MIN_CHOICES
                .iter()
                .position(|m| *m == self.state.doze_min)
                .unwrap_or(0),
            // Index 0 is the first value `settings::values` prints, which is the **on** word:
            // `true` is the card shown, so the negation is what turns the flag into a position in
            // a list whose head is "on".
            slot_ui::settings::Row::Sound => usize::from(!self.state.ff_sound),
            slot_ui::settings::Row::InsertResume => usize::from(!self.state.insert_resume),
            slot_ui::settings::Row::EjectSave => usize::from(!self.state.eject_save),
            slot_ui::settings::Row::DateTime => 0,
            // ★ **The refresh row has exactly one value now** (2026-10-28 六六：「选项去掉仅留
            // 描述名」) — the empty string, so `face` draws only the centred name. `values` prints
            // one word and this has to say 0; the old two-word "refresh / refreshed" receipt is
            // gone with `library_note`.
            slot_ui::settings::Row::Refresh => 0,
            // Index 0 is BaseOS's generated wordmark; custom BMPs begin at index 1.
            // Clamp against the complete list so removing a candidate while Slot is open
            // cannot make the settings row point outside it.
            slot_ui::settings::Row::Boot => self.boot_pick.min(self.bootlogos.len()),
            // ★ 2026-10-21：Wi-Fi 行两个词 —— 「未配置」(0) / 「已配置」(1)。读的是开机那次
            //   记下的卡上事实，配置浮层保存/删除时同步更新，所以屏上不会说谎。
            slot_ui::settings::Row::Wifi => usize::from(self.wifi_conf.is_some()),
        }
    }

    /// Where `A` lands in a list of `n`. **Wrapped** (2026-10-28): `A` is the only key a value
    /// change gets now — the direction keys all move the caret — so a change has to be able to
    /// come back around; a clamped list whose only button advances would strand the caret at its
    /// far end. A two-word list (a switch) is the same thing with `n = 2`: `A` toggles.
    fn step_index(index: usize, n: usize, right: bool) -> usize {
        if n <= 1 {
            0
        } else if right {
            (index + 1) % n
        } else {
            (index + n - 1) % n
        }
    }

    /// Change whatever the row in hand is set to. Persists on every change, for the same reason
    /// the brightness and the volume do: a setting is the card remembering, and one that a quick
    /// power-off could lose is a setting the user cannot trust.
    ///
    /// ★ **2026-10-28：只有 `A` 调它，且总是 `right = true`** —— 方向键全给了光标移动，值只有
    ///   `A` 能改，所以 `step_index` 现在是环绕走（见那里）：一个两词的开关按一下翻转，一个
    ///   四档的列表按一下进一档、绕回开头。开门/执行行（`opens`）不走到这里，走 `settings_open`。
    fn settings_change(&mut self, row: slot_ui::settings::Row, right: bool) {
        match row {
            // ★ 2026-10-10 六六：「快进档位…统一」. The two speed rows are **one control** rather
            //   than two: same key, same step, same clamped ends, same persistence. They differ
            //   only in which list they walk, and that list is the card's.
            //
            //   Nothing is handed to the emu worker here. `Session::sync_speed` reads both gears
            //   off the card every frame, which is what makes the change land on the next hold of
            //   R2 with no second place to push from — see `App::ff_steps`.
            slot_ui::settings::Row::FastForward => {
                let at = self.settings_value(row);
                let to = Self::step_index(at, slot_store::FF_SPEED_CHOICES.len(), right);
                if to == at {
                    return;
                }
                self.state.ff_speed = slot_store::FF_SPEED_CHOICES[to];
            }
            slot_ui::settings::Row::Rewind => {
                let at = self.settings_value(row);
                let to = Self::step_index(at, slot_store::REWIND_SPEED_CHOICES.len(), right);
                if to == at {
                    return;
                }
                self.state.rewind_speed = slot_store::REWIND_SPEED_CHOICES[to];
            }
            // ★ 2026-10-16：第二个开关行，和标识同一套走法：两个值、钳住、落到卡上。
            //   没有“立刻生效”要交接的东西：声音是 worker 每帧从卡上重新读的（`Session::sync_speed` → `EmuHandle::set_ff_sound`），
            //   和两个倍速档一样，下一次按住 R2 / L2 就是新行为。
            slot_ui::settings::Row::Sound => {
                let at = self.settings_value(row);
                let to = Self::step_index(at, 2, right);
                if to == at {
                    return;
                }
                self.state.ff_sound = to == 0;
            }
            // ★ 2026-10-28：两个新开关，和声音 / 标识同一套走法：两值、钳住、落到卡上。
            //   没有立刻生效要交接的东西——insert_resume 在下次插卡 / 开机读档时生效，
            //   eject_save 在下一次拔卡写档时生效，都不用推给正在跑的会话。
            slot_ui::settings::Row::InsertResume => {
                let at = self.settings_value(row);
                let to = Self::step_index(at, 2, right);
                if to == at {
                    return;
                }
                self.state.insert_resume = to == 0;
            }
            slot_ui::settings::Row::EjectSave => {
                let at = self.settings_value(row);
                let to = Self::step_index(at, 2, right);
                if to == at {
                    return;
                }
                self.state.eject_save = to == 0;
            }
            // ★ 2026-10-23 删除（六六：「美术资源…已经迭代不需要这功能了」）：这一行连同
            //   `ArtSource` 开关一起退出设置页。`art_source` 状态与 `set_art_source` 调用
            //   仍保留（卡带脸的烘焙来源还在用），只是屏上不再有这一行。
            slot_ui::settings::Row::Sleep if self.baseos_sleep.is_some() => return,
            slot_ui::settings::Row::Sleep => {
                let at = self.settings_value(row);
                let to = Self::step_index(at, slot_store::DOZE_MIN_CHOICES.len(), right);
                if to == at {
                    return;
                }
                self.state.doze_min = slot_store::DOZE_MIN_CHOICES[to];
                // Handed over now rather than at the next boot, so the number on the screen is
                // the number the device will actually wait. Takes effect on the next doze — see
                // `Power::set_timeout`. Measured before the borrow of `power` is taken: `timeout`
                // reads the card and `power` is a field on the same `self`. `None` (「仅息屏」)
                // is handed over as `None` too — that is the whole of "the timer is off".
                let timeout = self.doze_timeout();
                if let Some(power) = &mut self.power {
                    power.set_timeout(timeout);
                }
            }
            // Nothing to change: this row is a door, and `←`/`→` on a door does nothing rather
            // than pretending to.
            slot_ui::settings::Row::DateTime => return,
            // ★ The second door, and the same rule (2026-10-17). It differs from the clock's in
            // that its *second* value exists — but that value is the receipt for a press, not a
            // state to be on, so walking onto it with `→` would be walking onto a past tense.
            slot_ui::settings::Row::Refresh => return,
            // A cycles between BaseOS's generated wordmark and the card's custom BMPs.
            // Selecting a custom BMP writes it through BaseOS's mounted BOOT volume; selecting
            // the generated default clears the persistent custom-selection record.
            // BaseOS default is index 0; custom BMPs follow it. A advances
            // through the complete list and commits immediately.
            slot_ui::settings::Row::Boot => {
                let at = self.settings_value(row);
                let to = Self::step_index(at, self.bootlogos.len() + 1, right);
                if to == at {
                    return;
                }
                self.boot_pick = to;
                self.apply_boot();
                return;
            }
            // Wi-Fi 也是同一道门：值在浮层里（扫描/录入/保存），`A` 打开，不在这里改。
            slot_ui::settings::Row::Wifi => return,
        }
        self.persist();
    }

    /// Apply the selected boot artwork through the BaseOS contract. Index 0
    /// clears the custom selection and restores the generated BaseOS wordmark;
    /// indices 1..=N install the corresponding 640x480 BMP for the next boot.
    fn apply_boot(&mut self) {
        let at = self.settings_value(slot_ui::settings::Row::Boot);
        if at == self.boot_index {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };

        let result = if at == 0 {
            crate::bootlogo::reset(&root)
        } else if let Some(entry) = self.bootlogos.get(at - 1) {
            crate::bootlogo::apply(&root, entry)
        } else {
            Err(crate::bootlogo::Failure::NotAPicture)
        };

        match result {
            Ok(()) => {
                self.boot_index = at;
                self.boot_pick = at;
                self.toast_now(slot_ui::Toast::BootSet);
            }
            Err(why) => {
                eprintln!("slot: bootlogo: {why:?}");
                self.boot_pick = self.boot_index;
                self.toast_now(slot_ui::Toast::BootFailed);
            }
        }
    }

    /// ★ **`B` on the settings screen: leave it** (2026-10-20). Back to the menu — one level,
    /// because that is the screen that opened this one.
    ///
    /// ★ **2026-10-28：开机画面不再有"浏览未写"的中间态** —— `A` 一步一换、换到就写
    ///   （`settings_change` 的 Boot 臂），所以离开时没有 `boot_pick` 要拨回；失败那一下
    ///   `apply_boot` 自己就把 `boot_pick` 放回 `boot_index` 了。这里就只是回菜单。
    fn settings_back(&mut self, _row: slot_ui::settings::Row) {
        self.phase = Phase::Menu {
            index: slot_ui::menu::first_slot(),
        };
    }

    /// What `A` does on the row in hand. Only a row that `opens()` ever reaches here — the clock
    /// picker, the refresh, the Wi-Fi form.
    ///
    /// ★ **2026-10-28：开机画面与键位映射都不再从这里走。** 开机画面退成了普通列表（`A`
    ///   一步一换、换到就写，在 `settings_change` 的 Boot 臂）；键位映射那一行随设置删掉了
    ///   （游戏里另有入口）。所以 `settings_open` 只剩三道门。
    fn settings_open(&mut self, row: slot_ui::settings::Row) {
        match row {
            slot_ui::settings::Row::DateTime => {
                // Seeded from utc **and the offset already on the card**, so opening it and
                // confirming it without touching anything is a no-op rather than an accidental
                // change of timezone. See `ClockPicker::local`.
                let utc = self.power.as_ref().map_or_else(system_secs, |p| p.now());
                self.phase = Phase::SetClock {
                    picker: ClockPicker::local(utc, i64::from(self.state.utc_offset_min)),
                    from_menu: true,
                };
            }
            slot_ui::settings::Row::Refresh => self.refresh_library(),
            // ★ 2026-10-21：**打开 Wi-Fi 配置浮层**。里面三视图：当前状态 /
            //   扫描列表 / 录入网格；保存直接写卡上的 `System/wifi.conf`。
            slot_ui::settings::Row::Wifi => self.open_wifi(),
            _ => {}
        }
    }

    /// **Read the card again, now** — what `A` does on the refresh row (2026-10-17).
    ///
    /// The scan itself is `slot_store::rescan`, which is deliberately *not* the boot's
    /// `scan_cached`: a boot is allowed to be told "nothing changed", and this is the one caller
    /// that must never be. See that function for why the fast path would make the press look like
    /// a key that did nothing.
    ///
    /// What happens on this side is the shelf swap, and the order matters:
    ///
    /// 1. **The old faces come off the row first**, because only the frontend owns the compositor
    ///    and the handles have to be gathered while they can still be lined up with the carts that
    ///    were showing them. `Shelf::replace_library` hands them back; they wait in
    ///    `stale_faces` until the next frame releases them.
    /// 2. **The library row is rebuilt in place.** Not a new `Shelf` — that would lose which
    ///    machine was up, the mouth and the size, all of which are the caller's decisions. The
    ///    row the user is *on* is the one being replaced, which on this screen is always the game
    ///    row: the settings screen is only reachable from the menu, and the menu's other rows are
    ///    not shelves of roms.
    /// 3. **A failed scan changes nothing.** The card is the only thing here that can fail, and a
    ///    half-swapped shelf would be worse than a stale one — so the read happens into a `Vec`
    ///    first and only a `Ok` reaches the row. The error goes on the card's log, which is the
    ///    only place a device with no console can say anything.
    /// 4. **`library_dirty` goes up last**, because the resident window is the frontend's to
    ///    rebuild — the same handover `shelf_dirty` makes for a machine swap, and it is taken in
    ///    the same place.
    ///
    /// ★ 2026-10-28：刷新行已无「已刷新」回执词（`Refresh` 不再有值），所以这里也**没有**要
    ///   摆回执的 `library_note` 了 —— `A` 就是做，做完了该换的库已经换了。
    fn refresh_library(&mut self) {
        self.search = None;
        self.search_origin = None;
        let Some(root) = self.root.clone() else {
            return;
        };
        let carts = match rescan(&root) {
            Ok(carts) => carts,
            Err(e) => {
                // The shelf is left exactly as it was. A device with no console says this on the
                // card, beside the boot log, and nowhere else.
                eprintln!("slot: refresh: {e}");
                return;
            }
        };
        // Where the caret was, by name: the list is about to be rebuilt from scratch and an index
        // into the old one would point at whatever happens to be there now.
        let keep = self
            .shelf
            .current()
            .and_then(|i| self.shelf.carts.get(i))
            .map(|c| c.stem.clone());
        let stale = self.shelf.replace_library(carts, keep.as_deref());
        self.stale_faces.extend(stale);
        // ★★ 2026-11-07（六六：「刷新 ROM 后货架游戏跨机种混乱」）：**刷新换的是库，不是这一行。**
        //
        // `Shelf::replace_library` 按它的契约把 `view` 留在**整个库**上 —— 那是这次扫描的
        // 结果，不是货架该显示的东西。这一行从来只放一个机种的卡：`show_system`（换机种）、
        // `cycle_folder`（换子文件夹）、`show_all` / `show_favorites`（收藏夹）每一次换视图都
        // 重建它，**唯独刷新这条路没有重建**。于是刷完一次，GBA 的壳子里就排进了 PS1 的碟和
        // GB 的竖卡，九百来张一起上场，要等长按 Y 切一次机种才自愈 —— 因为那条路会重建。
        //
        // 补的就是这条漏掉的重建：刷新前是哪张视图（某机种整排 / 该机种的收藏夹）就还回哪张。
        let size_before = self.shelf.size();
        let view = self.row_view_after_reload();
        self.shelf.set_view(view, keep.as_deref());
        // 行框（侧位卡的影子、未构图的空卡）是按**这一行的尺寸**烘的。同机种同目录时尺寸
        // 不变，只有真的变了才让前端重烘 —— 这条路径上几乎永远不触发，但漏了就是旧框。
        let size_now = self.shelf.size();
        if (size_before.w, size_before.h) != (size_now.w, size_now.h) {
            self.shelf_dirty = true;
        }
        // ★ **The tally is the game row's, whatever row is up.** `retally_letters` counts the row
        // in hand, and this screen can be reached with the disc row behind it — so asking
        // it here would count the wrong list and leave the game shelf's dial wrong until the next
        // machine switch. The row is read directly instead: it is the one that was just rebuilt.
        let visible: Vec<Cart> = self.shelf.visible().into_iter().cloned().collect();
        self.letter_counts = tally_letters(&visible);
        // The letter *faces* are per-wheel-slot and do not change; only the tally does.
        self.library_dirty = true;
    }

    /// How long a shut lid waits before the machine stops, as the card asked for it — or `None`
    /// when the timer is switched off (`doze_min == 0`, the screen's 「仅息屏」 choice).
    ///
    /// **The one place the minutes become a `Duration`**, so the boot, the setting and the hand
    /// over to `Power` cannot disagree about what "five" means.
    ///
    /// ⚠️ **`None` is not "a zero-second wait"** — a `Duration::ZERO` would make `doze_expired`
    /// true on the very next frame, so a lid that shut would power the device off at once. The
    /// choice removes the deadline rather than shortening it, which is why this is an `Option`
    /// and not a number every caller has to remember to check.
    pub fn uses_baseos_sleep(&self) -> bool {
        self.baseos_sleep.is_some()
    }

    pub fn doze_timeout(&self) -> Option<std::time::Duration> {
        if self.baseos_sleep.is_some() {
            return None;
        }
        (self.state.doze_min != 0)
            .then(|| std::time::Duration::from_secs(u64::from(self.state.doze_min) * 60))
    }

    /// How many core frames one present runs while `R2` is held, as the card has it.
    ///
    /// Read by `Session::sync_speed` every frame rather than pushed on the keypress: the emu
    /// worker is not reachable from this screen, and a push that happened only from the settings
    /// screen would leave the first session after a boot — the one where nobody opens a screen —
    /// running whatever the worker defaults to.
    pub fn ff_steps(&self) -> u8 {
        self.state.ff_speed
    }

    /// How many snapshots one present pops while `L2` is held, as the card has it. The worker
    /// divides it by `emu::SNAPSHOT_EVERY`; this side never converts.
    pub fn rewind_speed(&self) -> u8 {
        self.state.rewind_speed
    }

    /// Whether fast forward and rewind keep playing the game's audio, as the card has it.
    ///
    /// Read by `Session::sync_speed` every frame rather than pushed on the keypress, the same
    /// bargain the two gears make: the worker is not reachable from the settings screen, and a
    /// push that happened only there would leave the first session after a boot — the one
    /// where nobody opens a screen — silent no matter what the card says.
    pub fn ff_sound(&self) -> bool {
        self.state.ff_sound
    }

    /// Whether seating a cart (or booting with one in the slot) auto-loads the resume snapshot.
    ///
    /// Read by `Session` at cart start rather than pushed on the keypress: the worker is not
    /// reachable from the settings screen, and a push there would leave the first session after a
    /// boot — the one where nobody opens a screen — un-reading its resume no matter what the card
    /// says. Same bargain the gears and the sound make.
    pub fn insert_resume(&self) -> bool {
        self.state.insert_resume
    }

    /// Whether ejecting a cart writes the resume snapshot. The native `.sav` is written on eject
    /// regardless — see `App::flush_eject` — and lid / power / autosave are outside this switch.
    pub fn eject_save(&self) -> bool {
        self.state.eject_save
    }

    /// The card's operation keymap, as the session rebuilds the pad: one `(physical, target)`
    /// name pair per customised key, then the turbo rate on the foot row. A card that has
    /// never opened the sheet carries an empty table and the default rate, which is the whole
    /// of `Remap::identity` — so a first boot reads back exactly the buttons the game always
    /// had. Read by `Session::sync_keymap` every frame rather than pushed on the keypress, the
    /// same bargain the two gears make: the pad is not reachable from the menu, and a change
    /// that only reached the card would leave a running game on the binding it started with.
    pub fn keymap_snapshot(&self) -> (Vec<(String, String)>, u8) {
        // ★★ 2026-10-26：**六把键永远齐全地给出去**，缺的那几行由本机种的出厂值补上。
        //
        // 为什么不把"没写的那行"留给 `Remap::from_keymap` 去兜底：那张表的兜底是
        // `keys::stock`（= Advance 读法），而这里要知道的是**机种**。补齐在这一个地方做，
        // `mask.rs` 一行不用改；而 Session 的比较键也因此天生带着机种 —— 从 GBA 换到 PS1 时
        // 六行内容不同，Pad 自然重建。
        let system = self.current_machine();
        let mut map = self.keymap_effective(system);
        for &p in slot_ui::keys::physical_for(system) {
            let name = slot_ui::keys::name(p);
            let target = slot_ui::current_for(system, p, &map).name.to_string();
            map.insert(name.to_string(), target);
        }
        (map.into_iter().collect(), self.state.turbo_hz)
    }

    /// ★★ 2026-10-26：**这台机种看到的那张操作键表。**
    ///
    /// 旧卡的全局表（`state.keymap`）先铺底，本机种自己那份（`state.keymaps[<SYS>]`）逐键覆盖。
    /// 一张从 10-21 用到现在、只写过 `keymap_Y=GAME_A` 的卡，读出来仍然是"每个机种都是 Y=A"，
    /// 直到某个机种被单独改过 —— 升级二进制不该把玩家的键位翻掉。
    pub fn keymap_effective(
        &self,
        system: System,
    ) -> std::collections::BTreeMap<String, String> {
        let mut map = self.state.keymap.clone();
        if let Some(own) = self.state.keymaps.get(system.dir_name()) {
            for (k, v) in own {
                map.insert(k.clone(), v.clone());
            }
        }
        map
    }

    pub fn set_clock_faces(&mut self, line: TexId, hint: TexId) {
        self.clock_faces = Some((line, hint));
    }

    /// The blank cart a cart whose own face is not built yet is drawn as. Uploaded at boot:
    /// it is the same for every cart and never changes — except when the row's machine does,
    /// which is why this hands back what it replaced.
    pub fn set_cart_placeholder(&mut self, face: TexId) -> Option<TexId> {
        self.row_mut().set_placeholder(face)
    }

    /// Hands back the texture it replaced: the default wallpaper is re-inked on every theme
    /// change, and a face nobody releases is a leak that grows with the number of themes the
    /// user tries.
    pub fn set_wallpaper(&mut self, face: TexId) -> Option<TexId> {
        self.wallpaper.replace(face)
    }

    /// 默认壁纸那圈柔光。`None` = 屏上是卡自己的 `Wallpapers/` 图（不发光）。
    pub fn set_wall_glow(&mut self, glow: Option<(TexId, u32, u32)>) {
        self.wall_glow = glow;
    }

    pub fn set_desktop(&mut self, face: TexId) {
        self.desktop = Some(face);
    }

    pub fn set_bolt_face(&mut self, bolt: TexId) {
        self.bolt = Some(bolt);
    }

    pub fn set_battery_percent_face(&mut self, face: TexId, w: u32) {
        self.battery_percent = slot_ui::Printed::new(face, w);
    }

    pub fn set_shelf_clock_face(&mut self, face: TexId, w: u32) {
        self.shelf_clock = slot_ui::Printed::new(face, w);
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// Whether the deck is the screen in front of the user. A named question rather than a
    /// `matches!` at each of the several places that ask it, so the deck's own rules live in one
    /// match and cannot drift between the drawing, the walking and the input.
    pub fn is_listening(&self) -> bool {
        matches!(self.phase, Phase::Listening { .. })
    }

    /// **Whether the screen on show draws with the panel's two faces.**
    ///
    /// The deck and the info page are the only two, and this is the frontend's cue to read those
    /// faces off the card — **they are 12.75 MB and nothing on the shelf uses them**, so they are
    /// read when the first screen that needs them comes up rather than at boot. See
    /// `Frontend::ensure_panel_fonts` for the measurement behind that.
    pub fn needs_panel_fonts(&self) -> bool {
        matches!(self.phase, Phase::Listening { .. } | Phase::DiscInfo)
    }

    /// 信息页是不是在屏上。给前端决定"这一帧要不要烘那一页"——
    /// 不在屏上就**别烘**：那一页要合成一整张 240×160 的 1-bit 帧（38 400 像素
    /// 逐像素阈值化 + 上采样到 720×480），而它一天里绝大多数帧都不该存在。
    pub fn is_disc_info(&self) -> bool {
        matches!(self.phase, Phase::DiscInfo)
    }

    /// 信息页现在在第几页。**0 = 碟片与曲目，1 = 键位指南**（`lcd::INFO_PAGES` = 2）。
    ///
    /// **前端只读**：它合成时要挑这一页。而翻页走 [`Self::step_info_page`]，
    /// 于是"翻到第几页"与"第几页要重烘"是**同一条路径上的两件事**，
    /// 不会各自跑偏（那种偏的症状是"按了 ← 数字变了图没变"）。
    pub fn info_page(&self) -> usize {
        self.info_page
    }

    /// 信息页翻页。**环绕** —— 底下一行常驻写着 `n/2 ←→ 翻页`，
    /// 而一个翻到头就停住的翻页会让"再按一次"变成无效键。
    pub fn step_info_page(&mut self, dir: i32) {
        let n = self.deck_info().pages() as i32;
        self.info_page = (((self.info_page as i32 + dir) % n + n) % n) as usize;
    }

    /// 信息页那一整帧，前端合成好传进来。**`None` = 还没烘**，那一帧屏上只有底色。
    pub fn set_info_panel(&mut self, panel: Option<(TexId, u32, u32)>) {
        self.info_panel = panel;
    }

    pub fn carts(&self) -> &[Cart] {
        &self.row().carts
    }

    /// 名字反查一张卡，**先问当前货架的机种**（见 `Shelf::find`）。
    ///
    /// ★ 2026-11-09：凡"手上只有一个 stem"的地方都该走这里，不要再
    /// `carts().iter().find(|c| c.stem == stem)` —— 那是在全库里取**排序靠前**的那条，
    /// 而同名跨机种是常态（`Games/FC/魂斗罗.nes` 与 `Games/MD/魂斗罗.md`，`F` < `M`
    /// ⇒ 永远先命中 FC）。玩家报的「加载 MD 游戏时显示的是 FC 卡带且进去也是 FC」
    /// 就是这条路径串的：`insert` 把选中的 cart 压成 stem 之后，插卡动画与核心启动
    /// 都只能靠名字反查。
    pub fn cart_by_stem(&self, stem: &str) -> Option<&Cart> {
        self.shelf.find(stem).map(|(c, _)| c)
    }

    /// Which cart the caret is on. Read by the boot path to decide which faces are worth
    /// rasterising before the first frame, and by the background filler to order the rest.
    pub fn shelf_index(&self) -> usize {
        self.row().current().unwrap_or(0)
    }

    /// The carts the row is drawing, as library indices — what a build has to cover, as opposed
    /// to a contiguous run of indices around the caret. See `Shelf::draw_window`.
    pub fn shelf_draw_window(&self) -> Vec<usize> {
        self.row().draw_window()
    }

    /// How many carts the shelf's view holds, as opposed to how many are on the card.
    pub fn shelf_view_len(&self) -> usize {
        self.row().view_len()
    }

    /// How far a cart is from the caret that would be showing it — the row on screen when the
    /// view holds it, otherwise the front of its own machine's shelf, where a switch lands.
    /// What the resident window and the filler both measure in. See `Shelf::shelf_distance`.
    pub fn shelf_distance(&self, cart: usize) -> Option<usize> {
        self.row().shelf_distance(cart)
    }

    /// The cart index in each slot the row would draw, in slot order — the draw's own answer to
    /// "which cart is this", read back for the boot log rather than recomputed.
    pub fn shelf_slot_carts(&self) -> Vec<usize> {
        self.shelf
            .slot_rects(None, 0.0, 0.0)
            .iter()
            .map(|r| r.cart)
            .collect()
    }

    /// Exactly one cart on the card. The shelf is unreachable and eject is refused.
    pub fn single_cart(&self) -> bool {
        self.row().carts.len() == 1
    }

    /// Face textures in `carts` order; `None` where the face is still being rasterised.
    /// Only the compositor can mint a `TexId`.
    pub fn set_faces(&mut self, faces: Vec<Option<TexId>>) {
        self.row_mut().set_faces(faces);
    }

    /// One face, uploaded after `set_faces`, as the background filler finishes it.
    pub fn set_face(&mut self, i: usize, tex: TexId) {
        self.row_mut().set_face(i, Some(tex));
    }

    /// Forgets a face whose texture has been released.
    pub fn clear_face(&mut self, i: usize) {
        self.row_mut().clear_face(i);
    }

    /// Which face texture a cart is currently holding, if any.
    pub fn face_of(&self, i: usize) -> Option<TexId> {
        self.row().face_of(i)
    }

    /// Drop the overlay the previous cart left on screen, for a cart whose system has no overlay
    /// set (GBA) or a card carrying no art for its system. `overlay_pixels` becomes `None` and
    /// `overlay_dirty` is set, so the frontend releases the GL texture and the built-in look
    /// stands. Used in place of `set_overlay_set` for GBA: GBA would otherwise be read as
    /// `slot` 0 (GB) and re-decode a Game Boy model overlay onto a GBA game.
    pub(crate) fn clear_overlay_set(&mut self) {
        self.overlay_pixels = None;
        self.overlay_dirty = true;
    }

    /// Hand the seated cart's overlay set to the app, from `Session::spawn_core`: every
    /// `Overlay/<sys>.png` / `<sys>-NN.png` for the cart's system, in rotation order (`slot` 0
    /// for GB, 1 for GBC), with the persisted index for that system applied. The chosen file is
    /// decoded here and `overlay_dirty` is set so the frontend re-mints the GL texture once.
    /// GBA has no overlay set at all and is routed through `clear_overlay_set` instead.
    pub(crate) fn set_overlay_set(&mut self, list: Vec<crate::root::OverlayFile>, slot: usize) {
        self.overlay_list = list;
        let slot = slot.min(crate::root::OVERLAY_RINGS - 1);
        self.overlay_slot = slot;
        let choices = Self::overlay_choices(slot, self.overlay_list.len());
        self.overlay_index = if choices == 0 {
            0
        } else {
            self.overlay_indices[slot].min(choices - 1)
        };
        self.refresh_overlay_pixels();
    }

    /// How many overlay choices a system has. A Game Boy's three models come first and the card's
    /// own overlays after them; a Game Boy Color offers each of its overlays once per grid state
    /// (without, white, black — `GBC_GRID_STATES` of them), so the mesh is switched by walking the
    /// ring rather than by a key of its own. The consoles ride the same ring as the Color — their
    /// scanlines are the mesh, at half and full rather than white and black.
    fn overlay_choices(slot: usize, ordinary: usize) -> usize {
        if slot == 0 {
            DisplayFilter::GB_MODEL_COUNT + ordinary
        } else if slot == 9 {
            // ★ 2026-10-29（640 树）：GBA 走 OVERLAY，但**不需要网格态**。
            //   槽 9 只有一个 ordinary 条目，不乘 GBC_GRID_STATES。
            ordinary
        } else {
            ordinary * DisplayFilter::GBC_GRID_STATES
        }
    }

    /// Which model a combined index names, if it names one.
    fn model_of_index(slot: usize, index: usize) -> Option<usize> {
        (slot == 0 && index < DisplayFilter::GB_MODEL_COUNT).then_some(index)
    }

    /// Where the card's *own* overlay for this index is, if the index is one of those rather
    /// than a model.
    fn ordinary_overlay_at(&self) -> Option<&crate::root::OverlayFile> {
        let at = if self.overlay_slot == 0 {
            self.overlay_index
                .saturating_sub(DisplayFilter::GB_MODEL_COUNT)
        } else if self.overlay_slot == 9 {
            // ★ 2026-10-29（640 树）：GBA 槽位没有网格态，索引直接映射到 overlay_list。
            self.overlay_index
        } else {
            // A Game Boy Color's ring carries each overlay's grid states in a row, so the overlay
            // is one entry in `GBC_GRID_STATES` — see `overlay_choices`.
            self.overlay_index / DisplayFilter::GBC_GRID_STATES
        };
        self.overlay_list.get(at)
    }

    /// Re-decode the chosen overlay file into `overlay_pixels` and mark the texture dirty.
    /// `None` pixels (`None` from a missing/corrupt file, or an empty list) is a real answer:
    /// the frontend reads it as "release the texture", so the built-in look stands.
    fn refresh_overlay_pixels(&mut self) {
        // A model's art is not in `overlay_list` — `overlay_files` cannot name it and should not
        // — so the model index is resolved to its own file here.
        self.overlay_pixels = match Self::model_of_index(self.overlay_slot, self.overlay_index) {
            Some(m) => self
                .root
                .clone()
                .and_then(|root| {
                    crate::root::model_overlay(
                        &root,
                        "gb",
                        crate::palettes::GB_MODELS[m.min(crate::palettes::GB_MODELS.len() - 1)].0,
                    )
                })
                .and_then(|path| slot_ui::decode_rgba(&path)),
            None => self
                .ordinary_overlay_at()
                .and_then(|overlay| slot_ui::decode_rgba(&overlay.path)),
        };
        self.overlay_dirty = true;
    }

    /// Whether the overlay up asks for the screen reflection — its file name carried the `x`
    /// marker. Read by the frontend to hand the compositor the overlay texture as the effect's
    /// zone mask; `None`/false leaves the game drawn exactly as before.
    pub(crate) fn overlay_reflects(&self) -> bool {
        // Every model mask is a reflection layer: that is what they were drawn as, and it is why
        // their names carry the same trailing `x` the ordinary ones use to say so.
        if Self::model_of_index(self.overlay_slot, self.overlay_index).is_some() {
            return true;
        }
        self.ordinary_overlay_at().is_some_and(|o| o.reflect)
    }

    /// The overlay texture the compositor minted, if any. The reflection samples its alpha as
    /// the zone, so the same texture is both the art and the effect's mask.
    pub(crate) fn overlay_tex(&self) -> Option<TexId> {
        self.overlay_tex
    }

    /// Step to the next overlay for the highlighted cart's system (the one in the slot while a
    /// game is playing). Reached from SELECT+X on a Game Boy and a Game Boy Color, and from
    /// SELECT+R2 as well on the Game Boy.
    ///
    /// A Game Boy Color's ring is **`GBC_GRID_STATES` times as long as its art**: every overlay is
    /// offered without the grid, with a white one, and with a black one, so the mesh is a set of
    /// further states of each overlay rather than a setting of its own. The choice is therefore an
    /// index into that ring, and both the art and the grid are read back out of it — see
    /// `ordinary_overlay_at` and `display_grid`.
    ///
    /// ★ 2026-10-22: the consoles ride the same ring — the Famicom and the Super Famicom have
    /// their own art and their own scanline states (off, half, full), and `overlay_ring` is
    /// where the slot and the file-name stem come from.
    ///
    /// A system with a single file has nothing to rotate, and an Advance has no overlay set at
    /// all; both are silent no-ops rather than a refusal, since the press is a step through a
    /// ring and a card may simply carry one image.
    fn overlay_next(&mut self) {
        let Some(system) = self.shelf.current_cart().map(|c| c.system()) else {
            return;
        };
        // ★ 2026-10-22: the consoles join the ring. `overlay_ring` is the one place that knows
        // which slot a machine owns and what its files are called; the Advance has no ring at
        // all, so it drops out here exactly as it always did.
        let Some((slot, which)) = crate::root::overlay_ring(system) else {
            return;
        };
        let Some(root) = self.root.clone() else {
            return;
        };
        let list = crate::root::overlay_files(&root, which);
        // A Game Boy's ring is its three models and then the card's own overlays; a Game Boy
        // Color's is just its overlays.
        let choices = Self::overlay_choices(slot, list.len());
        if choices <= 1 {
            return;
        }
        // Note the ring is never empty on a Game Boy even when the card carries no art at all:
        // `choices` counts the three models before anything in `Overlay/`.
        self.overlay_indices[slot] = (self.overlay_indices[slot] + 1) % choices;
        crate::root::write_overlay_indices(&root, self.overlay_indices);
        // Landing on a model adopts its palette — and nothing is written for that here, because
        // there is nothing to write: a model's four shades are not an entry in the table and are
        // not reachable from the palette key, so `gb_shades` reads them straight off the model
        // that is up. The palette the card holds is left exactly as it was, which is what makes
        // walking through the DMG and out again land back on the look that was chosen before it.
        // With the cart in the slot, swap the live set too so the change is on screen now.
        if matches!(self.phase, Phase::Playing { .. }) {
            self.overlay_list = list;
            self.overlay_slot = slot;
            self.overlay_index = self.overlay_indices[slot];
            self.refresh_overlay_pixels();
        }
    }

    /// `true` while the overlay texture needs (re)building. Read by the frontend's `sync_overlay`.
    pub(crate) fn overlay_dirty(&self) -> bool {
        self.overlay_dirty
    }

    /// The decoded overlay pixels, if any — read by `sync_overlay` to (re)build the texture.
    pub(crate) fn overlay_pixels(&self) -> Option<&(Vec<u8>, u32, u32)> {
        self.overlay_pixels.as_ref()
    }

    /// Drop the current overlay texture handle, returning it for the compositor to release.
    pub(crate) fn take_overlay_tex(&mut self) -> Option<TexId> {
        self.overlay_tex.take()
    }

    /// Store the freshly minted overlay texture handle (or `None` when there is no overlay).
    pub(crate) fn set_overlay_tex(&mut self, tex: Option<TexId>) {
        self.overlay_tex = tex;
    }

    /// Clear the dirty flag once `sync_overlay` has reconciled the texture with `overlay_pixels`.
    pub(crate) fn set_overlay_clean(&mut self) {
        self.overlay_dirty = false;
    }

    /// 卷闸是不是「在动」：落下还没到满闸、或者正在卷起回退。满闸之后停顿等用户指的那一段
    /// **不算在动**——否则机种面板的按键会被 `input_locked` 与 `apply` 两道门一起锁死
    /// （`open_sys_picker` 把 `shelf_switch` 设成 `Some`，而清掉它的 `settle_system_switch`
    /// 又必须靠面板的 A/B 输入触发，于是死锁：面板看得见，按键全无反应）。
    fn shelf_switch_moving(&self) -> bool {
        match &self.shelf_switch {
            Some(sw) => sw.up.is_some() || sw.t < slot_ui::SHUTTER_SWAP_AT_S,
            None => false,
        }
    }

    /// Whether the app is taking no input right now. True only while the shutter is actually
    /// *moving* — descending before it has covered the screen, or rolling back up after a
    /// choice. The full-shutter pause that waits for the user to point at a machine (the
    /// 2026-10-26 panel) is **not** locked: its keys must reach `App`.
    pub fn input_locked(&self) -> bool {
        self.shelf_switch_moving()
    }

    /// ★ 2026-10-22（六六）：机种 logo 高光区，画在卡座背层。按机种各一张，两边货架
    /// （游戏行 `shelf` / 碟片行 `disc_shelf`）都设 —— 前端上传时用
    /// `fill_logo_highlight` 把洋红填成当前主题的 `bright` 色，换主题时重传。
    pub fn set_slot_logo(&mut self, system: System, tex: Option<(TexId, u32, u32)>) {
        self.shelf.set_logo(system, tex);
        self.disc_shelf.set_logo(system, tex);
    }

    /// ★★ 2026-10-24（六六）：插卡位洞里那张机种背板。一张图一个机种，前端在开机（以及
    /// 换主题后的 `rebake`）时上传；重设同一个机种就替换那一张，不会累积。
    pub fn set_slot_back(&mut self, system: System, tex: Option<(TexId, u32, u32)>) {
        self.slot_back.retain(|(s, _)| *s != system);
        if let Some(t) = tex {
            self.slot_back.push((system, t));
        }
    }

    /// 卷闸上那行 CHANGE —— 字与它的柔光各一张。素材的洋红由前端换成本主题的发光色，
    /// 柔光也按同一个色烘，所以换主题两样一起重传。
    pub fn set_change_face(
        &mut self,
        glyph: Option<(TexId, u32, u32)>,
        glow: Option<(TexId, u32, u32)>,
    ) {
        self.change_face = glyph;
        self.change_glow = glow;
    }

    /// ★ 2026-10-26：插卡/插盘动画里货架区那行"GAME START LOADING…"（洋红已换成本主题色）
    /// 和它的柔光。前端在 `rebake` 里上传，换主题跟着重烘。
    pub fn set_loading_face(&mut self, face: Option<(TexId, u32, u32)>, glow: Option<(TexId, u32, u32)>) {
        self.loading_face = face;
        self.loading_glow = glow;
    }

    /// Both themes' disc badge. One call because the two are the same picture of the same word
    /// and the only reason they are separate at all is that a card may bake its own.
    pub fn set_disc_ind_faces(&mut self, dark: Option<(TexId, u32, u32)>, light: Option<(TexId, u32, u32)>) {
        self.disc_ind_faces = dark.into_iter().collect();
        self.disc_ind_light = light.into_iter().collect();
    }

    /// The library indices of one machine's carts, in library order — the view a system shelf
    /// shows. A card can carry all three machines at once, so this is how the row is narrowed
    /// to the one whose shelf is up.
    /// One machine's shelf, in the order it shows. Kept as a call rather than a field so the
    /// shelf stays the only thing that knows how a machine's list is built.
    fn system_view(&mut self, system: System) -> Vec<usize> {
        // Never empty: a machine whose games all sit inside a subfolder falls into that
        // subfolder rather than showing a blank row (`machine_view_or_first_folder`).
        self.shelf.machine_view_or_first_folder(system)
    }

    /// Put one machine's shelf up: rebuild the view to its carts, drop any favourites sub-view,
    /// park the caret at the front, and remember the choice on the card.
    fn show_system(&mut self, system: System) {
        self.shelf_system = system;
        // The row is laid out for whichever machine is up: an Advance cart is landscape and a
        // Game Boy cart portrait, so the geometry travels with the shelf rather than the app.
        self.shelf.set_system(system);
        // ...and so does the row's *frame*: the shadow under a dimmed cart and the blank cart a
        // not-yet-built face stands in as are both cut to one machine's shape, and they were cut
        // for the shelf this device came up on. Raised here and taken by the frontend, which is
        // the only party with a compositor.
        self.shelf_dirty = true;
        self.fav_view = false;
        let view = self.system_view(system);
        self.shelf.set_view(view, None);
        self.retally_letters();
        if let Some(root) = &self.root {
            crate::root::write_shelf_system(root, system);
        }
    }

    // ── the disc row ──────────────────────────────────────────────────────────────────────

    /// The row that is up. **Every reader goes through this**, which is the whole trick: the
    /// keyboard, the drawing and the frontend all ask "the row" rather than reaching for
    /// `shelf`, so putting a second row up is one assignment rather than a search for every
    /// caller that forgot.
    pub fn row(&self) -> &Shelf {
        match self.row {
            ShelfRow::Games => &self.shelf,
            ShelfRow::Discs => &self.disc_shelf,
        }
    }

    pub fn row_mut(&mut self) -> &mut Shelf {
        match self.row {
            ShelfRow::Games => &mut self.shelf,
            ShelfRow::Discs => &mut self.disc_shelf,
        }
    }

    /// Which row is the one on screen. What the face pipeline asks before it picks
    /// `cart_face` over `cd_face`.
    pub fn shelf_kind(&self) -> ShelfRow {
        self.row
    }

    /// Play the card's music through `device` — **the shelf's own output**, not a second one.
    ///
    /// ★ 2026-10-07 真机实锤：卡座的解码线程从前自己 `open_sink()`，而设备的分从设备是独占的
    /// （`plug:default` 落到裸 `hw:`），`Session::boot` 已经把它占住了 ⇒ 第二次 `open` 必然失败
    /// ⇒ 解码没有任何节流（5.1 秒解出 209.2 秒），而声卡上一个字节都没有。用户报的
    /// 「没声音 + 播放速度很快」就是这一条。**共享同一台设备**之后两条一起消失。
    pub fn attach_audio(&mut self, device: std::sync::Arc<crate::audio::Device>) {
        self.player.attach_audio(device);
    }

    /// The card's music as the row's elements.
    ///
    /// **A sleeve is not a `System`, and this is the adapter.** `System` answers "which machine"
    /// and the twenty-one places that read it all want a core, a save folder and a `States/`
    /// path — none of which a sleeve has. So a sleeve is presented to the row as a `Cart` whose
    /// rom is its own first track: `Cart::system()` reads the extension, `.mp3` is not a rom
    /// extension, and the answer (`Gba`) is never read back on a music row because the row's
    /// geometry comes from `set_size` and its face from `cd_face`. The `code` is empty so the
    /// shell stays the built-in one — a sleeve's colour is the card's art, not a game's.
    ///
    /// **The cover comes from `Music/Covers/` through the index**, not from `Labels/`. The label
    /// tree is the *cartridge* sticker convention and its keys are machine-scoped; album art is
    /// filed by album, and a card that keeps its art in one folder can add a sleeve without
    /// knowing the frontend exists. `Labels/` stays as the second choice below, for a card that
    /// was filled the earlier way.
    fn build_disc_row(&mut self) {
        // **The row is built from `self.discs`, not from the card.** The scan is a convenience
        // for the case where nothing has walked the music folder yet, and the label tree is a
        // fallback for a card that filed its covers the earlier way; neither is needed to *draw*
        // the row, so a caller that already holds the discs — a probe, a test — gets a row rather
        // than an empty shelf. A card, when there is one, is asked for both.
        if self.discs.is_empty() {
            if let Some(root) = self.root.clone() {
                self.discs = slot_media::discs(&root);
            }
        }
        let carts: Vec<Cart> = self
            .discs
            .iter()
            .map(|t| {
                let rom = t
                    .tracks
                    .first()
                    .map(|x| x.path.clone())
                    .unwrap_or_else(|| std::path::PathBuf::from(&t.stem));
                // ★★ 2026-10-28（六六）：「4 你重定位到 Music/Covers」—— 封面只有一条路：
                //   `Music/Covers/`（外加碟片自己那个文件夹，见 `Covers::find`）。原先还兜一层
                //   `Labels/Disc/`、`Labels/Music/`、`Labels/<stem>.png`，那是 Labels 体系的东西，
                //   整套已经取消；何况封面与光盘盒现在都住 `Music/Covers/`，没有第二处要找。
                //   两边都没有 ⇒ 内建空白封套，这是实话，不是画错。
                let label = self.covers.find(t);
                Cart {
                    stem: t.stem.clone(),
                    rom,
                    label,
                    // Discs are not carts: the whole-cart picture is a game cart's road,
                    // and this row draws sleeves.
                    art: None,
                    title: t.name.clone(),
                    code: String::new(),
                    cgb: 0,
                    initial: t.initial,
                }
            })
            .collect();

        // Where the caret was, by name: the row is rebuilt from scratch below and an index into
        // the old one would point at whatever happens to be there now.
        let keep = self
            .disc_shelf
            .current()
            .and_then(|i| self.disc_shelf.carts.get(i))
            .map(|c| c.stem.clone());
        self.disc_shelf = Shelf::new(carts);
        // **The one call that makes it a music row.** Everything else in `Shelf` — the ring, the
        // spring, the glide, the favourites view, the folder switch — is a function of the size
        // and the list, so it carries over untouched.
        //
        // **`cd::SIZE`, not `disc::SIZE`** — the row is made of sleeves as of 2026-10-04. The
        // cassette's own size is still in the tree and still builds, so going back is this one
        // line rather than a `git checkout`.
        self.disc_shelf.set_size(slot_ui::cd::SIZE);
        // No cartridge bay under a row of sleeves — see `Shelf::set_mouth`.
        self.disc_shelf.set_mouth(false);
        // Every disc, in the order the scan gave them — which is the order the row glides
        // through, so the caret lands where the eye expects after a swap.
        let view: Vec<usize> = (0..self.disc_shelf.carts.len()).collect();
        // `set_view` parks the caret on `keep` when it is still there, which is what makes
        // coming back from the game row land on the same disc rather than the first one.
        self.disc_shelf.set_view(view, keep.as_deref());
        self.disc_shelf.glide_to_target();
    }


    /// The swap's clock and the row's motion. Runs every frame; with no swap in flight it
    /// simply clears the motion, so a still row is laid out exactly as before this existed.
    /// ★★ 2026-10-24（六六）：「**卷闸转场**」。
    ///
    /// 旧版是货架位移：旧卡到点往上飞出屏、新卡从顶上掉下来（`slot_ui::Motion`）。既然
    /// 插卡位已经收成桌面图上的一个洞、卡带原地化，那个"飞走+掉落"就没有落脚点了；
    /// 换成卷闸 —— **帘子落下来盖住，换装在帘子后面发生，再卷上去**。
    ///
    /// ★★ 2026-10-26（六六）：**中间那一截不再走时间，改成等人。**
    ///
    /// 原来 `t` 一路走到 `TOTAL_S`，"满闸"只是 P3 那 0.40 秒；现在 `t` 到
    /// `SHUTTER_SWAP_AT_S` **就冻住**，帘子停在满闸状态，机种十格浮在它上面 ——
    /// 那 0.40 秒变成了"用户指一台"的时间。回卷（`up`）只在 `settle_system_switch`
    /// 被按 A / B 之后才开始，仍然走 `shutter_at` 同一条曲线（从 `TOTAL_S - UP_S` 接着算），
    /// 所以帘子的样子、速度、和新顶栏淡入的时机一个字节都没变。
    fn update_shelf_switch(&mut self, dt: f32) {
        let Some(mut sw) = self.shelf_switch.take() else {
            self.row_mut().set_motion(None);
            return;
        };
        // 货架不动。`Motion` 是旧的位移转场，帘子接手之后它一次都不该被设上。
        self.row_mut().set_motion(None);
        match sw.up {
            Some(u) => {
                let u = u + dt;
                if u >= slot_ui::SHUTTER_UP_S {
                    // 卷完了，这一趟结束。不写回 —— 下一帧 `shutter()` 就是 `None`。
                    return;
                }
                sw.up = Some(u);
            }
            None => {
                if sw.t < slot_ui::SHUTTER_SWAP_AT_S {
                    sw.t = (sw.t + dt).min(slot_ui::SHUTTER_SWAP_AT_S);
                    if sw.t >= slot_ui::SHUTTER_SWAP_AT_S {
                        // 刚到满闸：十格从这一刻开始浮现（`SysPicker::reveal` 只认第一次）。
                        let now = self.now();
                        if let Some(picker) = self.sys_picker.as_mut() {
                            picker.reveal(now);
                        }
                    }
                }
                // 满闸却没有面板可等（不该发生）：别把帘子挂死在屏上，直接往回卷。
                if sw.t >= slot_ui::SHUTTER_SWAP_AT_S && self.sys_picker.is_none() {
                    sw.up = Some(0.0);
                }
            }
        }
        self.shelf_switch = Some(sw);
    }

    /// 这一帧的卷闸状态（盖多少 / 顶栏在不在 / CHANGE 亮不亮）。
    /// 没有转场在飞时是 `None`（一帧都不画）。
    ///
    /// 两条时间轴在这里合成一条喂给 `slot_ui::shutter::at`：落下段直接用 `t`（冻在满闸），
    /// 回卷段从"满闸 + HOLD"那一刻接着算 —— 于是 `at()` 里那四段的形状、比例、缓动
    /// 全都不用改，只是 P3 被拉长了。
    pub fn shutter(&self) -> Option<slot_ui::Shutter> {
        let sw = self.shelf_switch.as_ref()?;
        let t = match sw.up {
            Some(u) => slot_ui::SHUTTER_TOTAL_S - slot_ui::SHUTTER_UP_S + u,
            None => sw.t.min(slot_ui::SHUTTER_SWAP_AT_S),
        };
        Some(slot_ui::shutter_at(t))
    }

    /// The badge in the bottom-left corner — the mirror of the favourites star in the
    /// bottom-right. Since 2026-10-29 (六六) this is **only the disc row's own word**: the
    /// machine logos (GB / GBC / GBA) that used to sit here are gone from the new version, and a
    /// machine row's corner is now clean. A reading, not a control: the settings switch is the
    /// control.
    fn draw_shelf_indicator(&self, out: &mut Vec<Draw>) {
        // ★ **2026-10-10：设置屏第一行开关就在这里生效**（六六：「货架页左下角机种标识的开关」）。
        //   2026-10-29（六六）：机种 logo 已从代码里去掉（`draw_shelf_logo` 整个删除），
        //   剩下这一条只服务碟片行的 "Disc"。
        //
        //   放在最前面（而不是塞进 `if alpha <= 0.0` 里）：碟片行切换时它才不会自己闪回一帧。
        if !self.state.shelf_badge {
            return;
        }
        // The top of the bottom band, which is the box the badge is centred in.
        let y = OUT_H as f32 - MOUTH_H;
        // The disc row's badge is its own face, not a machine's: a disc has no machine. The
        // games shelf now draws nothing here — the corner is clean.
        if self.row == ShelfRow::Discs {
            self.draw_disc_logo(1.0, y, out);
        }
    }

    /// The disc row's badge, bottom-left: the word "Disc", uploaded with the machine logos and
    /// placed exactly where they are, so the row's own badge does not jump when it comes up.
    fn draw_disc_logo(&self, alpha: f32, y: f32, out: &mut Vec<Draw>) {
        if alpha <= 0.0 {
            return;
        }
        let faces = match slot_ui::palette::mode() {
            slot_ui::Mode::Light => &self.disc_ind_light,
            slot_ui::Mode::Dark => &self.disc_ind_faces,
        };
        let Some(&(tex, w, h)) = faces.first() else {
            return;
        };
        if h == 0 {
            return;
        }
        // The same box the machine badge is fitted into, so the two are interchangeable.
        let box_h = MOUTH_H * 0.52;
        let scale = (box_h / h as f32).min(1.0);
        let (w, h) = (w as f32 * scale, h as f32 * scale);
        out.push(Draw::Tex {
            x: SHELF_IND_LEFT,
            y: y + (MOUTH_H - h) / 2.0,
            w,
            h,
            tex,
            alpha,
        });
    }


    /// One face per slot in `letters::SLOTS` order, each with the size it was rasterised at,
    /// uploaded at boot. The sizes come with them because a face cropped to its ink is as wide
    /// as the letter is, and drawing it without them would set every letter to one width.
    pub fn set_letter_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.letter_faces = faces;
    }

    /// The metal between two letters, with the size it was rasterised at — a face cropped to
    /// its ink is as wide as it is and no wider, so the size has to travel with the id the same
    /// way a letter's does.
    pub fn set_letter_pill_face(&mut self, pill: (TexId, u32, u32)) {
        self.letter_pill_face = Some(pill);
    }

    pub fn set_letter_ridge_face(&mut self, ridge: (TexId, u32, u32)) {
        self.letter_ridge_face = Some(ridge);
    }

    /// Left or right on the shelf: move the index one letter and bring the row with it.
    ///
    /// The ring keeps the repeat, so this is the press and nothing else. The caret is seated on
    /// the first cart of the letter it landed on — that is the whole point of the index — and
    /// the row is handed the move as a glide, so it sweeps across every cart it is skipping
    /// rather than arriving as if only one had moved.
    ///
    /// `by` is the direction of the step: +1 for right (the next letter), -1 for left (the
    /// previous one).
    fn step_letters(&mut self, by: i32, now: Millis) {
        // The dial counts the row that is up, whatever that row is: the machine shelf, a
        // subfolder, or the favourites. `retally_letters` recounts it for the current view, and
        // `seat_on_letter` seats within that view, so a letter jump never lands on a cart the
        // row is not showing (2026-10-10, 六六).
        let counts = self.letter_counts;
        if !self.letters.hold(by, now, &counts) {
            return;
        }
        self.seat_on_letter();
    }

    /// The same move, without a press behind it: the repeat of a held key.
    fn repeat_letters(&mut self, now: Millis) {
        let counts = self.letter_counts;
        if self.letters.tick(now, &counts) {
            self.seat_on_letter();
        }
    }

    /// Puts the caret on the first cart of the letter the strip is showing, and hands the row
    /// the move as a glide so it sweeps across every cart it is skipping rather than flicking
    /// through only the last one. The direction is the row's own to take — it is a ring, and
    /// the glide runs to the nearest image of the chosen cart — so the strip and the row agree
    /// about which way is "onward" without this having to say so.
    fn seat_on_letter(&mut self) {
        let slot = self.letters.target();
        // Found in the *view*, not the library: the shelf holds one machine's carts, and the
        // strip counts those (`retally_letters`), so `index` must be a view position.
        let Some(i) = self.row().view_position_of_letter(slot) else {
            // A slot the strip can only have reached by being counted as non-empty, so this is
            // unreachable; the next frame's `centre_on` puts it back if it ever happens.
            return;
        };
        self.row_mut().index = i;
        // A held arrow key is a direction the shelf is no longer travelling in, and leaving
        // it armed would fire a cart step over the letter that was just chosen.
        self.row_mut().release_hold();
        self.row_mut().glide_to_target();
    }

    /// Handed over when the core is spawned, which is on the way into the slot.
    pub fn set_snapshot(&mut self, snapshot: Box<dyn Snapshot>) {
        self.snapshot = Some(snapshot);
    }

    /// The `Core` that `session.rs` resolved for the cart it just spawned. Called in the same
    /// breath as `set_snapshot`, from the one place a cart's core is ever decided, so every
    /// later read or write in this file has a stored answer to take rather than a reason to
    /// ask `core_for` again.
    pub fn set_core(&mut self, core: Core) {
        self.core = core;
    }

    /// Whether a netpacket session is live right now. libretro disables an entire class of
    /// time manipulation for as long as one is — see `may_rewind`/`may_load_state` — because
    /// rewinding or loading a state on one device desynchronises the other with no way back
    /// to agreement.
    pub fn link_active(&self) -> bool {
        self.link.is_some()
    }

    /// Begins a session. `client_id` is libretro's own: 0 the host, 1 the joiner — the only
    /// two this product has. Nothing here touches a transport or a core; that lives on the
    /// emulator thread, wherever `EmuHandle::begin_link` is called from the same gesture this
    /// answers. A session always starts from the cart's battery save, never a state — that
    /// falls out for free here, since nothing on this path touches the state ring at all.
    pub fn begin_link(&mut self, client_id: u16) {
        self.link = Some(LinkSession { client_id });
    }

    /// Which side of the session this device is, for whatever the UI ends up showing while
    /// one is live. `None` when there is nothing to ask about.
    pub fn link_client_id(&self) -> Option<u16> {
        self.link.as_ref().map(|s| s.client_id)
    }

    /// Rewinding one device desynchronises the other with no way back, which is exactly what
    /// libretro's netpacket contract forbids for as long as a session is open.
    pub fn may_rewind(&self) -> bool {
        !self.link_active()
    }

    /// Loading a state is the same hazard rewinding is: it moves this device's machine to a
    /// moment the peer never agreed to and has no way to follow.
    pub fn may_load_state(&self) -> bool {
        !self.link_active()
    }

    /// Fast forward runs this device's machine out ahead of what the peer has actually been
    /// sent — the same desync `may_rewind` refuses for running it backwards instead, and
    /// named in the very libretro.h sentence `may_rewind`'s own contract comes from
    /// ("pausing, slow motion, fast forward, rewinding, save state loading... are disabled").
    pub fn may_fast_forward(&self) -> bool {
        !self.link_active()
    }

    /// ★ 2026-10-20：慢放。**libretro.h 那句话里"slow motion"是和 pausing / fast forward /
    ///   rewinding 一起被点名的**（见上面 `may_fast_forward` 的引文），所以联机中同样不给：
    ///   一台机子按半速在走，对端永远追不回来。
    pub fn may_slow(&self) -> bool {
        !self.link_active()
    }

    /// Ends the session and leaves the cart playing single player. Never an error: the peer
    /// vanishing and the user ending it deliberately look the same from here.
    ///
    /// This is the app's own bookkeeping only, exactly like `link` itself (see its doc
    /// comment) — it does not touch the core or the transport, both of which live on the
    /// emulator thread. `Session::act` is what actually reaches them: it watches
    /// `link_active()` around every `apply`, and mirrors an ending onto
    /// `EmuHandle::end_link()`, which is what tells the core (`RetroCore::stop_link`, if it
    /// offered one to call) and drops the transport. Anything that ends a session without
    /// going through `apply` would need to repeat that mirroring by hand — there is no such
    /// call site today.
    pub fn end_link(&mut self) {
        self.link = None;
    }

    /// The app has no device, so the sound it wants is left here for whoever does.
    pub fn take_sfx(&mut self) -> Option<Sfx> {
        self.sfx.take()
    }

    /// ★ 2026-10-22（六六）：派发一次 **UI 提示音**，带去抖。
    ///
    /// # 为什么要有去抖
    ///
    /// 方向/字母轮的**长按连发**每帧都会走到派发点（`step_letters`、`hold_left` 的 repeat），
    /// 若每个事件都发一声，按住不放就是一串机枪。50ms 是"两次移动之间最短的能被听成两下的
    /// 间隔" —— 比它快的一律并成一声。插/拔两条动作音**不走这里**：它们卡在触点上，由
    /// `Phase::Inserting`/`Ejecting` 自己的 `at` 精确触发，不能被动过。
    ///
    /// # 三类音效从哪里进来
    ///
    /// 调用点在**动作发生的那一臂**：`Blip` = 焦点/方向移动，`Tick` = 步进一格，
    /// `Click` = 一次确认。BGM 不走这里（它是会话层按机种起的持续声，见 `Sfx::Bgm`）。
    ///
    /// **素材未到位时静默**（`Sfx::render` 对没有 `asset` 的成员返回空），所以这一整条
    /// 现在跑起来是"什么都没听见"，接口本身已经通了。
    pub fn ui_sfx(&mut self, sfx: Sfx) {
        // The setting is respected here rather than at every call site: one place decides,
        // and a new dispatch point cannot forget to ask.
        if !self.state.ui_sfx {
            return;
        }
        let now = self.now();
        if let Some(last) = self.ui_sfx_at {
            if now.saturating_sub(last) < UI_SFX_DEBOUNCE_MS {
                return;
            }
        }
        self.ui_sfx_at = Some(now);
        // One-shot only: a BGM is a session's, not a frame's.
        if sfx.is_one_shot() {
            self.sfx = Some(sfx);
        }
    }

    /// ★ 2026-10-22（六六）：**各机种货架 BGM** 的机种号 —— 会话层拿它去取
    /// `bgm_<机种>.pcm`。游戏行取当前货架机种，碟片行不播（碟片不是机种）。
    pub fn shelf_bgm(&self) -> Option<Sfx> {
        if self.row != ShelfRow::Games {
            return None;
        }
        Some(Sfx::Bgm(self.shelf_system as u8))
    }

    /// ★ 2026-10-22（六六）：UI 音效的总开关，落地在设置页（预留位）。
    pub fn ui_sfx_enabled(&self) -> bool {
        self.state.ui_sfx
    }

    /// The panel comes up at the level the card remembers rather than at whatever the
    /// kernel left it at.
    pub fn set_power(&mut self, mut power: Power) {
        power.set_backlight(self.state.brightness);
        // The card's own answer for how long a shut lid waits, rather than the constant the
        // frontend had to build it with. `Power` cannot read a card — that is the frontend's
        // business — so the setting is handed over here, which is the first moment there is a
        // platform to hand it to. `settings_change` is the other half of this pair, and `None`
        // (「仅息屏」) means the platform holds no timer at all.
        power.set_timeout(self.doze_timeout());
        // The device's own clock, which the host's stands in for. Boot has nothing better to
        // seed the picker from, so a device with a live RTC only gets its confirmation here.
        // The first moment the device's own clock can be asked, and so the first moment a
        // clock that was never set can be told apart from one that was. Boot has already
        // taken `clock_set` at its word by here, which is exactly the case that leaves a
        // dead RTC with no way back to the one screen that could fix it.
        let secs = power.now();
        match &mut self.phase {
            Phase::SetClock { picker, .. } => *picker = ClockPicker::from_secs(secs),
            _ if secs < CLOCK_FLOOR => {
                self.phase = Phase::SetClock {
                    picker: ClockPicker::from_secs(secs),
                    from_menu: false,
                }
            }
            _ => {}
        }
        self.power = Some(power);
        // There is nothing to read before this call — no gauge for `battery_at`, no charge
        // for `charge_at`, and whatever `led_state` computed with no battery is not a real
        // state to have already told a platform that did not exist yet either. All three are
        // placeholders with nothing behind them, not real deadlines or a real prior write, so
        // the first tick after this one has to act as if nothing has been read or written
        // yet — or the case band's left shelf sits blank and the LED sits stale for the first
        // poll of whichever cadence is longer, which is the one moment either is cheapest to
        // have been wrong to skip.
        self.battery_at = self.now();
        self.charge_at = self.now();
        self.last_led = None;
    }

    /// The motor. Never persisted and never a level: it belongs to the cart that asked for
    /// it and stops with it.
    pub fn set_rumble(&mut self, strength: u16) {
        if let Some(power) = &mut self.power {
            power.set_rumble(strength);
        }
    }

    /// Set by the doze timeout and by a graceful power off. The binary is what acts on it:
    /// everything durable has already been written by the time it is true.
    ///
    pub fn powering_off(&self) -> bool {
        self.powering_off
    }

    /// What the binary waits for. The decision is made the instant the choice is, but the
    /// machine is not allowed to stop until the ordinary loop has drawn and presented the
    /// shutdown screen. Rendering out of band instead — one extra draw and swap between the
    /// choice and `poweroff` — hung the device: the swap can block on a GPU about to be torn
    /// down, and slot then never reached `poweroff` at all.
    pub fn ready_to_power_off(&self) -> bool {
        self.powering_off && self.now() >= self.act_at
    }

    pub fn ready_to_restart(&self) -> bool {
        self.restarting && self.now() >= self.act_at
    }

    /// Whether the shutdown screen is what should be on the panel. True from the instant the
    /// choice is made, which is earlier than `powering_off`.
    pub fn shutting_down(&self) -> bool {
        self.powering_off || self.restarting
    }

    /// Set by the menu's Restart. Goes through the same shutdown as a power off — busybox
    /// init runs rcK for a reboot too — so the GPU module is unloaded either way, which is
    /// what stops this hardware hanging with the rails up.
    pub fn restarting(&self) -> bool {
        self.restarting
    }

    pub fn restart(&mut self) {
        if let Some(power) = &mut self.power {
            power.restart();
        }
    }

    pub fn power_menu(&self) -> Option<usize> {
        self.power_menu
    }

    /// The core the chip is in or heading for, and `None` once the picker has gone. Still
    /// `Some` while the lid is going back on.
    pub fn core_picker(&self) -> Option<Core> {
        self.core_picker.map(|p| p.seat())
    }

    /// The chip's pose this frame, for whatever draws it.
    pub fn core_picker_chip(&self) -> Option<Chip> {
        self.core_picker.map(|p| p.chip(self.now()))
    }

    /// Whether the in-game menu is up. Read by whoever owns the emulator as well as by the
    /// draw: the game underneath is paused for as long as it is.
    pub fn game_menu_open(&self) -> bool {
        self.game_menu.is_some()
    }

    /// Which screen of the in-game menu is up, and `None` while it is closed.
    pub fn game_menu(&self) -> Option<GameMenu> {
        self.game_menu
    }

    /// What is on the menu right now, in the order it is drawn. Empty while it is closed,
    /// and on the two screens that are a sentence rather than a list.
    pub fn game_menu_rows(&self) -> Vec<&'static str> {
        match self.game_menu {
            Some(GameMenu::Rows(_)) => {
                let system = self.seated_system();
                self.game_rows().iter().map(|r| r.text_for(system)).collect()
            }
            Some(GameMenu::Link(_)) => LinkRow::ALL.iter().map(|r| r.text()).collect(),
            _ => Vec::new(),
        }
    }

    /// ★ 2026-10-21：**操作键映射弹窗是否开着**。货架设置页和游戏内菜单都能开它，两者都用这一
    /// 个答案；`held()`/`overlaid()` 也读它，所以弹窗开着的时候底下的游戏是暂停的、按键不
    /// 会漏给核心。
    pub fn keymap_edit(&self) -> Option<&KeymapEdit> {
        self.keymap_edit.as_ref()
    }

    /// 弹窗开着时前端烘好的面，`None` 是还没烘出来（此时不画而不是画一个洞）。
    pub fn keymap_faces(&self) -> Option<&KeymapFaces> {
        self.keymap_faces.as_ref()
    }

    pub fn set_keymap_faces(&mut self, faces: Option<KeymapFaces>) {
        self.keymap_faces = faces;
    }

    /// 七行各自当前显示的那个值，前端拿它当"要不要重烘行面"的键：改了哪一行，就只重烘那一行。
    /// 前六行是六个目标的名字，最后一行是频率。`pub` 是给前端读的 —— 它只读，不写。
    pub fn keymap_row_values(&self) -> Vec<String> {
        let system = self.current_machine();
        let map = self.keymap_effective(system);
        let mut out: Vec<String> = slot_ui::keys::physical_for(system)
            .iter()
            .map(|p| {
                let t = slot_ui::current_for(system, *p, &map);
                slot_ui::keys::face_label(system, t.name).to_string()
            })
            .collect();
        out.push(slot_ui::keymap_rate_face(self.state.turbo_hz));
        out
    }

    /// ★ 2026-10-21：**开操作键映射弹窗**。两处入口共用（设置页那一行 / 游戏内菜单那一行），
    /// 它只记住开的时候那张表，`B` 关掉时按它还原 —— 见 `close_keymap`。
    ///
    /// 已经把弹窗开着的再开一次是空操作：设置页和游戏菜单都在 `apply()` 的更外面就被截住了，
    /// 这个守卫是给"同一帧两条路都到"留的余量。
    pub fn open_keymap(&mut self) {
        if self.keymap_edit.is_some() {
            return;
        }
        self.keymap_edit = Some(KeymapEdit {
            row: 0,
            picking: None,
            confirm: false,
            original: self.state.keymap.clone(),
            original_systems: self.state.keymaps.clone(),
            original_hz: self.state.turbo_hz,
        });
    }

    /// ★ 2026-10-21：关掉弹窗。`keep` 为假时把表**还原成开的时候那张** —— 这就是 `B`/取消。
    /// 改是实时写进 `state` 的（这样转着游戏就能试新键），所以取消必须真回滚，否则"取消"
    /// 会把一个没保存的改动留在卡上。
    fn close_keymap(&mut self, keep: bool) {
        if let Some(edit) = self.keymap_edit.take() {
            if !keep {
                self.state.keymap = edit.original;
                self.state.keymaps = edit.original_systems;
                self.state.turbo_hz = edit.original_hz;
            }
        }
        // **面不在这里放走**：纹理的释放归前端（`sync_keymap` 看见弹窗关了整族释放）。
        // 这里置 `None` 会让前端以为"还没建"，于是上一族纹理永久泄漏。
    }

    /// The wire a link that just came up runs over, handed on exactly once. `App` never
    /// touches a transport itself — the core and the socket both live on the emulator
    /// thread — so this is left here for whoever owns that thread to collect.
    pub fn take_link_transport(&mut self) -> Option<(u16, Box<dyn LinkChannel>)> {
        self.link_transport.take()
    }

    /// The cart under the highlight, and `None` on an empty shelf.
    pub fn selected_stem(&self) -> Option<&str> {
        self.row().current_cart().map(|c| c.stem.as_str())
    }

    /// **这张卡印出来的名字** —— 机种 + stem 一起交给 `shown_name`，街机在那里换成中文。
    ///
    /// 和 [`selected_stem`](Self::selected_stem) 并存而不是替换它：stem 是**身份**（缓存键、
    /// 金手指查表、贴纸匹配都认它），名字是**给人看的**。两者一旦混用，街机那批卡的缓存键
    /// 会变成中文，换个机种重扫就全对不上了。
    pub fn selected_name(&self) -> Option<String> {
        self.row()
            .current_cart()
            .map(|c| slot_store::shown_name(c.system(), &c.stem))
    }

    /// Which machine's picture belongs on the panel.
    ///
    /// The card's three shelves collapse into the renderer's two here, because Game Boy and
    /// Game Boy Color hand back the same 160x144 frame — `slot_store::System` is the three and
    /// `slot_gfx::System` is the two. `Gba` stands in for an empty shelf, where there is no
    /// picture to misplace either way.
    pub fn screen_system(&self) -> ScreenSystem {
        // ★★ 2026-10-24（六六）：「**SFC/FC 画面被拉伸**」（以及 SFC 那片黑）。
        //
        //   这里原来只有两个答案：Game Boy 家和「其余都是 Advance」。于是 SFC / FC / NeoGeo /
        //   街机的帧被当成 **240×160** 的 Advance 帧，塞进 720×480 的矩形里 —— 256×224 的画
        //   面横向多拉 18%（正是他看到的"拉伸"），而且 `GamePass::set_system` 每帧把
        //   `tex_size` 写回 Advance 的 240×160，`upload` 于是每帧都判定尺寸不符、每帧重建一次
        //   纹理 —— 一帧删一次建一次，屏幕上什么都留不下（SFC 的黑屏）。
        //   ⇒ 一个机种一个答案，和 `slot_gfx::System` 一一对应。
        //
        // ★★ 2026-10-25：**把兜底 `_ => Gba` 换成逐条列举**。上一版这里最后一行是
        //   `_ => ScreenSystem::Gba`，而它正是"新机种悄悄当成 Advance"的那条路 ——
        //   加 MD/PS1 时若只补几条、忘了这一处，编译器不会吭声，屏幕上是拉伸的画面
        //   加上每帧重建纹理的黑屏，恰是上面那段注释记的同一个事故。现在写成穷尽匹配：
        //   以后再添机种，这里编不过，而不是静默出错。
        match self.shelf.current_cart().map(|c| c.system()) {
            Some(System::Gb) | Some(System::Gbc) => ScreenSystem::Gb,
            Some(System::Gba) => ScreenSystem::Gba,
            Some(System::Sfc) => ScreenSystem::Sfc,
            Some(System::Fc) => ScreenSystem::Fc,
            Some(System::Neogeo) => ScreenSystem::Neogeo,
            Some(System::Arcade) => ScreenSystem::Arcade,
            Some(System::Md) => ScreenSystem::Md,
            Some(System::Ps1) => ScreenSystem::Ps1,
            Some(System::Pce) => ScreenSystem::Pce,
            // 空卡：没有卡带可问，货架停在哪个机种就用哪个的几何 —— 与 `current_machine`
            // 同一个口径，免得空卡和"货架上一个游戏都没有"给出两种画面尺寸。
            None => match self.shelf_system {
                System::Gb | System::Gbc => ScreenSystem::Gb,
                System::Gba => ScreenSystem::Gba,
                System::Sfc => ScreenSystem::Sfc,
                System::Fc => ScreenSystem::Fc,
                System::Neogeo => ScreenSystem::Neogeo,
                System::Arcade => ScreenSystem::Arcade,
                System::Md => ScreenSystem::Md,
                System::Ps1 => ScreenSystem::Ps1,
                System::Pce => ScreenSystem::Pce,
            },
        }
    }

    /// The cached reading. `None` until the first slow tick, and on any device with no gauge.
    pub fn battery(&self) -> Option<Battery> {
        self.battery
    }

    /// Does not return when there is a platform to power off. A unit test has none, and
    /// there the flag is the whole of it.
    pub fn poweroff(&mut self) {
        if let Some(power) = &mut self.power {
            power.poweroff();
        }
    }

    /// The face buttons belong to whatever is on screen. The shelf and the switcher each
    /// take them; while the game is playing they are the game's and the app sees only the
    /// gestures that are never the game's.
    pub fn apply(&mut self, action: Action) {
        // A system-shelf swap owns the screen while it is *moving*: every key is inert until it
        // lands, so nothing can be pressed into the middle of the movement. `Session::act`
        // gates the volume and mute on `input_locked` for the same reason — those are handled
        // on the session's side of `apply` and would otherwise slip through.
        // ★★ 2026-10-27 修正：满闸停顿（机种面板等用户指）不再锁输入，否则面板按键会被
        //   死锁 —— 清掉 `shelf_switch` 的 `settle_system_switch` 自己就靠面板的 A/B。只在
        //   卷闸真正「运动」中（落下未到满闸 / 卷起回退）锁。
        if self.shelf_switch_moving() {
            return;
        }
        // Ahead of everything, including the device's own keys: the menu is a decision the
        // user is in the middle of making, and a volume press underneath it would be one
        // more thing happening while they read.
        if self.power_menu.is_some() {
            match action {
                Action::LidClose => return self.doze(),
                Action::LidOpen => return self.wake(),
                _ => return self.power_menu_input(action),
            }
        }
        // (WiFi 直连对话框的输入拦截已移除——功能暂缓。)
        // The cheat table owns every button on the game's side while it is up: up/down move, A
        // toggles the row, B leaves, and the lid is honoured so the device can still sleep under it.
        if self.cheat_menu.is_some() {
            match action {
                Action::LidClose => return self.doze(),
                Action::LidOpen => return self.wake(),
                _ => return self.cheat_menu_input(action),
            }
        }
        // The palette browser owns them the same way while it is up: it is a modal over a paused
        // game, and the lid is honoured so the device can still sleep under it.
        if self.pal_cursor.is_some() {
            match action {
                Action::LidClose => return self.doze(),
                Action::LidOpen => return self.wake(),
                _ => return self.palette_browser_input(action),
            }
        }
        // ★ 2026-10-21：操作键映射弹窗。它比色板浏览器更外一层 —— **货架设置页也能开它**，
        //   所以这里必须在相位分派之前截住，否则从设置页开出来的弹窗会被 `Phase::Settings`
        //   那一臂吞掉。和上面两个浮层一样，盖子还归设备。
        if self.keymap_edit.is_some() {
            match action {
                Action::LidClose => return self.doze(),
                Action::LidOpen => return self.wake(),
                _ => return self.keymap_input(action),
            }
        }
        // The lid, the light and the sound belong to the device rather than to whatever is
        // on screen, so they are taken before the phase gets a look at the action.
        match action {
            Action::LidClose => return self.doze(),
            Action::LidOpen => return self.wake(),
            Action::PowerPress => {
                // A live session ends here rather than flushing: this is the one button a
                // trade partner mid-exchange can still reach, and ending the session is a
                // decision, not "nothing to flush" — the two must not be the same press.
                if self.link_active() {
                    self.end_link();
                    return;
                }
                return self.flush_resume();
            }
            Action::PowerTap => return self.power_press(),
            Action::PowerHold => return self.open_power_menu(),
            // The release no longer means anything once the menu is what a hold raises:
            // the choice is the commitment, and it is made with A.
            Action::PowerOff => return,
            _ => {}
        }
        // Ahead of the levels too. A screen with no way back is not one to be adjusting the
        // backlight from, and the clock owns all four directions.
        if let Phase::SetClock { picker, from_menu } = &mut self.phase {
            match action {
                Action::GbaDown(Btn::Left) | Action::ShelfLeft => picker.left(),
                Action::GbaDown(Btn::Right) | Action::ShelfRight => picker.right(),
                Action::GbaDown(Btn::Up) => picker.up(),
                Action::GbaDown(Btn::Down) => picker.down(),
                Action::GbaDown(Btn::A) | Action::Insert => self.confirm_clock(),
                // ★ **`B` 只在"从设置里进来"时有意义**（2026-10-10）。首启那一次它没地方可回
                //   —— 那正是 `from_menu` 的全部差别：一个是从设置页要来改时间的，另一个是
                //   这台机器**第一次**问时间。见 `confirm_clock`。
                Action::GbaDown(Btn::B) if *from_menu => {
                    self.show_settings(slot_ui::settings::Row::DateTime);
                }
                _ => {}
            }
            return;
        }
        if self.adjust(action) {
            return;
        }
        // The release reaches the shelf whatever is on screen. A direction let go of during
        // an insert would otherwise still be held when the cart comes back out.
        match action {
            Action::GbaUp(Btn::Left) => self.row_mut().release_left(),
            Action::GbaUp(Btn::Right) => self.row_mut().release_right(),
            _ => {}
        }
        // Beside the power menu's own block rather than inside the phase match, so the two
        // menus can never both take a press: that one returns above this, and this returns
        // above the phase. Below the lid, the button and the levels, unlike that one — this
        // is a menu over a game that is still running, not a machine about to stop, so the
        // device's own keys keep working while it is up.
        if self.game_menu.is_some() {
            return self.game_menu_input(action);
        }
        // The direct-connect dialog takes the keys while it is up, the same way the game
        // menu does: A brings the card's own WiFi up, B cancels the prompt or — once it is
        // live — takes the radio back down, and nothing underneath answers.
        if self.direct_link.is_some() {
            return self.direct_link_input(action);
        }
        // The Wi-Fi config screen takes the keys the same way: it is opened from the
        // settings screen, and everything it does — scan, type, save — is its own.
        if self.wifi.is_some() {
            return self.wifi_input(action);
        }
        if self.search.is_some() {
            return self.search_input(action);
        }
        let now = self.now();
        // Ahead of the match that borrows the phase: the card's limit is a fact about how many
        // rows are on it, not about the press that is being answered.
        let card_max = self.card_scroll_max();
        match self.phase {
            Phase::Shelf => match action {
                // START rather than SELECT, and the difference is not cosmetic. SELECT is
                // the chord key: held, it turns Up/Down into brightness and Left/Right into
                // blue light, and `adjust` answers those on every screen including this one.
                // Opening a menu the instant SELECT goes down would eat the first half of
                // every one of those chords; waiting out the 600 ms window instead would put
                // that delay in front of the menu. START is bound to nothing here and reaches
                // no core from the shelf, so it costs neither.
                Action::GbaDown(Btn::Start)
                    if self.core_picker.is_none() && self.sys_picker.is_none() =>
                {
                    self.open_core_picker()
                }
                // Colour correction (SELECT+Y) is the game panel's now: the shelf no longer
                // changes it, so that chord is free on this screen to be bound to something
                // else. The two screen keys — SELECT+X and SELECT+R2 — were already the game
                // panel's alone and are answered in `Phase::Playing` only. The shelf cycle is a
                // hold of Y, spent in `sys_hold`, not a chord.
                // SELECT+VOL, and the shelf is the only screen that answers it: the change
                // reopens the audio device, which is free here and a gap in the sound anywhere
                // else. See `Action::AudioProfileNext`.
                Action::AudioProfileNext
                    if self.core_picker.is_none() && self.sys_picker.is_none() =>
                {
                    self.cycle_audio(true)
                }
                Action::AudioProfilePrev
                    if self.core_picker.is_none() && self.sys_picker.is_none() =>
                {
                    self.cycle_audio(false)
                }
                // ★★ 2026-10-21 **恢复**：`SELECT+START` 是货架切日/夜主题的那一下。先前
                // 把它连表和分支一起摘掉是误读 —— 六六只要求"**游戏里**把 START 还给游戏"，
                // 不是"货架上也不要切主题"。现在那条和弦只在**没有卡**时成立（手势层的
                // `set_start_to_game` 每帧由 `Session` 按仓里有没有卡置位），所以这里照旧
                // 收它；有游戏的时候它根本不会走到这一臂。
                Action::ModeToggle
                    if self.core_picker.is_none() && self.sys_picker.is_none() =>
                {
                    self.toggle_mode()
                }
                // ★★ 2026-10-26：**机种面板先于货架**（也先于核心选择器 —— 两个面板不会
                //   同时开着，这一臂排在前面只是让"模态优先"这件事读起来更直）。
                //   它开着的时候方向键、A、B 都是它的，一个都不许漏到下面那一行去。
                _ if self.sys_picker.is_some() => self.sys_picker_input(action),
                // Ahead of the shelf's own movement, so an open picker takes the arrows
                // before the row of carts underneath it does.
                _ if self.core_picker.is_some() => self.core_picker_input(action),
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => {
                    self.ui_sfx(Sfx::Blip);
                    self.row_mut().hold_left(now)
                }
                Action::ShelfRight | Action::GbaDown(Btn::Right) => {
                    self.ui_sfx(Sfx::Blip);
                    self.row_mut().hold_right(now)
                }
                // L and R are the whole of the index, and the only pair of keys that could be:
                // they are either end of the strip printed across the top of the case, and on
                // their own they are the GBA's own shoulders with no core under this screen to
                // want them. The arrows are the *row's* — left and right walk the carts and up
                // and down mean nothing up there — and SELECT turns up and down into
                // brightness, which `adjust` answers before any of this is reached.
                Action::GbaDown(Btn::L1) => {
                    self.ui_sfx(Sfx::Tick);
                    self.step_letters(-1, now)
                }
                // SELECT+R1 walks the mode's colouring ramps. The bare L1 above it is the
                // letter index, and a held SELECT turns R1 into the chord before this arm is
                // reached — so the two cannot collide: the chord is consumed in `gesture`,
                // which is exactly why the chord table can hand this shoulder to the shelf.
                // The shelf is the only screen that answers it: a look repaints the device
                // rather than the game, and the game's own colour keys are SELECT+X/Y.
                Action::SaveState
                    if self.core_picker.is_none() && self.sys_picker.is_none() =>
                {
                    self.cycle_look()
                }
                Action::GbaDown(Btn::R1) => {
                    self.ui_sfx(Sfx::Tick);
                    self.step_letters(1, now)
                }
                Action::GbaUp(Btn::L1) => self.letters.release(-1),
                Action::GbaUp(Btn::R1) => self.letters.release(1),
                Action::OpenMenu => {
                    self.phase = Phase::Menu {
                        index: slot_ui::menu::first_slot(),
                    }
                }
                // A is two actions and the press cannot tell them apart yet, so the cart
                // goes in on the release. The hold has already taken it if it got there
                // first, and then the release is not a second press.
                Action::GbaDown(Btn::A) => self.play_held = Some(now),
                Action::GbaUp(Btn::A) => {
                    if self.play_held.take().is_some() {
                        // ★ 2026-10-22：一次确认。素材未到位时这声是静的，接口已通。
                        self.ui_sfx(Sfx::Click);
                        self.insert(false);
                    }
                }
                // X, the screen's other two-gesture button, on the same mechanism as A's: the
                // press is remembered, the release decides which gesture it was, and the hold
                // has already spent the press if it got to the threshold first. A short press
                // stars the cart; a hold swaps the shelf it is shown on.
                Action::GbaDown(Btn::X) => self.fav_held = Some(now),
                Action::GbaUp(Btn::X) => {
                    if self.fav_held.take().is_some() {
                        self.toggle_favorite();
                    }
                }
                // Y holds to swap which machine's shelf is up. Nothing fires on the press: the
                // hold is spent in `sys_hold` on the timer and the release only clears.
                Action::GbaDown(Btn::Y) => self.sys_held = Some(now),
                // A tap walks this machine's subfolders; a hold (`sys_hold`) swaps the machine
                // instead and has already spent `sys_held` by this point — so `take()` coming back
                // `Some` is exactly "the press was a tap".
                Action::GbaUp(Btn::Y) => {
                    if self.sys_held.take().is_some() {
                        self.cycle_folder();
                    }
                }
                // B holds to open the direct-connect dialog: the card's own WiFi comes up on
                // `System/slotnet.sh`, the dialog shows the address DHCP handed out, and B
                // again takes the radio back down. The same press-remembered mechanism as A
                // and X — `mtp_hold` spends it on the timer, and a tap does nothing. The
                // entry sat out at 六六's ask while the bring-up still lived in the dock's
                // own `ags-net`; it is back now that the same work runs on the card, so the
                // same card does it on either dock.
                Action::GbaDown(Btn::B) => self.mtp_held = Some(now),
                // A tap must consume the pending hold; only a real hold opens direct-connect.
                Action::GbaUp(Btn::B) => {
                    self.mtp_held = None;
                }
                // SELECT+Y is free on the game shelf; the disc shelf keeps its own controls.
                Action::ColorCycle if self.row == ShelfRow::Games => self.open_search(),
                Action::Insert => self.insert(false),
                _ => {}
            },
            // Eject reaches an insert as well, so a cart whose core never arrived can still
            // be got out. Nothing else here applies until there is a game.
            Phase::Inserting { .. } if action == Action::Eject => self.eject(),
            Phase::Playing { .. } => match action {
                Action::Eject => self.eject(),
                // SELECT+X. The same chord, and it means whichever of the two the machine has:
                // the machines with a screen bezel walk their art (`Overlay/` — a Game Boy's three
                // models and the card's own files, a Game Boy Color's files, each of the Color's
                // repeated `GBC_GRID_STATES` times so the mesh rides its entry), and the one
                // machine that ships no art at all — the Advance — steps its `MASK_STATES`
                // aperture presets instead (off, the LCD3X table at four strengths, the scanline
                // at two). Art where there is art, grid where there is none.
                // ★ 2026-10-27（六六）：SELECT+X 按机种三分 ——
                //   GBA 走 `mask_mode` 孔径档；GB/GBC 走 `overlay_index` 遮罩艺术环；
                //   主机种走**扫描线**，但落在哪个字段上取决于环里有没有素材。
                // ★★ 2026-11-03：主机种（FC/SFC/MD/NEOGEO/ARCADE/PS1/PCE）不再按
                //   "有没有遮罩艺术"分成两种按键语义两套字段 —— 那正是玩家按不出扫描线的
                //   原因（`scanline_state` 写了存了，`applied_mask` 从不读它）。现在两路合一：
                //   环里有素材就切素材（扫描线档随环走，FC/SFC 的边框也能换），没素材就切
                //   `scanline_state` 的三态（无 / 50% / 100%），与 `display_grid` 同一个判据。
                Action::MaskCycle if self.console_scanlines() => {
                    if self.overlay_list.is_empty() {
                        self.cycle_scanline();
                    } else {
                        self.overlay_next();
                    }
                }
                Action::MaskCycle if self.current_machine() != System::Gba => self.overlay_next(),
                Action::MaskCycle => self.cycle_mask(),
                Action::ColorCycle => self.cycle_cc(),
                // （SELECT+Y 的长按不再开色板浏览器：手势层不再发 `ColorHold`，浏览器
                //   只从游戏菜单进，见 `open_palette_browser_from_menu`。
                //   SELECT+R2 也曾是遮罩环的第二条路，也从和弦表里删了：Game Boy 上和
                //   SELECT+X 完全一样，另两台上什么都不做 —— 见
                //   `slot_input::gesture::chord`。
                // ）
                Action::GameMenu => self.open_game_menu(),
                Action::Polaroids => self.open_polaroids(),
                Action::SaveState => self.save_state(),
                Action::LoadState => self.load_newest(),
                // Rewinding interrupts communication libretro's contract says must not be
                // interrupted. Declined the same way every other "nothing doing" action in
                // this file is, so the press reads as answered rather than dropped.
                Action::RewindStart if !self.may_rewind() => self.refuse(),
                // Fast forward is the same interruption run forwards. `Session::sync_speed`
                // is what actually withholds `Speed::Fast` for as long as `may_fast_forward`
                // says no — this is only the shake, so the press reads as answered.
                Action::FfStart if !self.may_fast_forward() => self.refuse(),
                // ★ 2026-10-20：**MENU 短按 = 游戏菜单**。原先游戏里按短 MENU 什么都不发生
                //   （它只在货架和卡座上有意义），而游戏菜单只有 SELECT+MENU 一条路，那是一条
                //   要两只手的和弦。现在单键也开，SELECT+MENU 那条留着 —— 两条路开同一个面板，
                //   老习惯不失效。面板里有什么是下一轮的事（`open_game_menu` 现在只有 Link 一行）。
                Action::OpenMenu => self.open_game_menu(),
                // 慢放（L2 双击）的两条边：联机中拒掉，其余给一句屏上的提示 —— 半速的画面和全速
                //   的画面一眼看不出区别，没有提示的话这按键等于没反应。
                Action::SlowStart if !self.may_slow() => self.refuse(),
                Action::SlowStart => self.toast_now(Toast::SlowOn),
                Action::SlowStop => self.toast_now(Toast::SlowOff),
                _ => {}
            },
            Phase::Polaroids { .. } => match action {
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.flick(Polaroids::left),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => self.flick(Polaroids::right),
                Action::GbaDown(Btn::A) => self.load_selected(),
                Action::GbaDown(Btn::B) | Action::Polaroids => self.close_polaroids(),
                // The offer lives on this screen and nowhere else. X and Y are free
                // everywhere: the GBA has neither, so the game can never want them.
                Action::GbaDown(Btn::X) => self.undo(self.now()),
                Action::GbaDown(Btn::Y) => self.delete_selected(),
                _ => {}
            },
            // MENU closes it as well as opening it, so the button that got you here gets you
            // back without having to know that B also works. The arrows belong to the card
            // while it is up: it is the only screen with more on it than fits, so it is the one
            // screen where up and down have somewhere to go.
            // The caret walks the grid. `menu::step` is what knows the grid's shape — that one
            // button spans the whole second row — and that knowledge is in `slot-ui`, because
            // that is where the grid's shape lives.
            Phase::Menu { index } => match action {
                Action::GbaDown(Btn::Left) => self.menu_to(index, -1, 0),
                Action::GbaDown(Btn::Right) => self.menu_to(index, 1, 0),
                Action::GbaDown(Btn::Up) => self.menu_to(index, 0, -1),
                Action::GbaDown(Btn::Down) => self.menu_to(index, 0, 1),
                Action::GbaDown(Btn::A) => self.menu_open(index),
                // ★ **回的是游戏货架，不是"上一次那一行"。** 2026-10-07 六六报
                //   「现在回不到游戏货架了，按 MENU 退到关于页的时候再按 B 是回碟片货架」：
                //   原先这里只写 `Phase::Shelf`，落在哪一行取决于 `disc_row_up` 那个**粘性**
                //   状态 —— 进过一次播放器它就一直是 true，于是从关于页按 B 永远落在碟片行，
                //   而这一页从两行都打得开，出口却只认一行。现在说清楚：**关于页出去 = 游戏货架**。
                //   去碟片行有它自己的出口，就是卡座里那个 MENU 短按（"换碟"那条）。
                _ if action == Action::GbaDown(Btn::B) || action == Action::OpenMenu => {
                    self.set_row(ShelfRow::Games);
                    self.phase = Phase::Shelf
                }
                _ => {}
            },
            // Page, do not nudge: the card is a list you travel, not a cursor you steer.
            // Setting the phase rather than poking a field keeps the eased scroll and the
            // wanted scroll in the same place as the rest of the state.
            //
            // Both B and MENU come back here rather than to the shelf, so the stack unwinds one
            // step at a time and the foot line's "B or MENU to go back" means one thing.
            Phase::Shortcuts { scroll, want } => match action {
                Action::GbaDown(Btn::Up) => {
                    let w = (want - CARD_PAGE).max(0.0);
                    self.phase = Phase::Shortcuts { scroll, want: w };
                }
                Action::GbaDown(Btn::Down) => {
                    let w = (want + CARD_PAGE).min(card_max);
                    self.phase = Phase::Shortcuts { scroll, want: w };
                }
                _ if action == Action::GbaDown(Btn::B) || action == Action::OpenMenu => {
                    self.phase = Phase::Menu {
                        index: slot_ui::menu::first_slot(),
                    }
                }
                _ => {}
            },
            // ── 设置页（2026-10-10 六六）：**↑↓ 选行，←→ 改值，A 进那张屏，B 回菜单。**
            //
            //   ★★ **2026-10-20 唯一例外：开机画面那一行 `←`/`→` 只浏览，`A` 才替换，
            //   `B` 取消**（六六定的走法）。它改的是机器通电那一刻的图，方向键每按一次
            //   就写一次分区会把"随便看看"变成刷机 —— 详见 `settings_change` 那一臂。
            //
            //   行是粘的（`row` 就在相位里），所以从时间屏按 B 回来落在原来那一行，而不是
            //   弹回第一行 —— 用户为某一项进来，退回去就该退到那一项。
            //
            //   ★ **2026-10-10：现在四行、窗口只放三行，所以列表滚动。** `scroll` 由输入**原
            //   样带着走**（不在这里重算）：它要缓动追 `settings::scroll_for(row)`，而缓动的
            //   起点正是"上一次停在哪"。在这里直接跳到目标的话，每按一次 ↑↓ 都是一次瞬移，
            //   而 `Phase::Settings` 里那个 settle 臂也就没有东西可做了 —— 见 `tick`。
            //
            //   `B` 回**菜单**而不是货架：这一页是菜单开的，栈就该一层一层退，和 `Shortcuts`
            //   那条一模一样的理由（那一臂上面写着）。
            Phase::Settings { row, scroll } => match action {
                Action::GbaDown(Btn::Up) => {
                    self.phase = Phase::Settings {
                        row: row.up(),
                        scroll,
                    }
                }
                Action::GbaDown(Btn::Down) => {
                    self.phase = Phase::Settings {
                        row: row.down(),
                        scroll,
                    }
                }
                // ★ **2026-10-28：`←`/`→` 也是光标** —— 跨列，不改值。改值只有 `A`。
                //   跨列不改行号（`index/COLS` 不变），所以 `scroll` 跟着原样走，tick 的
                //   settle 臂算出的 `scroll_for` 也不会变 —— 水平挪动不引起任何滚动。
                Action::GbaDown(Btn::Left) | Action::ShelfLeft => {
                    self.phase = Phase::Settings {
                        row: row.left(),
                        scroll,
                    }
                }
                Action::GbaDown(Btn::Right) | Action::ShelfRight => {
                    self.phase = Phase::Settings {
                        row: row.right(),
                        scroll,
                    }
                }
                // ★ **2026-10-28：`A` 是唯一的动作键** —— 开门/执行（`opens`）走
                //   `settings_open`，改值走 `settings_change`（一步一换、环绕）。
                Action::GbaDown(Btn::A) | Action::Insert => {
                    if row.opens() {
                        self.settings_open(row);
                    } else {
                        self.settings_change(row, true);
                    }
                }
                // ★ **`B` 回菜单**（2026-10-20）：这一页是菜单开的，栈就该一层一层退。开机画面
                //   已无"浏览未写"的中间态（2026-10-28），`A` 一步一换、换到就写，所以这里
                //   没有要取消的东西 —— `settings_back` 只是回菜单。
                _ if action == Action::GbaDown(Btn::B) || action == Action::OpenMenu => {
                    self.settings_back(row)
                }
                _ => {}
            },
            // ★ The deck's five keys, in the order 六六 gave: MENU / ◀◀ / ▶ / ▶▶ / MODE,
            //   left to right, matching the media row of the `LCD UI KIT` card.
            //
            // **The arrows move the caret and `A` presses the key it is on** — the same shape the
            // old three-key row had, and the same shape a transport has: you point, then you
            // press. What changed is the count and what each key does.
            //
            // The `playing` in the phase is **what the transport was last asked to do**, and the
            // frame reads `deck_playing` for whether sound is actually coming out. They part
            // company at the end of a track, and the phase is the one allowed to be stale: it is
            // what the glyph wears, and a glyph that waits a frame for the decoder to answer reads
            // as a dropped press.
            // ── 卡座：键位表。基础是 2026-10-06 六六定的那一版，**2026-11-02 六六再改了一轮**。
            //
            //   | 键 | 动作 | 屏上哪儿看得见 |
            //   |---|---|---|
            //   | **A** | **短按** 播放 / 暂停 · **长按** **停止**（回曲首，不离开这一屏） | 元信息行左 `PLAY`/`PAUSE`/`STOP` |
            //   | **B** | **返回碟片货架**（离开播放器，停在碟片那一行） | — |
            //   | **X** | 切面板面（频谱 / 歌词） | 整块屏 |
            //   | **Y** | 换色板（40 套） | 整块屏 |
            //   | **← / →** | **快退 / 快进**（10 秒一步，按住连跳） | 走带条的时间 |
            //   | **↑ / ↓** | 上一首 / 下一首 | 歌名行 |
            //   | **L1 / R1** | 上一张 / 下一张碟 | 歌名行 |
            //   | **START** | 换 EQ 预设 | 元信息行右 `[NULL]` |
            //   | **SELECT** | （空操作） | — |
            //   | **MENU 短按** | **碟片信息页**（全屏） | 整块屏 |
            //   | **MENU 长按** | 回 MENU 页（退出播放器） | — |
            //
            // 本轮（★ 2026-11-02 六六）改了四处，都是他点的：
            // * **A 分两档**：短按播放/暂停（原样），**新增长按 = 停止**。长按的短/长判在
            //   **手势层**（`Gesture` 的 `A_HOLD_MS`，与 `MENU_HOLD_MS` 同值），松手才落定，
            //   长按发 `DeckStop`；所以 `A` 短按这条（下面 `GbaDown(A)`）只在**短按**时到得了这里。
            // * **停止从 B 挪到 A 的长按**；**B 让给「返回碟片货架」**（`OpenMenu` 原来干的那件事）。
            // * **信息页从 SELECT 挪到 MENU 短按**（`OpenMenu` 进 `DiscInfo`），**SELECT 空操作**。
            // * MENU 长按回 MENU 页**不动**。
            //
            // 上一版（2026-10-06）留的三点仍然成立：
            // * **X / Y 在裸键** —— 换面板面与换色板是这一屏最高频的两个动作，
            //   原来要按 SELECT+X / SELECT+Y（两根手）。**那两个和弦仍然留着**（`MaskCycle`/`ColorCycle`），
            //   老习惯不至于失灵；卡座上它们本来就没有别的用处。
            // * **←/→ 从「上/下一首」改成「快退/快进」**，上/下一首挪到 ↑/↓。
            //   而这一条要**播放器第一次有定位能力**才成立（`music::Cmd::Seek`）——
            //   在此之前"快退"只能是重新 open 整个文件（= 回开头），那不叫快退。
            Phase::Listening { key: _, playing: _, disc: _ } => match action {
                Action::GbaDown(Btn::A) => {
                    self.deck_action(slot_ui::lcd::Key::Play);
                    self.sync_phase_playing();
                }
                // ★ 2026-11-02（六六）：**长按 A = 停止**（回曲首并停下，**不离开这一屏**）。
                //   停止这个动作从 B 挪到 A 的长按上 —— B 让给"返回碟片货架"。
                Action::DeckStop => {
                    self.deck_stop();
                    self.sync_phase_playing();
                }
                // B = **返回碟片货架**：**离开播放器**的那条路还在这一颗键上（不再是 MENU）。
                Action::GbaDown(Btn::B) => {
                    // 回货架 ⇒ **停在碟片那一行**，而不是"切一下"：从播放器退出来的人
                    // 是要换碟的，让他自己再按两次方向键找到那一行是白给的。
                    self.leave_deck();
                    self.transport = slot_ui::lcd::Transport::Paused;
                    self.show_row(ShelfRow::Discs);
                    self.phase = Phase::Shelf;
                }
                // 裸键换面板面 / 换色板。
                Action::GbaDown(Btn::X) => {
                    self.toggle_panel();
                }
                Action::GbaDown(Btn::Y) => {
                    self.cycle_ramp();
                }
                // 快退 / 快进：按下立刻跳一步，按住由 `timers` 连跳，松开停。
                Action::GbaDown(Btn::Left) => {
                    self.deck_seek_dir = -1.0;
                    self.deck_seek_at = self.now() + SEEK_REPEAT_MS;
                    self.deck_seek(-1.0);
                }
                Action::GbaUp(Btn::Left) => self.deck_seek_end(),
                Action::GbaDown(Btn::Right) => {
                    self.deck_seek_dir = 1.0;
                    self.deck_seek_at = self.now() + SEEK_REPEAT_MS;
                    self.deck_seek(1.0);
                }
                Action::GbaUp(Btn::Right) => self.deck_seek_end(),
                // 上一首 / 下一首。
                Action::GbaDown(Btn::Up) => {
                    self.deck_action(slot_ui::lcd::Key::Prev);
                    self.sync_phase_playing();
                }
                Action::GbaDown(Btn::Down) => {
                    self.deck_action(slot_ui::lcd::Key::Next);
                    self.sync_phase_playing();
                }
                Action::GbaDown(Btn::Start) => {
                    self.cycle_eq();
                }
                // ★ 2026-11-02（六六）：**SELECT 空操作**。信息页改由 MENU 短按（`OpenMenu`）进，
                //   B 让给「返回碟片货架」。SELECT 在这条屏上不再承担功能——落到 `_ => {}`。
                // L and R change the disc — **the shoulders are for the disc, the arrows are
                // for the track.** One pair of keys per job.
                Action::GbaDown(Btn::L1) => {
                    if let Some(started) = self.change_disc(-1) {
                        self.transport = if started {
                            slot_ui::lcd::Transport::Playing
                        } else {
                            self.transport
                        };
                        let t = self.disc;
                        self.phase = Phase::Listening {
                            key: self.key_focus.min(4),
                            playing: started,
                            disc: t,
                        };
                    }
                }
                Action::GbaDown(Btn::R1) => {
                    if let Some(started) = self.change_disc(1) {
                        self.transport = if started {
                            slot_ui::lcd::Transport::Playing
                        } else {
                            self.transport
                        };
                        let t = self.disc;
                        self.phase = Phase::Listening {
                            key: self.key_focus.min(4),
                            playing: started,
                            disc: t,
                        };
                    }
                }
                // SELECT+X / SELECT+Y 保留：老习惯不失效，而它们在卡座上本就没有别的用处。
                Action::MaskCycle => {
                    self.toggle_panel();
                }
                Action::ColorCycle => {
                    self.cycle_ramp();
                }
                // ★ MENU 的两级（★ 2026-11-02 六六改键位后）：**短按进碟片信息页，长按回 MENU 页**。
                //   两个都是手势层分好的两件事（`OpenMenu` / `MenuHold`），这一侧不必再读时钟 ——
                //   而"读时钟分辨短按长按"是那种在 60fps 下必然出错的写法。
                //   长按时手势层**同时**发一个 `Eject`（老行为：弹仓），而卡座上没有仓可弹，
                //   所以 `Eject` 落到下面的 `_ => {}`；货架那边的 `Eject` 一个字没动。
                Action::MenuHold => {
                    // Paused rather than stopped: the thread and the open track stay, so coming
                    // back to the deck resumes this disc where it stopped. A deck that forgot
                    // what it was playing because the user checked the menu would be wrong.
                    self.leave_deck();
                    self.transport = slot_ui::lcd::Transport::Paused;
                    self.phase = Phase::Menu {
                        index: slot_ui::menu::first_slot(),
                    };
                }
                // ★ 2026-11-02（六六）：**MENU 短按 = 进碟片信息页**（原由 SELECT 承担，现 SELECT 空操作）。
                //   记下 `info_back`，信息页的 B/SELECT 会据此回到卡座。
                Action::OpenMenu => {
                    self.deck_seek_end();
                    // 记住来路，见 `info_back`。
                    if let Phase::Listening { key, playing, disc } = self.phase {
                        self.info_back = Some((key, playing, disc));
                    }
                    self.phase = Phase::DiscInfo;
                }
                _ => {}
            },
            // ── 信息页：**← → 翻页，B 或 SELECT 收掉它**，回到它被呼出时的那块屏。
            //   （2026-10-07 之前写的是"其余任何一键都收掉它"，那一版把 `GbaUp` 也算进去了。）
            //
            //   ★ 2026-10-06 重做时改的：这一页从**一页**变成**两页**（碟片与曲目 / 键位指南），
            //   所以 ← → 从"空着的键"变成了**翻页**。选这两个键有两个理由：
            //   ① 它们在这一页本来就没有别的用处（卡座上是快退快进，而停下来看字时没人要快退）；
            //   ② 底下一行常驻写着 `n/2 ←→ 翻页`（`lcd::info_foot`），**屏上说了怎么翻**，
            //   所以不需要教会用户。
            //
            //   ⚠️ **翻页后这一帧不会立刻重烘**（烘焙在 `sync_info_panel`，下一帧才跑）——
            //   那是**故意**的：一帧的旧图比一次同步合成卡顿好，而且这一页本来就不是
            //   每帧都在变的东西。
            //
            //   回的是 `info_back` 而不是硬写 `Listening`：信息页理论上可以在别的屏上被呼出，
            //   硬写会把用户扔到一个他没在的屏上。
            Phase::DiscInfo => {
                self.deck_seek_end();
                match action {
                    Action::GbaDown(Btn::Left) => {
                        self.step_info_page(-1);
                    }
                    Action::GbaDown(Btn::Right) => {
                        self.step_info_page(1);
                    }
                    // ★ `Y` 换色板在这一页**照样有效** —— 这一页现在用的是面板那一套四阶色板
                    //   （`sync_info_panel` 把 `deck_ramp` 传进 `lcd::render`），所以在播放屏上
                    //   换色板的那一下，在这里也该看得见。而它**必须**在这里有效，否则
                    //   `info_built` 里的色板档位这一格就永远不会变。
                    Action::GbaDown(Btn::Y) => {
                        self.cycle_ramp();
                    }
                    // ★ **收起来的是"按下去"，不是"松开"。** 2026-10-07 六六报
                    //   「按 SELECT 的信息页不能常驻，长按也是一闪而过」—— 真因就是下面原先
                    //   那个 `_ =>` 兜底：松开 SELECT 会让手势层在 `SELECT_TAP_MS = 50` 之后
                    //   **补发一个 `GbaUp(Select)`**（见 `gesture.rs`：按下先发 `GbaDown`，
                    //   50 ms 后再发 `GbaUp`），那一拍正好落进 `_` 里 ⇒ **刚开 50 ms 就被自己关掉**。
                    //   同一个坑还会吃掉"松开 ←/→"。所以现在只有明确的两下能关它。
                    Action::GbaDown(Btn::B) | Action::GbaDown(Btn::Select) => {
                        self.phase = match self.info_back {
                            Some((key, playing, disc)) => Phase::Listening { key, playing, disc },
                            None => Phase::Shelf,
                        };
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Applied at a stated moment rather than at whatever the accumulated clock has reached.
    /// The clock is set rather than advanced: a caller that says when something happened is
    /// stating the whole timeline, not adding to one.
    pub fn apply_at(&mut self, action: Action, now: Millis) {
        self.clock = now as f64;
        self.apply(action);
    }

    /// `true` if the action was one of the three levels, whether or not it moved. A press
    /// that hits an end still shows the bar, which is how the end announces itself.
    fn adjust(&mut self, action: Action) -> bool {
        if action == Action::MuteToggle {
            self.mute_toggle();
            return true;
        }
        let s = &self.state;
        let (kind, value) = match action {
            Action::BrightnessUp => (HudKind::Brightness, up(s.brightness, 1, BRIGHTNESS_MAX)),
            Action::BrightnessDown => (HudKind::Brightness, s.brightness.saturating_sub(1)),
            Action::BlueLightUp => (HudKind::BlueLight, up(s.blue_light, 1, BLUE_LIGHT_MAX)),
            Action::BlueLightDown => (HudKind::BlueLight, s.blue_light.saturating_sub(1)),
            Action::VolumeUp => (HudKind::Volume, up(s.volume, VOLUME_STEP, VOLUME_MAX)),
            Action::VolumeDown => (HudKind::Volume, s.volume.saturating_sub(VOLUME_STEP)),
            _ => return false,
        };
        if kind == HudKind::Volume {
            self.remember_volume();
        }
        let level = match kind {
            HudKind::Brightness => &mut self.state.brightness,
            HudKind::BlueLight => &mut self.state.blue_light,
            HudKind::Volume => &mut self.state.volume,
            // The bar is shared with rewind, which is not a level and is never an action.
            HudKind::Rewind => return false,
        };
        let moved = *level != value;
        *level = value;
        // Turning it up or down is the plainest way to say you want to hear it again.
        let unmuted = kind == HudKind::Volume && std::mem::take(&mut self.state.muted);
        // ★ **The deck keeps its own level, and only this pushes it.** `state.volume` is what the
        //   bar reads and what the core is told; the music player reads its own atomic on the
        //   decode thread, so a press that moved `state` alone was a key that landed nowhere —
        //   六六 2026-10-07「音量调节键失灵」, and it was broken *only while music was playing*,
        //   which is exactly the window in which nothing else in this file runs. One line covers
        //   both screens, and it reads `output_volume` so a muted deck stays silent.
        if kind == HudKind::Volume {
            self.deck.set_volume(self.output_volume());
        }
        let (shown, now) = (self.hud_value(kind, value), self.now());
        self.hud.show(kind, shown, self.state.muted, now);
        if let (HudKind::Brightness, Some(power)) = (kind, &mut self.power) {
            power.set_backlight(value);
        }
        // A key held against an end would otherwise rewrite the file at the repeat rate.
        if moved || unmuted {
            self.persist();
        }
        true
    }

    /// Where the volume stood before the press about to happen. Only the last two are kept:
    /// a chord is two presses, and anything older belongs to a gesture that already ended.
    fn remember_volume(&mut self) {
        if self.vol_before.len() == 2 {
            self.vol_before.remove(0);
        }
        self.vol_before
            .push((self.state.volume, self.state.muted, self.now()));
    }

    /// Silence is a state rather than a level, so muting neither moves the number nor is
    /// moved by the two presses that asked for it. Both keys fire their own adjustment on
    /// the way to the chord, and from an end those two do not cancel.
    fn mute_toggle(&mut self) {
        let now = self.now();
        if let Some((volume, muted, _)) = self
            .vol_before
            .iter()
            .find(|(_, _, at)| now.saturating_sub(*at) <= MUTE_CHORD_MS)
            .copied()
        {
            self.state.volume = volume;
            self.state.muted = muted;
        }
        self.vol_before.clear();
        self.state.muted = !self.state.muted;
        // The deck has its own level, so the mute has to reach it as well as the core.
        self.deck.set_volume(self.output_volume());
        self.hud
            .show(HudKind::Volume, self.output_volume(), self.state.muted, now);
        self.persist();
    }

    /// What the bar reads. Muted draws as an empty bar under the muted glyph, which is what
    /// zero already looks like and is what it already means.
    fn hud_value(&self, kind: HudKind, value: u8) -> u8 {
        match kind {
            HudKind::Volume => self.output_volume(),
            _ => value,
        }
    }

    /// Pushed from outside because only the emulator knows how much history is left. Held
    /// open until `hide_rewind`, unlike the levels.
    pub fn show_rewind(&mut self, fill: u8) {
        let now = self.now();
        // Rewind is not a level and cannot be silenced, so it is never the muted glyph.
        self.hud.show(HudKind::Rewind, fill, false, now);
    }

    pub fn hide_rewind(&mut self) {
        self.hud.release_rewind();
    }

    /// Pushed from outside for the same reason the rewind fill is: held and latched are one
    /// action apiece to the app and two different things on screen.
    pub fn set_ff(&mut self, ff: FfState) {
        self.hud.set_ff(ff);
    }

    pub fn ff_badge(&self) -> Option<Icon> {
        self.hud.badge()
    }

    pub fn blue_light(&self) -> u8 {
        self.state.blue_light
    }

    /// The level the user chose, which a mute does not touch.
    pub fn volume(&self) -> u8 {
        self.state.volume
    }

    pub fn muted(&self) -> bool {
        self.state.muted
    }

    /// What the sink is actually to be set to. The only one of the two the audio path may
    /// read: a muted device at level 70 is silent, not 70.
    pub fn output_volume(&self) -> u8 {
        if self.state.muted {
            0
        } else {
            self.state.volume
        }
    }

    /// The level the **music deck** is really playing at — a number of its own that only a volume
    /// press moves. Nothing on screen draws it, so this exists for the tests: without it, "the
    /// press reached the deck" is not a thing that can be asserted.
    pub fn deck_volume(&self) -> u8 {
        self.deck.volume()
    }

    pub fn hud_icon(&self) -> Icon {
        self.hud.glyph()
    }

    pub fn now(&self) -> Millis {
        self.clock as Millis
    }

    fn flick(&mut self, step: fn(&mut Polaroids)) {
        if let Some(p) = &mut self.polaroids {
            step(p);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.clock += dt as f64 * 1000.0;
        self.timers();
        // The system-shelf swap, before the phase walk: it advances the beats, swaps the view
        // at the bottom of the ascend, and hands the row its motion for this frame. With no
        // swap in flight it clears the motion, so a still row is laid out as it always was.
        self.update_shelf_switch(dt);
        // A queue poll rather than a syscall, so the frame loop can afford it every frame —
        // which is the whole reason the slow parts of starting a link are on a thread of
        // their own.
        self.poll_link();
        let now = self.now();
        // The cart opens once its board is on the GPU, so a slow build is a pause on the shelf
        // rather than an animation spent before its first frame.
        let ready = self.core_faces_ready();
        if let Some(picker) = &mut self.core_picker {
            if picker.waiting() && (ready || picker.waited(now) >= FACES_WAIT_MS) {
                picker.start(now);
            }
        }
        // The lid is back on, so the shelf is the shelf again.
        if self.core_picker.is_some_and(|p| p.finished(now)) {
            self.core_picker = None;
        }
        // A direction still held as the shelf leaves the screen is not held when it comes
        // back: the row repeats only while it is the thing being looked at.
        if !self.on_shelf() {
            self.row_mut().release_hold();
            self.letters.release_hold();
        }
        let mut touched = false;
        // ★ 2026-10-27：投币素材有没有烘出来（老卡没有）。借用规则上这个值要在 match 之前取：
        //   `match &mut self.phase` 已经把 `self` 占住了，里面调不了 `&self` 的方法。
        let coins_loaded = !self.coins.is_empty();
        // ★ 2026-10-26：光盘交接（插入 → `Playing`）那一刻的插入时间。match 里借着
        //   `self.phase`，写不了 `self`，所以先落在这个局部里，出了 match 再抄进 `disc_live`。
        let mut disc_handoff: Option<f32> = None;
        let next = match &mut self.phase {
            Phase::Shelf => {
                // The row that is up, not the cart row: a disc row that does not tick does not
                // glide, and the caret would jump instead of travelling.
                self.row_mut().tick(now);
                self.row_mut().update(dt);
                None
            }
            Phase::Inserting {
                cart,
                t,
                core_ready,
                resumed,
                disc,
                ..
            } => {
                let was = *t;
                // ★★ 2026-10-27（六六）：「**MD 插卡没动作，卡顿后就在卡槽里了**」。
                //
                //   根子不在 MD 自己的动画：`Session::sync_core` 在**进入 `Inserting` 的
                //   第一帧**里同步调 `open_core`（dlopen 核心 + `retro_init` + 读 ROM），
                //   那一下是真停 —— 冷启动的 `genesis_plus_gx` 实测能停几百毫秒。
                //   而 `dt` 是 `frontend.rs::advance` 里 `self.last.elapsed()` 的**真实经过
                //   时间**，于是那一帧 `t` 从 0 直接跨过 `SEATED_AT`(0.45)，`seat()` 一次到 1：
                //   整段插卡动画被那一帧吃掉，屏幕上只剩"卡顿 → 卡带已经在槽里"。
                //
                //   ⇒ 这条时钟**按帧封顶**：一帧最多走 `INSERT_STEP_MAX`（1/20 s）。卡顿吃掉
                //   的是"帧数"，不再是"行程"：动画一定从头走满，代价只是卡顿期间画面停在
                //   抬头前那一格（`draw_loading` 的呼吸字照常在转，读起来就是"在加载"）。
                //   60 fps 下 dt≈0.0167，远在封顶之下，正常帧一帧都不受影响。
                *t += dt.min(INSERT_STEP_MAX);
                // Started early enough that the contacts in the clip land on the frame
                // the cart does. A resumed cart never travelled, so it never touched
                // anything.
                let at = SEATED_AT - Sfx::Insert.lead();
                touched = !*resumed && was < at && *t >= at;

                // 更新光盘状态的核心就绪时间
                if let Some(disc_state) = disc.as_mut() {
                    if *core_ready && disc_state.core_at.is_none() {
                        disc_state.core_at = Some(*t);
                    }
                }

                // ★★ 2026-10-26（六六）：**光盘不再刹停再进游戏**。
                //   原话："旋转到游戏加载完毕进入的时候还是太突兀，能不能加载完毕的时候光盘
                //   继续转，屏幕出现 CRT 电视开机的效果"。
                //   旧版这里是 `t >= core_at + DISC_SPIN_DOWN_S`：主轴先刹 0.40 s、盘面在
                //   随机相位停死，然后画面才硬切进来 —— 那 0.40 s 就是"突兀"。
                //   现在核心一就绪就换相位，主轴**不刹**（`DiscLive` 接着末帧的相位继续转），
                //   CRT 开机的那一下交给 `self.screen`（见 `Playing` 的绘制臂）。
                //
                //   `max(DISC_SPIN_START)` 是地板：核心再快，也要等光盘真沉进洞里、
                //   主轴起转之后再交给 CRT —— 否则画面在盘还悬在洞口时就开始长，盘会瞬移。
                let should_transition = match disc.as_ref() {
                    // ★ 2026-10-27：投币 —— 代币整枚出屏 + 街机箱体升到位之后才准进游戏。
                    // ★ 2026-10-28（六六）：**不再提前**。箱体完全升到位（`coin_total_s()`）
                    //   且核心就绪才切 `Playing`、CRT 才开 —— 否则 CRT 长满整屏前箱体还没
                    //   升到位，CRT 动画被截断（六六：「感觉现在提前 CRT 动画播不全」）。
                    Some(d) if d.coin => *t >= slot_ui::coin_total_s() && *core_ready,
                    Some(d) => d
                        .core_at
                        .map(|ca| *t >= ca.max(slot_ui::DISC_SPIN_START))
                        .unwrap_or(false),
                    // 普通卡带：等 INSERT_S + core_ready
                    None => *t >= INSERT_S && *core_ready,
                };
                // 只有"盘还在转"需要交接相位；代币不自转，没有可交接的东西。
                if should_transition && disc.as_ref().is_some_and(|d| !d.coin) {
                    // 交接：把这一刻的相位/时钟/插入时间抄下来，绘制臂靠它把"还在转"接上。
                    disc_handoff = Some(*t);
                }
                should_transition.then(|| Phase::Playing {
                    cart: std::mem::take(cart),
                })
            }
            // Two movements, in the order the insert made them: the panel goes out, and only
            // once there is nothing on it does the cart start to travel. A cart sliding out
            // across a live picture is the insert played back with its halves overlapping.
            // The clock only starts once the picture is out, and it starts below zero: the
            // beat before the cart moves is that stretch. The contacts let go as it starts
            // moving, which is neither when the button was held nor while the screen is
            // still going down.
            Phase::Ejecting { t, cart, .. } => {
                if self.screen <= 0.0 {
                    let was = *t;
                    // 同 `Inserting` 的封顶（见 `INSERT_STEP_MAX`）：拔卡这一下要先同步写即时
                    // 存档，一次卡顿同样会把 `EJECT_S` 一帧走完 —— 那就成了"卡带瞬间弹出"。
                    *t += dt.min(INSERT_STEP_MAX);
                    touched = was < 0.0 && *t >= 0.0;
                }
                // ★ 2026-10-26（六六）：PS1 光盘五拍弹出要走满 `DISC_EJECT_S`（1.23 s），
                //   普通卡带用 `EJECT_S`（0.45 s）。两者各管各的时长。
                //   `system_of` 借整块 self，在 `&mut self.phase` 里会撞借；直接读
                //   `self.shelf`（与 phase 不相交）绕过。★ 2026-11-09：改走 `Shelf::find`
                //   （当前机种优先），否则同名跨机种时弹出时长按另一台机器的卡算。
                let system = self.shelf.find(cart.as_str()).map(|(c, _)| c.system());
                let is_disc = system == Some(slot_store::System::Ps1);
                let limit = if is_disc {
                    DISC_EJECT_S
                } else if system.is_some_and(slot_store::System::is_coin) && coins_loaded {
                    // ★ 2026-10-27：投币的弹出就是插入那条时间轴倒放，长度和插入一样。
                    //   没有素材时画的是卡带那一套，长度也得跟着退回 `EJECT_S`。
                    slot_ui::coin_total_s()
                } else {
                    EJECT_S
                };
                (*t >= limit).then_some(Phase::Shelf)
            }
            // The card closes on where the last press asked it to go rather than arriving there
            // between two frames: a page of text that changed while the eye was mid-line is a
            // page that has to be found again. `want` is the press and `scroll` is the picture,
            // which is the same split the letter strip keeps.
            Phase::Shortcuts { scroll, want } => {
                if (*want - *scroll).abs() < 0.5 {
                    *scroll = *want;
                } else {
                    *scroll += (*want - *scroll) * (CARD_EASE * dt).min(1.0);
                }
                None
            }
            // The settings list settles on the row in hand the same way the card settles on where
            // the last press asked it to go, and for the same reason: a field still moving while
            // the eye is on it is a field that has to be found again.
            //
            // ★ **`want` is derived here rather than stored beside the row** (2026-10-10). One
            //   press moves one row, so the row already says where the window belongs; a second
            //   field could only disagree with it, and the disagreement would show up as a gap
            //   after `↑` wraps from the first row to the last. The phase therefore carries only
            //   the settled offset, which is what the ease needs as a starting point.
            Phase::Settings { row, scroll } => {
                let want = slot_ui::settings::scroll_for(row.index());
                if (want - *scroll).abs() < 0.5 {
                    *scroll = want;
                } else {
                    *scroll += (want - *scroll) * (SETTINGS_EASE * dt).min(1.0);
                }
                None
            }
            _ => None,
        };
        // ★ 2026-10-21：菜单里那页操作键映射的缓动 settle。挂在这里而不是 settle 臂里：
        // 上面那个 match 借着 `phase`，而这一页的状态在 `game_menu` 里 —— 两只手不冲突。
        if self.game_menu.is_some() {
            self.game_menu_shortcuts_settle(dt);
        }
        // The strip, after the row has moved rather than as part of it. Out here because it
        // needs the whole of `self` — the repeat reaches the caret, which is the shelf's — and
        // the match above is holding `phase` borrowed. The order within it is the order the
        // three things depend on each other: the repeat may move the marker, the spring may
        // move it further, and the last line puts the marker back on the cart the row is
        // actually showing. That last line is what makes the strip a readout as well as a
        // control — an arrow on the row moves the caret and the strip follows it with nothing
        // telling it to.
        if self.on_shelf() {
            self.repeat_letters(now);
            if let Some(cart) = self.row().current_cart() {
                self.letters.centre_on(cart.initial);
            }
            self.letters.settle(dt);
        }
        // One clip for the whole movement, and the only thing done to it is when it starts.
        //
        // ★★ 2026-10-26：**光盘不是卡带。** PS1 的进出是托盘/吸入 + 伺服起停，卡带是滑轨 + 触点 ——
        // 两条完全不同的机械。共用那两条真实录音会听起来像这台机器装错了零件。
        // 判据用相位手里那张卡的机种（弹出时 cart 还在，插入时也在），不靠别的旗子。
        // ★★ 2026-10-27（六六）：「缺的音效你顺便生成吧」。
        //   **第三种零件**：ARCADE / NEOGEO 投的不是卡带，是金属币。之前这里只有一条
        //   "是不是 PS1"的判断，投币落进 `_ => Sfx::Insert` —— 每投一次币响的都是卡带
        //   滑轨 + 触点那条真实录音，等于给这台机器装错了零件。
        //   判据取相位手里那张卡的机种（弹出时 cart 还在，插入时也在），不靠别的旗子。
        if touched {
            let system = match &self.phase {
                Phase::Inserting { cart, .. } | Phase::Ejecting { cart, .. } => {
                    self.shelf.find(cart.as_str()).map(|(c, _)| c.system())
                }
                _ => None,
            };
            self.sfx = Some(match (system, &self.phase) {
                (Some(slot_store::System::Ps1), Phase::Ejecting { .. }) => Sfx::DiscOut,
                (Some(slot_store::System::Ps1), _) => Sfx::DiscIn,
                // 投币：没有"退币"那条录音，币在金属道上走，正反都说得通，共用一条。
                (Some(s), _) if s.is_coin() => Sfx::Coin,
                (_, Phase::Ejecting { .. }) => Sfx::Eject,
                _ => Sfx::Insert,
            });
        }
        if let Some(phase) = next {
            // Ahead of the screen step, so the frame the cart finishes arriving is already
            // the first frame of the power on rather than one more frame of nothing.
            self.phase = phase;
            // The cart is out. Whatever it was carrying goes with it.
            self.refused_from = None;
            // `slot.state` mirrors the slot, so it changes where the phase does: seated on
            // the way in, empty on the way back to the shelf whether that was an eject or
            // a refusal.
            let seated = match &self.phase {
                Phase::Playing { cart } => Some(cart.clone()),
                _ => None,
            };
            self.record_cart(seated);
            // ★★ 2026-10-26（六六）：光盘刚就绪、CRT 正要开机 —— 把这一刻的相位抄下来，
            //   让"还在转"从插入动画的末帧续上（`Playing` 的绘制臂在 `screen < 1` 时用它）。
            self.disc_live = disc_handoff.map(|t| DiscLive::handoff(t, self.clock));
            // ★ 2026-10-21：重开本局不再走这条路。它曾经在这里"落回货架就立刻干净重插"，
            //   但插回去会先接上拔卡那一刻的即时存档，等于把重开做成了白做。现在是核心自己的
            //   `retro_reset`（见 `restart_game`），相位一帧都不动，这里也就没有要补的事了。
        }
        self.step_screen(dt);
    }

    /// The game layer's own power, which answers to the phase rather than to an event: an
    /// insert, a resume and a wake all bring the picture up the same way.
    fn step_screen(&mut self, dt: f32) {
        let lit = matches!(self.phase, Phase::Playing { .. } | Phase::Polaroids { .. });
        let step = if lit {
            dt / POWER_ON_S
        } else {
            -dt / POWER_OFF_S
        };
        self.screen = (self.screen + step).clamp(0.0, 1.0);
        // ★ 2026-10-26：画面已经铺满整屏，插卡位那个洞也被一起盖住了 —— 洞里那颗还在转的
        //   光盘到此为止，之后不需要（也不该）再画。
        if self.screen >= 1.0 {
            self.disc_live = None;
        }
    }

    /// 0.0 dark, 1.0 fully on. The compositor scales and brightens the game layer by it, and
    /// nothing may draw the game at all while it is zero.
    pub fn screen_power(&self) -> f32 {
        self.screen
    }

    pub fn set_game_ready(&mut self, ready: bool) {
        self.game_ready = ready;
    }

    /// Whether the draw list carries the game layer. A core that has published nothing would
    /// otherwise show the last cart's final frame for the length of the insert.
    pub fn game_visible(&self) -> bool {
        self.game_ready && self.screen > 0.0
    }

    /// Jumps the clock without advancing an animation. The autosave and the doze timeout
    /// are minutes apart, which is further than a test wants to walk a frame at a time.
    pub fn tick_ms(&mut self, now: Millis) {
        self.clock = self.clock.max(now as f64);
        self.timers();
    }

    /// Everything the clock alone drives. The play hold is the one thing here the user did
    /// ask for; it is only the clock that decides which of the two things it was.
    fn timers(&mut self) {
        self.play_hold();
        self.fav_hold();
        self.sys_hold();
        self.mtp_hold();
        self.wifi_poll();
        self.deck_advance();
        self.deck_seek_hold();
        // The "connect the cable" beat hands over to the live screen on its own, a moment after
        // the gadget has been switched.
        // While the connect beat is up, poll for the address the enter script leaves behind: the
        // beat hands over to the live screen the moment one appears, however long the join took.
        if self.direct_link == Some(1) && self.now().saturating_sub(self.direct_at) >= 1000 {
            self.direct_at = self.now();
            if let Ok(ip) = std::fs::read_to_string("/tmp/slot-ip") {
                let ip = ip.trim().to_string();
                if !ip.is_empty() {
                    self.direct_ip = Some(ip);
                    self.direct_ip_dirty = true;
                    self.direct_link = Some(2);
                }
            }
        }
        // The grace period can run out with the switcher open, so the hint answers to the
        // clock rather than to whatever was on offer on the way in.
        let offer = self.undo_label();
        if let Some(p) = &mut self.polaroids {
            p.set_undo(offer);
        }
        if self.doze_expired() {
            self.on_doze_timeout();
        }
        if self.now() >= self.autosave_at {
            self.flush_resume();
        }
        if self.now() >= self.battery_at {
            self.battery_at = self.now() + BATTERY_POLL_MS;
            self.battery = self.power.as_ref().and_then(|p| p.battery());
            if let Some(b) = self.battery {
                self.on_battery(b);
            }
        }
        // Only the charge half. The percent it is written beside is at most one slow tick
        // old, which is the staleness the slow tick was always chosen for.
        if self.now() >= self.charge_at {
            self.charge_at = self.now() + CHARGE_POLL_MS;
            if let (Some(power), Some(b)) = (self.power.as_ref(), self.battery.as_mut()) {
                b.charge = power.charge();
            }
            // On the fast tick rather than the slow one: an amber-on-plug-in that lags ten
            // seconds behind the cable is worse than no LED at all.
            let state = self.led_state();
            self.set_led(state);
        }
    }

    /// Modelled on the OG SP: green running, red low, amber charging, green once it is full.
    /// Charging outranks low, since a flat device on a cable is filling rather than dying.
    pub fn led_state(&self) -> LedState {
        let Some(b) = self.battery else {
            return LedState::Running;
        };
        match b.charge {
            Charge::Charging => LedState::Charging,
            Charge::Full => LedState::Charged,
            _ if b.percent <= BATTERY_LOW => LedState::Low,
            _ => LedState::Running,
        }
    }

    /// The one place that ever reaches the platform's own `set_led`, so the edge kept in
    /// `last_led` cannot be bypassed by a call site that forgot it. Called every second with
    /// whatever `led_state` just computed, so on any device that never asserts a charge state
    /// this is the only branch pair — `Low` and `Running` — a write ever leaves this function
    /// with; a state repeated from the previous second returns before touching the platform.
    fn set_led(&mut self, state: LedState) {
        // A shutdown darkens the case and nothing lights it again. The fast tick recomputes
        // `led_state` from the gauge every second and knows nothing about a shutdown in
        // progress, so a charge tick landing inside the window between the choice and
        // `poweroff` put the light straight back to green for the five seconds rcK takes.
        // Guarded here rather than at that call site for the same reason the edge is: this is
        // the one seam, and a caller cannot forget what it never has to remember.
        if self.shutting_down() && state != LedState::Off {
            return;
        }
        if self.last_led == Some(state) {
            return;
        }
        self.last_led = Some(state);
        if let Some(power) = self.power.as_mut() {
            power.set_led(state);
        }
    }

    fn record_cart(&mut self, cart: Option<String>) {
        if self.state.cart == cart {
            return;
        }
        self.state.cart = cart;
        self.persist();
    }

    fn persist(&self) {
        let Some(root) = &self.root else {
            return;
        };
        if let Err(e) = write_slot_state(root, &self.state) {
            eprintln!("slot: slot.state: {e}");
        }
    }

    /// ★ 2026-10-21：保存设置弹窗的 **X = 按游戏保存**。设置本身就是按游戏一份
    /// （`Saves/<stem>` 那一族跟 stem 走），这里的"按游戏"是显示档位那一族
    /// （`System/display.txt` 的 mask/cc）—— 它们现在是全局一份，按游戏保存意味着
    /// 记下当前这张卡的名字，回插时还原。
    ///
    /// ★ **待六六拍板**：display.txt 的按游戏持久化还没落（要动 `DisplayFilter` 的存储
    /// 形状），这一版先把"当前游戏"记进 `slot.state`（`state.cart` 已经在记），保存动作
    /// 本身只是把全局那份当场写盘 —— 语义上等价于"确认保存"，路径见
    /// `MEMORY-游戏菜单.md` 的差异表。
    fn persist_per_game(&self) {
        self.persist();
        let Some(root) = &self.root else {
            return;
        };
        // 显示档位那一族是改即写（`cycle_mask`/`cycle_cc` 都当场 `write_display_modes`），
        // 所以这里再写一次是幂等的：按游戏保存 = 确认全局那份就是这张卡的。
        // `write_display_modes` 返回 `()`（不返回 `Result`），失败静默。
        crate::root::write_display_modes(root, self.display.mask_mode, self.display.cc, self.display.scanline_state);
    }

    /// ★ 2026-10-21：保存设置弹窗的 **Y = 恢复默认**。回到 `SlotState::default()` 那一份
    /// （亮度/背光/音量/主题/货架角标等），当场写盘 —— 恢复默认和保存是同一件事的两半，
    /// 只做一半会让"恢复"在下次开机时又弹回去。
    ///
    /// 显示档位（`mask_mode` / `cc`）不在 `SlotState` 里：它自己走 `System/display.txt`，
    /// 由 `display` 结构体当场写盘（`write_display_modes` 是改即写，返回 `()`，不返回
    /// `Result`）。这里重置 `state` 并写一份干净的 `slot.state`；显示档位保持现状，
    /// 因为它的"默认"是开机读文件读出来的那份，重置它等于把开机配置也抹掉。
    fn reset_settings(&mut self) {
        self.state = SlotState::default();
        self.state.cart = self.selected_stem().map(str::to_string);
        self.mode_dirty = true;
        self.persist();
    }

    /// ★ 2026-10-21：游戏菜单"操作键映射"那一页能滚多远。和货架那页
    /// （`card_scroll_max`）同一套算法：行高之和 + 脚底线，超出 720 的部分是滚动余量。
    fn game_menu_shortcuts_max(&self) -> f32 {
        self.card_scroll_max()
    }

    /// ★ 2026-10-21：操作键映射那页的缓动 settle，从 `tick` 里每帧叫。`scroll` 追 `want`，
    /// 同一个阻尼系数和货架那页共用（`CARD_EASE * dt`，见 `Phase::Shortcuts` 的 settle 段）。
    fn game_menu_shortcuts_settle(&mut self, dt: f32) {
        let Some(GameMenu::Shortcuts(scroll, want)) = self.game_menu else {
            return;
        };
        if (scroll - want).abs() < 0.5 {
            if scroll != want {
                self.game_menu = Some(GameMenu::Shortcuts(want, want));
            }
            return;
        }
        let next = scroll + (want - scroll) * (CARD_EASE * dt).min(1.0);
        self.game_menu = Some(GameMenu::Shortcuts(next, want));
    }

    /// ★ 2026-10-21：**色板浏览器从菜单里进**。原入口是长按 SELECT+Y（`open_palette_browser`
    /// 的 `Phase::Playing` 守卫和模型守卫都留着），菜单这一条路立 `pal_from_menu`：
    /// 浏览器的 B/A 退出时看它决定回游戏还是回菜单。
    fn open_palette_browser_from_menu(&mut self) {
        if self.pal_cursor.is_some() {
            self.game_menu = Some(GameMenu::Rows(3));
            return;
        }
        self.pal_from_menu = true;
        self.open_palette_browser();
        // 开成了才把面板从 Rows 换到浏览器那一屏；`refuse()`（GB 模型在手上）时
        // `pal_cursor` 还是 `None`，菜单留在原来的行上，旗子收回。
        if self.pal_cursor.is_some() {
            self.game_menu = None;
        } else {
            self.pal_from_menu = false;
        }
    }

    /// ★ 2026-10-21：**屏幕档位**那一行 —— GBA 走网格（`cycle_mask`），GB/GBC 走遮罩
    /// 艺术环（`overlay_next`）。跟 `Action::MaskCycle` 那两条分派同一个 fork，一格一档、
    /// 当场写盘，因为"档位"是改了就上屏的东西，菜单开着也要能看见它在变。
    fn cycle_machine_grid(&mut self) {
        if self.current_machine() != System::Gba {
            self.overlay_next();
        } else {
            self.cycle_mask();
        }
    }

    pub fn on_core_ready(&mut self) {
        if let Phase::Inserting { core_ready, .. } = &mut self.phase {
            *core_ready = true;
        }
    }

    pub fn on_core_failed(&mut self) {
        let caught = self.seat();
        let Phase::Inserting { cart, .. } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        // Resumed at the depth it caught rather than at zero, so the refusal reads as one
        // movement instead of a jump to seated and back out.
        let t = (1.0 - caught) * EJECT_S;
        self.phase = Phase::Ejecting { cart, t };
        // No shake here. The cart is on screen and carries the alert instead, and a screen
        // that flinched as well would read as two separate failures.
        self.refused_from = Some(t);
    }

    /// Any action the app will not carry out. There are no words for it and no state to
    /// clear: it decays on its own clock, wherever it is being drawn.
    pub fn refuse(&mut self) {
        // ★ 2026-10-26：**拒绝也有声音**，而且**只在这一处发** —— 全前端十条拒绝路径
        // （没卡的机种、坏卡、不能倒带/快进/慢放、菜单拒绝……）共用这个入口，在别处再补一声就重了。
        self.ui_sfx(Sfx::Refuse);
        self.refusal = Some(Refusal::started(self.now()));
    }

    pub fn refusal_active(&self, now: Millis) -> bool {
        self.refusal.is_some_and(|r| r.active(now))
    }

    /// How far the cart is into the slot: 0.0 standing on the shelf, 1.0 swallowed.
    pub fn seat(&self) -> f32 {
        match &self.phase {
            Phase::Shelf => 0.0,
            Phase::Inserting { t, resumed, .. } => {
                if *resumed {
                    1.0
                } else {
                    (t / SEATED_AT).clamp(0.0, 1.0)
                }
            }
            Phase::Ejecting { t, .. } => 1.0 - (t / EJECT_S).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    /// ★★ 2026-10-27（六六）：「光效 1 按你说的做」—— **触点那一下**，0..1。
    ///
    /// 声音一直是两段（滑轨 + 触点），画面却只在"行程"上做事：光随卡带往下插变亮，然后被
    /// 卡带盖住 —— 卡带**贴死**那一帧什么都没有，读起来像卡带飘进了一个洞。
    ///
    /// 起点取的是**离开 / 到达座位**那一刻，不是行程：插卡从 `SEATED_AT` 起算，拔卡从
    /// `t = 0`（簧片刚松开）起算，两头都是"触点那一瞬"，共用同一条曲线（0.16 s，二阶收敛）。
    ///
    /// 开局续玩不算 —— 卡带本来就在里面，没有"插进去"这个动作，闪一下反而是假的。
    fn seat_flash(&self) -> f32 {
        const SPAN: f32 = 0.16;
        let since = match &self.phase {
            Phase::Inserting {
                t, resumed: false, ..
            } if *t >= SEATED_AT => *t - SEATED_AT,
            Phase::Ejecting { t, .. } => *t,
            _ => return 0.0,
        };
        let u = 1.0 - (since / SPAN).clamp(0.0, 1.0);
        u * u
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        // Ahead of every phase, because a shutdown is not a screen the user navigated to.
        // rcK takes about five seconds on this hardware — it stops the frontend and unloads
        // the GPU module before the kernel is allowed to halt — and five seconds of black
        // panel after holding the button is indistinguishable from a device that has hung.
        if let Some(index) = self.power_menu {
            self.draw_power_menu(index, out);
            return;
        }
        // The direct-connect dialog, ahead of every phase below it: while it is up it is the
        // whole screen, the same as the power and cheat menus above it.
        if let Some(stage) = self.direct_link {
            self.draw_direct_link(stage, out);
            return;
        }
        // The Wi-Fi config screen, in the same seat: the whole panel while it is up.
        if self.wifi.is_some() {
            self.draw_wifi(out);
            return;
        }
        if self.search.is_some() {
            self.draw_search(out);
            return;
        }
        if let Some(index) = self.cheat_menu {
            self.draw_cheat_menu(index, out);
            return;
        }
        if self.shutting_down() {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, 1.0],
            });
            // The row the user picked is the row the screen repeats back.
            let which = if self.restarting {
                PowerChoice::Restart
            } else {
                PowerChoice::PowerOff
            };
            if let Some((tex, w, h)) = self.shutdown_faces.get(which.index()).copied() {
                out.push(Draw::Tex {
                    x: ((OUT_W - w) / 2) as f32,
                    y: ((OUT_H - h) / 2) as f32,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
            return;
        }
        // Whether the letter band went down this frame. The HUD reads it to place its plate: the
        // band is only on the shelf, and it is the one phase where the top of the screen is not
        // free for a plate to sit against.
        let mut band_on = false;
        match &self.phase {
            // Nothing else is on screen and nothing goes over it, the HUD included: the
            // levels are unreachable here and there is no game to say anything about.
            Phase::SetClock { picker, .. } => {
                let (line, hint) = match self.clock_faces {
                    Some((line, hint)) => (Some(line), Some(hint)),
                    None => (None, None),
                };
                picker.draw(line, hint, out);
                return;
            }
            Phase::Shelf => {
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                // ★ 2026-10-23（六六）：货架页桌面图（System/Desktop.png）替代程序化顶条/底条。
                //   画在壁纸之上、卡带之下；中心透明区让卡带/标题透出，四边灰框/黑条盖住壁纸。
                //   只在 Games 货架页启用；碟片行（无顶条底条）不画，免得黑边裁到封套。
                let show_desktop = self.row == ShelfRow::Games && self.desktop.is_some();
                match (self.core_picker_shown(), self.selected_stem()) {
                    // The highlighted cart is the picker's to draw while its lid is off, and the
                    // rest of the row makes way for it the way it does for a cart going in.
                    // Until its faces are up the picker has only bare parts, so the cart stands.
                    (Some(picker), Some(stem)) => {
                        // Eased on the whole progress rather than on either beat: the row makes
                        // way across the slide and the lift as one movement.
                        let open = ease(picker.openness(self.now()));
                        // Dimmed by as much of the open as has happened, so the dark arrives
                        // with the lid coming off and leaves with it going back on.
                        let dim = 1.0 + (CORE_PICKER_DIM - 1.0) * open;
                        // 同 `Shelf::draw`（2026-10-22 机种框顶层）：内侧洞先画压卡带之下，外壳
                        // 后画压卡带之上，卡带插进去遮住插口弧度、下缘被外壳裁掉。
                        // ★ 2026-10-23（六六）：桌面图（代码层）已自带底条与 360×40 插卡位，
                        //   启用时程序化卡座会被它的透明缺口露出一截、跟桌面框打架 —— 关掉。
                        if !show_desktop {
                            draw_slot_inside(out);
                        } else {
                            // 背板在最里层：卡座内侧 → 机种背板 → 卡带 → 桌面框。
                            self.draw_slot_back(out);
                        }
                        self.shelf
                            .draw_row(Some(stem), 0.0, CORE_PICKER_RECEDE * open, dim, out);
                        if !show_desktop {
                            draw_slot_shell(out);
                        }
                        // ★ 2026-10-23（六六）：桌面图盖在货架层之上——底条压住卡带下缘，
                        //   机种核心选择（lid off）时卡带也在被插入，桌面图同样压在其上。
                        if show_desktop {
                            self.push_desktop(out);
                        }
                    }
                    _ => {
                        // ★ **Which row this is decides whether the case is drawn at all**
                        //   (2026-10-07, 六六). The bands top and bottom are the *cartridge
                        //   case's* furniture — the clock and the letter index live on the
                        //   mirror of the cart bay, and the bay itself is the bottom of the
                        //   screen on every cartridge screen. A sleeve is not a cartridge and
                        //   has no bay: 「做CD拿出来吸入下面也太麻烦，干脆顶条底条都撤掉吧」. So on
                        //   the disc row neither band goes down, nothing is drawn on them, and
                        //   the row has the whole 480 px — which is also what lets a 364 px
                        //   sleeve fit on screen at all (it did not before).
                        //
                        //   The shelf's own slot is turned off at the other end, by
                        //   `Shelf::set_mouth` in `disc_row`; a row drawn by `Shelf::draw`
                        //   would otherwise put the mouth back under the sleeves.
                        let case = self.row == ShelfRow::Games;
                        if show_desktop {
                            self.draw_slot_back(out);
                        }
                        // ★ 2026-10-25 01:00（六六：「取消所有动态吧」）——
                        //   这里曾经是 `draw_idle(self.shelf_shake(), self.idle_now(), out)`：
                        //   静置时中间那张卡绕竖轴轻轻偏转、脚下柔光跟着呼吸。
                        //   现在**一律走 `draw`**（内部就是 `Idle::NONE`），货架上什么都不动。
                        //   机制本身还在 `slot_ui::Shelf::draw_idle` / `Idle`，
                        //   要重开只改这一行；来历与参数见 `.workbuddy/memory/MEMORY-待机动效.md`。
                        self.row().draw(self.shelf_shake(), out);
                        // ★ 2026-10-23（六六）：桌面图盖在货架层之上——底条（桌面图自带黑条）
                        //   压住卡带下缘，卡带像插进槽里；字母轮/标题/状态/收藏星仍画在桌面图之上。
                        if show_desktop {
                            self.push_desktop(out);
                        }
                        // The stars, over the row and under the case's own furniture: a starred
                        // cart wears one above it, and the shelf's own star sits in the corner
                        // saying which shelf is up. **Not the case's furniture, so it stays on
                        // every row** — a starred disc still wants its mark.
                        self.draw_fav_stars(out);
                        self.draw_cart_marks(out);
                        if case {
                            // ★ 2026-10-23（六六）：桌面图已自带顶条/底条的黑框与边缘光，
                            //   启用桌面图时不再画程序化的 `draw_top_band` / `draw_edge_glow`，
                            //   免得两层黑边叠在一起。字母轮/收藏星/机种指示/状态仍画在桌面图之上。
                            if !show_desktop {
                                // The band at the top of the case, and the index printed along it.
                                // The band is the machine's — the cart bay's mirror, drawn from the
                                // bay's own numbers — so it goes down as furniture and the letters
                                // go over it. Not drawn while the picker has the lid off: the index
                                // says where in the library the caret is, and with a cart open there
                                // is no library on screen for it to be somewhere in.
                                draw_top_band(out);
                                // The light off both bands of the case, drawn with the band because
                                // the two boundaries are the same object seen at the top and the
                                // bottom of the screen. After the row, so a cart on its way into the
                                // slot is lit by it rather than drawn over it.
                                draw_edge_glow(out);
                            }
                            band_on = true;
                            // The letter strip is the library's index, and the favourites shelf
                            // is not the library: the dial has stood down, so the band is left
                            // with an empty window rather than an index that no longer means
                            // anything.
                            // The letter strip is the index of whatever row is up — the machine's
                            // shelf, a subfolder, or the favourites — and all three are libraries
                            // the strip can walk (2026-10-10, 六六). The disc row is the only
                            // screen without one, because it carries no band to print it on.
                            // ★ 2026-10-24（六六）\uff1a先铺胶囊背板\uff0c字母在它上面。
                            if let Some((tex, _, _)) = self.letter_pill_face {
                                let (px, py, pw, ph) = slot_ui::pill_quad();
                                out.push(Draw::Tex {
                                    x: px,
                                    y: py,
                                    w: pw,
                                    h: ph,
                                    tex,
                                    alpha: 1.0,
                                });
                            }
                            self.letters.draw_strip(
                                &self.letter_faces,
                                self.letter_ridge_face,
                                &self.letter_counts,
                                out,
                            );
                            // ★ 2026-10-24（真机抓到）：收藏心压在字母条上
                            //   —— 底条右下角既有字母条又有收藏心，6px 的实心把最后一个字母
                            //   整个盖住。字母条是库的索引，先生；收藏状态已经
                            //   写在卡带自己脸上（贴纸右上角那颗心），
                            //   所以有字母条的行不再画角标。
                            if self.row == ShelfRow::Discs {
                                self.draw_fav_indicator(out);
                            }
                            // ★ 2026-10-23（六六）：机种指示（左下角机种 logo）按六六要求默认关闭
                            //   —— `slot_state.rs::SHELF_BADGE_DEFAULT=false`（见该常量注释）。
                            //   函数/字段/上传保留不动，仅默认不绘制；若日后要彻底删代码再收尾。
                            self.draw_shelf_indicator(out);
                        }
                    }
                }
                // After the row and before the case: it names what the row is showing, so it
                // belongs to the gap the row leaves rather than to the plastic below it.
                // ★ 2026-10-21：标题的**背光**，压在字下面。
                //   对称衰减、比标题面高 8 px —— 字是浮在一层光里，不是坐在一条色带上。
                //   宽度取标题面自己的宽度（640），所以它和那行字的中心永远对得上，
                //   而不用去猜"这个名字有多宽"（那是另一条会漂的账）。
                // â 2026-10-24ï¼å­å­ï¼ï¼ãåç§°å«å èåï¼çèµ·æ¥èã
                //   ââ é£å±å¯¹ç§°è¡°åçèåæ¤äºï¼å¨æè²è²æ¿ä¸å®åä¸å±æ²¹æ±¡ï¼å­åèä¸å¤æ¸ã
                //   é¢ä»ç¶å¨çï¼`light_faces.glow`ï¼ï¼åªæ¯ä¸åç»ï¼è¦æ¢å¤è¿ä¸æ®µå³å¯ã
                if let Some((tex, w, _)) = self.shelf_title_face {
                    // ★ 2026-10-24（六六）：名字限 300px，**不缩号**——
                    //   超过就在这个 300px 的窗里**循环推着走**：首尾各停 1.2s，
                    //   中间 28px/s 匀速。`Draw::Glyph` 采样一条滑动的 uv 子矩形，
                    //   所以是真正的窗口，不会溢到两边的角标区。
                    let win = slot_ui::SHELF_TITLE_W as f32;
                    let face = w as f32;
                    let win_x = (OUT_W as f32 - win) / 2.0;
                    let y = self.shelf_title_y();
                    if face <= win + 0.5 {
                        out.push(Draw::Tex {
                            x: (OUT_W as f32 - face) / 2.0,
                            y,
                            w: face,
                            h: SHELF_TITLE_H as f32,
                            tex,
                            alpha: 1.0,
                        });
                    } else {
                        let span = face - win;
                        let (hold, speed) = (1.2_f32, 28.0_f32);
                        let travel = span / speed;
                        let cycle = travel + hold * 2.0;
                        let t = self
                            .title_scroll
                            .map(|c| c.elapsed().as_secs_f32())
                            .unwrap_or(0.0)
                            % cycle;
                        let slide = if t < hold {
                            0.0
                        } else if t < hold + travel {
                            (t - hold) * speed
                        } else {
                            span
                        };
                        let u0 = slide / face;
                        let u1 = (slide + win) / face;
                        out.push(Draw::Glyph {
                            x: win_x,
                            y,
                            w: win,
                            h: SHELF_TITLE_H as f32,
                            tex,
                            uv: [u0, 0.0, u1, 1.0],
                            alpha: 1.0,
                        });
                    }
                }
                // ★ 2026-10-24（六六）：顶栏内两条**分割线**，把 40px 信息区与 320px 名称区分开。
                let divider = {
                    let mut c = slot_ui::palette::ink_f();
                    c[3] *= 0.35;
                    c
                };
                for i in 0..2 {
                    let (dx, dy, dw, dh) = slot_ui::plate_divider_rect(i);
                    out.push(Draw::Rect {
                        x: dx,
                        y: dy,
                        w: dw,
                        h: dh,
                        colour: divider,
                    });
                }
                // ★ 2026-10-24（六六）：收藏符号搬到**名称栏左段 30px**里，
                //   按 24px 居中（名称限 300px，右段 30px 留给电量）。
                self.draw_fav_plate_mark(out);
                // Last of the shelf's own furniture, because it is printed on the band the
                // letters are cut into and belongs over it: the clock at one end of the case
                // and the battery at the other, both clear of the window between them.
                // ★ The clock and the battery are printed **on the top band**, so they go with
                //   it. With no band there is nothing under them but the sleeve (2026-10-07).
                if self.row == ShelfRow::Games {
                    // ★ 2026-10-23（六六）：主题暗/亮指示图标（Icon::of_mode）按六六要求拿掉，传 None。
                    draw_status(
                        self.battery,
                        self.battery_percent,
                        self.bolt,
                        self.shelf_clock,
                        None,
                        out,
                    );
                }
            }
            // The label, and the grid of what is behind MENU. No scroll: three buttons are three
            // buttons, and the screen that used to run past the panel is `Shortcuts` below.
            Phase::Menu { index } => {
                // The same ground the shelf stands on, scrim and all. The label is a dark
                // object and the scrim is what a dark object needs to read over a photograph —
                // it is there for the carts for exactly the same reason.
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                draw_sticker_at(self.sticker_face, slot_ui::menu::PAD, out);
                self.draw_menu(*index, out);
                self.draw_menu_hint(out);
                return;
            }
            // The settings list, on the same ground as the menu that opened it but **without
            // its sticker**: a page of parameters does not want a second copy of the label the
            // user just tapped through (2026-10-10 六六:「设置面板页就不用加『关于』贴纸了吧
            // 有点多余」). The list takes the place the sticker had. Rows rather than buttons:
            // see `slot_ui::settings` for why, and for why the width is the menu's.
            Phase::Settings { row, scroll } => {
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                self.draw_settings(*row, *scroll, out);
                self.draw_settings_hint(*row, out);
                // ★ 2026-10-21：**这一臂在 `draw()` 里提前 return**，所以从设置页开出来的
                //   键位弹窗得在这里自己补画一次 —— 否则它要到"游戏里那一臂"才被画到，而
                //   货架设置页根本走不到那儿。弹窗自带纱，压在设置页上正好。
                if self.keymap_edit.is_some() {
                    self.draw_keymap(out);
                }
                return;
            }
            // The card, and nothing else. It is the one screen in the frontend taller than the
            // panel, so it is the one that scrolls.
            Phase::Shortcuts { scroll, .. } => {
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                let mut y = CARD_TOP - *scroll;
                for face in self.shortcut_rows.iter().copied() {
                    // Off the panel is off the panel: twenty rows of which five are ever on
                    // screen is not a reason to hand the compositor twenty.
                    if y < OUT_H as f32 && y + face.2 as f32 > 0.0 {
                        draw_shortcut_row(Some(face), y, out);
                    }
                    y += face.2 as f32;
                }
                // Pinned. It is the line that says the rest of the card moves, so it cannot be
                // the thing that moves.
                if let Some((_, _, h)) = self.shortcut_hint {
                    // The band, then the line on it. The band is the panel's and the line is the
                    // card's, so they are laid from two different widths.
                    let top = OUT_H as f32 - h as f32;
                    draw_hint_bar(top, out);
                    draw_shortcut_row(self.shortcut_hint, top, out);
                }
                return;
            }
            // ── The deck: **the panel is the whole screen.**
            //
            // ★ 2026-10-06 六六：「屏幕被往上顶了因为初版你生成的三个播放按钮没拿掉」。
            //   面板是 720×480，屏也是 720×480 —— 它就该 1:1 铺满。初版在这块屏上还留着
            //   一卷磁带、一个名字窗、**三个播放键**和两个计数器，而面板是照着"键排之上那块
            //   地"居中的（`(OUT_H-h)/2 - KEY_H/2` = **-23**），于是整块屏被顶上去了 23px。
            //   现在这些家具一个都不画：**面板一上来就铺满，屏上不再有第二样东西。**
            //
            //   家具没有删掉，但只在**没有面板时**当退路（开机第一帧还没烘出面板）——
            //   一块黑屏比一件旧家具糟得多。那条路上不画播放键：面板缺席从来不是"该用键了"，
            //   而是"还没烘好"，而退路上的键既按不动（焦点画不出来）又占地方。
            // ── 信息页（`SELECT`）：**满屏一整帧，与面板同一块玻璃**。
            //
            // ★ 2026-10-06 重做：这一臂**不再自己铺底、不再逐行画纹理**。它和上面
            //   `Phase::Listening` 那一臂是**同一个形状** —— 一张 `PANEL_W × PANEL_H` 的
            //   纹理 1:1 铺满，底下什么都不画。
            //
            //   上一版是"铺 `opening()` 底 + 逐行 `word_face` 纹理"，那是**货架/菜单那一套
            //   渲染**（Plix 矢量字、抗锯齿、主题的一个色阶、**没有格网**），所以并排放的时候
            //   根本不像同一台机器。现在它在 `sync_info_panel` 里由 `lcd::info_frame` /
            //   `lcd::info_keys_frame` 合成，**与面板共用每一个零件**。
            //
            //   **没有面板时铺底而不是退回旧画法**：这一屏只有一帧的时间是"还没烘好"
            //   （SELECT 下去的那一帧），而 `opening()` 那个底色**正是它自己现在的样子** ——
            //   不再是"另一种渲染"，只是没有字的这一张。
            Phase::DiscInfo => match self.info_panel {
                Some((tex, w, h)) => out.push(Draw::Tex {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    tex,
                    alpha: 1.0,
                }),
                None => out.push(Draw::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    colour: slot_ui::opening(),
                }),
            },
            Phase::Listening { .. } => {
                match self.lcd_panel {
                    Some((tex, w, h)) => out.push(Draw::Tex {
                                x: 0.0,
                                y: 0.0,
                        w: OUT_W as f32,
                        h: OUT_H as f32,
                        tex,
                        alpha: 1.0,
                    }),
                    None => {
                        draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                        // 名字窗：卡的顶部那条带子（面板不在时它才需要存在）。
                        if let Some((tex, w, h)) = self.deck_name {
                            out.push(Draw::Tex {
                                x: (OUT_W as f32 - w as f32) / 2.0,
                                y: (slot_ui::deck::FACE_Y - h as f32) / 2.0,
                                w: w as f32,
                                h: h as f32,
                                tex,
                                alpha: 1.0,
                            });
                        }
                        // 磁带（盒面贴图）与唱针读数，同样是退路。
                        if let Some((tex, w, h)) = self.deck_face {
                            out.push(Draw::Tex {
                                x: (OUT_W as f32 - w as f32) / 2.0,
                                y: slot_ui::deck::FACE_Y,
                                w: w as f32,
                                h: h as f32,
                                tex,
                                alpha: 1.0,
                            });
                        }
                        if let Some((tex, w, h)) = self.deck_lyric_face {
                            out.push(Draw::Tex {
                                x: (OUT_W as f32 - w as f32) / 2.0,
                                y: (slot_ui::deck::FACE_Y - h as f32) / 2.0,
                                w: w as f32,
                                h: h as f32,
                                tex,
                                alpha: 0.72,
                            });
                        }
                        self.draw_deck_meters(out);
                    }
                }
                return;
            }
            // The shelf recedes behind the cart on the way in; on the way out the live
            // game is what darkens, and the compositor has already drawn it.
            Phase::Inserting { cart, resumed, disc, t, .. } => {
                // Spec section 3: a resumed cart shows no shelf, not even one frame of it.
                if !resumed {
                    draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                    self.draw_slot_back(out);
                    self.shelf.draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                    // ★ 2026-10-26（六六）：**货架区的呼吸发光文字**，所有机种共用这一处 ——
                    //   位置在卡带/光盘**之前**：字是货架的一部分（像丝印），卡带从它前面经过，
                    //   不是字压在卡带上。开机转场（`Playing{screen<1}`）里不画，那时画面已经
                    //   在长，货架正在被盖掉。
                    //   放进 `!resumed` 里，是因为开局续玩"一帧货架都不许出现"（spec 第 3 条），
                    //   字是货架的一部分，不该单独留在屏上。
                    self.draw_loading(out);
                }
                // PS1 光盘插入：绘制下落 + 旋转动画
                //
                // ★ 2026-10-27（六六）：ARCADE / NEOGEO 是**投币** —— 前半段同光盘，
                //   代币整枚出屏之后街机箱体从插卡口升起（见 `App::draw_coin`）。
                match disc {
                    // 开局续玩时货架一帧都不许出现，代币也不能再从货架位掉一次 ——
                    // 直接画终态（箱体升到位），和卡带那一支"续玩就坐在槽里"同一个意思。
                    Some(d) if d.coin => {
                        let t = if *resumed { slot_ui::coin_total_s() } else { *t };
                        // 卡上没有投币素材（老卡）就退回卡带那一套 —— 否则屏上只剩
                        // 桌面图和 LOADING 字，代币和箱体一个都不出现，读起来像卡死。
                        match self.system_of(cart).and_then(|s| self.coin_of(s)) {
                            Some(_) => self.draw_coin(out, t, cart),
                            None => self.chrome(cart, self.seat(), out),
                        }
                    }
                    Some(d) => slot_ui::draw_disc_insert(
                        out,
                        *t,
                        d,
                        DISC_DIA_DEFAULT,
                        self.disc_face.as_ref(),
                    ),
                    None => self.chrome(cart, self.seat(), out),
                }
                // After the cart, never before it: the frame is what the cart goes into.
                self.push_desktop(out);
                // ★ 2026-10-27：触点那一下。**必须在桌面图之后** —— 亮的是洞口外沿那圈
                //   塑料，画在它之前会被壳盖掉（见 `slot_ui::draw_contact_flash`）。
                if disc.is_none() {
                    slot_ui::draw_contact_flash(out, self.seat_flash());
                }
            }
            // The insert run backwards, all of it: the veil lifts, the row closes back up
            // and the cart comes out, every one of them off the same progress running the
            // other way. Darkening on the way out as well as on the way in was the screen
            // playing the same movement twice rather than reversing it.
            Phase::Ejecting { cart, t, .. } => {
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                self.draw_slot_back(out);
                self.shelf.draw_row(Some(cart), 0.0, self.seat(), 1.0, out);

                // PS1 光盘弹出：加速反转 → 托盘弹出 → 减速收回（插入的倒放）
                let is_disc = self.system_of(cart) == Some(slot_store::System::Ps1);
                // ★ 2026-10-27（六六）：**投币的弹出 = 插入那条时间轴倒放** —— 箱体沉回
                //   槽里、代币从屏外升回货架位。和光盘一样，先把 CRT 把画面收掉。
                let is_coin = self
                    .system_of(cart)
                    .is_some_and(|s| s.is_coin() && self.coin_of(s).is_some());
                if is_disc {
                    let cx = slot_gfx::OUT_W as f32 / 2.0;
                    slot_ui::draw_disc_eject(out, *t, cx, DISC_DIA_DEFAULT, self.disc_face.as_ref());
                } else if is_coin {
                    self.draw_coin(out, slot_ui::coin_total_s() - *t, cart);
                } else {
                    self.chrome(cart, self.seat(), out);
                }

                self.push_desktop(out);
                // ★ 2026-10-27：离座那一瞬也亮一下（簧片刚松开，光从缝里露出来），
                //   和插卡那头同一条曲线。光盘/投币各有自己的光，不叠这一层。
                if !is_disc && !is_coin {
                    slot_ui::draw_contact_flash(out, self.seat_flash());
                }
                // ★★ 2026-10-26（六六）：**关机走 CRT 那一套**。
                //   原话："退出的时候就是 CRT 电视关机那样，收束成中间一条辉光线后淡出
                //   → 光盘停转（但必须回正）→ 弹出到货架位 → 恢复货架可操作性"。
                //   关机曲线本来就在 `slot_gfx::power`，由 `self.screen` 驱动：进了 `Ejecting`
                //   就不再算 lit，`screen` 用 `POWER_OFF_S`（0.16 s）倒退，画面被合成器收成
                //   中间一条 2px 辉光线再淡出。**缺的只是把画面推上去** —— 不推，屏是瞬间
                //   消失的，那就是"退出还是立刻回货架"。
                if is_disc || is_coin {
                    self.push_screen_veil(out);
                    self.push_game(out);
                }
            }
            // ★★ 2026-10-26（六六）：**PS1 进游戏走 CRT 开机，不走 chrome。**
            //   原话："加载完毕的时候光盘继续转，屏幕出现 CRT 电视开机的效果（画面从中间
            //   一点辉光 → 横向的一条辉光线 → 展开后是游戏画面 → 铺满屏幕，开始游玩）"。
            //
            //   旧版这里（以及下面那条）画的是 `chrome` —— 也就是**一盘占位卡带**。PS1 没有
            //   卡带，那是一块 300×300 的方块凭空出现，然后画面才硬切进来，所以"突兀"。
            //
            //   现在的顺序就是层序：货架的桌面图先摆好（和插入末帧一模一样，一个像素不差），
            //   洞里那颗光盘**还在满速转**（`disc_live`），然后画面推在**最上面** ——
            //   它从 360,240 那一点辉光长开，把插卡位、塑料框一起盖掉。长满整屏时
            //   下面那条臂接手，画面上没有任何东西多出来也没有东西消失。
            Phase::Playing { cart }
                if self.screen < 1.0
                    && self.system_of(cart) == Some(slot_store::System::Ps1) =>
            {
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                self.draw_slot_back(out);
                self.shelf.draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                if let Some(live) = self.disc_live {
                    let (angle, t) = live.phases(self.clock);
                    slot_ui::draw_disc_live(
                        out,
                        angle,
                        t,
                        DISC_DIA_DEFAULT,
                        self.disc_face.as_ref(),
                    );
                }
                self.push_desktop(out);
                // 最后：黑纱 + 画面压在最上面。CRT 从中间一点长满整屏，机器跟着一起暗下去。
                self.push_screen_veil(out);
                // ★ 2026-10-26（六六）：「持续到进入游戏」—— 字一路跟着，画面长满才没。
                //   画在黑纱**之后**（不被它压暗），靠 `(1 - screen)` 自己淡出。
                self.draw_loading(out);
                self.push_game(out);
            }
            // ★★ 2026-10-27（六六）：「随后游戏加载完毕的时候 **CRT 特效进游戏画面**」。
            //   这一臂和 PS1 那一臂**同构**：机器（桌面图 + 背板 + 货架）先摆好，升到位的
            //   街机箱体留在屏上，画面从中间一点辉光长开，把箱子连同塑料框一起盖掉。
            //   唯一的不同是"洞里那个还会动的东西"—— PS1 是还在转的盘，这里是箱体 + 洋红区
            //   的呼吸光（箱体是静态的，动的是它自己那圈光）。
            Phase::Playing { cart }
                if self.screen < 1.0
                    && self
                        .system_of(cart)
                        .is_some_and(|s| s.is_coin() && self.coin_of(s).is_some()) =>
            {
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                self.draw_slot_back(out);
                self.shelf.draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                // ★ 2026-10-27：**接同一条插入时钟**（`loading_at` 记的是按下 A 那一刻），
                //   所以箱体从 `Inserting` 交到 `Playing` 时不会跳 —— 2026-10-28 起 CRT 在
                //   箱体完全升到位（`coin_total_s()`）之后才开，交接时箱体已在顶部静止。
                let t = (((self.clock - self.loading_at) / 1000.0) as f32).max(0.0);
                self.draw_coin(out, t, cart);
                self.push_desktop(out);
                self.push_screen_veil(out);
                self.draw_loading(out);
                self.push_game(out);
            }
            // The slot stays on screen until the picture behind it has finished arriving,
            // so the game blooms out of a lit lip rather than replacing it.
            //
            // ★★ 2026-10-27（六六）：「**把各机种的背板加进各机种插卡动画（跟 PS1 现在的
            //   状态一样）**」。
            //
            //   这一臂是插入的收尾：卡带已经落座、画面正在从中间长满（`screen` 0→1）。
            //   原来这里只画两样 —— `draw_loading` 和 `chrome`（卡带 + 画面上线），
            //   **桌面图和插卡位背板都不画**，于是"机器"在画面长开的前一刻整块消失，
            //   屏上只剩一盘卡带浮在黑底上；而 PS1 那一臂（上面那条）一直留着整套货架。
            //
            //   现在两臂**同构**，连顺序都一样：
            //   壁纸 → 背板（带 LOGO 的 380×44 亮条）→ 货架行 → 卡带 → 桌面图（塑料）
            //   → 黑纱 → 呼吸字 → 画面。
            //   `chrome_with(.., false)` 是为了这个顺序：`SlotChrome` 平时自己把 `Draw::Game`
            //   推在卡带之后，这里必须让画面排在**桌面图之后**，否则塑料会压在画面上。
            Phase::Playing { cart } if self.screen < 1.0 => {
                draw_backdrop(self.wallpaper, self.wall_glow_at(), out);
                self.draw_slot_back(out);
                self.shelf.draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                self.chrome_with(cart, 0.0, false, out);
                self.push_desktop(out);
                self.push_screen_veil(out);
                // 同上：字跟着画面一起收，`(1 - screen)` 在 `draw_loading` 里乘。
                self.draw_loading(out);
                self.push_game(out);
            }
            Phase::Playing { .. } => {
                self.push_game(out);
                self.draw_palette_browser(out);
            }
            // The paused game stays underneath, covered by the screenshot the switcher
            // draws over the whole screen.
            Phase::Polaroids { .. } => {
                self.push_game(out);
                if let Some(p) = &self.polaroids {
                    p.draw(
                        self.battery,
                        self.battery_percent,
                        self.bolt,
                        self.shelf_clock,
                        out,
                    );
                }
            }
            // The device answers a shut lid with the backlight; the host has no panel to
            // darken, so the doze is drawn. Nothing goes over it, the HUD included.
            Phase::Doze { .. } => {
                out.push(Draw::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    colour: [0.0, 0.0, 0.0, 1.0],
                });
                return;
            }
        }
        // After the shelf, never before it: drawn first it would be painted over by the very
        // row of carts it is a menu for, and START would look like a button that does
        // nothing. Only the shelf can raise it, so no phase needs excluding here — the
        // phases that own the whole panel have already returned.
        if let Some(picker) = self.core_picker_shown() {
            self.draw_core_picker(&picker, out);
        }
        // Over the game and under the HUD, for the same reason the picker is over the shelf:
        // it is a menu about the thing still on screen behind it, and the level bars have to
        // stay visible while it is up. Only a running game can raise it, so no phase needs
        // excluding here — the ones that own the whole panel have already returned.
        //
        // ★ 2026-10-21：**键位弹窗开着的时候，游戏菜单让位**。两张都是带纱的浮层，叠在一起
        //   只会互相压；键位弹窗是更外的一层，它开着就画它，菜单等它关了再回来。
        if self.keymap_edit.is_none() {
            if let Some(menu) = self.game_menu {
                self.draw_game_menu(menu, out);
            }
        }
        // ★ 2026-10-21：**操作键映射弹窗** —— 货架设置页和游戏菜单两处都能开它，所以它画在
        //   所有相位之上、HUD 之下，自带一层 70% 黑纱和一块板。
        if self.keymap_edit.is_some() {
            self.draw_keymap(out);
        }
        // Over everything, in every phase. The bar is never what the user is looking at.
        //
        // The plate hangs under the letter band on the shelf and from the top of the screen
        // everywhere else, which is the one thing about the HUD a phase gets to decide: the band
        // is only on the shelf, and covering it with a plate would hide the index and the clock
        // for the second and a half a level is up.
        // ★ 2026-11-03（六六）：「状态条挪到插卡口上方，并放到壁纸的上一层」→ 同日：
        //   「不要留缝」→「货架 HUD 宽度缩到 360px，跟插卡位同框；高度加一点，两边改圆角」
        //   →「放插卡位是不是更自然？反正就是一两秒的事」。
        //   于是货架的板**住进插卡口**：`Plate::shelf()` 的宽高都取插口常量，`top` 就是
        //   `SLOT_OPEN_Y`（430），底边落在插口下沿 474 —— 只在电平/提示那 1.5 秒出现，
        //   盖的是洞口。其他相位用 `Plate::full()`、`top = 0`，贴屏幕顶端、与改动前一致。
        //   HUD 在 `draw()` 末尾才画，所以永远在壁纸/桌面图之上。
        let (plate, plate_top) = if band_on {
            (slot_ui::Plate::shelf(), slot_ui::SLOT_OPEN_Y)
        } else {
            (slot_ui::Plate::full(), 0.0)
        };
        self.hud.draw(self.now(), plate_top, plate, out);
        // ★★ 2026-10-24（六六）：卷闸压在所有东西之上 —— 它盖住的是整台机器，连 HUD 一起。
        //   只有货架能起转场（`open_sys_picker` 的第一道门就是 `on_shelf()`），所以别的相位
        //   永远拿到 `None`，这里不必再判相位。
        if let Some(shutter) = self.shutter() {
            slot_ui::draw_shutter(shutter, self.change_face, self.change_glow, out);
        }
        // ★★ 2026-10-26（六六）：**机种十格画在帘子之上**，不是另一层浮层。
        //
        //   顺序是这一块屏的全部：货架 → 帘子（盖满）→ 纱（帘子压暗）→ 十格。所以它必须
        //   排在 `draw_shutter` **之后** —— 那一刻帘子已经停在满闸，用户看到的是"帘子
        //   落下来，然后十格浮在帘子上"。画在前面就成了"面板浮在货架上，帘子从上面压下来"，
        //   完全不是六六要的东西。
        //
        //   它自己按 `alpha` 决定露多少：帘子还在落的那 0.6 秒里 alpha 是 0，一个像素都不画。
        if let Some(picker) = &self.sys_picker {
            self.draw_sys_picker(picker, out);
        }
    }

    pub fn screen_shake(&self) -> f32 {
        // ★ 2026-10-27（六六）：「箱体升起的时候伴随屏幕震动」—— 它比拒绝抖动优先，
        //   两者不会同时发生，但这里只该有一个来源。
        if let Some(px) = self.coin_shake() {
            return px;
        }
        // ★ 2026-11-02（六六）：「不能直接在按按钮的时候震动不离开 MENU 页吗？」——
        //   菜单页没有货架的 `shelf_shake`，所以「没碟」这类拒绝在菜单里要靠整屏抖动才看得见。
        if matches!(self.phase, Phase::Menu { .. }) {
            return self.refusal.map_or(0.0, |r| r.offset(self.now()));
        }
        self.shake_at(self.now())
    }

    /// 街机箱体顶出来那 0.55 s：整屏按 `COIN_SHAKE_HZ` 抖，振幅从 `COIN_SHAKE_PX` 线性掉到 0。
    /// `None` = 这一段没有震动（其余任何相位都是）。
    fn coin_shake(&self) -> Option<f32> {
        let Phase::Inserting {
            t, disc, resumed, ..
        } = &self.phase
        else {
            return None;
        };
        if *resumed || !disc.as_ref().is_some_and(|d| d.coin) {
            return None;
        }
        let u = ((*t - slot_ui::coin_in_s()) / slot_ui::CABINET_RISE_S).clamp(0.0, 1.0);
        if u <= 0.0 || u >= 1.0 {
            return None;
        }
        let amp = COIN_SHAKE_PX * (1.0 - u);
        let phase = (self.clock as f32 / 1000.0) * std::f32::consts::TAU * COIN_SHAKE_HZ;
        Some(amp * phase.sin())
    }

    /// Offscreen pixels the whole presented image is displaced by. The screen flinches only
    /// while the game is playing, which is the one phase whose content fills the frame.
    pub fn shake_at(&self, now: Millis) -> f32 {
        self.shake_when(matches!(self.phase, Phase::Playing { .. }), now)
    }

    /// Pixels the cart row is displaced by. On the shelf the frame is mostly backdrop, so
    /// shaking the whole image would just slide the letterbox in at the edges.
    pub fn shelf_shake(&self) -> f32 {
        // The chip is what flinches while the picker is up, and two things shaking at once reads
        // as two separate refusals.
        // ★ 2026-10-26：机种面板开着时同理 —— 抖的是手里那一格，不是背后整排货架。
        self.shake_when(
            self.on_shelf() && self.core_picker.is_none() && self.sys_picker.is_none(),
            self.now(),
        )
    }

    /// Shake whatever represents the thing that was refused, and only that: two of them at
    /// once reads as two separate failures.
    fn shake_when(&self, mine: bool, now: Millis) -> f32 {
        if !mine {
            return 0.0;
        }
        self.refusal.map_or(0.0, |r| r.offset(now))
    }

    /// The game layer's place in the list. Where there is no slot on screen it is the whole
    /// picture; the chrome puts it in the same list, in front of the cart.
    fn push_game(&self, out: &mut Vec<Draw>) {
        if self.game_visible() {
            // A tinted-backlight colour correction makes a GB picture glow one colour; the
            // letterbox around it would otherwise stay black and read as a frame that does not
            // belong. Fill it with the same colour, so the whole panel is one tinted surface.
            //
            // **Plain GB only.** GBC is a colour machine whose games use their own palettes, so
            // its surround keeps the black letterbox — a mono tint there would be a lie about
            // the machine, and GBC is themed through an overlay PNG instead. A GBA picture
            // fills the frame, so it has no letterbox to fill either. And only modes 3..=6 have
            // a single colour to fill with.
            if matches!(
                self.shelf.current_cart().map(|c| c.system()),
                Some(System::Gb)
            ) {
                if let Some(c) = self.display.cc_border(self.current_machine()) {
                    out.push(Draw::Rect {
                        x: 0.0,
                        y: 0.0,
                        w: OUT_W as f32,
                        h: OUT_H as f32,
                        colour: [c[0], c[1], c[2], 1.0],
                    });
                }
            }
            out.push(Draw::Game);
            // Over the game and under the HUD: a per-system screen overlay PNG from the card,
            // composited with its own alpha. Opaque pixels replace the picture (bezel, grid),
            // transparent ones let it through. No texture means the built-in look.
            if let Some(tex) = self.overlay_tex {
                out.push(Draw::Tex {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    tex,
                    alpha: 1.0,
                });
            }
            // ...and the glow over the art, not under it: the art is the plastic's colour and
            // the glow is light landing on that plastic. Push it whether or not the card has an
            // overlay — the compositor is what knows whether anything reflective is up, and a
            // marker that costs one comparison is cheaper than asking the same question twice.
            out.push(Draw::Glow);
        }
    }

    fn chrome(&self, stem: &str, dim: f32, out: &mut Vec<Draw>) {
        self.chrome_with(stem, dim, true, out)
    }

    /// ★ 2026-10-27（六六）：「把各机种的背板加进各机种插卡动画（跟 PS1 现在的状态一样）」。
    ///
    /// `game = false` 让卡座位这一层**不推 `Draw::Game`**，画面留给调用方在桌面图（机器）
    /// 之后自己推 —— 插入收尾那一段必须是这个顺序，否则塑料框会压在刚长出来的画面上。
    fn chrome_with(&self, stem: &str, dim: f32, game: bool, out: &mut Vec<Draw>) {
        let Some((cart, face)) = self.shelf.find(stem) else {
            return;
        };
        let alpha = self.alert_alpha();
        // What is going into the slot, said in the terms the chrome speaks: its shape, how deep
        // this slot takes it, and the ink its stand-in wears. Derived from the cart *here*, so
        // that the chrome itself never has to ask what a cart is — which is the whole of what
        // lets something that is not a cartridge use this path.
        let system = cart.system();
        SlotChrome {
            // ★ 2026-10-27 23:1x：盒子按**这张卡自己的目录**取 —— `Games/FC` 与 `Games/NES`
            //   同属 `System::Fc`，可两张缺省图一个是横的一个是竖的（`size_for_dir`）。
            //   插进槽里的卡和货架上那张必须是同一个盒子，否则按下 A 的瞬间它会变一次形。
            size: size_for_dir(cart.dir_name(), system),
            seat_kind: Seat::from(system),
            ink: label_colour(&label_text(cart)),
            face,
            seat: self.seat(),
            alert: self.alert_face.filter(|_| alpha > 0.0).map(|t| (t, alpha)),
            dim,
            screen: self.screen,
            game: self.game_ready && game,
        }
        .draw(out);
    }

    /// ★ 2026-10-24（六六）：**货架框桌面图铺满整屏，并且永远画在卡带之上**。
    ///
    /// 它的中间是透明窗口、插卡位是一条 360×44 的透明洞，其余全是塑料。卡带"沉进机器"
    /// 这件事现在完全靠这一层压在它上面 —— 不再是 `SlotChrome` 里那条程序化底条
    /// （2026-10-24 起那条连同 `SEATED_Y` / `GB_INSET` 一起解绑了）。
    /// 货架页、插卡、拔卡三处用的是同一份图、同一个顺序（卡带在前，桌面图在后），
    /// 所以屏幕之间不会出现"两个插卡位"或者卡带压在塑料上面。
    /// 插卡位洞里那张机种背板 —— **画在卡带之前**，所以卡带进来就盖住它，退出去又露出来。
    ///
    /// 位置直接取桌面图的开口（`SLOT_OPEN_*`，360×44），背板 380×44 比开口宽 20：多出来的
    /// 那 10px 挂在开口两侧的塑料下面，被随后画的桌面图盖住 —— 于是开口里看到的永远是背板
    /// 的中间 360，边缘由桌面图自己的洞形裁。不按开口裁一次，是因为洞的口部是外扩 `)(`，
    /// 用矩形裁会把那两道弧切平。
    fn draw_slot_back(&self, out: &mut Vec<Draw>) {
        let Some((_, (tex, w, h))) = self.slot_back.iter().find(|(s, _)| *s == self.shelf_system)
        else {
            return;
        };
        if *w == 0 || *h == 0 {
            return;
        }
        let (w, h) = (*w as f32, *h as f32);
        // ★ 2026-10-27（六六）：「拒绝时让光瞬灭一下」。这条带 LOGO 的亮条就是插卡位的
        //   待机灯 —— 拒绝 = 没通电，所以是**断一下**（`Refusal::dip`），不是加新光。
        //   90 ms 的缺电，正好压在抖动那一下的前面。
        let dip = self.refusal.map_or(0.0, |r| r.dip(self.now()));
        out.push(Draw::Tex {
            x: slot_ui::SLOT_OPEN_X + (slot_ui::SLOT_OPEN_W - w) / 2.0,
            y: slot_ui::SLOT_OPEN_Y + (slot_ui::SLOT_OPEN_H - h) / 2.0,
            w,
            h,
            tex: *tex,
            alpha: 1.0 - 0.92 * dip,
        });
    }

    /// 画面之外那一圈黑，进/出 PS1 游戏的 CRT 转场专用（★ 2026-10-26）。
    ///
    /// 游玩时桌面图**不画**，所以整屏除了 640×480 的画面就是黑的。转场里机器是画出来的，
    /// 于是两端会各跳一下（进：两侧 40 px 从 wallpaper 变黑；出：从黑变 wallpaper）。
    /// 一层黑纱、透明度**正好等于 `screen`**，把两端钉死：
    ///
    /// * `screen = 1` ⇒ 全黑，桌面图看不见 —— 和游玩中一模一样；
    /// * `screen = 0` ⇒ 透明，货架/桌面图完整 —— 和插入末帧、停转那五拍一模一样。
    ///
    /// 中间那 0.22 / 0.16 s 因此是"机器跟着画面一起亮起来 / 一起暗下去"，而不是换了张脸。
    /// 画在桌面图**之后**、`Draw::Game` **之前**。
    fn push_screen_veil(&self, out: &mut Vec<Draw>) {
        if self.screen <= 0.0 {
            return;
        }
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: [0.0, 0.0, 0.0, self.screen],
        });
    }

    /// ★ 2026-10-26（六六）：「给所有机种的插卡/插光盘动画加上货架区的呼吸发光文字
    /// （颜色随主题）」→ 之后又两条：「**应该居中**，且**持续到进入游戏**」。
    ///
    /// 画的是 `System/Carts/slot_loading.png`（720×240 成品图，"GAME START / LOADING..."），
    /// 1:1 贴。`LOADING_Y` 把**笔画中心**（图里在 y=119）摆到屏中心 240 —— 不是把图的
    /// 上沿对 0：那样看着偏高，而货架里所有东西（中间卡带、光盘、背板）都是对 240 居中的。
    ///
    /// **呼吸**：`alpha = fade_in × (LO + (1-LO)·(0.5 + 0.5·cos(2π·t/T))) × (1 - screen)`。
    /// 余弦从 1 起（不是从 0 起）—— 按下 A 的那一帧字就是亮的，然后才转暗再转亮。
    /// 最后那个 `(1 - screen)` 是"持续到进入游戏"的收尾：插卡/插盘动画期间 `screen` 恒 0
    /// （不是 `Playing`，`step_screen` 一直往下压），字不受影响；一进 `Playing`，画面从
    /// 中间那一点长满整屏的 0.22 s 里它跟着淡出，长满时正好没了。
    ///
    /// 时间取 `self.clock`（`loading_at` 记按下 A 的那一刻），**不是** `Phase::Inserting.t`：
    /// 进了 `Playing` 就没有 `t` 了，而字要跨过那条相位边界继续呼吸，用时钟才不会跳一下。
    fn draw_loading(&self, out: &mut Vec<Draw>) {
        let Some((tex, w, h)) = self.loading_face else {
            return;
        };
        let t = (((self.clock - self.loading_at) / 1000.0) as f32).max(0.0);
        let breath = 0.5 + 0.5 * (std::f32::consts::TAU * t / LOADING_PERIOD_S).cos();
        let fade_in = (t / LOADING_FADE_IN_S).clamp(0.0, 1.0);
        let alpha = fade_in * (LOADING_LO + (1.0 - LOADING_LO) * breath) * (1.0 - self.screen);
        if alpha <= 0.004 {
            return;
        }
        let w = w as f32;
        let h = h as f32;
        // 柔光比字大一圈（四周各 `LOADING_GLOW_PAD`），所以按自己的尺寸重新居中，
        // 中心的 `y` 与字对齐（顶部让出那一圈留白）。
        if let Some((gtex, gw, gh)) = self.loading_glow {
            let padv = (gh as f32 - h) / 2.0;
            out.push(Draw::Tex {
                x: (OUT_W as f32 - gw as f32) / 2.0,
                y: LOADING_Y - padv,
                w: gw as f32,
                h: gh as f32,
                tex: gtex,
                alpha: alpha * 0.85,
            });
        }
        out.push(Draw::Tex {
            x: (OUT_W as f32 - w) / 2.0,
            y: LOADING_Y,
            w,
            h,
            tex,
            alpha,
        });
    }

    fn push_desktop(&self, out: &mut Vec<Draw>) {
        let Some(tex) = self.desktop else {
            return;
        };
        out.push(Draw::Tex {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            tex,
            alpha: 1.0,
        });
    }

    /// Whether the cart on its way back out is carrying the refusal symbol.
    pub fn alert_visible(&self) -> bool {
        self.alert_alpha() > 0.0
    }

    /// How lit that symbol is. It holds for most of the exit and is gone before the end of
    /// it, so the alert leaves with the cart rather than being cut off by the shelf.
    pub fn alert_alpha(&self) -> f32 {
        let (Some(from), Phase::Ejecting { t, .. }) = (self.refused_from, &self.phase) else {
            return 0.0;
        };
        let span = EJECT_S - from;
        if span <= 0.0 {
            return 0.0;
        }
        let u = ((t - from) / span).clamp(0.0, 1.0);
        ((ALERT_GONE - u) / (ALERT_GONE - ALERT_HOLD)).clamp(0.0, 1.0)
    }

    pub fn set_shelf_title_face(&mut self, face: Option<(TexId, u32, u32)>) {
        self.shelf_title_face = face;
        self.title_scroll = Some(std::time::Instant::now());
    }

    /// ★ 2026-10-21：三行货架脚下的柔光池，一次设齐。
    ///
    /// 不像 `set_cart_shadow` 那样只设当前行 —— 那一行的纹理会跟着换行丢掉，
    /// 而"手里那张卡脚下有光"是三行货架共同的行为，不是某一行的。
    pub fn set_focus_glow(&mut self, face: Option<TexId>) {
        self.shelf.set_focus(face);
        self.disc_shelf.set_focus(face);
    }

    /// ★ 2026-10-21：三张发光件，由 `frontend::upload_fixed` 传进来。
    /// 全是 `Option`：烘焙失败时货架照常画，只是不亮 —— 一块屏上少一个光点，
    /// 比整台机器起不来轻得多。
    pub fn set_light_faces(&mut self, faces: LightFaces) {
        self.light_faces = faces;
        self.set_focus_glow(faces.pool);
    }

    pub fn set_alert_face(&mut self, face: TexId) {
        self.alert_face = Some(face);
    }

    pub fn set_shutdown_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.shutdown_faces = faces;
    }

    pub fn set_power_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.power_menu_faces = faces;
    }

    /// ★ 2026-10-29（六六）：电源菜单背景图。只上传一次（固定图），换主题不重烘 —— 它
    /// 本就不随主题。`None` 让 `draw_power_menu` 回退主题底。
    pub fn set_power_bg_face(&mut self, face: TexId) {
        self.power_bg_face = Some(face);
    }

    /// Recorded against the highlighted cart, since that is the only cart they are ever built for.
    pub fn set_core_board_faces(&mut self, board: TexId, lid: TexId) {
        self.core_board_face = Some(board);
        self.core_lid_face = Some(lid);
        self.core_faces_stem = self.selected_stem().map(str::to_string);
    }

    /// `sockets` and `chips` in `Core::ALL` order.
    pub fn set_core_part_faces(
        &mut self,
        sockets: Vec<TexId>,
        chips: Vec<TexId>,
        blank: TexId,
        shadow: TexId,
    ) {
        self.core_socket_faces = sockets;
        self.core_chip_faces = chips;
        self.core_blank_chip_face = Some(blank);
        self.core_chip_shadow_face = Some(shadow);
    }

    pub fn set_core_legend_faces(&mut self, faces: Vec<(TexId, u32)>) {
        self.core_legend_faces = faces;
    }

    /// One per `GameRow::ALL`, in that order, whether or not the cart in the slot shows it.
    /// **Two sets**: `idle` is the resting button, `hot` is the one in hand (see the field).
    pub fn set_game_menu_faces(
        &mut self,
        idle: Vec<(TexId, u32, u32)>,
        hot: Vec<(TexId, u32, u32)>,
    ) {
        self.game_menu_faces = idle;
        self.game_menu_hot = hot;
    }

    /// 把两排菜单面**拿走**，交给要重烘的那一方去释放 —— 只有 `Grid` 那一行会跟着机种换字
    /// （「网格切换」/「遮罩切换」），所以插上 GB/GBC 卡之后这一排要重印一次。拿走而不是借出：
    /// 重烘会立刻填进一批新的，中间不留一个"旧的和新的一半对一半"的时刻。
    pub fn take_game_menu_faces(&mut self) -> Vec<(TexId, u32, u32)> {
        let mut all = std::mem::take(&mut self.game_menu_faces);
        all.extend(std::mem::take(&mut self.game_menu_hot));
        all
    }

    /// ★ 2026-10-21：菜单标题行的 face —— 游戏名，前端跟着 stem 烘。
    pub fn set_game_title_face(&mut self, face: Option<(TexId, u32, u32)>) {
        self.game_title_face = face;
    }

    /// ★ 2026-10-21：`game_title_face` 是为哪个 stem 烘的（缓存键）。`sync_game_title`
    ///   比的就是这两个。
    pub(crate) fn game_title_built_for(&self) -> Option<&str> {
        self.game_title_built_for.as_deref()
    }

    pub(crate) fn set_game_title_built_for(&mut self, stem: Option<String>) {
        self.game_title_built_for = stem;
    }

    /// ★ 2026-10-26：PS1 光盘封面贴图 —— 前端从 `Cart.label` 加载并上传后交进来。
    pub fn set_disc_face(&mut self, face: Option<(TexId, u32, u32)>) {
        self.disc_face = face;
    }

    /// ★ 2026-10-26：`disc_face` 是为哪个 stem 烘的（缓存键）。
    pub(crate) fn disc_face_built_for(&self) -> Option<&str> {
        self.disc_face_built_for.as_deref()
    }

    pub(crate) fn set_disc_face_built_for(&mut self, stem: Option<String>) {
        self.disc_face_built_for = stem;
    }

    /// ★ 2026-10-26：读取当前光盘贴图（绘制时使用）。
    pub fn disc_face(&self) -> Option<(TexId, u32, u32)> {
        self.disc_face
    }

    /// ★ 2026-10-26：PS1 光盘封面贴图的缓存键 —— 当前卡在光盘相位（插入 / 游戏 / 弹出）
    ///   且是 PS1 时返回其 stem，否则 `None`。前端拿它和 `disc_face_built_for` 比较，
    ///   不同才重烘。
    pub(crate) fn disc_face_stem(&self) -> Option<String> {
        let (cart, is_disc) = match &self.phase {
            Phase::Inserting { cart, disc, .. } => (cart.as_str(), disc.is_some()),
            Phase::Playing { cart } | Phase::Ejecting { cart, .. } => (cart.as_str(), true),
            _ => return None,
        };
        if !is_disc {
            return None;
        }
        if self.system_of(cart) != Some(System::Ps1) {
            return None;
        }
        Some(cart.to_string())
    }

    /// ★ 2026-10-26：PS1 光盘封面贴图路径 —— 优先 `Cart.label`（封面贴纸），
    ///   没有则用 `Cart.art`（完整光盘图）兜底。PS1 游戏通常没有贴纸但有整张光盘图。
    pub(crate) fn disc_face_path(&self) -> Option<PathBuf> {
        let stem = self.disc_face_stem()?;
        // ★ 2026-11-09：当前机种优先（`Shelf::find`）。同名跨机种时，从前这里拿的是
        //   排序靠前那条的封面 —— 与"进去也是 FC"同一个根因。
        let (cart, _) = self.shelf.find(&stem)?;
        cart.label.clone().or_else(|| cart.art.clone())
    }

    /// ★ 2026-10-27（六六）：投币素材（ARCADE / NEOGEO 各一套），开机烘一次交进来。
    pub fn set_coin_assets(&mut self, coins: Vec<(slot_store::System, Coin)>) {
        self.coins = coins;
    }

    /// 这台机种的投币素材；老卡缺素材时 `None`，调用方退回卡带那套。
    fn coin_of(&self, system: slot_store::System) -> Option<Coin> {
        self.coins
            .iter()
            .find(|(s, _)| *s == system)
            .map(|(_, c)| *c)
    }

    /// 洋红区发光的呼吸曲线：0.55..1.0，周期 1.6 s。
    ///
    /// ★ 2026-10-28：从 `coin_glow_alpha` 抽出来，默认壁纸那圈光走同一条 —— 屏上两处呼吸
    /// 同相，就不会读成"两个各转各的"。
    fn breath_alpha(&self) -> f32 {
        let t = self.clock as f32 / 1000.0;
        0.55 + 0.45 * (0.5 + 0.5 * (std::f32::consts::TAU * t / 1.6).cos())
    }

    /// 街机箱体洋红区的呼吸 alpha：0.55..1.0，周期 1.6 s —— 和光盘那圈光同一个节奏。
    fn coin_glow_alpha(&self) -> f32 {
        self.breath_alpha()
    }

    /// 默认壁纸那圈光，带这一刻的呼吸值。`None` = 屏上是卡自己的壁纸，不画这一层。
    fn wall_glow_at(&self) -> Option<(TexId, u32, u32, f32)> {
        let (tex, w, h) = self.wall_glow?;
        Some((tex, w, h, self.breath_alpha()))
    }

    /// 投币那一段的两个图层：代币下落（**整枚出屏**）+ 街机箱体从插卡口升起。
    ///
    /// `t` 是**插入本地时间**（秒）。弹出方向是这条时间轴倒放，所以这个函数两边共用：
    /// 弹出现在 `COIN_TOTAL_S - t` 传进来，代币会从屏外回到货架位、箱体沉回槽里。
    fn draw_coin(&self, out: &mut Vec<Draw>, t: f32, cart: &str) {
        let Some(system) = self.system_of(cart) else {
            return;
        };
        let Some(coin) = self.coin_of(system) else {
            return;
        };
        // ★★ 2026-10-27（六六）：「换随机色吧」—— 代币**直接复用货架烘出来的那张脸**
        //   （300×300 圆牌，洋红圈已经按这张卡自己的稳定随机色填好）。两边同一个纹理，
        //   按下 A 不可能换色，也不必再多烘一份。脸还没烘出来时 `draw_disc` 会画默认圆盘。
        let token = self
            .shelf
            .find(cart)
            .and_then(|(_, face)| face)
            .map(|tex| (tex, slot_ui::COIN_DIA as u32, slot_ui::COIN_DIA as u32));
        // 代币走完整段（`COIN_EXIT_CY` 之后它在屏外，那一个 quad 由合成器丢掉）：
        // 留着它是为了洞里那圈光 —— 箱体升起来的时候槽是亮的。
        slot_ui::draw_coin_insert(out, t, slot_ui::COIN_DIA, token.as_ref());
        let rise = ((t - slot_ui::coin_in_s()) / slot_ui::CABINET_RISE_S).clamp(0.0, 1.0);
        if rise > 0.0 {
            slot_ui::draw_cabinet(
                out,
                &coin.cab,
                coin.glow.as_ref(),
                self.coin_glow_alpha(),
                rise,
            );
        }
    }

    /// ★ 2026-10-21：保存设置弹窗的三族面，弹窗开着的那一帧由前端上传。
    pub fn set_save_settings_faces(&mut self, faces: Option<(Upload, Vec<Upload>, Vec<Upload>)>) {
        self.save_settings_faces = faces;
    }

    /// One per `LinkRow::ALL`, in that order.
    pub fn set_link_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_menu_faces = faces;
    }

    /// One per `LinkStep::ALL`, and one per `LinkFail::SHOWN`, in those orders. Uploaded at
    /// boot with every other menu face: a link that is failing is the worst moment to be
    /// asking a font for a sentence.
    pub fn set_link_step_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_step_faces = faces;
    }

    pub fn set_link_fail_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_fail_faces = faces;
    }

    /// The case's own ground, and the rows on it. No plate behind them: the menu is three
    /// words and a choice, and a box around those was furniture the screen did not need.
    ///
    /// The ground is drawn here rather than in `draw_menu_rows` because it is the only thing
    /// about this menu that is its own: the core picker draws over a shelf it did not paint,
    /// and the in-game menu over a game it did not either. See `draw_menu_rows` for why the
    /// bar behind the row in hand is the colour it is.
    ///
    /// ★ 2026-10-29（六六）：背景从主题底换成**固定的霓虹隧道图**，选项恒白 —— 整个界面
    /// 不参与主题色板与暗色亮色模式。图没上传（`power_bg_face` 为 `None`）时回退主题底，
    /// 避免整屏黑掉；选中条也走恒定半透明白（`MenuBar::Constant`），不随主题。
    fn draw_power_menu(&self, index: usize, out: &mut Vec<Draw>) {
        match self.power_bg_face {
            Some(tex) => out.push(Draw::Tex {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                tex,
                alpha: 1.0,
            }),
            None => out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: slot_ui::opening(),
            }),
        }
        draw_menu_rows(
            &self.power_menu_faces,
            Some(index),
            centred_top(self.power_menu_faces.len(), POWER_MENU_PITCH),
            POWER_MENU_PITCH,
            MenuBar::Constant([1.0, 1.0, 1.0, 0.22]),
            out,
        );
    }

    /// The menu's slots: the ones that are built, lit if one is in hand, and the ones that
    /// are not, as bare recesses.
    fn draw_menu(&self, index: usize, out: &mut Vec<Draw>) {
        for (i, slot) in slot_ui::menu::Item::GRID.iter().enumerate() {
            let (x, y) = slot_ui::menu::slot_at(i);
            let face = match (slot, i == index) {
                (Some(_), true) => self.menu_buttons[i].map(|b| b.chosen),
                (Some(_), false) => self.menu_buttons[i].map(|b| b.idle),
                (None, _) => self.menu_empty,
            };
            if let Some((tex, w, h)) = face {
                out.push(Draw::Tex {
                    x,
                    y,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
        }
    }

    /// The menu's own foot line, on the same bar the card's uses and centred the same way.
    fn draw_menu_hint(&self, out: &mut Vec<Draw>) {
        let Some(face) = self.menu_hint else {
            return;
        };
        let top = OUT_H as f32 - face.2 as f32;
        draw_hint_bar(top, out);
        draw_shortcut_row(Some(face), top, out);
    }

    /// The settings rows, each from whichever of its own faces this card's value puts it on.
    ///
    /// The row in hand is *lit*, not moved: a list that shifted under the caret would make the
    /// user read it twice, and the menu's buttons already taught this screen what "in hand"
    /// looks like.
    fn draw_settings(&self, row: slot_ui::settings::Row, scroll: f32, out: &mut Vec<Draw>) {
        for r in slot_ui::settings::Row::ALL {
            let value = self.settings_value(r);
            let chosen = r == row;
            let face = self
                .settings_rows
                .get(r.index())
                .and_then(|values| values.get(value))
                .map(|pair| pair[usize::from(chosen)]);
            // `draw_settings_row` is what drops a row that has slid up out of the window —
            // see `settings::visible`. This loop hands over every row there is.
            slot_ui::draw_settings_row(face, r.index(), scroll, out);
        }
    }

    /// The settings screen's foot line, on the same bar and from the same row renderer as the
    /// menu's. Which of the two lines is printed is `Row::opens`'s business — a row that opens a
    /// screen cannot be told to press left and right.
    fn draw_settings_hint(&self, row: slot_ui::settings::Row, out: &mut Vec<Draw>) {
        // **By the row's own index, not by `opens`.** The two stopped being the same question
        // when the refresh row arrived: it opens nothing and still wants its own line.
        let Some(face) = self.settings_hint[row.hint_index()] else {
            return;
        };
        let top = OUT_H as f32 - face.2 as f32;
        draw_hint_bar(top, out);
        draw_shortcut_row(Some(face), top, out);
    }

    /// The direct-connect dialog: a scrim over everything, then the lines for its stage centred on
    /// the panel. Stage 0 asks, stage 1 waits for the cable, stage 2 is the live mode.
    fn draw_direct_link(&self, stage: u8, out: &mut Vec<Draw>) {
        // ★ 2026-11-03（六六）：同 WiFi 页 —— 不透明底把货架壁纸盖没了，改成半透明纱。
        let mut veil = slot_ui::opening();
        veil[3] = 0.72;
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: veil,
        });
        match stage {
            0 => {
                self.draw_centred_line(0, 200.0, out);
                self.draw_centred_line(1, 262.0, out);
            }
            1 => self.draw_centred_line(2, 240.0, out),
            _ => {
                self.draw_centred_line(3, 172.0, out);
                if let Some((tex, w, h)) = self.direct_ip_face {
                    out.push(Draw::Tex {
                        x: (OUT_W as f32 - w as f32) / 2.0,
                        y: 210.0,
                        w: w as f32,
                        h: h as f32,
                        tex,
                        alpha: 1.0,
                    });
                }
                self.draw_centred_line(4, 320.0, out);
            }
        }
    }

    /// One baked dialog line, centred horizontally on the panel at the given vertical centre.
    fn draw_centred_line(&self, i: usize, cy: f32, out: &mut Vec<Draw>) {
        if let Some(&(tex, w, h)) = self.direct_faces.get(i) {
            out.push(Draw::Tex {
                x: (OUT_W as f32 - w as f32) / 2.0,
                y: cy - h as f32 / 2.0,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
    }

    /// The direct-connect lines, uploaded once at boot.
    pub fn set_direct_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.direct_faces = faces;
    }

    /// Open the direct-connect prompt — a long hold of B on the shelf.
    fn open_direct_link(&mut self) {
        self.direct_link.get_or_insert(0);
    }

    /// The direct-connect dialog's keys, answered while it is up and nowhere else. A on the
    /// prompt runs `System/slotnet.sh up`; B cancels the prompt, or leaves the live mode
    /// once it is up — which is what takes the radio back down. The wait between the two
    /// needs no key of its own: the address arriving is what ends it.
    fn direct_link_input(&mut self, action: Action) {
        match action {
            Action::GbaDown(Btn::A) => self.direct_confirm(),
            Action::GbaDown(Btn::B) => self.direct_cancel(),
            _ => {}
        }
    }

    /// A on the dialog: bring the card's own WiFi up. Nothing else happens on this stage;
    /// the wait that follows ends when the address arrives, or when B takes it back.
    fn direct_confirm(&mut self) {
        if self.direct_link == Some(0) {
            self.direct_link = Some(1);
            self.direct_at = self.now();
            self.direct_ip = None;
            self.direct_ip_face = None;
            let _ = std::fs::remove_file("/tmp/slot-ip");
            self.run_direct_script(true);
        }
    }

    /// B on the dialog: cancel the prompt, or leave the live mode (drop the radio). Any
    /// stage past the prompt has already run the enter script — a join still trying when
    /// the user walks away is exactly the one that must not be left up — so the teardown
    /// runs on all of them.
    fn direct_cancel(&mut self) {
        if self.direct_link.is_some_and(|s| s >= 1) {
            self.run_direct_script(false);
        }
        self.direct_link = None;
        self.direct_ip = None;
        self.direct_ip_face = None;
        self.direct_ip_dirty = false;
    }

    // ── Wi-Fi 配置浮层（2026-10-21，设置屏「无线网络」行那道门） ─────────────────

    /// 开。和键位弹窗同一条理由做成 App 级浮层：设置页在它底下原样等着，`B` 一路退回。
    pub fn open_wifi(&mut self) {
        if self.wifi.is_some() {
            return;
        }
        self.wifi = Some(WifiEdit {
            view: WifiView::Status,
            nets: Vec::new(),
            pick: 0,
            scanning: false,
            typing: None,
            note: None,
        });
        self.wifi_dirty = true;
    }

    /// 关。没有"取消"可言 —— 每一次改动都是当时写进卡的，`B` 只是走。
    fn close_wifi(&mut self) {
        self.wifi = None;
        self.wifi_rx = None;
        // 面不在这里放走：纹理归前端（`sync_wifi` 看见关了整族释放）。
    }

    pub fn wifi(&self) -> Option<&WifiEdit> {
        self.wifi.as_ref()
    }

    /// The card's credentials as the settings row's two words know them.
    pub fn wifi_conf(&self) -> Option<&slot_store::WifiConf> {
        self.wifi_conf.as_ref()
    }

    pub fn wifi_faces(&self) -> Option<&WifiFaces> {
        self.wifi_faces.as_ref()
    }

    pub fn wifi_dirty(&self) -> bool {
        self.wifi_dirty
    }

    pub fn set_wifi_faces(&mut self, faces: Option<WifiFaces>) {
        self.wifi_faces = faces;
        self.wifi_dirty = false;
    }

    /// 网格的字形，开机烤好交进来（`'•'` 是 PSK 的掩码点）。
    pub fn set_wifi_glyphs(&mut self, glyphs: Vec<(char, TexId, u32, u32)>) {
        self.wifi_glyphs = Some(glyphs);
    }

    /// One character's face, or `None` for one the grid does not carry — which is every
    /// non-ASCII character. A CJK SSID is configured on a desktop, by design.
    fn wifi_glyph(&self, c: char) -> Option<(TexId, u32, u32)> {
        self.wifi_glyphs
            .as_ref()?
            .iter()
            .find(|(ch, ..)| *ch == c)
            .map(|(_, t, w, h)| (*t, *w, *h))
    }

    /// Ask the card's script what is in the air. Seconds of work — the radio has to come
    /// up, the supplicant has to start, the APs have to answer — so it runs on a worker
    /// and the screen says so while it is out. The radio is left as it was found: a scan
    /// is not a connection.
    fn start_wifi_scan(&mut self) {
        if self.wifi.as_ref().is_some_and(|w| w.scanning) {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.wifi_rx = Some(rx);
        std::thread::spawn(move || {
            let nets = match std::process::Command::new("/bin/sh")
                .arg(root.join("System/slotnet.sh"))
                .arg("scan")
                .output()
            {
                Ok(out) => parse_wifi_scan(&String::from_utf8_lossy(&out.stdout)),
                Err(_) => Vec::new(),
            };
            let _ = tx.send(nets);
        });
        if let Some(w) = self.wifi.as_mut() {
            w.scanning = true;
            w.note = None;
        }
        self.wifi_dirty = true;
    }

    /// Collect a finished scan, from `timers` where every other slow thing's answer is
    /// collected. An empty list is an honest answer and lands on the list view saying so.
    fn wifi_poll(&mut self) {
        let Some(rx) = self.wifi_rx.as_ref() else {
            return;
        };
        let Ok(nets) = rx.try_recv() else {
            return;
        };
        self.wifi_rx = None;
        if let Some(w) = self.wifi.as_mut() {
            w.nets = nets;
            w.scanning = false;
            w.view = WifiView::List;
            w.pick = 0;
        }
        self.wifi_dirty = true;
    }

    /// Write the credentials where the script reads them. The note line is the only
    /// confirmation on this screen; the settings row's two words change with it.
    fn wifi_save(&mut self, ssid: String, psk: String) {
        let conf = slot_store::WifiConf { ssid, psk };
        if let Some(root) = &self.root {
            let _ = slot_store::WifiConf::write(root, &conf);
        }
        self.wifi_conf = Some(conf);
        let now = self.now();
        if let Some(w) = self.wifi.as_mut() {
            w.typing = None;
            w.view = WifiView::Status;
            w.note = Some(now);
        }
        self.wifi_dirty = true;
    }

    /// Take the credentials off the card. The script then has nothing to join, which is
    /// exactly what "not configured" means; the status view says so with no note needed.
    fn wifi_delete(&mut self) {
        if let Some(root) = &self.root {
            let _ = std::fs::remove_file(root.join(slot_store::WIFI_FILE));
        }
        self.wifi_conf = None;
        if let Some(w) = self.wifi.as_mut() {
            w.note = None;
        }
        self.wifi_dirty = true;
    }

    /// The screen's keys, ahead of every phase while it is up — the same seat the keymap
    /// sheet and the direct-connect dialog take.
    fn wifi_input(&mut self, action: Action) {
        if self
            .wifi
            .as_ref()
            .is_some_and(|w| w.typing.is_some())
        {
            self.wifi_typing_input(action);
            return;
        }
        let Some((view, scanning)) = self.wifi.as_ref().map(|w| (w.view, w.scanning)) else {
            return;
        };
        match view {
            WifiView::Status => match action {
                // 扫着的时候 `A` 不再排第二次：一次扫描要几秒，排队没有意义。
                Action::GbaDown(Btn::A) if !scanning => self.start_wifi_scan(),
                Action::GbaDown(Btn::X) => {
                    self.wifi_start_typing(WifiField::Ssid, String::new())
                }
                Action::GbaDown(Btn::Y) => self.wifi_delete(),
                Action::GbaDown(Btn::B) => self.close_wifi(),
                _ => {}
            },
            WifiView::List => match action {
                Action::GbaDown(Btn::Up) => {
                    if let Some(w) = self.wifi.as_mut() {
                        w.pick = w.pick.saturating_sub(1);
                    }
                }
                Action::GbaDown(Btn::Down) => {
                    if let Some(w) = self.wifi.as_mut() {
                        w.pick = (w.pick + 1).min(w.nets.len().saturating_sub(1));
                    }
                }
                Action::GbaDown(Btn::A) => self.wifi_pick(),
                Action::GbaDown(Btn::X) => {
                    self.wifi_start_typing(WifiField::Ssid, String::new())
                }
                Action::GbaDown(Btn::B) => {
                    if let Some(w) = self.wifi.as_mut() {
                        w.view = WifiView::Status;
                    }
                    self.wifi_dirty = true;
                }
                _ => {}
            },
        }
    }

    /// `A` on the scan list. A secured network asks for its password; an open one is
    /// saved as it stands. Either way the name is settled here — the grid only ever asks
    /// for the secret.
    fn wifi_pick(&mut self) {
        let Some(net) = self
            .wifi
            .as_ref()
            .and_then(|w| w.nets.get(w.pick).cloned())
        else {
            return;
        };
        if net.secured {
            self.wifi_start_typing(WifiField::Psk, net.ssid);
        } else {
            self.wifi_save(net.ssid, String::new());
        }
    }

    fn wifi_start_typing(&mut self, field: WifiField, ssid: String) {
        if let Some(w) = self.wifi.as_mut() {
            w.typing = Some(WifiTyping {
                field,
                ssid,
                psk: String::new(),
                layer: 0,
                row: 0,
                col: 0,
            });
            w.note = None;
        }
        self.wifi_dirty = true;
    }

    /// The grid's keys: the caret walks the cells, `A` types the one under it, `B`
    /// erases, `L`/`R` change layer, `START` finishes, `MENU` throws the typing away.
    fn wifi_typing_input(&mut self, action: Action) {
        match action {
            Action::GbaDown(Btn::Left) => {
                if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
                    t.col = t.col.saturating_sub(1);
                }
            }
            Action::GbaDown(Btn::Right) => {
                if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
                    t.col = (t.col + 1).min(WIFI_LAYERS[t.layer].cols - 1);
                }
            }
            Action::GbaDown(Btn::Up) => {
                if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
                    t.row = t.row.saturating_sub(1);
                }
            }
            Action::GbaDown(Btn::Down) => {
                if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
                    t.row = (t.row + 1).min(WIFI_LAYERS[t.layer].rows - 1);
                }
            }
            Action::GbaDown(Btn::L1) => {
                let n = WIFI_LAYERS.len();
                if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
                    t.layer = (t.layer + n - 1) % n;
                    t.row = t.row.min(WIFI_LAYERS[t.layer].rows - 1);
                    t.col = t.col.min(WIFI_LAYERS[t.layer].cols - 1);
                }
            }
            Action::GbaDown(Btn::R1) => {
                let n = WIFI_LAYERS.len();
                if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
                    t.layer = (t.layer + 1) % n;
                    t.row = t.row.min(WIFI_LAYERS[t.layer].rows - 1);
                    t.col = t.col.min(WIFI_LAYERS[t.layer].cols - 1);
                }
            }
            Action::GbaDown(Btn::A) => self.wifi_type_cell(),
            Action::GbaDown(Btn::B) => self.wifi_erase(),
            Action::GbaDown(Btn::Start) => self.wifi_finish_typing(),
            // MENU 是"不要了"：网格里 `B` 已经是退格，取消得另找一个键。
            Action::OpenMenu => {
                if let Some(w) = self.wifi.as_mut() {
                    w.typing = None;
                }
                self.wifi_dirty = true;
            }
            _ => {}
        }
    }

    /// `A` in the grid: the cell's character goes into the field in hand. A full field
    /// refuses it — an SSID is 32 at most and a WPA passphrase 63 — and the refusal is
    /// silent because the field in hand is on screen, full, in front of the user.
    fn wifi_type_cell(&mut self) {
        let Some(w) = self.wifi.as_ref() else {
            return;
        };
        let Some(t) = w.typing.as_ref() else {
            return;
        };
        let layer = WIFI_LAYERS[t.layer];
        let Some(&byte) = layer.chars.as_bytes().get(t.row * layer.cols + t.col) else {
            return;
        };
        let c = byte as char;
        let (len, limit) = match t.field {
            WifiField::Ssid => (t.ssid.chars().count(), 32),
            WifiField::Psk => (t.psk.chars().count(), 63),
        };
        if len >= limit {
            return;
        }
        if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
            match t.field {
                WifiField::Ssid => t.ssid.push(c),
                WifiField::Psk => t.psk.push(c),
            }
        }
    }

    /// `B` in the grid: erase the last character of the field in hand.
    fn wifi_erase(&mut self) {
        if let Some(t) = self.wifi.as_mut().and_then(|w| w.typing.as_mut()) {
            match t.field {
                WifiField::Ssid => {
                    t.ssid.pop();
                }
                WifiField::Psk => {
                    t.psk.pop();
                }
            }
        }
    }

    /// `START` in the grid. A name moves on to its password — an empty one goes nowhere,
    /// since a network with no name is one nothing can join. A password is the end, and
    /// an empty one is an open network, which is a configuration and not a mistake.
    fn wifi_finish_typing(&mut self) {
        let Some(t) = self.wifi.as_ref().and_then(|w| w.typing.clone()) else {
            return;
        };
        match t.field {
            WifiField::Ssid => {
                if !t.ssid.is_empty() {
                    self.wifi_start_typing(WifiField::Psk, t.ssid);
                }
            }
            WifiField::Psk => self.wifi_save(t.ssid, t.psk),
        }
    }

    /// The Wi-Fi config screen. Three views over one panel: what the card is set to, what
    /// is in the air, and the grid while something is being typed.
    fn draw_wifi(&self, out: &mut Vec<Draw>) {
        // ★ 2026-11-03（六六）：「字体有点小而且看起来有点单调，能把默认壁纸加进去那几个页面吗」
        //   原来这里整屏铺不透明 `opening()`，把设置页的壁纸整个盖住，WiFi 页就只剩一片平底。
        //   改成一层半透明纱：底下的壁纸透出来，字仍压得清（和 keymap 弹窗的「自带纱」同一手法）。
        let mut veil = slot_ui::opening();
        veil[3] = 0.72;
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: veil,
        });
        let Some(faces) = self.wifi_faces.as_ref() else {
            return;
        };
        let Some(w) = self.wifi.as_ref() else {
            return;
        };
        let ink = slot_ui::palette::ink_f();
        let dim = [ink[0] * 0.35, ink[1] * 0.35, ink[2] * 0.35, 1.0];
        // 标题，居中。
        let (t, tw, th) = faces.title;
        out.push(Draw::Tex {
            x: (OUT_W as f32 - tw as f32) / 2.0,
            y: WIFI_TITLE_Y,
            w: tw as f32,
            h: th as f32,
            tex: t,
            alpha: 1.0,
        });
        let typing = w.typing.as_ref();
        if let Some(t) = typing {
            self.draw_wifi_typing(faces, t, ink, dim, out);
        } else {
            match w.view {
                WifiView::Status => {
                    self.draw_wifi_status(faces, w, ink, dim, out);
                }
                WifiView::List => self.draw_wifi_list(faces, w, ink, dim, out),
            }
        }
        // 页脚提示：三个视图各一句，都在开机烤好的那三张里。
        let hint = match typing {
            Some(_) => faces.hint_edit,
            None => match w.view {
                WifiView::Status => faces.hint_status,
                WifiView::List => faces.hint_list,
            },
        };
        let (t, hw, hh) = hint;
        out.push(Draw::Tex {
            x: (OUT_W as f32 - hw as f32) / 2.0,
            y: WIFI_FOOT_Y,
            w: hw as f32,
            h: hh as f32,
            tex: t,
            alpha: 1.0,
        });
    }

    /// The status view: what the card is set to, and the three things that can be done.
    fn draw_wifi_status(
        &self,
        faces: &WifiFaces,
        w: &WifiEdit,
        _ink: [f32; 4],
        _dim: [f32; 4],
        out: &mut Vec<Draw>,
    ) {
        let x = WIFI_MARGIN;
        let (t, tw, th) = faces.label_current;
        out.push(Draw::Tex {
            x,
            y: WIFI_BODY_Y,
            w: tw as f32,
            h: th as f32,
            tex: t,
            alpha: 1.0,
        });
        let line = if w.scanning {
            Some(faces.line_scanning)
        } else {
            faces.ssid.or(Some(faces.line_none))
        };
        if let Some((t, tw, th)) = line {
            out.push(Draw::Tex {
                x,
                y: WIFI_BODY_Y + WIFI_PITCH,
                w: tw as f32,
                h: th as f32,
                tex: t,
                alpha: 1.0,
            });
        }
        // 三个动作。扫着的时候第一个换成"正在扫描"；删配置没配可删时藏起来。
        let mut y = WIFI_BODY_Y + WIFI_PITCH * 3.0;
        let first = if w.scanning {
            faces.line_scanning
        } else {
            faces.act_scan
        };
        for (t, tw, th) in [first, faces.act_manual] {
            out.push(Draw::Tex {
                x,
                y,
                w: tw as f32,
                h: th as f32,
                tex: t,
                alpha: 1.0,
            });
            y += WIFI_PITCH;
        }
        if self.wifi_conf.is_some() {
            let (t, tw, th) = faces.act_del;
            out.push(Draw::Tex {
                x,
                y,
                w: tw as f32,
                h: th as f32,
                tex: t,
                alpha: 1.0,
            });
        }
        // 那一行注脚（"已保存"），八秒后自己退场。台词是定句，脸在开机那批里。
        if let Some(at) = w.note {
            if self.now().saturating_sub(at) < 8000 {
                let (t, tw, th) = faces.line_saved;
                out.push(Draw::Tex {
                    x,
                    y: WIFI_BODY_Y + WIFI_PITCH * 6.0,
                    w: tw as f32,
                    h: th as f32,
                    tex: t,
                    alpha: 1.0,
                });
            }
        }
    }

    /// The scan list: one network per line, the caret on the one in hand, and the signal
    /// as three bars — the only honest way to say "how far" without numbers nobody reads.
    fn draw_wifi_list(
        &self,
        faces: &WifiFaces,
        w: &WifiEdit,
        ink: [f32; 4],
        dim: [f32; 4],
        out: &mut Vec<Draw>,
    ) {
        let x = WIFI_MARGIN;
        let (t, tw, th) = faces.line_pick;
        out.push(Draw::Tex {
            x,
            y: WIFI_BODY_Y,
            w: tw as f32,
            h: th as f32,
            tex: t,
            alpha: 1.0,
        });
        if w.nets.is_empty() {
            let (t, tw, th) = faces.line_no_nets;
            out.push(Draw::Tex {
                x,
                y: WIFI_BODY_Y + WIFI_PITCH,
                w: tw as f32,
                h: th as f32,
                tex: t,
                alpha: 1.0,
            });
            return;
        }
        let visible = WIFI_LIST_MAX.min(w.nets.len());
        let first = w.pick.min(w.nets.len().saturating_sub(visible));
        for row in 0..visible {
            let i = first + row;
            let Some(net) = w.nets.get(i) else { continue };
            let y = WIFI_BODY_Y + WIFI_PITCH + row as f32 * WIFI_PITCH;
            if i == w.pick {
                // 选中条：一整条淡底，字画在它上面。四十四高的字块，行距四十八 ——
                // 上下各留一，条本身四十六，正好把字箍在中间（旧值 −6 / 32 是二十高的
                // 字块配三十四行距时代的余量）。
                out.push(Draw::Rect {
                    x: x - 8.0,
                    y: y - 1.0,
                    w: OUT_W as f32 - (x - 8.0) * 2.0,
                    h: WIFI_PITCH - 2.0,
                    colour: dim,
                });
            }
            if let Some((t, tw, th)) = faces.nets.get(i).copied() {
                out.push(Draw::Tex {
                    x,
                    y,
                    w: tw as f32,
                    h: th as f32,
                    tex: t,
                    alpha: 1.0,
                });
            }
            // 三格信号：−50 以上满格，−70 以上两格，再远一格。
            let bars = if net.signal > -50 {
                3
            } else if net.signal > -70 {
                2
            } else {
                1
            };
            let bx = OUT_W as f32 - WIFI_MARGIN - 3.0 * 10.0;
            for b in 0..3 {
                out.push(Draw::Rect {
                    x: bx + b as f32 * 10.0,
                    y: y + 16.0,
                    w: 6.0,
                    h: 6.0 + b as f32 * 3.0,
                    colour: if b < bars { ink } else { dim },
                });
            }
        }
    }

    /// The typing view: the field in hand (masked for a password), then the grid.
    fn draw_wifi_typing(
        &self,
        faces: &WifiFaces,
        t: &WifiTyping,
        ink: [f32; 4],
        dim: [f32; 4],
        out: &mut Vec<Draw>,
    ) {
        let x = WIFI_MARGIN;
        let (label, tw, th) = match t.field {
            WifiField::Ssid => faces.label_ssid,
            WifiField::Psk => faces.label_psk,
        };
        out.push(Draw::Tex {
            x,
            y: WIFI_EDIT_Y,
            w: tw as f32,
            h: th as f32,
            tex: label,
            alpha: 1.0,
        });
        // 输进去的东西：名字照实，密码画掩码点。逐字拿网格的字形拼，改一个键不用重烤。
        let text = match t.field {
            WifiField::Ssid => t.ssid.as_str(),
            WifiField::Psk => t.psk.as_str(),
        };
        let mut cx = x;
        let cy = WIFI_EDIT_TEXT_Y;
        for c in text.chars() {
            let g = if matches!(t.field, WifiField::Psk) {
                self.wifi_glyph('•')
            } else {
                self.wifi_glyph(c)
            };
            let Some((tex, gw, gh)) = g else { continue };
            out.push(Draw::Tex {
                x: cx,
                y: cy,
                w: gw as f32,
                h: gh as f32,
                tex,
                alpha: 1.0,
            });
            cx += gw as f32 + 2.0;
        }
        // 光标：字后面一条竖杠。
        out.push(Draw::Rect {
            x: cx,
            y: cy,
            w: 2.0,
            h: 20.0,
            colour: ink,
        });
        // 密码下面那句"留空就是开放网络"——只有密码视图画。
        if matches!(t.field, WifiField::Psk) {
            let (t2, tw, th) = faces.label_open;
            out.push(Draw::Tex {
                x,
                y: WIFI_EDIT_NOTE_Y,
                w: tw as f32,
                h: th as f32,
                tex: t2,
                alpha: 1.0,
            });
        }
        // 网格：当前层，选中格一块淡底，字画在它上面。
        let layer = WIFI_LAYERS[t.layer];
        let grid_w = layer.cols as f32 * (WIFI_CELL + WIFI_CELL_GAP) - WIFI_CELL_GAP;
        let gx = (OUT_W as f32 - grid_w) / 2.0;
        let gy = WIFI_GRID_Y;
        for row in 0..layer.rows {
            for col in 0..layer.cols {
                let at = row * layer.cols + col;
                let Some(&byte) = layer.chars.as_bytes().get(at) else {
                    continue;
                };
                let cx = gx + col as f32 * (WIFI_CELL + WIFI_CELL_GAP);
                let cy = gy + row as f32 * (WIFI_CELL + WIFI_CELL_GAP);
                let chosen = row == t.row && col == t.col;
                out.push(Draw::Rect {
                    x: cx,
                    y: cy,
                    w: WIFI_CELL,
                    h: WIFI_CELL,
                    colour: if chosen { ink } else { dim },
                });
                let c = byte as char;
                if c != ' ' {
                    if let Some((tex, gw, gh)) = self.wifi_glyph(c) {
                        out.push(Draw::Tex {
                            x: cx + (WIFI_CELL - gw as f32) / 2.0,
                            y: cy + (WIFI_CELL - gh as f32) / 2.0,
                            w: gw as f32,
                            h: gh as f32,
                            tex,
                            alpha: 1.0,
                        });
                    }
                }
            }
        }
    }

    // =====================================================================================
    // ★ 2026-11-11：货架首字母搜索（SELECT+Y）
    //
    // 键盘复刻 Wi-Fi 录入那张四层全键盘：同样的 `WIFI_LAYERS`、同样的 `wifi_glyphs`、
    // 同样的几何常量。区别只在「打进去的串拿去筛货架」而不是「写进 wifi.conf」。
    // 不做 T9：这台机器没有 0–9 数字键，九宫格只能退化成另一个方向键网格，
    // 而拼音首字母串也没有词库可供 T9 预测（2026-11-11 评估结论）。
    // =====================================================================================

    /// 前端用：浮层开没开、定句脸在不在、以及整族放走。
    pub fn search_open(&self) -> bool {
        self.search.is_some()
    }
    pub fn search_faces(&self) -> Option<&SearchFaces> {
        self.search_faces.as_ref()
    }
    pub fn set_search_faces(&mut self, faces: Option<SearchFaces>) {
        self.search_faces = faces;
    }

    /// 触发：货架上 SELECT+Y。进来的那一刻把整份 `view` 和光标上的卡名一起存下，
    /// 取消时原样还回去；`search_origin` 只在没有筛选链时记一次，供「X 全部」回全库。
    fn open_search(&mut self) {
        if self.row != ShelfRow::Games || self.search.is_some() {
            return;
        }
        let back_view = self.shelf.view_snapshot();
        let back_stem = self.shelf.current_cart().map(|c| c.stem.clone());
        if self.search_origin.is_none() {
            self.search_origin = Some((back_view.clone(), back_stem.clone()));
        }
        self.search = Some(SearchState {
            query: String::new(),
            layer: 0,
            row: 0,
            col: 0,
            hits: back_view.clone(),
            back_view,
            back_stem,
        });
        // 空查询不调 apply_search：一进来就把光标弹到第一张卡，是这功能最不需要的动画。
    }

    /// `START` 应用（货架留在筛出来的那批上）/ `MENU` 取消（原样还原）。
    /// 空结果时两者同义：没有任何一批结果值得留下来。
    fn close_search(&mut self, apply: bool) {
        let Some(s) = self.search.take() else {
            return;
        };
        if !apply || s.hits.is_empty() {
            self.shelf.set_view(s.back_view, s.back_stem.as_deref());
        }
        // 应用时 `search_origin` 故意留着：下一次开搜索按 X 仍能一步回全库。
    }

    /// 网格的键。和 Wi-Fi 录入页同一套手感，只多了 X / START / MENU 三个动作。
    fn search_input(&mut self, action: Action) {
        match action {
            Action::GbaDown(Btn::Left) => {
                if let Some(s) = self.search.as_mut() {
                    s.col = s.col.saturating_sub(1);
                }
            }
            Action::GbaDown(Btn::Right) => {
                if let Some(s) = self.search.as_mut() {
                    let max = WIFI_LAYERS[s.layer].cols - 1;
                    s.col = (s.col + 1).min(max);
                }
            }
            Action::GbaDown(Btn::Up) => {
                if let Some(s) = self.search.as_mut() {
                    s.row = s.row.saturating_sub(1);
                }
            }
            Action::GbaDown(Btn::Down) => {
                if let Some(s) = self.search.as_mut() {
                    let max = WIFI_LAYERS[s.layer].rows - 1;
                    s.row = (s.row + 1).min(max);
                }
            }
            Action::GbaDown(Btn::L1) => {
                let n = WIFI_LAYERS.len();
                if let Some(s) = self.search.as_mut() {
                    s.layer = (s.layer + n - 1) % n;
                    s.row = s.row.min(WIFI_LAYERS[s.layer].rows - 1);
                    s.col = s.col.min(WIFI_LAYERS[s.layer].cols - 1);
                }
            }
            Action::GbaDown(Btn::R1) => {
                let n = WIFI_LAYERS.len();
                if let Some(s) = self.search.as_mut() {
                    s.layer = (s.layer + 1) % n;
                    s.row = s.row.min(WIFI_LAYERS[s.layer].rows - 1);
                    s.col = s.col.min(WIFI_LAYERS[s.layer].cols - 1);
                }
            }
            Action::GbaDown(Btn::A) => self.search_type_cell(),
            Action::GbaDown(Btn::B) => {
                if let Some(s) = self.search.as_mut() {
                    s.query.pop();
                }
                self.apply_search();
            }
            // 「X 全部」：清输入并把货架放回这一轮筛选开始之前的样子。
            Action::GbaDown(Btn::X) => self.search_clear(),
            Action::GbaDown(Btn::Start) => self.close_search(true),
            Action::OpenMenu => self.close_search(false),
            _ => {}
        }
    }

    /// A：把光标下那一格敲进查询串，立刻重筛。
    fn search_type_cell(&mut self) {
        let Some(s) = self.search.as_ref() else {
            return;
        };
        let layer = WIFI_LAYERS[s.layer];
        let Some(&byte) = layer.chars.as_bytes().get(s.row * layer.cols + s.col) else {
            return;
        };
        let c = byte as char;
        // 键盘末尾那几格是留白，不是字符；24 个字符已经顶满 640px 的输入行。
        if c == ' ' || s.query.chars().count() >= 24 {
            return;
        }
        if let Some(s) = self.search.as_mut() {
            s.query.push(c);
        }
        self.apply_search();
    }

    /// 「X 全部」。`origin` 是第一次开搜索时的视图，不是这一次进来的视图 ——
    /// 所以 START 应用过筛选之后，再开一次搜索按 X 也能回到没筛之前。
    fn search_clear(&mut self) {
        let Some((view, stem)) = self.search_origin.clone() else {
            return;
        };
        if let Some(s) = self.search.as_mut() {
            s.query.clear();
            s.back_view = view.clone();
            s.back_stem = stem.clone();
            s.hits = view.clone();
        }
        self.shelf.set_view(view, stem.as_deref());
        self.search_origin = None;
    }

    /// 边打边缩。**只在 `back_view` 里筛**：收藏夹、子文件夹、机种货架都是它的子集，
    /// 搜索不该把用户刚切好的范围偷偷放大回全库。
    fn apply_search(&mut self) {
        let Some(s) = self.search.as_ref() else {
            return;
        };
        let q = s.query.clone();
        let base = s.back_view.clone();
        if q.trim().is_empty() {
            self.shelf.set_view(base.clone(), None);
            if let Some(s) = self.search.as_mut() {
                s.hits = base;
            }
            return;
        }
        let hits: Vec<usize> = base
            .iter()
            .copied()
            .filter(|&i| {
                self.shelf
                    .carts
                    .get(i)
                    .is_some_and(|c| slot_store::search::matches(&q, &c.stem))
            })
            .collect();
        if hits.is_empty() {
            // 空 view 不是「没有结果」，是顶层空兜底（§6d）会莫名其妙跳进子文件夹。
            // 保留上一批结果，只把提示行换成「没有匹配的游戏」。
            if let Some(s) = self.search.as_mut() {
                s.hits.clear();
            }
            return;
        }
        self.shelf.set_view(hits.clone(), None);
        if let Some(s) = self.search.as_mut() {
            s.hits = hits;
        }
    }

    /// 整屏浮层：半透明纱 + 标题 + 输入串 + 动作提示 + 层标 + 键盘 + 页脚。
    /// 字形全部来自 `wifi_glyphs`，一个键都不用重烘纹理。
    fn draw_search(&self, out: &mut Vec<Draw>) {
        let mut veil = slot_ui::opening();
        veil[3] = 0.78;
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: veil,
        });
        let (Some(faces), Some(s)) = (self.search_faces.as_ref(), self.search.as_ref()) else {
            return;
        };
        let ink = slot_ui::palette::ink_f();
        let dim = [ink[0] * 0.35, ink[1] * 0.35, ink[2] * 0.35, 1.0];
        let (t, tw, th) = faces.title;
        out.push(Draw::Tex {
            x: (OUT_W as f32 - tw as f32) / 2.0,
            y: WIFI_TITLE_Y,
            w: tw as f32,
            h: th as f32,
            tex: t,
            alpha: 1.0,
        });
        // 输入串：照实回显（不是密码），逐字拿网格字形拼。
        let mut cx = WIFI_MARGIN;
        for c in s.query.chars() {
            let Some((tex, gw, gh)) = self.wifi_glyph(c) else {
                continue;
            };
            out.push(Draw::Tex {
                x: cx,
                y: WIFI_EDIT_TEXT_Y,
                w: gw as f32,
                h: gh as f32,
                tex,
                alpha: 1.0,
            });
            cx += gw as f32 + 2.0;
        }
        out.push(Draw::Rect {
            x: cx,
            y: WIFI_EDIT_TEXT_Y,
            w: 2.0,
            h: 20.0,
            colour: ink,
        });
        // 动作提示；一个字都搜不到时换成专用提示，绝不交一个空 view 给货架。
        let (a, aw, ah) = if s.hits.is_empty() && !s.query.trim().is_empty() {
            faces.empty
        } else {
            faces.hint_act
        };
        out.push(Draw::Tex {
            x: WIFI_MARGIN,
            y: WIFI_EDIT_NOTE_Y,
            w: aw as f32,
            h: ah as f32,
            tex: a,
            alpha: 1.0,
        });
        // 当前层标记：abc / ABC / 123 / #+=，右对齐在提示行尾端。
        let tag: &[char] = match s.layer {
            0 => &['a', 'b', 'c'],
            1 => &['A', 'B', 'C'],
            2 => &['1', '2', '3'],
            _ => &['#', '+', '='],
        };
        let mut tx = OUT_W as f32 - WIFI_MARGIN;
        for &c in tag.iter().rev() {
            if let Some((tex, gw, gh)) = self.wifi_glyph(c) {
                tx -= gw as f32 + 2.0;
                out.push(Draw::Tex {
                    x: tx,
                    y: WIFI_EDIT_NOTE_Y + 12.0,
                    w: gw as f32,
                    h: gh as f32,
                    tex,
                    alpha: 1.0,
                });
            }
        }
        // 四层全键盘：选中格一块淡底，字画在它上面。
        let layer = WIFI_LAYERS[s.layer];
        let grid_w = layer.cols as f32 * (WIFI_CELL + WIFI_CELL_GAP) - WIFI_CELL_GAP;
        let gx = (OUT_W as f32 - grid_w) / 2.0;
        for row in 0..layer.rows {
            for col in 0..layer.cols {
                let at = row * layer.cols + col;
                let Some(&byte) = layer.chars.as_bytes().get(at) else {
                    continue;
                };
                let x = gx + col as f32 * (WIFI_CELL + WIFI_CELL_GAP);
                let y = WIFI_GRID_Y + row as f32 * (WIFI_CELL + WIFI_CELL_GAP);
                let chosen = row == s.row && col == s.col;
                out.push(Draw::Rect {
                    x,
                    y,
                    w: WIFI_CELL,
                    h: WIFI_CELL,
                    colour: if chosen { ink } else { dim },
                });
                let c = byte as char;
                if c != ' ' {
                    if let Some((tex, gw, gh)) = self.wifi_glyph(c) {
                        out.push(Draw::Tex {
                            x: x + (WIFI_CELL - gw as f32) / 2.0,
                            y: y + (WIFI_CELL - gh as f32) / 2.0,
                            w: gw as f32,
                            h: gh as f32,
                            tex,
                            alpha: 1.0,
                        });
                    }
                }
            }
        }
        let (h, hw, hh) = faces.hint_grid;
        out.push(Draw::Tex {
            x: (OUT_W as f32 - hw as f32) / 2.0,
            y: WIFI_FOOT_Y,
            w: hw as f32,
            h: hh as f32,
            tex: h,
            alpha: 1.0,
        });
    }

    /// Run the enter/exit shell detached, so the dialog keeps drawing while the radio comes up.
    fn run_direct_script(&self, enter: bool) {
        let script = if enter { DIRECT_ENTER } else { DIRECT_EXIT };
        let _ = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("{} >/dev/null 2>&1", script))
            .spawn();
    }

    /// The address the WiFi link came up on, and the frontend's bake hook for it. The dialog's own
    /// lines are baked once at boot, but this one is whatever DHCP handed out, so the frontend
    /// bakes it when `direct_ip_dirty` says it has arrived.
    pub fn direct_ip_dirty(&self) -> bool {
        self.direct_ip_dirty
    }

    pub fn direct_ip_str(&self) -> Option<&str> {
        self.direct_ip.as_deref()
    }

    pub fn set_direct_ip_face(&mut self, face: Option<(TexId, u32, u32)>) {
        self.direct_ip_face = face;
    }

    pub fn clear_direct_ip_dirty(&mut self) {
        self.direct_ip_dirty = false;
    }

    /// ★★ 2026-10-26（六六）：**机种面板** —— 十台机器两行五列，手里那一格放大到 120
    /// 并带一圈随主题走的呼吸光。
    ///
    /// 这一层只做三件事：把 `root::MACHINES` 翻成十个格子（哪一台有卡、有没有自己的标识）、
    /// 把面板自己的时钟读成这一帧的几个数（光标、上一格、走了多少、整块的不透明度），
    /// 交给 `slot_ui` 去画。几何、配色、光晕全在那边 —— 这里不该知道 120 这个数。
    fn draw_sys_picker(&self, picker: &SysPicker, out: &mut Vec<Draw>) {
        let Some(faces) = &self.sys_faces else {
            return;
        };
        let now = self.now();
        let cells: Vec<slot_ui::SysPickerCell> = crate::root::MACHINES
            .iter()
            .enumerate()
            .map(|(i, system)| slot_ui::SysPickerCell {
                icon: self.sys_icons.get(i).copied().flatten(),
                // 这一台在卡上一张卡都没有：格子还在，只是先暗下去，按 A 会被拒绝。
                empty: !self.shelf.carts.iter().any(|c| c.system() == *system),
            })
            .collect();
        let view = slot_ui::SysPickerView {
            cursor: picker.cursor(),
            prev: picker.prev(),
            moved: picker.moved(now),
            alpha: picker.alpha(now),
            // 呼吸走的还是灯那一条曲线（3400 ms 一个来回，0.55..1.0）：这台机器上"亮着"
            // 只有一种节奏，多一个节奏就变成两套语言。
            breathe: slot_ui::light_breathe(now as u32),
            shake: picker.shake(now),
            cells: &cells,
        };
        slot_ui::draw_sys_picker(faces, &view, out);
    }

    /// 十台的标识贴图与这一块屏的家具，由前端烘一次交过来。
    ///
    /// 烘在 `upload_late_furniture` 里（开机后第一帧）：这十三张图没有一个在开机那 4.4 秒
    /// 的路上，而长按 Y 至少要按够 500 ms —— 时间上永远追得上。
    pub fn set_sys_picker_faces(
        &mut self,
        icons: Vec<Option<(Upload, Upload)>>,
        faces: slot_ui::SysPickerFaces,
    ) {
        self.sys_icons = icons;
        self.sys_faces = Some(faces);
    }

    /// The picker, but only once it has started opening: `None` while it is still standing on
    /// the shelf waiting for this cart's faces, so nothing of it is on screen yet and the row
    /// has not made way for it.
    fn core_picker_shown(&self) -> Option<CorePicker> {
        self.core_picker.filter(|p| !p.waiting())
    }

    /// The open cart over the shelf that is making way for it: the board growing out of the cart
    /// that stood there, both sockets on it, the chip in one of them or in the air between, the
    /// lid slid off it and lifted away with the cart's own face on it, and the legend. Over the
    /// shelf and under the HUD: brightness and blue light are still answered while it is up.
    ///
    /// `ready` is this cart's own board and lid, not merely whatever is on the GPU: a picker
    /// that started on `FACES_WAIT_MS`'s cap has neither yet, and must never wear a build left
    /// over from the cart the caret was on before — showing nothing is the only honest choice
    /// until this cart's own faces land, so the board, the sockets, the chip and the chip's own
    /// shadow wait for `ready` and the lid falls back to the shelf's plain face for this cart.
    fn draw_core_picker(&self, picker: &CorePicker, out: &mut Vec<Draw>) {
        let now = self.now();
        let progress = picker.openness(now);
        // The shadows and the legend come in with the lift, not with the slide.
        let lift = lift_of(progress);
        let board = board_at(progress);
        let zoom = board_zoom(board);
        let ready = self.core_faces_ready();
        // A card that supplies its own board has the chips drawn into the art, so there is
        // nothing for the picker to place: no sockets, no chip, and no shadow for a chip that
        // is not coming across. What is left of the picker on such a card is the two lamps.
        //
        // ★ 2026-10-24（六六）：「只有 GBA 需要换核心啊，其它机种没这个需求」——
        //   这个 picker 本身就只对 GBA 打开（`open_core_picker` 开头那道
        //   `if cart.system() != System::Gba { return; }`），所以板面美术是**一张**，
        //   问它不需要机种。
        let own_board = slot_ui::board_art().is_some();

        if ready {
            // Opaque from the first frame, and the sockets and the chip with it: the back half
            // was always there under the front, and the slide only uncovers it.
            if let Some(tex) = self.core_board_face {
                out.push(Draw::Tex {
                    x: board.x,
                    y: board.y,
                    w: board.w,
                    h: board.h,
                    tex,
                    alpha: 1.0,
                });
            }
            // The card's board carries its own chips, so the lamps are the whole of what the
            // picker adds: the one the cursor is over lit orange, the other out, and the lit one
            // green once the choice has been made and the picker is on its way out. Nothing is
            // baked into the board texture, because the state changes every press.
            if let Some(art) = slot_ui::board_art() {
                let chip = picker.chip(now);
                let here = usize::from(chip.across >= 0.5);
                let done = picker.closing();
                for (i, lamp) in art.lamps.iter().enumerate() {
                    let colour = if i == here {
                        if done {
                            [0.18, 0.76, 0.37, 1.0]
                        } else {
                            [1.0, 0.55, 0.10, 1.0]
                        }
                    } else {
                        // Out, but not invisible: against the board's own plastic a dark lamp
                        // reads as a hole rather than as a lamp that is off.
                        [0.62, 0.62, 0.65, 1.0]
                    };
                    out.push(Draw::Rect {
                        x: board.x + lamp.x * board.w,
                        y: board.y + lamp.y * board.h,
                        w: lamp.w * board.w,
                        h: lamp.h * board.h,
                        colour,
                    });
                }
            }
            // The sockets are the **built-in** board's, and only its: a card that drew its own
            // board drew its chips into it as well, so placing these two on top leaves two empty
            // positions standing over a finished board — the picker's own furniture printed over
            // somebody else's drawing. The chip below is gated for the same reason and always
            // was; this loop was not, which is the pair of leftovers the player saw.
            if !own_board {
                // A face drawn at its own size is only sharp on whole pixels.
                for (i, tex) in self.core_socket_faces.iter().copied().enumerate() {
                    let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
                    out.push(Draw::Tex {
                        x: x.round(),
                        y: y.round(),
                        w: SOCKET_W as f32 * zoom,
                        h: SOCKET_H as f32 * zoom,
                        tex,
                        alpha: 1.0,
                    });
                }
            }

            let chip = picker.chip(now);
            let u = CHIP_U[0] + (CHIP_U[1] - CHIP_U[0]) * chip.across;
            if !own_board && chip.lift > 0.0 {
                if let Some(tex) = self.core_chip_shadow_face {
                    // Under the body's middle and 90 units down the board, where the mockup's
                    // oval falls: low enough to read as cast on the board rather than tucked
                    // under the pins.
                    let (cx, cy) = on_board(board, u + 19.0, CHIP_V + 29.4);
                    let (w, h) = (SHADOW_W as f32 * zoom, SHADOW_H as f32 * zoom);
                    out.push(Draw::Tex {
                        x: cx - w / 2.0,
                        y: cy - h / 2.0,
                        w,
                        h,
                        tex,
                        alpha: 0.6 * chip.lift * lift,
                    });
                }
            }
            // The blank stand-in is for a chip in flight, and there is no chip to fly across a
            // board that has its own: drawn anyway it would be a grey chip flashing over a
            // finished board, which is worse than nothing.
            let face = if own_board {
                None
            } else {
                match chip.seated {
                    Some(core) => self.core_chip_faces.get(core.index()).copied(),
                    None => self.core_blank_chip_face,
                }
            };
            if let Some(tex) = face {
                let (x, y) = on_board(board, u, CHIP_V - HOP_LIFT * chip.lift);
                let body = Placed {
                    x: x + chip.shake,
                    y,
                    w: CHIP_W as f32 * zoom,
                    h: CHIP_H as f32 * zoom,
                };
                // Whole pixels, as the sockets: a seated chip is drawn at its own size too.
                let at = grown(body, TURN_PAD as f32 * zoom);
                out.push(Draw::Turned {
                    x: at.x.round(),
                    y: at.y.round(),
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn: chip.tip,
                });
            }
        }

        // The soft oval on the ground under the lid. Without it the lid reads as printed on the
        // backdrop rather than held up off the board. The chip's shadow, stretched: it grows
        // with the lid and comes in as the lid rises. Drawn whether or not this cart's faces are
        // ready: the lid is always something, the fallback included, and it always casts one.
        if let Some(tex) = self.core_chip_shadow_face {
            let (lid, _) = lid_at(progress);
            let k = lid.w / lid_at(1.0).0.w;
            let (w, h) = (LID_SHADOW_W * k, LID_SHADOW_H * k);
            out.push(Draw::Tex {
                x: lid.x + (lid.w - w) / 2.0,
                y: lid.y + lid.h + LID_SHADOW_DROP * k - h / 2.0,
                w,
                h,
                tex,
                alpha: LID_SHADOW_ALPHA * lift,
            });
        }

        if ready {
            // Always opaque: at the very start and end of the movement the lid is the cart on
            // the shelf, and a cart there does not fade.
            if let Some(tex) = self.core_lid_face {
                let (lid, turn) = lid_at(progress);
                let at = grown(lid, TURN_PAD as f32 * lid.w / CART_W as f32);
                out.push(Draw::Turned {
                    x: at.x,
                    y: at.y,
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn,
                });
            }
        } else if let Some((_, Some(tex))) =
            self.selected_stem().and_then(|stem| self.row().find(stem))
        {
            // The cap ran out before this cart's own lid arrived. The shelf's own face for the
            // cart is the only thing left to lift — not `core_lid_face`, which would still be
            // whatever cart the worker built last — and it is drawn unpadded: unlike a face
            // built for the picker, the shelf's face carries no transparent border to grow into.
            let (lid, turn) = lid_at(progress);
            out.push(Draw::Turned {
                x: lid.x,
                y: lid.y,
                w: lid.w,
                h: lid.h,
                tex,
                alpha: 1.0,
                turn,
            });
        }

        // Cancel under the open cart's left edge, Swap centred on the panel, Choose under its
        // right edge, each placed by what shows of it — the key caps and the word — and not by
        // the transparent strip every hint face carries after its label.
        if let [cancel, swap, choose] = self.core_legend_faces.as_slice() {
            let right = BOARD_X + BOARD_W as f32;
            let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
            for (tex, w, x) in [
                (cancel.0, cancel.1, BOARD_X),
                (swap.0, swap.1, (OUT_W as f32 - seen(swap.1)) / 2.0),
                (choose.0, choose.1, right - seen(choose.1)),
            ] {
                out.push(Draw::Tex {
                    x: x.round(),
                    y: CORE_LEGEND_Y,
                    w: w as f32,
                    h: HINT_H as f32,
                    tex,
                    alpha: lift,
                });
            }
        }
    }

    /// The power menu's rows, at its pitch, in its materials — the third menu on the device
    /// and the third to be the same object. What differs is where it goes in the frame and
    /// what it is drawn over: this one goes over the paused game, which is still on screen
    /// behind it and is what the player gets back by cancelling.
    fn draw_game_menu(&self, menu: GameMenu, out: &mut Vec<Draw>) {
        // ★ 2026-10-21 六六：「游戏菜单不能做得跟色板浏览器那样吗，就是游戏中按 MENU 出来
        //   一个弹窗」—— 之前这里铺一整屏不透明的 `opening()`，游戏被整个盖死；现在改成
        //   色板浏览器那层 **70% 透明黑纱**（`[0,0,0,0.7]`，同 `palette_browser` 的 `VEIL`），
        //   暂停的那局游戏从纱底下透出来，菜单是浮在上面的弹窗而不是整个画面。
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: [0.0, 0.0, 0.0, 0.7],
        });
        // The two working screens are a sentence rather than a list, so nothing on them is
        // in hand and nothing is marked.
        let (faces, index) = match menu {
            GameMenu::Rows(index) => (self.game_row_faces(), Some(index)),
            // ★ 2026-10-20：两张槽位表。标题是那一行（"即时存档"/"即时读档"）自己，下面是
            //   槽位那一行字，再下面是快照 —— **画面按 2 倍画**（GBA 原生 240×160 →
            //   480×320），因为一张跟卡片一样大的图才看得出是哪一局，而原生尺寸放在 720×480
            //   的屏上只有一个巴掌大。
            GameMenu::Save(_) | GameMenu::Load(_) => {
                let row = if matches!(menu, GameMenu::Save(_)) { 0 } else { 1 };
                self.draw_slot_sheet(row, out);
                return;
            }
            // ★ 2026-10-21：**操作键映射** —— `shortcuts.rs` 的同一张 ROWS 表，整卡滚动。
            //   这一级自己开画，不走下面的行列表。
            GameMenu::Shortcuts(scroll, _) => {
                self.draw_game_shortcuts(scroll, out);
                return;
            }
            // ★ 2026-10-21：**保存设置弹窗** —— 一块小板上四个键位一行，标题一行。
            GameMenu::SaveSettings => {
                self.draw_save_settings(out);
                return;
            }
            GameMenu::Link(index) => (self.link_menu_faces.clone(), Some(index)),
            GameMenu::Working(step) => (one(&self.link_step_faces, Some(step.index())), None),
            GameMenu::Failed(fail) => (one(&self.link_fail_faces, fail.shown()), None),
        };
        // ★ 2026-10-21：**规格的"最顶上显示游戏名"。** 标题跟着板走，不再"板外一行" ——
        //   单列时板只有九行的窄条，标题挂在外面；两列之后板是一整块，标题就是它的抬头。
        let title = self.game_title_face();
        // ── 两列网格 ────────────────────────────────────────────────────────────────
        // 九行两列 = 五行。行面各自是 `menu_face_at` 量出来的宽度，这里只决定每格在哪，
        // 行在格里居中 —— 和色板浏览器的格子是同一套（格子定位置，内容自己居中）。
        let cols = GAME_MENU_COLS.min(faces.len().max(1));
        let grid_rows = faces.len().div_ceil(cols);
        let title_h = title.map_or(0.0, |(_, _, h)| h as f32 + GAME_MENU_TITLE_GAP);
        let grid_h = GAME_MENU_PITCH * grid_rows as f32;
        let content_h = title_h + grid_h;
        let panel_w = OUT_W as f32 - 2.0 * GAME_MENU_PAD_X;
        let panel_h = content_h + 2.0 * GAME_MENU_PAD_Y;
        let px = GAME_MENU_PAD_X;
        let py = ((OUT_H as f32 - panel_h) / 2.0).max(6.0);
        out.push(Draw::Rect {
            x: px,
            y: py,
            w: panel_w,
            h: panel_h,
            colour: slot_ui::palette::panel(),
        });
        slot_ui::light::outline(out, px, py, panel_w, panel_h, slot_ui::edge());
        if let Some((tex, _, th)) = self.light_faces.power {
            out.push(Draw::Tex {
                x: px,
                y: py,
                w: panel_w,
                h: th as f32,
                tex,
                alpha: slot_ui::light_gain(),
            });
        }
        // 标题：板内居中一行。
        let mut grid_top = py + GAME_MENU_PAD_Y;
        if let Some((tex, w, h)) = title {
            out.push(Draw::Tex {
                x: (OUT_W as f32 - w as f32) / 2.0,
                y: grid_top,
                tex,
                w: w as f32,
                h: h as f32,
                alpha: 1.0,
            });
            grid_top += h as f32 + GAME_MENU_TITLE_GAP;
        }
        // 格子：两列。★★ 2026-10-21 傍晚第二版：**一格就是一枚按钮**（面宽＝格宽），
        //   手里那一格换 `hot` 那张面 —— 亮底深字，和电源菜单选中行同一套。原先那条
        //   hairline 圈框撤掉了：按钮自己已经有底有边，再套一圈框就是两层框。
        let cell_w = game_menu_cell_w();
        // ★ 2026-10-21：**最后一行只剩一格时，那一格居中。** 删掉「退出到货架」之后这一屏
        //   是七项（八项只有 gpSP 带联机那一行），两列排下来最后一行只填左边，右边空着 ——
        //   一个孤零零靠在左边的格子看着像漏画了一格。居中之后它自成一行，读作"最后这一下"。
        let lonely = faces.len() % cols == 1;
        for (i, face) in faces.iter().copied().enumerate() {
            let col = i % cols;
            let row = i / cols;
            let cell_x = if lonely && row == grid_rows - 1 {
                px + (panel_w - cell_w) / 2.0
            } else {
                px + GAME_MENU_PAD_X * 0.6 + col as f32 * (cell_w + GAME_MENU_GAP_X)
            };
            // 竖直方向在行距里居中：行距比按钮高，多出来的那一截就是格与格之间的空气。
            let (tex, w, h) = if index == Some(i) {
                self.game_menu_hot.get(i).copied().unwrap_or(face)
            } else {
                face
            };
            out.push(Draw::Tex {
                x: cell_x + (cell_w - w as f32) / 2.0,
                y: grid_top + row as f32 * GAME_MENU_PITCH + (GAME_MENU_PITCH - h as f32) / 2.0,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
    }

    /// ★ 2026-10-21：菜单标题行的 face —— 游戏名（`selected_stem` 的清洗版），前端烘好
    /// 交进来，`None` 在没有卡或者还没烘出来的时候。
    fn game_title_face(&self) -> Option<(TexId, u32, u32)> {
        self.game_title_face
    }

    /// ★ 2026-10-21：菜单里的**操作键映射**页 —— `shortcuts.rs` 的同一张 ROWS 表，
    /// 整卡上下滚。货架那页（`Phase::Shortcuts`）怎么画，这一页就怎么画：行脸是启动时
    /// 预栅化的（`shortcut_rows`），这里零栅化成本，画的是同一张卡 ——
    /// `draw_shortcut_row` / `draw_hint_bar` 两个都是现成的，不另写一套。
    fn draw_game_shortcuts(&self, scroll: f32, out: &mut Vec<Draw>) {
        // 行区从 26 起（货架那页的 `CARD_TOP`），脚底 hint 钉死。
        let mut y = CARD_TOP - scroll;
        for face in self.shortcut_rows.iter().copied() {
            // Off the panel is off the panel, the same cull the shelf's page makes.
            if y < OUT_H as f32 && y + face.2 as f32 > 0.0 {
                draw_shortcut_row(Some(face), y, out);
            }
            y += face.2 as f32;
        }
        // Pinned. It is the line that says the rest of the card moves, so it cannot be
        // the thing that moves.
        if let Some((_, _, h)) = self.shortcut_hint {
            let top = OUT_H as f32 - h as f32;
            draw_hint_bar(top, out);
            draw_shortcut_row(self.shortcut_hint, top, out);
        }
    }

    /// ★ 2026-10-21：**保存设置弹窗** —— 标题一行（"要保存这些设置吗？"），下面四个键位
    /// 一行一行：A 按机种保存 / X 按游戏保存 / Y 恢复默认 / B 取消。一块小板，规格原话
    /// 「按A弹窗询问是否保存」—— 询问就是一块板四个键，不是一句 toast。
    ///
    /// 面是前端**每次打开时**上传的（`sync_save_settings`，和 `sync_switcher` 同一模式：
    /// 开着的那一帧建好，关了就放走）。绘制臂只读 —— 这里栅化会把每帧变成五次光栅。
    fn draw_save_settings(&self, out: &mut Vec<Draw>) {
        let Some((title, caps, lines)) = &self.save_settings_faces else {
            return;
        };
        let title_h = title.2 as f32;
        let widest = lines
            .iter()
            .map(|(_, w, _)| *w as f32)
            .fold(0.0f32, f32::max)
            .max(title.1 as f32);
        let row_h = slot_ui::GAME_ROW_H as f32;
        let content_h = title_h + 8.0 + row_h * lines.len() as f32;
        let top = ((OUT_H as f32 - content_h) / 2.0).max(8.0);
        const PAD_X: f32 = 40.0;
        const PAD_Y: f32 = 26.0;
        let px = ((OUT_W as f32 - widest) / 2.0).round() - PAD_X;
        let pw = widest + PAD_X * 2.0;
        let py = top - PAD_Y;
        let ph = content_h + PAD_Y * 2.0;
        out.push(Draw::Rect {
            x: px,
            y: py,
            w: pw,
            h: ph,
            colour: slot_ui::palette::panel(),
        });
        slot_ui::light::outline(out, px, py, pw, ph, slot_ui::edge());
        if let Some((tex, _, th)) = self.light_faces.power {
            out.push(Draw::Tex {
                x: px,
                y: py,
                w: pw,
                h: th as f32,
                tex,
                alpha: slot_ui::light_gain(),
            });
        }
        out.push(Draw::Tex {
            x: (OUT_W as f32 - title.1 as f32) / 2.0,
            y: top,
            w: title.1 as f32,
            h: title_h,
            tex: title.0,
            alpha: 1.0,
        });
        let mut y = top + title_h + 8.0;
        for (cap, line) in caps.iter().zip(lines.iter()) {
            out.push(Draw::Tex {
                x: px + PAD_X,
                y: y + (row_h - cap.2 as f32) / 2.0,
                w: cap.1 as f32,
                h: cap.2 as f32,
                tex: cap.0,
                alpha: 1.0,
            });
            out.push(Draw::Tex {
                x: px + PAD_X + cap.1 as f32 + 12.0,
                y,
                w: line.1 as f32,
                h: line.2 as f32,
                tex: line.0,
                alpha: 1.0,
            });
            y += row_h;
        }
    }

    /// ★ 2026-10-21：**操作键映射弹窗**。和色板浏览器同一个成法：一层 70% 黑纱，一块
    /// `palette::panel()` 的板，顶边一条通电渐隐线，然后才是 `slot_ui::draw_keymap` 画的
    /// 标题 / 七行 / 脚线。开着目标清单时它自己把行换掉；保存确认时再叠一块小板。
    ///
    /// 面全由前端烘（`sync_keymap`），这里只读 —— 每帧栅化七行会把按键变成掉帧。
    fn draw_keymap(&self, out: &mut Vec<Draw>) {
        let Some(faces) = self.keymap_faces.as_ref() else {
            return;
        };
        let Some(edit) = self.keymap_edit.as_ref() else {
            return;
        };
        // 纱。整个面板，和色板浏览器同一条 0.7。
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: [0.0, 0.0, 0.0, 0.7],
        });
        // 板：固定尺寸，和游戏菜单那块一样（640 宽，40 边距）居中一屏 —— 不再跟着脚线伸缩，
        // 因为行距已经按模式摊开了，板始终是"一整块浮在游戏上的弹窗"。
        let x = slot_ui::KEYMAP_PANEL_X;
        let y = slot_ui::KEYMAP_PANEL_Y;
        let w = slot_ui::KEYMAP_PANEL_W;
        let hgt = slot_ui::KEYMAP_PANEL_H;
        out.push(Draw::Rect {
            x,
            y,
            w,
            h: hgt,
            colour: slot_ui::palette::panel(),
        });
        slot_ui::light::outline(out, x, y, w, hgt, slot_ui::edge());
        if let Some((tex, _, th)) = self.light_faces.power {
            out.push(Draw::Tex {
                x,
                y,
                w,
                h: th as f32,
                tex,
                alpha: slot_ui::light_gain(),
            });
        }
        // 标题 / 七行 / 脚线。给 `draw_keymap` 的是全高的一张板，行按 `KEYMAP_TOP` 摆。
        slot_ui::draw_keymap(
            Some(faces.title),
            &faces.rows,
            Some(faces.highlight),
            Some(faces.foot),
            Some(edit.row),
            edit.picking,
            &faces.targets,
            out,
        );
        // 保存确认：一块小板压在行区中央。
        if edit.confirm {
            self.draw_keymap_confirm(faces, out);
        }
    }

    /// 键位弹窗的「要保存吗」—— 标题一行，A 保存 / B 取消 两行，一块居中带描边的小板。
    fn draw_keymap_confirm(&self, faces: &KeymapFaces, out: &mut Vec<Draw>) {
        let row_h = slot_ui::GAME_ROW_H as f32;
        let title_h = faces.save_title.2 as f32;
        let widest = faces
            .save_lines
            .iter()
            .map(|(_, w, _)| *w as f32)
            .fold(0.0f32, f32::max)
            .max(faces.save_title.1 as f32);
        const PAD_X: f32 = 44.0;
        const PAD_Y: f32 = 24.0;
        let content_h = title_h + 10.0 + row_h * faces.save_lines.len() as f32;
        let pw = widest + PAD_X * 2.0;
        let ph = content_h + PAD_Y * 2.0;
        let px = ((OUT_W as f32 - pw) / 2.0).round();
        let py = ((OUT_H as f32 - ph) / 2.0).round();
        out.push(Draw::Rect {
            x: px,
            y: py,
            w: pw,
            h: ph,
            colour: slot_ui::palette::panel(),
        });
        slot_ui::light::outline(out, px, py, pw, ph, slot_ui::edge());
        out.push(Draw::Tex {
            x: (OUT_W as f32 - faces.save_title.1 as f32) / 2.0,
            y: py + PAD_Y,
            w: faces.save_title.1 as f32,
            h: title_h,
            tex: faces.save_title.0,
            alpha: 1.0,
        });
        let mut y = py + PAD_Y + title_h + 10.0;
        for (tex, lw, lh) in &faces.save_lines {
            // ★ 2026-10-21：**两行居中**。板宽是被标题撑出来的（"要保存这次键位改动吗？"比
            //   "A 保存"宽得多），靠左摆会让两行缩在左边一大片空白里。
            out.push(Draw::Tex {
                x: (OUT_W as f32 - *lw as f32) / 2.0,
                y: y + (row_h - *lh as f32) / 2.0,
                w: *lw as f32,
                h: *lh as f32,
                tex: *tex,
                alpha: 1.0,
            });
            y += row_h;
        }
    }

    /// ★ 2026-10-21：菜单的板 —— 一块比行区四周各大 `PAD` 的 `palette::panel()`，
    /// 四边一圈 `edge()` 的 1px，顶边铺一条通电渐隐线。
    ///
    /// 尺寸全部从行区反推：**板是行的后缀，不是行是板的后缀**。写死一块 420×260 的板，
    /// 换一种语言（英文行更宽）或者少一行（gpSP 独占的"联机"）就会露馅。
    ///
    /// 余量 `PAD_X = 40` / `PAD_Y = 26`：左右比上下大，因为一行字长起来是往两边走的，
    /// 而竖排的呼吸本来就短。这一档是 10-21 傍晚按"小气"的反馈调上去的。
    #[allow(dead_code)] // 游戏菜单 WIP（2026-10-21）：板已写好，等菜单那一版接线。
    fn draw_menu_panel(&self, widest: f32, top: f32, rows: usize, out: &mut Vec<Draw>) {
        /// 左右各留的边。28 比行的间距大得多，是"板包着内容"而不是"内容撑满板"。
        const PAD_X: f32 = 40.0;
        /// 上下各留的边。比左右小，因为竖排的呼吸本来就更短。
        const PAD_Y: f32 = 26.0;
        if rows == 0 || widest <= 0.0 {
            return;
        }
        let x = ((OUT_W as f32 - widest) / 2.0).round() - PAD_X;
        let w = widest + PAD_X * 2.0;
        let y = top - PAD_Y;
        let h = GAME_MENU_PITCH * rows as f32 + PAD_Y * 2.0;
        out.push(Draw::Rect {
            x,
            y,
            w,
            h,
            colour: slot_ui::palette::panel(),
        });
        slot_ui::light::outline(out, x, y, w, h, slot_ui::edge());
        // 顶边那条光：压在描边上，从左亮到右灭 —— 它说"这块板通着电"，
        // 也是整屏唯一一处**有方向**的光。
        if let Some((tex, _, th)) = self.light_faces.power {
            out.push(Draw::Tex {
                x,
                y,
                w,
                h: th as f32,
                tex,
                alpha: slot_ui::light_gain(),
            });
        }
    }

    /// One slot sheet: the row's own name up top, the picture under it at twice its size, and
    /// the slot's line beneath that.
    ///
    /// ★ **2026-10-21 傍晚：缩略图从 480×320 放大到 560×373**（GBA 原生 240×160 的
    ///   2.33 倍）。六六的原话是「缩略图你打算扔窗外嘛」—— 一张"看得出是哪一局"的图
    ///   是这一屏的全部意义，把它做小的代价是这张屏白存在。
    ///   720×480 上 560 宽留 80 给两边，373 高 + 标题 46 + caption 一行 ≈ 460，
    ///   正好一屏放得下、不再需要滚动。
    fn draw_slot_sheet(&self, row: usize, out: &mut Vec<Draw>) {
        const THUMB_W: f32 = 560.0;
        const THUMB_H: f32 = 373.0;
        const GAP: f32 = 12.0;
        // 标题那一行占的高度。跟着 `GAME_ROW_H` 走而不是写死：这一屏的标题就是
        // 菜单那一行字，行高变了它还按 44 算，板就会和字错开。
        const TOP: f32 = slot_ui::GAME_ROW_H as f32;
        // Where the picture sits: centred, and above the line that names it, because a
        // caption under its own picture is read as a caption while one above it is read as a
        // heading for whatever comes next.
        let x = (OUT_W as f32 - THUMB_W) / 2.0;
        let caption_h = self
            .slot_caption_face
            .map_or(0.0, |(_, _, h)| h as f32);
        let bottom = TOP + THUMB_H + 2.0 * GAP + caption_h;
        let top = ((OUT_H as f32 - bottom) / 2.0).max(8.0);

        // ★ 2026-10-21：和上面那张菜单同一块板 —— 两屏是同一件器物的一页，
        //   一块有边有光的板，一张图，一行字。板宽跟着缩略图走（720 屏上只有
        //   560 宽的图是这一屏最宽的东西，标题和字都比它窄）。
        //   余量和菜单那张板取同一档（40/26）：两屏挨着出现，余量不一致会读成两套界面。
        const SHEET_PAD: f32 = 40.0;
        const SHEET_VPAD: f32 = 26.0;
        let px = (OUT_W as f32 - THUMB_W) / 2.0 - SHEET_PAD;
        let pw = THUMB_W + SHEET_PAD * 2.0;
        let py = top - SHEET_VPAD;
        // 板高 = 内容高 + 上下各 26。写成 `bottom + 2*SHEET_VPAD` 而不是用 `py` 反推：
        // `py` 已经带了那个 -26，再减一次就是把板砍掉一截（第一版就是这么错的）。
        let ph = bottom + 2.0 * SHEET_VPAD;
        out.push(Draw::Rect {
            x: px,
            y: py,
            w: pw,
            h: ph,
            colour: slot_ui::palette::panel(),
        });
        slot_ui::light::outline(out, px, py, pw, ph, slot_ui::edge());
        if let Some((tex, _, th)) = self.light_faces.power {
            out.push(Draw::Tex {
                x: px,
                y: py,
                w: pw,
                h: th as f32,
                tex,
                alpha: slot_ui::light_gain(),
            });
        }

        let title = one(&self.game_menu_faces, Some(row));
        draw_menu_rows(&title, None, top, POWER_MENU_PITCH, MenuBar::Fill, out);
        let y = top + title.first().map_or(0.0, |(_, _, h)| *h as f32) + GAP;
        if let Some((id, _, _)) = self.slot_thumb_face {
            out.push(Draw::Tex {
                x,
                y,
                w: THUMB_W,
                h: THUMB_H,
                tex: id,
                alpha: 1.0,
            });
        }
        if let Some((id, w, h)) = self.slot_caption_face {
            out.push(Draw::Tex {
                x: (OUT_W as f32 - w as f32) / 2.0,
                y: y + THUMB_H + GAP,
                w: w as f32,
                h: h as f32,
                tex: id,
                alpha: 1.0,
            });
        }
    }

    /// A face per row this cart actually shows, in the order they are drawn. The uploads are
    /// one per `GameRow::ALL`, so a row this cart hides is skipped here rather than missing
    /// from the upload.
    fn game_row_faces(&self) -> Vec<(TexId, u32, u32)> {
        self.game_rows()
            .iter()
            .filter_map(|row| self.game_menu_faces.get(row.index()).copied())
            .collect()
    }

    fn on_shelf(&self) -> bool {
        matches!(self.phase, Phase::Shelf)
    }

    fn insert(&mut self, clean: bool) {
        if !self.on_shelf() {
            return;
        }
        // **A disc is not a cart.** On the music row the shelf's confirm opens the deck rather
        // than dropping a core into the slot: a disc has no core, and its own first track *is*
        // the thing to play. Put here rather than in the keyboard arm so both halves of A — the
        // tap (`GbaUp`) and `Action::Insert` — reach it, and so nothing else on this screen has
        // to know which row is up.
        match self.row {
            ShelfRow::Discs => return self.open_disc_deck(),
            ShelfRow::Games => {}
        }
        let Some((cart, rom)) = self
            .row()
            .current_cart()
            .map(|c| (c.stem.clone(), c.rom.clone()))
        else {
            return;
        };
        // ★ 2026-11-09：**同一机种里重号仍会串**（两个子文件夹各放一份同名 rom）。这一版只修
        //   跨机种，同机种那种取的是排在前面的那条 —— 选了 B 进去是 A，而且完全无声。
        //   在这里说一句，别让它再静默：有这行日志就说明卡上确实重号了。
        if let Some((found, _)) = self.shelf.find(&cart) {
            if found.rom != rom {
                eprintln!(
                    "slot: 同名卡 {cart}：选中的是 {}，反查到的是 {}",
                    rom.display(),
                    found.rom.display()
                );
            }
        }
        self.play_held = None;
        self.refusal = None;
        self.refused_from = None;

        // PS1 使用光盘插入动画：检测系统类型
        //
        // ★ 2026-10-27（六六）：**ARCADE / NEOGEO 走投币**（「等同于插卡/插光盘」）。
        //   前半段和光盘完全一样（从货架位掉进洞），分叉在 `DiscState::coin`：
        //   盘停在洞里转，代币整枚出屏、然后街机箱体从插卡口升起来。
        let system = self.shelf.find(&cart).map(|(c, _)| c.system());
        let disc = system
            .filter(|s| *s == slot_store::System::Ps1 || s.is_coin())
            .map(|s| slot_ui::DiscState {
                spin_start: DISC_DROP_S,
                core_at: None,
                face: None,
                coin: s.is_coin(),
            });

        self.phase = Phase::Inserting {
            cart,
            t: 0.0,
            core_ready: false,
            resumed: false,
            clean,
            disc,
        };
        // ★ 2026-10-26：货架区那行"GAME START LOADING…"的呼吸从这里起算（`draw_loading`）。
        //   用时钟而不是 `Phase::Inserting.t`：字要跨过 `Inserting → Playing` 那条相位边界
        //   继续呼吸到画面长满，而 `Playing` 没有 `t`。
        self.loading_at = self.clock;
    }

    /// Open the deck on the disc the music row's caret is on. What A means on the disc row.
    ///
    /// The row is the shelf of albums and the deck is one of them playing, so this is the row's
    /// confirm rather than a separate screen's. The disc is found **by stem** rather than by
    /// taking the caret's index for a disc index: the caret is a view index and today's view is
    /// the whole list so the two coincide, but a view that ever becomes a subset would make them
    /// disagree, and the stem is the one key both lists already agree on.
    ///
    /// **The disc goes in stopped, at 0:00, and waits.** The first version started the first
    /// track by itself, on the argument that a deck on screen and silent has to be told to play;
    /// 2026-10-07 六六 说「进去就直接播放」是不对的, and it is: a deck that starts itself takes the
    /// one decision the screen exists to offer, and the user has to find the key that undoes it
    /// before they can hear anything. So `play_track` loads the track (it is the one way a track
    /// is ever put on, and it opens the file), the pause below makes that a loaded-but-stopped
    /// deck, and A starts it as it always did. A first track that will not decode still leaves
    /// the deck silent and honest rather than refusing the whole move — the disc is physically
    /// in the deck either way.
    fn open_disc_deck(&mut self) {
        let Some(stem) = self.row().current_cart().map(|c| c.stem.clone()) else {
            return;
        };
        let Some(disc) = self.discs.iter().position(|d| d.stem == stem) else {
            return;
        };
        self.play_held = None;
        self.refusal = None;
        self.refused_from = None;
        self.disc = disc;
        self.track = 0;
        self.play_track(0);
        // Loaded, and stopped. `play_track` presses play as part of putting a track on, so the
        // pause here is what leaves the deck armed rather than running; the transport follows,
        // because a deck showing PLAY while it is stopped is the same lie in the other direction.
        self.player.pause();
        self.transport = slot_ui::lcd::Transport::Stopped;
        self.phase = Phase::Listening {
            key: 0,
            playing: false,
            disc,
        };
        // ★ 2026-10-27：磁带进仓那一声（盒盖 + 锁扣 + 压带轮 + 电机）。
        //   画面上"碟片放进机器"的那一帧和这一声是同一次事件，所以在这里发，不在走带键里。
        self.sfx = Some(Sfx::TapeIn);
    }

    /// Whether the cart going in is starting from the beginning. Read by whoever spawns the
    /// core, which is the one thing that has to know.
    pub fn starting_clean(&self) -> bool {
        matches!(self.phase, Phase::Inserting { clean: true, .. })
    }

    /// The hold fires under the finger rather than on the release, so it has an end the
    /// player can feel. A shelf that left the screen with A still down takes the arming with
    /// it: the press belonged to that screen.
    fn play_hold(&mut self) {
        let Some(at) = self.play_held else {
            return;
        };
        if !self.on_shelf() {
            self.play_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.insert(true);
        }
    }

    /// The other hold on this screen. Spent rather than left for the release, because the
    /// shelf stays up afterwards and a hold that kept firing every timer tick would swap the
    /// view over and over for as long as the button was down.
    fn fav_hold(&mut self) {
        let Some(at) = self.fav_held else {
            return;
        };
        if !self.on_shelf() {
            self.fav_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.fav_held = None;
            self.toggle_fav_view();
        }
    }

    /// A hold of Y: 帘子落下，满闸后帘子上浮出十格机种等用户指。
    /// Spent like `fav_hold`, so it fires once rather than on every timer tick.
    ///
    /// 从哪儿按都行 —— 机种自己的文件夹、还是它的子文件夹里，都一样。子文件夹曾经按不了
    /// （2026-10-10 定：让 Y 的短按只管走子文件夹），2026-10-23 放开：那等于把子文件夹变成
    /// 死胡同，要走回机种根目录才换得动，而卡上有七台、现在有十台。
    fn sys_hold(&mut self) {
        let Some(at) = self.sys_held else {
            return;
        };
        if !self.on_shelf() {
            self.sys_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.sys_held = None;
            self.open_sys_picker();
        }
    }

    /// ★★ 2026-10-26（六六）：**长按 Y 起卷闸，满闸后帘子上浮出十格机种。**
    ///
    /// 光标停在**现在这一台**上 —— 打开时不用先找「我在哪」，按 B 回去也不用想让它回到
    /// 哪里，它从来没离开过。（`root::MACHINES` 是顺序的唯一来源，光标下标与它一一对应，
    /// 见 `draw_sys_picker`。）
    ///
    /// **帘子从这一帧就开始落**，跟以前一模一样：`ShelfSwitch` 的 `t` 从 0 走，
    /// 到 `SHUTTER_SWAP_AT_S` 冻住等人。也就是说长按 Y 之后眼睛看到的第一件事仍然是
    /// 帘子下来，而不是一块面板忽然出现 —— 那是六六 10-26 明确否掉的做法。
    fn open_sys_picker(&mut self) {
        if self.sys_picker.is_some() || self.shelf_switch.is_some() || !self.on_shelf() {
            return;
        }
        // 面还没烘出来（开机后那一帧之前的极窄窗口）就不开：帘子会落在一个没有内容的
        // 等待上，用户只能干等。
        if self.sys_faces.as_ref().is_none_or(|f| !f.ready()) {
            return self.refuse();
        }
        let order = crate::root::MACHINES;
        let Some(at) = order.iter().position(|&s| s == self.shelf_system) else {
            return;
        };
        self.sys_picker = Some(SysPicker::open(at, self.now()));
        // 货架上按着的方向键属于下面那一行，不属于这块面板；同理那三个长按也一律作废 ——
        // 否则帘子落着的时候 A 的 500 ms 一到，插的会是背后那张卡。
        self.row_mut().release_hold();
        self.letters.release_hold();
        self.play_held = None;
        self.fav_held = None;
        self.mtp_held = None;
        // 帘子起。`to` 先填 `from`（= 不换）：万一这一趟被别的东西打断，卷起来的也是
        // 原来那一台。真换哪一台，是用户在满闸之后按 A 才写进来的。
        self.shelf_switch = Some(ShelfSwitch {
            from: self.shelf_system,
            to: self.shelf_system,
            t: 0.0,
            up: None,
        });
        // ★ 2026-10-27：卷闸落下的那一声。帘子在这一帧就开始动，声音也必须在这一帧起。
        self.sfx = Some(Sfx::ShutterDown);
    }

    /// 用户在帘子上指完了：`Some(to)` 换到那一台，`None` 不换。两种情况都让帘子卷回去。
    ///
    /// **这是整条转场里唯一改货架的地方**，而且只在帘子盖满整屏时执行 —— 用户看不见换的
    /// 那一下。选的就是当前这台（`to == from`）按"不换"处理：没必要为了同一台机器重扫一遍
    /// 货架、重写一遍 `System/shelf.txt`。
    fn settle_system_switch(&mut self, to: Option<System>) {
        // 面板先收掉。帘子正盖着，看不见；而且这一刻起它不再接受任何输入。
        self.sys_picker = None;
        let Some(mut sw) = self.shelf_switch.take() else {
            return;
        };
        let target = to.filter(|t| *t != sw.from);
        if let Some(t) = target {
            sw.to = t;
        }
        sw.up = Some(0.0);
        self.shelf_switch = Some(sw);
        // ★ 2026-10-27：卷起的那一声（**不是落下的倒放** —— 起手是链条把整排帘片拉起来）。
        self.sfx = Some(Sfx::ShutterUp);
        if let Some(t) = target {
            // 满闸，整屏被帘子盖着 —— 换装就在这一帧完成。
            self.show_system(t);
        }
    }

    /// 帘子上的十格吃掉整个屏幕的按键。
    ///
    /// 只有方向键与 A/B 有去处，其余一律吞掉（`_ =>` 什么都不做）：一个没被它接住的方向键
    /// 会滑到背后那一行上去，而用户看不见那一行在动。
    fn sys_picker_input(&mut self, action: Action) {
        let now = self.now();
        let Some(cursor) = self.sys_picker.as_ref().map(|p| p.cursor()) else {
            return;
        };
        let order = crate::root::MACHINES;
        let target = order[cursor.min(order.len() - 1)];
        // 这一台在卡上有没有游戏 —— 借 `self.shelf` 得赶在借 `self.sys_picker` 之前问。
        let has_carts = self.shelf.carts.iter().any(|c| c.system() == target);
        let Some(picker) = self.sys_picker.as_mut() else {
            return;
        };
        let (outcome, stepped) = match action {
            Action::ShelfLeft | Action::GbaDown(Btn::Left) => {
                (SysOutcome::Nothing, picker.move_by(-1, 0, now))
            }
            Action::ShelfRight | Action::GbaDown(Btn::Right) => {
                (SysOutcome::Nothing, picker.move_by(1, 0, now))
            }
            Action::GbaDown(Btn::Up) => (SysOutcome::Nothing, picker.move_by(0, -1, now)),
            Action::GbaDown(Btn::Down) => (SysOutcome::Nothing, picker.move_by(0, 1, now)),
            Action::GbaDown(Btn::A) => (picker.choose(target, has_carts, now), false),
            // B 与 MENU 都是"不换了"：B 是这一块屏自己写在脚下的那个出口，MENU 是"打开它
            // 的那颗键也能关上它"，与电源菜单同一条规矩。
            Action::GbaDown(Btn::B) | Action::OpenMenu => (picker.cancel(), false),
            _ => (SysOutcome::Nothing, false),
        };
        if stepped {
            // 走格子的声音与会话里那一声是同一个，不另配：手感的连续性比"这屏专属"重要。
            self.ui_sfx(Sfx::Blip);
        }
        match outcome {
            SysOutcome::Go(to) => {
                self.ui_sfx(Sfx::Click);
                self.settle_system_switch(Some(to));
            }
            // 取消：帘子照旧卷回去，只是卷起来露出的还是原来那一台。
            SysOutcome::Cancel => self.settle_system_switch(None),
            SysOutcome::Nothing => {
                // 选了台没卡的机种：`SysPicker::choose` 已经在抖那一格了，这里补一声。
                // **不调 `refuse()`** —— 那是整排货架的抖，而货架正被帘子盖着。
                if action == Action::GbaDown(Btn::A) && !has_carts {
                    // ★ 2026-10-26：这不是"走了一格"而是"这台没卡"，所以用拒绝声。
                    // `SysPicker::choose` 已经在抖那一格，声音跟着抖的含义走。
                    self.ui_sfx(Sfx::Refuse);
                }
            }
        }
    }

    /// A hold of B: open the direct-connect dialog. Spent like `fav_hold`.
    fn mtp_hold(&mut self) {
        let Some(at) = self.mtp_held else {
            return;
        };
        if !self.on_shelf() {
            self.mtp_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.mtp_held = None;
            self.open_direct_link();
        }
    }

    /// A short X on the shelf: star the cart under the caret, or take the star back.
    /// The star is the whole of the feedback — it appears on the cart, which is where the
    /// user is already looking.
    fn toggle_favorite(&mut self) {
        let Some(stem) = self.row().current_cart().map(|c| c.stem.clone()) else {
            return;
        };
        if !self.state.favorites.remove(&stem) {
            self.state.favorites.insert(stem);
        }
        self.persist();
        // The favourites shelf shows the starred carts and nothing else, so a cart that just
        // lost its star has to leave the view it is standing in.
        if self.fav_view {
            self.show_favorites();
        }
    }

    /// A hold of X: swap between the whole of the current machine's shelf and its starred
    /// carts. The library is three machines and only one shelf is up, so "the whole shelf" is
    /// this machine's carts, not all of them.
    fn toggle_fav_view(&mut self) {
        // **The favourites sub-view is a view over the cart row**, built from
        // `system_view(shelf_system)` — there is no such thing as a machine's favourites on the
        // disc row, because a disc has no machine. Refusing is right here (unlike the row toggle,
        // which has somewhere to go): the key means "favourites" and on this row there is no
        // answer to give.
        if self.row != ShelfRow::Games {
            return self.refuse();
        }
        if self.fav_view {
            self.show_all();
        } else {
            self.show_favorites();
        }
    }

    /// The whole of the machine whose shelf is up — what leaving the favourites sub-view
    /// returns to.
    fn show_all(&mut self) {
        let keep = self.row().current_cart().map(|c| c.stem.clone());
        let view = self.system_view(self.shelf_system);
        self.fav_view = false;
        self.shelf.set_view(view, keep.as_deref());
        self.retally_letters();
    }

    /// A tap of Y on the shelf: **swap between the game row and the disc row.** Long-press Y
    /// already swaps machines, and a disc is not a machine — it has no core and no save folder —
    /// so putting it on the long press would mean making `cycle_system` carry something it was
    /// never written for. The two rows are peers, and a peer is a tap.
    fn cycle_folder(&mut self) {
        // On the disc row there are no subfolders to step, so Y simply returns to the games —
        // the two rows are still one toggle from that side.
        if self.row != ShelfRow::Games {
            // Y on the disc row returns to the games shelf: the two are peers and the disc row
            // has no subfolders of its own (2026-10-10, 六六).
            self.row = ShelfRow::Games;
            return;
        }
        // On the games shelf Y walks this machine's subfolders: the machine's own folder
        // (top-level games only, subfolders hidden) -> subfolder A -> ... -> last subfolder ->
        // back to the machine folder. Long-press Y swaps the machine and works from any of
        // those, subfolder included — see `sys_hold`. The music / disc row is reached from
        // MENU -> Discs, not from Y.
        if !self.shelf.cycle_folder(self.shelf_system) {
            return self.refuse();
        }
        let keep = self.row().current_cart().map(|c| c.stem.clone());
        let view = self.system_view(self.shelf_system);
        self.shelf.set_view(view, keep.as_deref());
        self.retally_letters();
        // The view is a different set of carts, so the resident window has to be rebuilt the way
        // a machine switch rebuilds it: the screen would otherwise fill with stand-ins.
        self.shelf_dirty = true;
    }

    /// The starred carts of the machine whose shelf is up, in library order so a cart sits on
    /// the same shelf, at the same place among its neighbours, as the one it came from.
    /// Refused rather than shown empty when nothing is starred: an empty shelf is a screen with
    /// nothing on it and no way to tell why.
    fn show_favorites(&mut self) {
        let keep = self.row().current_cart().map(|c| c.stem.clone());
        let view = self.favorites_view();
        if view.is_empty() {
            return self.refuse();
        }
        self.fav_view = true;
        self.shelf.set_view(view, keep.as_deref());
        self.retally_letters();
    }

    /// 该机种货架的收藏夹视图，按库序 —— `show_favorites` 与刷新后的重建共用这一张清单，
    /// 免得两处对"什么算收藏"各答一次。
    fn favorites_view(&self) -> Vec<usize> {
        self.shelf
            .carts
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.system() == self.shelf_system && self.state.favorites.contains(&c.stem)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// **刷新之后这一行该显示什么**（2026-11-07）。见 `refresh_library`：`replace_library`
    /// 把 `view` 留在整个库上，而货架这一行永远只放一个机种，所以那张视图必须重建。
    ///
    /// 先看收藏夹：它是**子视图而不是机种**，刷新前它在就还原它；被这次刷新刷空了（刷掉的
    /// 正好是那几张星标）就退回该机种整排并把 `fav_view` 清掉 —— 否则这一行会是一张空视图，
    /// 而 `filtered()` 还以为收藏夹在，`X` 长按也换不回整排。
    fn row_view_after_reload(&mut self) -> Vec<usize> {
        if self.fav_view {
            let view = self.favorites_view();
            if !view.is_empty() {
                return view;
            }
            self.fav_view = false;
        }
        self.system_view(self.shelf_system)
    }

    /// The letter dial counts carts, so a view that is not the whole library has its own
    /// tally. Rebuilt on the edge rather than per frame: the card cannot change while slot is
    /// running, so the only thing that can move it is a view swap.
    fn retally_letters(&mut self) {
        let visible: Vec<Cart> = self.row().visible().into_iter().cloned().collect();
        self.letter_counts = tally_letters(&visible);
    }

    /// The star over every starred cart in the row.
    ///
    /// **Inside the cart**, in the moulded band the shell leaves between its curved top edge and
    /// the top of the label — `cart::label_panel` puts the label at 22.8% of the face, so that
    /// band is the top fifth of the shell. Directly above the sticker and still on the cartridge,
    /// under the arc, rather than floating over the row: the star is printed on the plastic, not
    /// pinned above the object. Scaled with the cart, so the enlarged selection wears a larger
    /// star and the row still reads as one row.
    fn draw_fav_stars(&self, out: &mut Vec<Draw>) {
        /// How tall the mark is on a cart, in screen pixels. Absolute rather than a share of the
        /// cart, because it is a reading and not part of the artwork: a star that grew with the
        /// selected cart would say the cart was more of a favourite than its neighbours.
        const STAR_PX: f32 = 20.0;
        let Some((tex, w, h)) = self.fav_star else {
            return;
        };
        if self.state.favorites.is_empty() {
            return;
        }
        let (w, h) = (w as f32, h as f32);
        for r in self.row().slot_rects(None, self.shelf_shake(), 0.0) {
            if !self
                .state
                .favorites
                .contains(&self.row().carts[r.cart].stem)
            {
                continue;
            }
            let (sh, sw) = (STAR_PX, w * (STAR_PX / h));
            // ★★ 2026-10-27（六六）：「GB/GBC 按现在的位置，其它的按你建议的」。
            //
            // 以前这个位置是「洋红框右上角」推出来的，可贴图换成整张之后洋红框 ≈ 整张脸，
            // 那个角就是卡带自己的角 —— 星标中心压在角上，MD/NES/SNES 有一半挂到卡带外面。
            // 现在:
            //   · 卡带行：`slot_ui::star_at(机种)` 是一张**写死的表**（GB/GBC 保留原值，
            //     圆形的 PS1/街机/代币按 45° 内缩，其余横向机种统一 0.90/0.12）；
            //   · 碟片行：那一行画的是**封套**（`cd.png`），星标落在封面右上角是对的，
            //     仍然跟着封面的洋红框走 —— 没有封套时才退回那块内置面板。
            let at = match self.row {
                ShelfRow::Discs => {
                    slot_ui::cd_cover_share()
                        .map(|(_, y0, x1, _)| (x1, y0))
                        .unwrap_or_else(|| {
                            let (_, y0, x1, _) =
                                self.row().size().label_panel_at(r.w as u32, r.h as u32);
                            (x1 as f32 / r.w, y0 as f32 / r.h)
                        })
                }
                ShelfRow::Games => slot_ui::star_at(self.shelf.system()),
            };
            let x = r.x + at.0 * r.w - sw / 2.0;
            let y = r.y + at.1 * r.h - sh / 2.0;
            // A bloom under it: the same star, larger and faint, so it reads as light coming
            // off the glyph rather than as a second outline. One extra quad, and the linear
            // tap on an upscaled alpha map is the blur — no pass and no blur shader.
            let g = sh * 0.45;
            out.push(Draw::Tex {
                x: x - g / 2.0,
                y: y - g / 2.0,
                w: sw + g,
                h: sh + g,
                tex,
                alpha: r.alpha * 0.30,
            });
            out.push(Draw::Tex {
                x,
                y,
                w: sw,
                h: sh,
                tex,
                alpha: r.alpha,
            });
        }
    }

    /// The card's own marks on a cart: a badge set into the sticker's reserved corner, and a
    /// film over the sticker or along the shell's edge.
    ///
    /// Both are read out of `System/Carts/carts.tsv` — the same table the shell colour comes
    /// from — and both are drawn **as quads over the cart's face** rather than baked into it:
    /// a face is built once and dropped when the cart leaves the window, and one baked with a
    /// badge in it would have to be rebuilt every time the table was edited. A cart the table
    /// says nothing about is left exactly as the build draws it.
    ///
    /// The geometry lives in `slot_ui::cart_marks`, where it can be tested without a GL
    /// context; this is only the pass that walks the row and hands each cart its quad.
    fn draw_cart_marks(&self, out: &mut Vec<Draw>) {
        // The table keys on a game code, which is something only a cartridge has.
        if self.row != ShelfRow::Games {
            return;
        }
        // Seconds, off the app's own clock — the sweep is the one thing on the shelf that
        // moves by time rather than by input.
        let t = (self.clock / 1000.0) as f32;
        let sel = self.shelf.current_cart().map(|c| c.stem.as_str());
        for r in self.shelf.slot_rects(None, self.shelf_shake(), 0.0) {
            let Some(cart) = self.shelf.carts.get(r.cart) else {
                continue;
            };
            let Some(row) = slot_ui::cart_rule_for_code(&cart.code) else {
                continue;
            };
            if row.badge.is_none() && row.foil.is_none() {
                continue;
            }
            // Where the sticker sits on this quad, as shares of it: the card's own art says
            // where its label is, and the built-in panel stands in when there is no art —
            // the same choice `draw_fav_stars` makes for the same reason.
            let share = slot_ui::cart_label_share(self.shelf.system()).unwrap_or_else(|| {
                let (x0, y0, x1, y1) = self.shelf.size().label_panel_at(r.w as u32, r.h as u32);
                (
                    x0 as f32 / r.w,
                    y0 as f32 / r.h,
                    x1 as f32 / r.w,
                    y1 as f32 / r.h,
                )
            });
            out.extend(slot_ui::cart_marks(
                row,
                &cart.code,
                (r.x, r.y, r.w, r.h),
                share,
                self.shelf.face_of(r.cart),
                self.badge_atlas,
                t,
                sel == Some(cart.stem.as_str()),
                r.alpha,
            ));
        }
    }

    /// Which shelf is up, in the bottom right corner of the screen — where the original printed
    /// the time: hollow while the ordinary shelf is showing, solid and yellow while the
    /// favourites are. A reading rather than a control — the hold of B is the control — so it
    /// never looks like something to press.
    ///
    /// The mode badge (sun in light, moon in dark) is not here: it is printed beside the clock at
    /// the top of the band, where it reads as part of the same row of type.
    /// ★ 2026-10-24（六六）：收藏符号的新位置——**名称栏左边那 30px**、按 24px 居中。
    /// 名称栏是 360：左 30 收藏 / 中 300 名字 / 右 30 电量，三段不会
    /// 互相压到。内容看起来是何意，就画何意：空心（不是实心）
    /// 是“这一栏不是收藏”，实心是“收藏中”。
    fn draw_fav_plate_mark(&self, out: &mut Vec<Draw>) {
        let lit = self.fav_view;
        let Some((tex, w, h)) = (if lit {
            self.fav_ind.map(|p| p.1)
        } else {
            self.fav_ind.map(|p| p.0)
        }) else {
            return;
        };
        if h == 0 {
            return;
        }
        let k = slot_ui::PLATE_MARK / h as f32;
        out.push(Draw::Tex {
            x: slot_ui::plate_fav_x() + (slot_ui::PLATE_MARK - w as f32 * k) / 2.0,
            y: slot_ui::plate_mark_y(),
            w: w as f32 * k,
            h: slot_ui::PLATE_MARK,
            tex,
            alpha: 1.0,
        });
    }

    fn draw_fav_indicator(&self, out: &mut Vec<Draw>) {
        // In light mode the corner mark reads better as the hollow glyph than the solid one,
        // so the favourites shelf does not fill it in there (2026-10-10, 六六).
        let lit = self.fav_view && slot_ui::palette::mode() != slot_ui::palette::Mode::Light;
        let Some((tex, w, h)) = (if lit {
            self.fav_ind.map(|p| p.1)
        } else {
            self.fav_ind.map(|p| p.0)
        }) else {
            return;
        };
        if h == 0 {
            return;
        }
        let k = FAV_IND_DRAW / h as f32;
        let (sw, sh) = (w as f32 * k, FAV_IND_DRAW);
        // ★ 2026-10-22（六六）：收藏黄星锚定在**底条最右角**，电量读数退到它左侧（见 `status.rs`
        //   的 `STAR_RESERVE`）。右对齐到右角、竖直居中在底条。上机实测再微调。
        let ind_x = OUT_W as f32 - STATUS_READ_MARGIN - sw;
        let y = OUT_H as f32 - MOUTH_H + MOUTH_H / 2.0 - sh / 2.0;
        out.push(Draw::Tex {
            x: ind_x,
            y,
            w: sw,
            h: sh,
            tex,
            alpha: 1.0,
        });
    }

    fn eject(&mut self) {
        // Nowhere to eject to. Refused rather than ignored, so the held MENU says no
        // instead of reading as a device that stopped listening.
        if self.single_cart() {
            return self.refuse();
        }
        // Inserting as well as Playing, so a slot with no core behind it can still be
        // emptied: that is the only way to watch the travel more than once.
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Inserting { cart, .. } => std::mem::take(cart),
            _ => return,
        };
        // A session does not survive its cart. Without this a phantom session outlives the
        // eject: `link_active()` stays true with no core left to carry it, `may_rewind`/
        // `may_load_state` stay wedged closed for whatever cart goes in next, and
        // `doze_expired`'s own guard refuses to let the device sleep again — forever, since
        // nothing left in the app ever flips it back. `Session::act`'s edge bridge (watching
        // `link_active()` fall across every `apply`) is what carries this to
        // `EmuHandle::end_link` on the emulator thread, the same way it does for a doze or a
        // power press.
        self.end_link();
        // The cart the menu was about is on its way out. Not reachable through the overlay
        // itself, which swallows the eject; this is here for whatever route into an eject
        // comes next, the way `begin_power_off` guards its own chokepoint rather than the
        // one caller that happened to need it.
        self.close_game_menu();
        self.close_cheat_menu();
        self.close_palette_browser();
        self.flush_eject(&cart);
        // The offer names a file in this cart's ring and a state only this cart's core can
        // read. Carried across the slot it would delete or load the wrong one.
        self.pending = None;
        // An eject asked for is not an eject refused, whatever was refused a moment ago.
        self.refusal = None;
        self.refused_from = None;

        // PS1 光盘弹出：使用 DISC_EJECT_S 时长。
        // ★ 2026-10-26：起始负值改成 `DISC_EJECT_HOLD_S`（0.10）——旧值 `-DISC_EJECT_S`
        //   会让屏黑了之后再干等 1.23 s 光盘才动（相位只在 `screen <= 0` 之后才推进）。
        let is_disc = self.system_of(&cart) == Some(slot_store::System::Ps1);
        let eject_t = if is_disc { -DISC_EJECT_HOLD_S } else { -EJECT_HOLD_S };
        // 光驱里那颗"还在转"的光盘到此为止：弹出走的是另一套五拍。
        self.disc_live = None;

        self.phase = Phase::Ejecting {
            cart,
            t: eject_t,
        };
    }

    /// Everything durable happens here, before the animation rather than after it: the
    /// card can be pulled while the cart is still sliding out. A write that failed leaves
    /// the cart recorded as seated, so the next boot resumes it and the end of the
    /// animation retries the clear.
    fn flush_eject(&mut self, stem: &str) {
        let (Some(root), Some(snapshot)) = (&self.root, &self.snapshot) else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: eject: the core gave up no state");
            return;
        };
        let Some(system) = self.system_of(stem) else {
            eprintln!("slot: eject: no cart in the shelf for {stem}");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "eject");
        // ★ 2026-10-28：「拔卡续存」关掉时，拔卡不写 resume 快照，但原生 `.sav` 照写（丢档不行）。
        //   lid / 关机 / 自动存档走 `flush_resume` → `persist::flush`，不走这条，不受影响。
        let state = if self.eject_save() { state } else { None };
        match persist::eject(
            root,
            self.core,
            system,
            stem,
            state.as_deref(),
            sav.as_deref(),
        ) {
            Ok(()) => self.state.cart = None,
            Err(e) => eprintln!("slot: eject: {e}"),
        }
    }

    /// Flush, then dark, then idle. The cart stays in the slot and `slot.state` is not
    /// touched: a sleep is not an eject, and the next boot has to resume this session
    /// whether the lid opens again or the battery runs out first.
    ///
    /// The one function every doze actually goes through: both `LidClose` arms (with the
    /// power menu open, and without) and `PowerTap` by way of `power_press` all return
    /// `self.doze()` rather than reimplementing it, so a guard here — and only here — closes
    /// every path in at once. Three copies of the same `if self.link_active()` at each call
    /// site is exactly the kind of duplication that let a mutation slip through unnoticed
    /// last time: `on_doze_timeout` carried a redundant copy of `doze_expired`'s own guard,
    /// and that second copy alone was enough to keep `doze_never_expires_while_a_session_is_live`
    /// passing after the real guard was mutated away.
    ///
    /// A live session ends here rather than surviving the doze — but the doze still happens:
    /// this used to `return` right after `end_link()`, which ended the session and then left
    /// the device sitting in `Phase::Playing`, wide awake, behind a lid the player had just
    /// shut. That is exactly the 400-700 mA outcome the paragraph below argues against,
    /// reached anyway, with the session dead on top of it — proven by `phase` still reading
    /// `Playing` ten seconds after a `LidClose` that hardware delivers exactly once per
    /// physical close, with no second press coming to "retry" into an actual doze.
    ///
    /// `Session::sync_speed` maps `Phase::Doze` to `Speed::Paused`, and pausing is one of the
    /// exact manipulations libretro's netpacket contract names as forbidden while players are
    /// connected — the same desync hazard as dropping the transport outright, not a lesser
    /// one. The alternative — holding the session open through a doze that keeps the core
    /// running *unpaused*, so the panel can go dark for free — is a bigger change than this
    /// fix (`sync_speed` would have to learn about sessions too) and would not even save the
    /// battery it sounds like it would: `doze_expired` already refuses to end a session on
    /// its own idle timer, so a session left open behind a shut lid would sit at 400-700 mA
    /// with the radio up for as long as the lid stayed shut, never once reaching the sub-45
    /// mA a real doze exists to reach. Ending the session costs a trade partner who shut the
    /// lid only to think for a moment — there is no answer here that costs nothing — but it
    /// is the one already chosen for `PowerPress`, and completing the doze underneath it is
    /// the only way to actually reach the low-power state this function exists for.
    // BaseOS owns physical backlight I/O only while this particular doze actually
    // enters its sleep helper. Music playback is the frontend's display-only mode.
    fn frontend_owns_backlight(&self) -> bool {
        !self.native_suspend_active
    }

    fn should_use_native_suspend(baseos_sleep_available: bool, music_playing: bool) -> bool {
        baseos_sleep_available && !music_playing
    }

    fn doze(&mut self) {
        if self.link_active() {
            self.end_link();
        }
        // The overlay is drawn over a game that is about to go dark, and a starter left
        // running behind it would keep a radio up through the doze.
        self.close_game_menu();
        self.close_cheat_menu();
        self.close_palette_browser();
        // A shut lid is walking away, not choosing. Nothing is written and nothing animates:
        // waking comes back to a plain shelf.
        self.core_picker = None;
        if matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.flush_resume();
        // Only a running cart is worth waking back into. A lid closed over an animation
        // wakes to the shelf, one press from where it was, rather than into a core that
        // may not have finished loading.
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(std::mem::take(cart)),
            _ => None,
        };
        // ★ 2026-11-02（六六）：「放音乐时屏蔽超时关机；唤醒回播放器」。
        //   合盖盖住的是屏，不是声音——解码线程是自己的、设备是共享的那台，所以
        //   随手听一起合上的只有屏幕。doze 把随身听的位置（哪张碟 / 哪颗键 / 是否
        //   在放）收进 Doze 相位，唤醒直接回到随身听界面，曲子接着放。
        //   ⚠️ **这里绝不能 `player.pause()`** —— 暂停是"走开"，合盖不是；音乐
        //   要在 lid 下继续。曲终续播见 `deck_advance`。
        // A live music decoder is the one exception to native suspend: a short
        // power press must blank only the LCD and leave the audio device running.
        // Use the decoder's live flag, not the phase snapshot, which may lag at
        // the exact moment a track ends or transport state changes.
        let music_playing = matches!(&self.phase, Phase::Listening { .. }) && self.deck.playing();
        self.native_suspend_active =
            Self::should_use_native_suspend(self.baseos_sleep.is_some(), music_playing);
        let deck = match &self.phase {
            Phase::Listening { key, playing, disc } => Some((*key, *disc, *playing)),
            _ => None,
        };
        self.polaroids = None;
        self.phase = Phase::Doze { cart, deck };
        self.dozed_at = self.now();
        // With native BaseOS suspend, the OS helper owns the physical backlight:
        // it samples the current brightness, blanks the panel and restores that exact
        // value after resume. Calling Power::on_close() here would write brightness=0
        // before the helper samples it, so the helper would faithfully restore 0 and
        // leave the LCD black until the user changes brightness manually.
        if self.frontend_owns_backlight() {
            if let Some(power) = &mut self.power {
                power.on_close();
            }
        }
        // Run BaseOS' sleep helper as a detached child: it stops Wi-Fi, unbinds the SDIO
        // function, selects mem_sleep=deep and blocks in /sys/power/state until the power key
        // wakes the board. The frontend frame loop simply remains suspended with the SoC.
        if self.native_suspend_active {
            if let Some(path) = self.baseos_sleep.clone() {
                if let Err(error) = std::process::Command::new(&path).spawn() {
                    eprintln!("slot: could not start BaseOS deep suspend ({}): {error}; restoring fallback timeout", path.display());
                    self.native_suspend_active = false;
                    self.baseos_sleep = None;
                    // Native suspend failed, so the frontend fallback owns the display again.
                    if let Some(power) = &mut self.power {
                        power.on_close();
                    }
                    let timeout = self.doze_timeout();
                    if let Some(power) = &mut self.power {
                        power.set_timeout(timeout);
                    }
                }
            }
        }
    }

    fn wake(&mut self) {
        let restore_frontend_backlight = self.frontend_owns_backlight();
        let Phase::Doze { cart, deck } = &mut self.phase else {
            return;
        };
        // ★ 2026-11-02（六六）：「回播放器」—— lid 合在随身听上，开盖回到随身听界面。
        //   `playing` 只决定相位显示什么（走带图标），**不发播放命令**：音乐在 lid 下
        //   一直在走，唤醒只需把屏接回去，不需要也不应该再按一次 play。
        self.phase = match (cart.take(), deck.take()) {
            (Some(cart), _) => Phase::Playing { cart },
            (None, Some((key, disc, playing))) => Phase::Listening { key, playing, disc },
            (None, None) => Phase::Shelf,
        };
        // The native helper has restored its captured value only on the suspend path.
        // For music display-off, wake the panel through the frontend's Power state.
        self.native_suspend_active = false;
        if restore_frontend_backlight {
            if let Some(power) = &mut self.power {
                power.on_open();
            }
        }
    }

    /// Fallback for platforms without a native suspend hook: the screen stays dark for
    /// the configured grace period, then the device powers off. RG353PS supplies
    /// `BASEOS_SLEEP`, so its normal path enters deep suspend-to-RAM and does not expire here.
    pub fn on_doze_timeout(&mut self) {
        if !matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.begin_power_off();
    }

    /// The lid's twin, and the only one of the two the device is certain to see. A tap
    /// dozes and a second one wakes.
    fn power_press(&mut self) {
        match self.phase {
            Phase::Doze { .. } => self.wake(),
            _ => self.doze(),
        }
    }

    /// A held button powers off, through the OS rather than the PMIC. The PMIC's own
    /// six-second hold cuts the rails in hardware with no sync, no unmount and no driver
    /// teardown; the software path unloads the GPU module first, which is the difference
    /// between a machine that stops and one that hangs with the rails up draining the
    /// battery. Six seconds remains the emergency underneath, and needs no help from here.
    ///
    /// Not an eject: the cart stays in the slot so the next boot resumes it. The flush is
    /// a no-op after a doze, which has already written the same file.
    /// The hold threshold raises the menu and nothing else. Every outcome from here is one
    /// the user chose rather than one the button committed them to, which is what makes the
    /// hold safe to discover by accident.
    fn open_power_menu(&mut self) {
        if self.power_menu.is_some() {
            return;
        }
        // Same hazard as the switcher: the menu pauses the core too — `Session::sync_speed`
        // maps `held()`, which the menu is one of, to `Speed::Paused` — one of the exact
        // manipulations libretro's netpacket contract forbids while a session is live. Unlike
        // `PowerPress` this button does not end the session for the player; it just declines,
        // the same shake every other "nothing doing" action in this file answers with.
        if self.link_active() {
            return self.refuse();
        }
        // Durable before the menu is even on screen: from here the user may hold on to the
        // PMIC's own six second cutoff, which takes the rails away whatever we wanted.
        self.flush_resume();
        self.power_menu = Some(0);
    }

    /// Up and down move, A commits, B leaves. Nothing times out: a menu that closed itself
    /// would do it exactly when the user looked away to think.
    fn power_menu_input(&mut self, action: Action) {
        let Some(index) = self.power_menu else {
            return;
        };
        let last = PowerChoice::ALL.len() - 1;
        match action {
            Action::GbaDown(Btn::Up) => self.power_menu = Some(index.saturating_sub(1)),
            Action::GbaDown(Btn::Down) => self.power_menu = Some((index + 1).min(last)),
            Action::GbaDown(Btn::B) => self.power_menu = None,
            Action::GbaDown(Btn::A) => {
                self.power_menu = None;
                // Both choices end the game whatever was underneath was drawn over, and only
                // one of them reaches `begin_power_off`: a restart sets its flag here and
                // goes straight to the shutdown screen.
                self.close_game_menu();
                match PowerChoice::ALL[index] {
                    PowerChoice::Restart => {
                        self.restarting = true;
                        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
                        self.set_led(LedState::Off);
                    }
                    PowerChoice::PowerOff => self.begin_power_off(),
                }
            }
            _ => {}
        }
    }

    /// The seated cart's cheat codes with their live on/off, in file order. Read by the
    /// frontend to rasterise the table's row faces and by the draw to colour each chip.
    pub(crate) fn cheats(&self) -> &[CheatItem] {
        &self.cheats
    }

    /// The codes that are **actually live**, as `(enabled, code)`, which is exactly what goes to
    /// the core: a code switched off (or every code, under a master that is off) is not sent.
    ///
    /// ★ 2026-10-31：**过滤在这里而不是在核心里。** mGBA 的 `retro_cheat_set` 把 `enabled`
    /// 参数标成 `UNUSED(enabled)`，收到的每条都加进它的作弊表 —— 所以「发过去一条 `false`」
    /// 和「发过去一条 `true`」对它是同一件事：都生效。旧实现把整张表连 `false` 一起发，
    /// 逐条开关因此在 mGBA 上从来只是界面上的绿点在动。RetroArch 处理同一件事的办法也是
    /// 只把 `enabled` 为真的条目交给核心，前端过滤是这条能力的唯一实现处。
    pub(crate) fn live_cheats(&self) -> Vec<(bool, String)> {
        self.cheats
            .iter()
            .filter(|c| self.cheats_on && c.enabled)
            .map(|c| (true, c.code.clone()))
            .collect()
    }

    /// Replace the cheat list (called when a cart's core spawns) and close any open table, so a
    /// switch to a cart with none does not leave the highlight pointing at the old codes.
    ///
    /// `enabled` comes from the file's own `cheatN_enable`, not from here: a `.cht` says which
    /// codes it wants live, and the card is the player's to edit. With nothing switched on in
    /// the file every row reads off, which is the format's default — so a freshly seated cart
    /// still launches with nothing applied unless the player asked for it on the card.
    pub(crate) fn set_cheats(&mut self, entries: Vec<crate::root::CheatEntry>) {
        self.cheats = entries
            .into_iter()
            .map(|e| CheatItem {
                code: e.code,
                desc: e.desc,
                enabled: e.enabled,
            })
            .collect();
        self.cheat_menu = None;
    }

    /// `Some` while the cheat table is up, `None` otherwise. Read by `Session` to pause the core
    /// and to decide what SELECT+A does: open it, or master-toggle-all while it is open.
    pub fn cheat_menu(&self) -> Option<usize> {
        self.cheat_menu
    }

    /// Whether the table's per-row edit needs pushing to the core. Cleared by `Session` once it
    /// has re-sent the list, because the table cannot reach the emulator itself.
    pub(crate) fn take_cheats_dirty(&mut self) -> bool {
        std::mem::take(&mut self.cheats_dirty)
    }

    /// Whether the mode moved since the last frame, and clears it. The binary takes this every
    /// frame the way it takes `take_cheats_dirty`: the palette is a global and the textures are
    /// not, so the one thing that has to be told is whoever owns the textures.
    pub(crate) fn take_mode_dirty(&mut self) -> bool {
        std::mem::take(&mut self.mode_dirty)
    }

    /// Whether a shelf swap has left the row's frame cut for the wrong machine. Taken once by
    /// the frontend, which has the only compositor.
    pub(crate) fn take_shelf_dirty(&mut self) -> bool {
        std::mem::take(&mut self.shelf_dirty)
    }

    /// Whether the refresh row rebuilt the library. Taken once by the frontend, for the same
    /// reason and at the same place as `take_shelf_dirty`: the faces are textures.
    pub(crate) fn take_library_dirty(&mut self) -> bool {
        std::mem::take(&mut self.library_dirty)
    }

    /// The face handles the refresh row retired. **Drained rather than peeked**, because the
    /// frontend releases each one exactly once and a second pass over a released `TexId` is a
    /// release of something the compositor may since have handed to somebody else.
    pub(crate) fn take_stale_faces(&mut self) -> Vec<TexId> {
        std::mem::take(&mut self.stale_faces)
    }

    /// SELECT+START: print the device the other way round.
    ///
    /// Three things, and the order is the order they have to happen in. The palette moves
    /// first, so that every quad drawn from here on — this frame included — is already the new
    /// colour. The state file follows, so a device switched off a second later comes back the
    /// way it was left. And the flag goes up last, because it is a request to someone else and
    /// nothing about this frame depends on the answer.
    ///
    /// The mode is a *setting* rather than a screen, and it is written through the same door
    /// the levels use, so a device switched off a second after the press comes back the way it
    /// was left rather than the way it booted.
    pub(crate) fn toggle_mode(&mut self) {
        let mode = slot_ui::palette::mode().other();
        slot_ui::palette::set_mode(mode);
        // A theme change repaints everything the palette touches: the fixed furniture (icons,
        // favourite stars, machine logos), the lazy caches (shelf title, clock, charge, about,
        // switcher), and the cart faces — which bake the shell colour in light mode
        // (`housing()`). `mode_dirty` pokes the frontend, which re-runs `rebake` in place on the
        // next frame: it re-rasterises the fixed faces and the visible shelf window, and drops
        // the rest so the resident pump rebuilds each as the caret reaches it. That is the whole
        // job a restart did, without re-running the boot — so no restart here.
        self.state.mode = mode;
        self.mode_dirty = true;
        self.persist();
    }

    /// SELECT+R1 on the shelf: the next colouring ramp of the mode the device is in.
    ///
    /// The same three steps `toggle_mode` runs, in the same order, for the same reasons: the
    /// palette moves first so every quad drawn from here on is already the new colour, the
    /// state file follows so a device switched off a second later comes back the way it was
    /// left, and the flag goes up last because it is a request to the frontend. The rebake it
    /// triggers is the same one — a look repaints everything the palette touches, because the
    /// ink the faces were baked with is the ink the look replaces.
    ///
    /// The look is *within* the mode, which is what keeps the two keys honest: SELECT+START
    /// still flips dark/light, and SELECT+R1 walks the six ramps that mode carries — a phosphor
    /// row in the dark, a paper row in the light. Zero is the device's own palette, so the
    /// first press always leaves the built-in colours and the press after that comes back to
    /// them; nothing about the default is ever a look's to change.
    pub(crate) fn cycle_look(&mut self) {
        let next = slot_ui::palette::next_look();
        slot_ui::palette::set_look(next);
        self.state.look = next as u8;
        self.mode_dirty = true;
        self.persist();
        // The look moves no pixel the user chose, so the name is the only confirmation the
        // press landed — the same reason the audio profile says its three.
        self.hud.toast(Toast::LookChanged, self.now());
    }

    /// SELECT+A over a running game. Opens the table — the "see the whole list and pick" screen
    /// a bare ON/OFF toast never was. The same chord again, while the table is up, flips every
    /// code at once: the old master toggle, now reachable from inside.
    pub(crate) fn open_cheat_menu(&mut self) {
        if self.cheat_menu.is_some() {
            return;
        }
        // The table is the only overlay that should be up while it is; an open in-game menu would
        // otherwise sit underneath it and steal nothing but still be half-true.
        self.game_menu = None;
        self.cheat_menu = Some(0);
    }

    /// Closes the table without touching any code's state.
    fn close_cheat_menu(&mut self) {
        self.cheat_menu = None;
    }

    /// `Some(cell)` while the palette browser is up, `None` otherwise. Read by `Session` to pause
    /// the core and to keep the pad out of it, the same way it reads the cheat table.
    pub fn palette_browser(&self) -> Option<usize> {
        self.pal_cursor
    }

    /// ★ 2026-10-21：浏览器是不是从**游戏菜单**里进的。B/A 退出时看这个：从组合键进的
    /// （原 SELECT+Y 长按）退回游戏，从菜单进的退回菜单的"色板浏览器"那一行。
    /// 槽位号 3 = `GameRow::ALL` 里 Palette 的下标。
    ///
    /// Hold of the palette key: the browser, on the page the key is walking, with the caret on the
    /// palette that is actually up.
    ///
    /// A Game Boy's list is hundreds of entries and this is the only way to see them; a colour
    /// machine's is eighteen, which is two pages and the reason it has a browser at all rather
    /// than a ring of eighteen presses. Only while a game is running, too: there is no picture on
    /// the shelf to recolour, and a hold there is simply nothing.
    ///
    /// **Refused while a model is up**, for the reason `cycle_cc` gives: a model's four shades are
    /// bound to its screen art, so a browser that offered to recolour the picture inside that case
    /// would be offering something it could not then do. Refused rather than dropped, so the press
    /// reads as answered.
    fn open_palette_browser(&mut self) {
        if self.pal_cursor.is_some() {
            return;
        }
        if !matches!(self.phase, Phase::Playing { .. }) {
            return;
        }
        // ★ 2026-10-21：组合键这条入口不是菜单那条路 —— 旗子落下，B 退出时回游戏。
        self.pal_from_menu = false;
        // A model's art is bound to its shades, on a Game Boy; nothing else here refuses.
        if self.current_machine() == System::Gb
            && App::model_of_index(self.overlay_slot, self.overlay_index).is_some()
        {
            self.refuse();
            return;
        }
        // The page and the caret both come from the entry actually in hand, so the browser opens
        // on the one that is up. Deriving the page rather than reading `pal_page` is what keeps a
        // colour machine's two pages of nine away from the Game Boy's page count of its own
        // hundreds-long table — the two are not the same number and never were.
        let machine = self.current_machine();
        let per = App::grades_per_page(machine);
        let at = self.display.cc[DisplayFilter::machine_slot(machine)] as usize;
        self.pal_page = at / per;
        self.pal_cursor = Some(at.saturating_sub(self.pal_page * per));
    }

    /// How many entries the browser's table has for the machine that is up.
    fn browser_total(&self, machine: System) -> usize {
        if machine == System::Gb {
            crate::palettes::GB_PALETTES.len()
        } else {
            GRADES.len()
        }
    }

    /// The name of one of those entries, and the swatch its cell is painted with: a Game Boy's own
    /// four shades, or the grade walked from black to white (`grade_swatch`).
    fn browser_entry(&self, machine: System, at: usize) -> (&'static str, slot_ui::PaletteSwatch) {
        if machine == System::Gb {
            let e = &crate::palettes::GB_PALETTES[at.min(crate::palettes::GB_PALETTES.len() - 1)];
            (e.0, slot_ui::PaletteSwatch::shades(e.1))
        } else {
            let g = &GRADES[at.min(GRADES.len() - 1)];
            (g.name, grade_swatch(g))
        }
    }

    fn close_palette_browser(&mut self) {
        self.pal_cursor = None;
    }

    /// The browser owns every button on the game's side while it is up. The arrows ring inside the
    /// page — a row down is three cells — L and R turn the page, A takes the palette under the
    /// caret and B leaves without it.
    ///
    /// The caret rings rather than stops at the edges because the page is what bounds it, and L/R
    /// are the way out; a caret that could reach an edge and stop would be a caret the user has to
    /// find their way back from.
    fn palette_browser_input(&mut self, action: Action) {
        let Some(cursor) = self.pal_cursor else {
            return;
        };
        let machine = self.current_machine();
        let total = self.browser_total(machine);
        let per = App::grades_per_page(machine);
        match action {
            Action::GbaDown(Btn::Left) | Action::ShelfLeft => {
                self.pal_cursor = Some(slot_store::step_cursor(
                    self.pal_page,
                    cursor,
                    -1,
                    total,
                    per,
                ));
            }
            Action::GbaDown(Btn::Right) | Action::ShelfRight => {
                self.pal_cursor = Some(slot_store::step_cursor(
                    self.pal_page,
                    cursor,
                    1,
                    total,
                    per,
                ));
            }
            Action::GbaDown(Btn::Up) => {
                self.pal_cursor = Some(slot_store::step_cursor(
                    self.pal_page,
                    cursor,
                    -3,
                    total,
                    per,
                ));
            }
            Action::GbaDown(Btn::Down) => {
                self.pal_cursor = Some(slot_store::step_cursor(
                    self.pal_page,
                    cursor,
                    3,
                    total,
                    per,
                ));
            }
            Action::GbaDown(Btn::L1) | Action::GbaDown(Btn::R1) => {
                // The caret is a cell on the old page, so a shorter page it arrives on clamps it
                // rather than dropping it off the end. Only the last page is short, and only by
                // thirteen cells, so this is one press of the boundary rather than a rule.
                let delta = if action == Action::GbaDown(Btn::L1) {
                    -1
                } else {
                    1
                };
                self.pal_page = slot_store::step_page(self.pal_page, delta, total, per);
                let n = slot_store::len_on(self.pal_page, total, per);
                self.pal_cursor = Some(cursor.min(n.saturating_sub(1)));
            }
            Action::GbaDown(Btn::A) => {
                let picked = slot_store::index_of(self.pal_page, cursor, total, per);
                self.display.cc[DisplayFilter::machine_slot(machine)] = picked as u16;
                if let Some(root) = &self.root {
                    crate::root::write_display_modes(root, self.display.mask_mode, self.display.cc, self.display.scanline_state);
                }
                self.close_palette_browser();
                self.return_from_palette();
            }
            Action::GbaDown(Btn::B) => {
                self.close_palette_browser();
                self.return_from_palette();
            }
            _ => {}
        }
    }

    /// ★ 2026-10-21：浏览器关掉之后回哪。从菜单进的回菜单的"色板浏览器"那一行
    /// （`GameRow::ALL` 里 Palette 的下标 3），从组合键进的回游戏。
    fn return_from_palette(&mut self) {
        if self.pal_from_menu {
            self.pal_from_menu = false;
            self.game_menu = Some(GameMenu::Rows(3));
        }
    }

    /// ★ 2026-10-21：**操作键映射弹窗的按键**。三层状态，一层套一层：
    ///
    /// 1. `confirm` 开着 —— 「改完了，要保存吗」。`A` 落笔进卡（`persist`），`B` 丢掉。
    /// 2. `picking` 开着 —— 15 个目标那张清单。上下走，`A` 选中，`B` 退回行列表。
    /// 3. 行列表 —— 上下走七行（六键 + 频率），`A` 在键行上开目标清单、在频率行上
    ///    `←`/`→` 改档，`B` 是退出（改过就先问保存，没改就直接退）。
    ///
    /// **改是实时写进 `state` 的**：这样弹窗开着的时候转着的游戏就能试新键，而"取消"靠
    /// `close_keymap(false)` 把开的时候那张表整个还原 —— 不这么做，取消会把一个没保存的
    /// 改动留在卡上。`confirm` 里那个 `A` 只是把它写盘，不是第一次生效。
    fn keymap_input(&mut self, action: Action) {
        let Some(edit) = self.keymap_edit.as_ref() else {
            return;
        };
        let system = self.current_machine();
        let pool = slot_ui::keys::targets_for(system);
        let phys = slot_ui::keys::physical_for(system);
        // ── 第三层：保存确认 ────────────────────────────────────────────────────────────
        if edit.confirm {
            match action {
                Action::GbaDown(Btn::A) => {
                    // 写盘，然后**保留**这次的改动（keep=true 不回滚），回上一级。
                    self.persist();
                    self.close_keymap(true);
                    self.keymap_back();
                }
                // B = 取消，丢掉这次改动，回上一级。
                Action::GbaDown(Btn::B) => {
                    self.close_keymap(false);
                    self.keymap_back();
                }
                // MENU 短按 = 不管还问不问，直接回游戏。
                Action::OpenMenu | Action::GameMenu => {
                    self.close_keymap(false);
                    self.keymap_to_game();
                }
                _ => {}
            }
            return;
        }
        // ── 第二层：目标清单 ────────────────────────────────────────────────────────────
        if edit.picking.is_some() {
            let pick = edit.picking.unwrap_or(0);
            let last = pool.len() - 1;
            match action {
                Action::GbaDown(Btn::Up) => {
                    if let Some(e) = self.keymap_edit.as_mut() {
                        e.picking = Some(pick.saturating_sub(1));
                    }
                }
                Action::GbaDown(Btn::Down) => {
                    if let Some(e) = self.keymap_edit.as_mut() {
                        e.picking = Some((pick + 1).min(last));
                    }
                }
                Action::GbaDown(Btn::A) => {
                    // 选中：把这把键写在这个目标上，清单合上，回到行列表。
                    let row = edit.row;
                    if row < phys.len() {
                        let p = phys[row];
                        let target = pool[pick.min(last)];
                        self.set_key(p, target.name);
                    }
                    if let Some(e) = self.keymap_edit.as_mut() {
                        e.picking = None;
                    }
                }
                // B = 取消清单，回到行列表（上一级）。
                Action::GbaDown(Btn::B) => {
                    if let Some(e) = self.keymap_edit.as_mut() {
                        e.picking = None;
                    }
                }
                // MENU 短按 = 直接回游戏。
                Action::OpenMenu | Action::GameMenu => {
                    self.close_keymap(false);
                    self.keymap_to_game();
                }
                _ => {}
            }
            return;
        }
        // ── 第一层：行列表 ─────────────────────────────────────────────────────────────
        let row = edit.row;
        let last_row = phys.len(); // 频率行 = 物理行数
        match action {
            Action::GbaDown(Btn::Up) => {
                if let Some(e) = self.keymap_edit.as_mut() {
                    e.row = if row == 0 { last_row } else { row - 1 };
                }
            }
            Action::GbaDown(Btn::Down) => {
                if let Some(e) = self.keymap_edit.as_mut() {
                    e.row = if row == last_row { 0 } else { row + 1 };
                }
            }
            Action::GbaDown(Btn::A) => {
                if row < phys.len() {
                    // 开目标清单，指着这把键当前那个目标上。
                    let p = phys[row];
                    // ★ 2026-10-26：光标要落在**本机种**现在那个目标上（PS1 的 X 默认是 `GAME_X`，
                    // 不是 GBA 的 `NONE`）。
                    let map = self.keymap_effective(system);
                    let at = pool
                        .iter()
                        .position(|t| t.name == slot_ui::current_for(system, p, &map).name)
                        .unwrap_or(0);
                    if let Some(e) = self.keymap_edit.as_mut() {
                        e.row = row;
                        e.picking = Some(at);
                    }
                }
            }
            // 频率行：`←`/`→` 在连发档位之间走（钳住，不环绕 —— 和这个屏每一张列表一样）。
            Action::GbaDown(Btn::Left) | Action::ShelfLeft if row == last_row => {
                self.step_turbo(false);
            }
            Action::GbaDown(Btn::Right) | Action::ShelfRight if row == last_row => {
                self.step_turbo(true);
            }
            // **B = 返回上一级**（不是回游戏）。改过就先问保存，没改就直接退到开它的那一屏。
            Action::GbaDown(Btn::B) => {
                if self.keymap_dirty() {
                    if let Some(e) = self.keymap_edit.as_mut() {
                        e.confirm = true;
                    }
                } else {
                    self.close_keymap(false);
                    self.keymap_back();
                }
            }
            // **MENU 短按 = 直接回游戏**，一路关到底。
            Action::OpenMenu | Action::GameMenu => {
                self.close_keymap(false);
                self.keymap_to_game();
            }
            _ => {}
        }
    }

    /// ★ 2026-10-21 六六：「B 键的功能是取消或返回上级菜单，但是现在 B 键按了直接回游戏了」。
    ///
    /// **B 只关弹窗本身，不动开它的那一屏。** 从游戏菜单进来的，`game_menu` 一直是
    /// `Some(Rows(2))`（"操作键映射"那一行），弹窗一关它自己就重新画出来了；从货架设置页
    /// 进来的，相位还是 `Phase::Settings`，也不用动。所以这里真的**什么都不用做** ——
    /// 之前那行 `self.game_menu = None` 才是病根，它把菜单一起关了，于是"退回游戏"。
    fn keymap_back(&mut self) {
        // 什么都不做，是有意的：见上面的说明。
    }

    /// ★ 2026-10-21：**MENU 短按：一路关到底，回游戏。**
    ///   游戏菜单还开着就一起关掉，然后底下的 `Phase::Playing` 就是游戏了。
    fn keymap_to_game(&mut self) {
        self.game_menu = None;
    }

    /// 把一把物理键写在某个目标上（按 `keys::name(phys)` 的拼法存进 `state.keymap`）。
    fn set_key(&mut self, phys: slot_input::Btn, target_name: &str) {
        // ★★ 2026-10-26：**写进这台机种自己那张表**，不再写进全局那张。
        // 在 PS1 上把 X 改成 `GAME_X` 不该让 GBA 的 X 也跟着变 —— 这就是整件事的起点。
        let system = self.current_machine();
        let dir = system.dir_name().to_string();
        let key = slot_ui::keys::name(phys).to_string();
        // **回到出厂值就是删掉那一行**，不是存一个和默认一样的字符串：表里没有这一行，
        // `keys::current_for` 就落到 `stock_for`，这正是"改过可回默认"要的读法。
        // 注意 `stock` 也要按**本机种**取：PS1 的 X 出厂就是 `GAME_X`，拿 GBA 的 `NONE`
        // 去比的话，"选回 GAME_X"会被当成改动写进去。
        let stock = slot_ui::keymap_stock_for(system, phys);
        {
            let own = self.state.keymaps.entry(dir.clone()).or_default();
            if target_name == stock.name {
                own.remove(&key);
            } else {
                own.insert(key, target_name.to_string());
            }
        }
        // 空表就不留一个空壳，卡上的文件干净。
        if self.state.keymaps.get(&dir).is_some_and(|m| m.is_empty()) {
            self.state.keymaps.remove(&dir);
        }
        // **一台都还没自定义过才不是"自定义"** —— 旧卡那张全局表也算动过。
        self.state.keymap_mode = !self.state.keymap.is_empty()
            || self.state.keymaps.values().any(|m| !m.is_empty());
    }

    /// 频率行 `←`/`→`：在 `TURBO_HZ` 之间钳住地走，改的是 `state.turbo_hz`。
    fn step_turbo(&mut self, right: bool) {
        let at = slot_ui::TURBO_HZ
            .iter()
            .position(|h| *h == self.state.turbo_hz)
            .unwrap_or(slot_ui::TURBO_HZ.len() / 2);
        let to = if right {
            (at + 1).min(slot_ui::TURBO_HZ.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.state.turbo_hz = slot_ui::TURBO_HZ[to];
    }

    /// 弹窗里的表和开的时候那张比，动过没有。保存/放弃两条路都靠它决定要不要问。
    fn keymap_dirty(&self) -> bool {
        match &self.keymap_edit {
            Some(e) => {
                self.state.keymap != e.original
                    || self.state.keymaps != e.original_systems
                    || self.state.turbo_hz != e.original_hz
            }
            None => false,
        }
    }

    /// What the browser's row of names is keyed on: the machine and the page that is up, or
    /// `None` while it is shut.
    ///
    /// A key rather than the names themselves, because a page's names are a compile-time fact
    /// about a table that never changes: the frontend rebuilds its fifteen faces when this moves
    /// and at no other time, so a caret walking inside a page costs nothing at all.
    pub fn palette_names_key(&self) -> Option<(System, usize)> {
        self.pal_cursor
            .map(|_| (self.current_machine(), self.pal_page))
    }

    /// The names of the entries on that page, in table order — one per cell, so every cell says
    /// what it is rather than only the one the caret is on. As long as the page and no longer:
    /// the last page of a table is the short one.
    ///
    /// **Empty on a Game Boy, and that is the whole of why this can be empty.** A cell is 196
    /// pixels wide and a colour machine's nine grades are named in four to six characters apiece,
    /// so they fit under their own cells and the page can be read across. A Game Boy's 343
    /// palettes are named by the core and the names are *long*: measured in the face the machine
    /// carries, 263 of them are wider than a cell at the type size the grades are set in, and the
    /// longest is still 30 pixels too wide at ten. Those pages keep the one line at the foot of
    /// the panel (`palette_name_want`), which has 640 pixels to work in and so never cuts a name.
    pub fn palette_names(&self) -> Vec<String> {
        let machine = self.current_machine();
        if machine == System::Gb {
            return Vec::new();
        }
        let per = App::grades_per_page(machine);
        let total = self.browser_total(machine);
        let base = (self.pal_page * per).min(total);
        let n = slot_store::len_on(self.pal_page, total, per);
        (0..n)
            .map(|i| self.browser_entry(machine, base + i).0.to_string())
            .collect()
    }

    /// The faces the frontend built for that page, in the same order.
    pub(crate) fn set_palette_names(&mut self, names: Vec<(TexId, u32, u32)>) {
        self.pal_names = names;
    }

    /// The name of the entry under the browser's caret, or `None` while it is shut **or on a
    /// machine whose cells are named** — the two readings are exclusive, and neither machine has
    /// both. The frontend mints a face from this and nothing else; comparing it to the last one it
    /// built is what keeps a moved caret to one rasterise and a still one to none.
    pub fn palette_name_want(&self) -> Option<String> {
        let cursor = self.pal_cursor?;
        let machine = self.current_machine();
        if machine != System::Gb {
            return None;
        }
        let per = App::grades_per_page(machine);
        let total = self.browser_total(machine);
        let at = slot_store::index_of(self.pal_page, cursor, total, per);
        Some(self.browser_entry(machine, at).0.to_string())
    }

    /// The face the frontend built for that name.
    pub(crate) fn set_palette_name_face(&mut self, face: Option<(TexId, u32, u32)>) {
        self.pal_name = face;
    }

    /// The palette browser, over the paused game. Only ever up over a cart in the slot, so the
    /// `Phase::Playing` arm of `draw` is the only caller.
    fn draw_palette_browser(&self, out: &mut Vec<Draw>) {
        let Some(cursor) = self.pal_cursor else {
            return;
        };
        let machine = self.current_machine();
        let per = App::grades_per_page(machine);
        let total = self.browser_total(machine);
        let base = (self.pal_page * per).min(total);
        let n = slot_store::len_on(self.pal_page, total, per);
        // A copy of the page's swatches, at most fifteen of them and only while the browser is
        // open. A Game Boy's entries carry their own four shades; a colour grade's cell is the
        // grade itself walked from black to white (`grade_swatch`).
        let swatches: Vec<slot_ui::PaletteSwatch> = (base..base + n)
            .map(|at| self.browser_entry(machine, at).1)
            .collect();
        let view = slot_ui::PaletteView {
            page: self.pal_page,
            cursor,
            pages: slot_ui::palette_pages(total, per),
            rows: slot_ui::palette_rows(per),
            swatches: &swatches,
            names: &self.pal_names,
            foot: self.pal_name,
        };
        slot_ui::draw_palette_browser(&view, out);
    }

    /// The master switch behind SELECT+A's second press: flips the lot on or off at once.
    pub(crate) fn toggle_all_cheats(&mut self) {
        self.cheats_on = !self.cheats_on;
    }

    /// The table owns every button on the game's side while it is up. Up and down move, A flips
    /// the highlighted code, B leaves. The master chord is handled in `Session::act`, before this
    /// sees the action, so it never arrives here as a toggle.
    fn cheat_menu_input(&mut self, action: Action) {
        let Some(index) = self.cheat_menu else {
            return;
        };
        let last = self.cheats.len().saturating_sub(1);
        match action {
            Action::GbaDown(Btn::Up) => self.cheat_menu = Some(index.saturating_sub(1)),
            Action::GbaDown(Btn::Down) => self.cheat_menu = Some((index + 1).min(last)),
            Action::GbaDown(Btn::A) => {
                if let Some(item) = self.cheats.get_mut(index) {
                    item.enabled = !item.enabled;
                }
                self.cheats_dirty = true;
            }
            Action::GbaDown(Btn::B) => self.close_cheat_menu(),
            _ => {}
        }
    }

    /// The "no cheats" panel, rasterised once at boot. Shown by `draw_cheat_menu` when the table
    /// opens on a cart that carries no codes.
    pub(crate) fn set_cheat_empty_face(&mut self, face: (TexId, u32, u32)) {
        self.cheat_empty_face = Some(face);
    }

    /// (Re)bind the row faces to a cart's codes, with the stem they belong to so the frontend
    /// knows when to rebuild. Called from the frontend's per-frame sync.
    pub(crate) fn set_cheat_faces(&mut self, stem: Option<String>, faces: Vec<(TexId, u32, u32)>) {
        self.cheat_face_stem = stem;
        self.cheat_faces = faces;
    }

    /// The stem whose faces `cheat_faces` currently holds. `None` until the first cart with
    /// cheats seats, and again after eject.
    pub(crate) fn cheat_face_stem(&self) -> Option<&str> {
        self.cheat_face_stem.as_deref()
    }

    /// How many row faces `cheat_faces` currently holds. Compared against `cheats().len()` so the
    /// frontend knows to rebuild after the codes load from the card post-spawn.
    pub(crate) fn cheat_face_count(&self) -> usize {
        self.cheat_faces.len()
    }

    /// The cheat table: a dark panel over the paused game, listing every code with a green (on)
    /// or grey (off) chip, the highlighted row carried on the menu's own bar. A window of
    /// `CHEAT_VISIBLE` rows scrolls so the highlight stays on screen no matter how long the list.
    fn draw_cheat_menu(&self, index: usize, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: slot_ui::opening(),
        });
        if self.cheats.is_empty() {
            if let Some((tex, w, h)) = self.cheat_empty_face {
                out.push(Draw::Tex {
                    x: ((OUT_W - w) / 2) as f32,
                    y: ((OUT_H - h) / 2) as f32,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
            return;
        }
        let n = self.cheats.len();
        let vis = n.min(CHEAT_VISIBLE);
        let start = if n <= CHEAT_VISIBLE {
            0
        } else {
            index
                .saturating_sub(CHEAT_VISIBLE / 2)
                .min(n - CHEAT_VISIBLE)
        };
        let top = centred_top(vis, CHEAT_PITCH);
        let left: f32 = 56.0;
        for v in 0..vis {
            let row = start + v;
            if row >= n {
                break;
            }
            let y = top + CHEAT_PITCH * v as f32;
            if row == index {
                out.push(Draw::Rect {
                    x: 8.0,
                    y: y + POWER_MENU_BAR_INSET,
                    w: (OUT_W - 16) as f32,
                    h: CHEAT_PITCH - 2.0 * POWER_MENU_BAR_INSET,
                    colour: slot_ui::edge(),
                });
            }
            // ON/OFF chip to the left of the code: green when live, grey otherwise. "Live" means
            // the code is switched on AND the master is on.
            let on = self.cheats_on && self.cheats[row].enabled;
            let iw = 14.0;
            let ix = 24.0;
            let iy = y + (CHEAT_PITCH - iw) / 2.0;
            out.push(Draw::Rect {
                x: ix,
                y: iy,
                w: iw,
                h: iw,
                colour: if on {
                    [0.30, 0.85, 0.45, 1.0]
                } else {
                    [0.55, 0.55, 0.6, 1.0]
                },
            });
            if let Some((tex, w, h)) = self.cheat_faces.get(row).copied() {
                out.push(Draw::Tex {
                    x: left,
                    y: y + (CHEAT_PITCH - h as f32) / 2.0,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
        }
    }

    /// SELECT on the shelf offers the highlighted cart's core, opening on the one it already
    /// uses so the menu answers "which is this?" before it asks "which do you want?".
    ///
    /// Both the read that positions the highlight and the write that follows need the card.
    /// Without one there is nothing to configure and nowhere to put an answer, so the button
    /// stays inert rather than raising a menu whose choice would evaporate.
    fn open_core_picker(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        // Nothing to configure with no cart under the highlight, and a picker that wrote to
        // an empty stem would leave a line for a cart that is not there.
        let Some(cart) = self.row().current_cart() else {
            return;
        };
        // Nothing to choose for anything that is not a GBA cart: gpSP does not take Game Boy,
        // so mGBA is not a preference to record but the only answer there is. The picker is two
        // sockets to hop between, and opening it over one socket is a question that can only be
        // refused.
        if cart.system() != System::Gba {
            return;
        }
        let seat = slot_store::core_for(&root, &cart.stem);
        let now = self.now();
        let mut picker = CorePicker::open(seat, now);
        if self.core_faces_ready() {
            picker.start(now);
        }
        self.core_picker = Some(picker);
        // Whatever the shelf had armed before START belonged to the shelf that was showing,
        // not to the cart now open over it: a held direction would keep repeating underneath
        // the lid, and a held A would still insert the cart once its 500 ms ran out.
        self.row_mut().release_hold();
        self.play_held = None;
        self.fav_held = None;
        self.sys_held = None;
        self.mtp_held = None;
    }

    /// Whether the board and lid on the GPU are the highlighted cart's, so its open can start.
    fn core_faces_ready(&self) -> bool {
        self.core_faces_stem
            .as_deref()
            .is_some_and(|stem| self.selected_stem() == Some(stem))
    }

    /// The picker owns every button while it is up, including the arrows the shelf uses: a
    /// board that let the row behind it move would act on a different cart than the one whose
    /// lid is off. The arrows point at the sockets, so they do not wrap.
    fn core_picker_input(&mut self, action: Action) {
        let press = match action {
            Action::GbaDown(Btn::Left) | Action::ShelfLeft => Press::Left,
            Action::GbaDown(Btn::Right) | Action::ShelfRight => Press::Right,
            Action::GbaDown(Btn::A) => Press::Keep,
            Action::GbaDown(Btn::B) => Press::Back,
            _ => return,
        };
        let now = self.now();
        let Some(picker) = &mut self.core_picker else {
            return;
        };
        let outcome = picker.press(press, now);
        if let Outcome::Write(core) = outcome {
            self.write_core(core);
        }
    }

    /// The rows this menu has right now. A question about the cart in the slot rather than
    /// about the menu, so it is asked per open and never held: a stored answer could only
    /// ever go stale against the cart it was about.
    fn game_rows(&self) -> Vec<GameRow> {
        GameRow::ALL
            .into_iter()
            .filter(|row| match row {
                // ★ 2026-10-20：两张槽位表跟核心无关 —— 任何核心都能 serialize，所以它们
                //   在任何卡上都出现。
                GameRow::Save | GameRow::Load => true,
                // 键位映射 / 色板 / 屏幕档位 / 保存设置：跟核心无关，任何卡上都有。
                // 键位映射是 `shortcuts.rs` 的同一张表，色板和档位跟机种走（画的和切的
                // 都在各自那一级里按机种分派），保存设置按机种/按游戏两条路都通。
                GameRow::Shortcuts | GameRow::Palette | GameRow::Grid | GameRow::SaveSettings => {
                    true
                }
                // gpSP is the only core with a netpacket interface to link over. Absent
                // rather than shown and refused: the fix is not on this screen — it is four
                // steps away on the shelf — and a row that says so is a row that teaches a
                // dead end.
                GameRow::Link => self.core == Core::Gpsp,
                // 重启模拟器跟核心无关：复位是每个核心自己的 `retro_reset`，任何卡都做得到。
                // ★ 2026-10-21：「退出到货架」那一行删了（见 `GameRow::Quit` 的位置），
                //   所以这里只剩重启 —— 离开这一局是 MENU 长按那条老路，不必在菜单里再开一个口。
                GameRow::Restart => true,
            })
            .collect()
    }

    /// Where the Link row sits among the rows that exist, so backing out of it lands back on
    /// it rather than on whatever happens to be first.
    fn link_row(&self) -> usize {
        self.game_rows()
            .iter()
            .position(|r| *r == GameRow::Link)
            .unwrap_or(0)
    }

    /// SELECT+MENU over a running game. Nothing is torn down and no phase changes: the cart
    /// is still seated behind it and cancelling gives it straight back.
    fn open_game_menu(&mut self) {
        if self.game_menu.is_some() {
            return;
        }
        // The same hazard the power menu's own guard exists for: this overlay pauses the
        // core underneath it (`Session::held` names it, and `sync_speed` maps that to
        // `Speed::Paused`), which is one of the exact manipulations libretro's netpacket
        // contract forbids while players are connected. Declined with the shake every other
        // "nothing doing" in this file answers with — and a device already in a session has
        // nothing to pick in here anyway.
        if self.link_active() {
            return self.refuse();
        }
        // An empty panel over a paused game is worse than no panel at all. Today that means
        // an mGBA cart raises nothing, Link being the only row there is; when a second row
        // lands this stops being about the link at all and stays exactly as true.
        if self.game_rows().is_empty() {
            return;
        }
        self.game_menu = Some(GameMenu::Rows(0));
    }

    /// The menu owns every button on the game's side of the device while it is up. Up and
    /// down wrap, for the core picker's reason: with two rows either arrow is the other's
    /// undo, and an end that stuck would need the player to know which one they were against.
    /// ★ 2026-10-20：把槽位表挪到某个槽位上。只有这里改 `Save`/`Load` 里的数字，所以"换槽
    ///   要重新烘快照和那一行字"只有一处要记。
    fn set_slot(&mut self, index: usize) {
        self.game_menu = match self.game_menu {
            Some(GameMenu::Save(_)) => Some(GameMenu::Save(index)),
            Some(GameMenu::Load(_)) => Some(GameMenu::Load(index)),
            other => other,
        };
    }

    /// The slot under the eye, when one of the two slot sheets is up.
    pub fn slot_index(&self) -> Option<usize> {
        match self.game_menu {
            Some(GameMenu::Save(i)) | Some(GameMenu::Load(i)) => Some(i),
            _ => None,
        }
    }

    /// What the line under the picture says: `03 · 空`, or `03 · 10-20 22:31`.
    ///
    /// **The time is the file's own mtime**, local — the same wall clock the polaroid captions
    /// and the shelf clock read, so a save made at 22:31 does not say 06:31 on a card that has
    /// never had its timezone set.
    pub fn slot_caption(&self) -> Option<String> {
        let i = self.slot_index()?;
        let slot = Slot::at(i);
        let when = self
            .slots()
            .and_then(|s| s.list().ok())
            .and_then(|l| l.into_iter().find(|e| e.slot == slot))
            .and_then(|e| e.secs)
            .map(|secs| {
                // `MM-DD HH:MM`: the year is the one thing nobody needs on a save they made
                // this evening, and the column is only so wide.
                let s = format_stamp(secs + i64::from(self.state.utc_offset_min) * 60);
                format!("{} {}", &s[5..10], &s[11..16].replace('-', ":"))
            });
        Some(format!(
            "{} · {}",
            slot.label(),
            when.unwrap_or_else(|| lang::SLOT_EMPTY.to_string())
        ))
    }

    /// The picture for the slot under the eye, as a path the compositor can load — `None` on
    /// an empty slot, which is what keeps an empty one from being drawn as a black rectangle.
    pub fn slot_thumb_path(&self) -> Option<PathBuf> {
        let i = self.slot_index()?;
        let slots = self.slots()?;
        let slot = Slot::at(i);
        slots
            .filled(slot)
            .then(|| slots.dir().join(format!("{}.png", slot.name())))
    }

    pub fn set_slot_faces(
        &mut self,
        caption: Option<(TexId, u32, u32)>,
        thumb: Option<(TexId, u32, u32)>,
    ) {
        self.slot_caption_face = caption;
        self.slot_thumb_face = thumb;
    }

    /// ★ 2026-10-21 傍晚（**改过一次**）：**重开本局** —— 在游戏里把核心拨回开机那一刻。
    ///
    /// # 先前那版为什么不对
    ///
    /// 第一版是"拔卡、再原样插回去"，理由是加载只有 `Phase::Inserting` 那一条入口。可那条
    /// 入口**会先接上即时存档**（`persist::read_resume`，为的是"开机接着玩"），于是插回去
    /// 落在拔卡那一刻的画面 —— 六六实测：「插拔后又回到拔卡时的即时存档」。要绕开它就得在
    /// insert 上再开一个"别接档"的分支，而那正是"第二份加载逻辑"，当初想避开的坑。
    ///
    /// # 现在
    ///
    /// 核心自己做（libretro 的 `retro_reset`）：CPU 回复位向量、内存清成开机值、BIOS 重走一遍
    /// —— 就是通电那一刻，只是没有断电。ROM 早在 `spawn` 时读进内存了，所以这里连读盘都没有。
    ///
    /// **相位一帧都不动**：不出停机动画、不出插拔动画、卡不从槽里走。要收的只是几样"这一局
    /// 的东西" —— 联机（对端会以为人还在）、菜单与浮层（它们盖在游戏上，而游戏已经不是那一局
    /// 了）、倒带历史（由 `EmuHandle::reset` 在核心那一侧清）。
    ///
    /// `SRAM` 不 flush：卡没拔，存档还在核心手里，正常退出那一刻才落盘 —— 中途写一次等于
    /// 让"重开"把手上这局的存档也定下来，那不是重开该做的事。
    fn restart_game(&mut self) {
        let Some(_) = self.cart_in_slot() else {
            return;
        };
        self.end_link();
        self.close_game_menu();
        self.close_cheat_menu();
        self.close_palette_browser();
        self.pending = None;
        self.refusal = None;
        self.refused_from = None;
        self.restart_core = true;
    }

    /// 手里那张卡的 stem，只在这一局真正立着的时候给。`restart_game` 用它当"现在有得重开"的
    /// 判据 —— 一个没有游戏的游戏菜单本来就不该有这一行，兜底也不该凭空复位一台空机器。
    fn cart_in_slot(&self) -> Option<&str> {
        match &self.phase {
            Phase::Playing { cart } => Some(cart.as_str()),
            Phase::Inserting { cart, .. } => Some(cart.as_str()),
            _ => None,
        }
    }

    /// 取走「重开本局」那次请求。`Session` 每帧问一次，取走即清 —— 一次点击只对应一次复位，
    /// 留在字段里会变成"每帧都重开"。
    pub fn take_restart_core(&mut self) -> bool {
        std::mem::take(&mut self.restart_core)
    }

    /// ★ 2026-10-21：**这一局是哪种机器**。只为菜单里那一行服务 —— `Grid` 在 GBA 上叫
    /// 「网格切换」、在 GB/GBC 上叫「遮罩切换」，两件不同的事共用同一个位置。
    ///
    /// 从卡自己的扩展名推（`Cart::system`），而不是从核心：同一个 mGBA 跑 GBA 也跑 GB/GBC，
    /// 核心名分不出这两者。没有卡的时候按 GBA 读 —— 这一行本来就只在有卡的时候画。
    pub fn seated_system(&self) -> System {
        self.cart_in_slot()
            .and_then(|stem| self.system_of(stem))
            .unwrap_or(System::Gba)
    }

    /// ★ 2026-10-21：清掉一个槽位。空的那一格按下去什么都不做 —— 没有可删的东西，
    ///   而"删除成功"的提示在空槽位上等于说谎。
    ///
    /// 删完不另做提示：这一屏自己就是提示。图消失、那行字变回 `03 · 空`，
    /// 比在角落弹一行"已删除"更直接，也少一张要烘的面。
    fn clear_slot(&mut self, slot: Slot) {
        let Some(slots) = self.slots() else {
            return;
        };
        if !slots.filled(slot) {
            return;
        }
        if let Err(e) = slots.remove(slot) {
            eprintln!("slot: clear: {e}");
            self.refuse();
        }
    }

    /// ★ 存到手里这一个槽位。写完就回上一级：存是"做完了"，留在一张一百项的表上只会让人
    ///   怀疑到底存没存上。
    fn save_to_slot(&mut self, slot: Slot) {
        let Some(slots) = self.slots() else {
            return self.refuse();
        };
        let Some(snapshot) = &self.snapshot else {
            return self.refuse();
        };
        if !snapshot.resume_trusted() {
            eprintln!("slot: save: the core refused the resume it was given, not writing a slot");
            return self.refuse();
        }
        let Some(state) = snapshot.state() else {
            eprintln!("slot: save: the core gave up no state");
            return self.refuse();
        };
        let thumb = snapshot.thumb().unwrap_or_default();
        if let Err(e) = slots.write(slot, &state, &thumb) {
            eprintln!("slot: save: {e}");
            return self.refuse();
        }
        self.hud.toast(Toast::StateSaved, self.now());
        // ★ 2026-10-27：写进槽位了才响（上面每条早退都是"没写"）。
        self.sfx = Some(Sfx::Save);
        self.game_menu = Some(GameMenu::Rows(0));
    }

    /// ★ 读手里这一个槽位。空槽位摇一下屏 —— 跟 `load_newest` 在没有存档时一个答复，因为
    ///   "按了没反应"和"读了但没变化"在玩家那边完全分不开。
    fn load_from_slot(&mut self, slot: Slot) {
        let Some(slots) = self.slots() else {
            return self.refuse();
        };
        let path = slots.dir().join(format!("{}.state", slot.name()));
        match slots.read(slot) {
            // `load_file` is the chokepoint every load funnels through: it is the one that
            // refuses a load over a live link, and the one that takes the undo snapshot
            // before the machine moves.
            Ok(Some(_)) if self.load_file(&path) => {
                self.hud.toast(Toast::StateLoaded, self.now());
                self.close_game_menu();
            }
            Ok(Some(_)) => {}
            // Loaded a state the core then refused, so the row read as an answer rather than
            // as a failure: `load_file` has already shaken the screen for it.
            Ok(None) => self.refuse(),
            Err(e) => {
                eprintln!("slot: load: {e}");
                self.refuse()
            }
        }
    }

    fn game_menu_input(&mut self, action: Action) {
        let Some(menu) = self.game_menu else {
            return;
        };
        match menu {
            GameMenu::Rows(row) => {
                let rows = self.game_rows();
                let last = rows.len();
                let cols = GAME_MENU_COLS.min(last.max(1));
                match action {
                    // ★ 2026-10-21：**两列网格的导航**。上下走一行（跳过本排另一格），
                    //   左右在同排里换格。九行两列 ⇒ 五行；最后一行只有一格，所以
                    //   "下一排"要落回合法下标，不能按 +cols 硬加。
                    Action::GbaDown(Btn::Up) if last > 0 => {
                        let to = grid_step(row, last, cols, -1, 0);
                        self.game_menu = Some(GameMenu::Rows(to));
                    }
                    Action::GbaDown(Btn::Down) if last > 0 => {
                        let to = grid_step(row, last, cols, 1, 0);
                        self.game_menu = Some(GameMenu::Rows(to));
                    }
                    Action::GbaDown(Btn::Left) if last > 0 => {
                        let to = grid_step(row, last, cols, 0, -1);
                        self.game_menu = Some(GameMenu::Rows(to));
                    }
                    Action::GbaDown(Btn::Right) if last > 0 => {
                        let to = grid_step(row, last, cols, 0, 1);
                        self.game_menu = Some(GameMenu::Rows(to));
                    }
                    Action::GbaDown(Btn::A) => match rows.get(row) {
                        // ★ 2026-10-20：两张槽位表都从 `Auto` 进（`Slot::at(0)`），而不是从
                        //   `01`——`Auto` 是唯一永远不空的那个，也是"按错了存哪去了"最容易被
                        //   找到的那个。上次用过的槽位不记：一张表一百项，记住一个下标的价值
                        //   远不如每次都从同一个地方开始。
                        Some(GameRow::Save) => self.game_menu = Some(GameMenu::Save(0)),
                        Some(GameRow::Load) => self.game_menu = Some(GameMenu::Load(0)),
                        // ★ 2026-10-21：规格的四行 —— 键位映射、色板、屏幕档位、保存设置。
                        //   前三个开各自的子级；色板浏览器从菜单里进（原长按 SELECT+Y 那条
                        //   组合键已取消），`GameMenu::Palette` 在下面 `draw`/输入里接管。
                        //
                        //   ★★ **2026-10-21 六六：「你链接的是全键位一览，只能看不能改」。**
                        //   原来这一行开的是只读的 `GameMenu::Shortcuts`（四十行键位一览）；
                        //   现在改成**可直接改的那张浮层弹窗**（`open_keymap`），和货架设置页
                        //   「操作键映射」那一行走的是同一个。只读一览还留在货架 MENU 的
                        //   `Phase::Shortcuts` 那张卡上，那里的定位是"查",这里是"改"。
                        Some(GameRow::Shortcuts) => self.open_keymap(),
                        Some(GameRow::Palette) => {
                            self.open_palette_browser_from_menu();
                        }
                        Some(GameRow::Grid) => self.cycle_machine_grid(),
                        // ★ 2026-10-21：保存设置 —— 弹窗确认，不是直接存。
                        Some(GameRow::SaveSettings) => {
                            self.game_menu = Some(GameMenu::SaveSettings);
                        }
                        Some(GameRow::Link) => self.game_menu = Some(GameMenu::Link(0)),
                        // ★ 2026-10-21 傍晚：直接做事的一行，不开子级。
                        Some(GameRow::Restart) => self.restart_game(),
                        None => {}
                    },
                    // ★ 2026-10-20：三条路关同一个面板 —— B、SELECT+MENU 那条和弦、**和 MENU
                    //   短按**。第三条是新的（上面 `Phase::Playing` 那一臂拿它开面板），开着
                    //   的时候它必须也能关，否则一个单键开出来的面板要两只手才关得上。
                    Action::GbaDown(Btn::B) | Action::GameMenu | Action::OpenMenu => {
                        self.close_game_menu()
                    }
                    _ => {}
                }
            }
            // ★ 2026-10-20：**槽位表**。左右换槽（一百个，环绕），A 存 / 读，B 或 MENU 回上一
            //   级 —— 这一页和它的子级都是"返回 / 取消 / 向上一级"同一个意思，所以退出不用
            //   专门占一行。上下在这里没有地方可去：槽位是一排，不是一列。
            GameMenu::Save(i) | GameMenu::Load(i) => {
                let saving = matches!(menu, GameMenu::Save(_));
                match action {
                    Action::GbaDown(Btn::Left) => self.set_slot((i + SLOT_COUNT - 1) % SLOT_COUNT),
                    Action::GbaDown(Btn::Right) => self.set_slot((i + 1) % SLOT_COUNT),
                    Action::GbaDown(Btn::A) => {
                        let slot = Slot::at(i);
                        if saving {
                            self.save_to_slot(slot);
                        } else {
                            self.load_from_slot(slot);
                        }
                    }
                    // ★ 2026-10-21：**X 清掉手里这一格**。
                    //
                    //   菜单开着的时候游戏拿不到任何键（这一整段的前提），所以 X 在这里是空的，
                    //   而"删掉一个存错的档"是这张表唯一一件它自己做不了的事 —— 不做的话，
                    //   一百个槽位只能靠覆盖来清理，而覆盖需要先把游戏打到那个点。
                    Action::GbaDown(Btn::X) => self.clear_slot(Slot::at(i)),
                    Action::GbaDown(Btn::B) | Action::OpenMenu => {
                        self.game_menu = Some(GameMenu::Rows(if saving { 0 } else { 1 }))
                    }
                    Action::GameMenu => self.close_game_menu(),
                    _ => {}
                }
            }
            // ★ 2026-10-21：**操作键映射** —— `shortcuts.rs` 的同一张 ROWS 表，上下滚动。
            //   四十个键位一行一行看，Up/Down 按 `CARD_PAGE` 滚（和货架那页同一步长），
            //   B 或 MENU 回上一级（菜单的 Rows）。
            GameMenu::Shortcuts(scroll, want) => {
                match action {
                    Action::GbaDown(Btn::Up) => {
                        self.game_menu =
                            Some(GameMenu::Shortcuts(scroll, (want - CARD_PAGE).max(0.0)));
                    }
                    Action::GbaDown(Btn::Down) => {
                        let max = self.game_menu_shortcuts_max();
                        self.game_menu =
                            Some(GameMenu::Shortcuts(scroll, (want + CARD_PAGE).min(max)));
                    }
                    Action::GbaDown(Btn::B) | Action::GameMenu | Action::OpenMenu => {
                        self.game_menu = Some(GameMenu::Rows(2))
                    }
                    _ => {}
                }
                // 缓动 settle 在 `tick` 里（见 `game_menu_shortcuts_settle`），这里只改 want。
            }
            // ★ 2026-10-21：**保存设置弹窗**。四个键各一条路：A 按机种全局保存 /
            //   X 按游戏保存 / Y 恢复默认 / B 取消。没有上下 —— 四个键就是全部。
            GameMenu::SaveSettings => match action {
                Action::GbaDown(Btn::A) => {
                    self.persist();
                    self.game_menu = Some(GameMenu::Rows(5));
                }
                Action::GbaDown(Btn::X) => {
                    self.persist_per_game();
                    self.game_menu = Some(GameMenu::Rows(5));
                }
                Action::GbaDown(Btn::Y) => {
                    self.reset_settings();
                    self.game_menu = Some(GameMenu::Rows(5));
                }
                Action::GbaDown(Btn::B) | Action::GameMenu | Action::OpenMenu => {
                    self.game_menu = Some(GameMenu::Rows(5));
                }
                _ => {}
            },
            GameMenu::Link(row) => {
                let last = LinkRow::ALL.len();
                match action {
                    Action::GbaDown(Btn::Up) => {
                        self.game_menu = Some(GameMenu::Link((row + last - 1) % last))
                    }
                    Action::GbaDown(Btn::Down) => {
                        self.game_menu = Some(GameMenu::Link((row + 1) % last))
                    }
                    Action::GbaDown(Btn::A) => {
                        if let Some(pick) = LinkRow::ALL.get(row).copied() {
                            self.start_link(
                                LinkStarter::spawn(pick.role(), link_port()),
                                pick.client_id(),
                            );
                        }
                    }
                    // Back to the rows rather than out of the menu: a player one press into
                    // a two-press choice is not asking to leave.
                    Action::GbaDown(Btn::B) => {
                        self.game_menu = Some(GameMenu::Rows(self.link_row()))
                    }
                    Action::GameMenu => self.close_game_menu(),
                    _ => {}
                }
            }
            // B asks the worker to stop and the screen stays where it is until it answers.
            // `link_radio::up` is an opaque blocking process spawn and nothing can interrupt
            // it for one to five seconds, so closing here would put the player back in their
            // game with an access point still coming up behind them.
            GameMenu::Working(_) => {
                if action == Action::GbaDown(Btn::B) {
                    if let Some(starting) = &mut self.starting {
                        starting.starter.cancel();
                    }
                }
            }
            // A sentence to read, and either button is the way off it.
            GameMenu::Failed(_) => match action {
                Action::GbaDown(Btn::A) | Action::GbaDown(Btn::B) | Action::GameMenu => {
                    self.close_game_menu()
                }
                _ => {}
            },
        }
    }

    /// Hands the overlay a worker that is already running, and puts the screen on the first
    /// step. Split from the pick that spawns one so a caller can supply its own: that is the
    /// only seam by which this screen can be driven with no network interface anywhere near
    /// it, and it is the same seam `LinkStarter::spawn_with` exists for one layer down.
    pub fn start_link(&mut self, starter: LinkStarter, client_id: u16) {
        // Whatever was already running is asked to stop on its way out. `LinkStarter` has no
        // `Drop`: dropping one silently leaves its radio up behind the screen.
        if let Some(mut old) = self.starting.replace(LinkStarting { starter, client_id }) {
            old.starter.cancel();
        }
        self.game_menu = Some(GameMenu::Working(LinkStep::Radio));
    }

    /// Ends the overlay and anything it had running.
    ///
    /// `LinkStarter` has no `Drop`, so a starter dropped mid-wait keeps working: a host
    /// dropped while waiting leaves its access point up for up to thirty seconds with
    /// nothing on the other end of it. Every path that ends the overlay comes through here,
    /// including the three that never touched it — a shut lid, a cart coming out and a power
    /// off all end the game this was drawn over.
    fn close_game_menu(&mut self) {
        self.game_menu = None;
        if let Some(mut starting) = self.starting.take() {
            starting.starter.cancel();
        }
    }

    /// One message a frame, which is all the worker ever has for it.
    fn poll_link(&mut self) {
        // Only while this overlay is the thing on screen. A power menu raised over it pauses
        // the core, and a session must not begin under one — the worker's message keeps in
        // its own queue until that menu is gone.
        if self.power_menu.is_some() {
            return;
        }
        let Some(mut starting) = self.starting.take() else {
            return;
        };
        match starting.starter.poll() {
            None => self.starting = Some(starting),
            Some(LinkProgress::At(step)) => {
                self.game_menu = Some(GameMenu::Working(step));
                self.starting = Some(starting);
            }
            // The overlay's job is done and the game comes back with a session live over it.
            // `begin_link` is this side's bookkeeping; the transport is left for whoever owns
            // the emulator thread to collect, since nothing here may touch a wire.
            Some(LinkProgress::Ready(link)) => {
                self.game_menu = None;
                self.begin_link(starting.client_id);
                self.link_transport = Some((starting.client_id, Box::new(link)));
            }
            // The player asked for this. Not a screen to read: straight back to the game.
            Some(LinkProgress::Failed(LinkFail::Cancelled)) => self.game_menu = None,
            Some(LinkProgress::Failed(fail)) => self.game_menu = Some(GameMenu::Failed(fail)),
        }
    }

    /// The choice, onto the card. Best effort, like every other card write here: a read only
    /// or absent card is a shelf that still works, not a boot failure. Nothing else in the
    /// app is told — `self.core` is the seated cart's, set when a core is actually spawned,
    /// and the shelf has none seated.
    fn write_core(&self, core: Core) {
        let (Some(root), Some(cart)) = (self.root.clone(), self.row().current_cart()) else {
            return;
        };
        if let Err(e) = slot_store::write_selected_core(&root, &cart.stem, core) {
            eprintln!("slot: core: could not write selected_core.ini: {e}");
        }
    }

    /// Every path to shutdown — a held button, an idle doze timing out, and a critical
    /// battery reading that needs no button at all — funnels through here, so none of them
    /// leaves the LED reporting Running or Charging through a shutdown the user is not
    /// watching finish. A real behaviour on a handheld: the case still has a light on it for
    /// as long as `poweroff` takes to actually cut power.
    fn begin_power_off(&mut self) {
        // Idempotent, and that is the whole of why: `doze_expired` is a level rather than an
        // edge and this leaves the phase on `Doze`, so `timers` calls back here every frame
        // for as long as the lid is shut. Re-arming `act_at` each time walked the deadline
        // ahead of the clock forever, and the device sat dark and awake until the lid opened
        // and took the phase out of `Doze` — at which point it powered off in the user's
        // hands, on the frame they came back to the session.
        if self.powering_off {
            return;
        }
        // A power-off pauses the core outright (`shutting_down()`, of which this is the
        // start, is one of the states `Session::sync_speed` maps to `Speed::Paused`) —
        // libretro's netpacket contract forbids that for as long as a session is live, the
        // same hazard `doze`'s own guard exists for. `doze` and the power menu's own open
        // already end a session before either of their own routes reaches here, which is
        // why this was previously always false in practice by the time any caller arrived —
        // right up until a critical battery reading turned out to be a fifth route in, with
        // no button, no menu and no doze anywhere upstream of it to have ended one first.
        // Guarding the chokepoint itself, rather than that one caller, is what keeps a sixth
        // route from reopening the same hole: whatever calls `begin_power_off` next inherits
        // this for free.
        if self.link_active() {
            self.end_link();
        }
        self.close_game_menu();
        self.close_cheat_menu();
        self.close_palette_browser();
        self.powering_off = true;
        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
        self.set_led(LedState::Off);
    }

    /// The gauge, polled from `timers` and injected by the tests. Only a charge state the
    /// device positively asserted suppresses the cutoff: unknown and discharging both power
    /// off at the threshold, which is what the frontend did before it could read one.
    pub fn on_battery(&mut self, b: Battery) {
        if b.percent > BATTERY_CRITICAL || self.powering_off {
            return;
        }
        if matches!(b.charge, Charge::Charging | Charge::Full) {
            return;
        }
        // A real power off, not a sleep. This is the one shutdown the user did not ask for,
        // and suspending a cell this empty only spends what is left of it more slowly.
        self.flush_resume();
        self.begin_power_off();
    }

    fn doze_expired(&self) -> bool {
        // Suspended for as long as a session is live: a trade partner reading a menu on the
        // other device must not have the link dropped out from under them by this one's own
        // idle timer.
        if self.link_active() {
            return false;
        }
        let (Phase::Doze { .. }, Some(power)) = (&self.phase, &self.power) else {
            return false;
        };
        // ★ 「仅息屏」: the panel is dark and the cart was flushed, but the machine is going
        // nowhere. A `Power` with no timer is a level rather than an edge — it stays true for
        // as long as the lid is shut, and `wake` is the only thing that ends it.
        let Some(timeout) = power.timeout() else {
            return false;
        };
        // ★ 2026-11-02（六六）：「放音乐时屏蔽超时关机」——随身听正在出声时合盖不是
        //   「睡觉」而是「后台放歌」，机器得继续转。判据是解码线程的实时 playing，
        //   **不是相位里的快照**：一首歌自然放完（`finished`）时快照还挂着 true，
        //   按快照判就会 lid 底下空转 400-700 mA 到天荒地老，那正是超时机制
        //   要防的东西；实时标志在那一刻已经 false，计时照常走。暂停/停止时合盖
        //   同理不屏蔽——没有声音就没有"后台播放"这回事。
        if self.deck.playing() {
            return false;
        }
        self.now().saturating_sub(self.dozed_at) >= timeout.as_millis() as Millis
    }

    /// resume.state and the battery save, with the slot left alone. A cart that is not
    /// playing has no state of its own to write.
    fn flush_resume(&mut self) {
        // The invariant is 60 s since the state was last durable, not 60 s since the last
        // autosave, so an attempt that had nothing to write still moves the deadline.
        self.autosave_at = self.now() + AUTOSAVE_MS;
        let (Some(root), Some(snapshot), Some(cart)) = (&self.root, &self.snapshot, self.seated())
        else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: flush: the core gave up no state");
            return;
        };
        let Some(system) = self.system_of(cart) else {
            eprintln!("slot: flush: no cart in the shelf for {cart}");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "flush");
        if let Err(e) = persist::flush(
            root,
            self.core,
            system,
            cart,
            state.as_deref(),
            sav.as_deref(),
        ) {
            eprintln!("slot: flush: {e}");
        }
    }

    /// The ring for the cart in the slot. `None` outside the binary, where there is no
    /// content root, which reads as a cart that has never been saved.
    /// The machine of the cart that owns `stem` — the folder its save and its states live under.
    /// Read from the shelf rather than kept on `App`, because the shelf already carries every
    /// cart's machine and a second copy is exactly the thing that drifts.
    ///
    /// ★ 2026-11-09：走 `cart_by_stem`（当前机种优先）。从前这里是全库 `find` —— 同名跨机种时
    ///   存档 / 即时存档目录会被指到另一台机器的同名卡上，与"进去也是 FC"同一个根因。
    fn system_of(&self, stem: &str) -> Option<System> {
        self.cart_by_stem(stem).map(|c| c.system())
    }

    fn ring(&self) -> Option<StateRing> {
        let (Some(root), Some(cart)) = (&self.root, self.seated()) else {
            return None;
        };
        let system = self.system_of(cart)?;
        Some(StateRing::new(root, self.core, system, cart))
    }

    /// ★ 2026-10-20：**槽位存档**，同一个目录、同一台核心，只是按 `Auto` + `01`–`99` 命名
    ///   （`slot_store::slots`）。菜单里那两张槽位表读写的是这个；`ring` 只剩 `resume`
    ///   和"把老存档交给槽位表认领"这两件事。
    fn slots(&self) -> Option<StateSlots> {
        let (Some(root), Some(cart)) = (&self.root, self.seated()) else {
            return None;
        };
        let system = self.system_of(cart)?;
        Some(StateSlots::new(root, self.core, system, cart))
    }

    fn seated(&self) -> Option<&str> {
        match &self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(cart),
            _ => None,
        }
    }

    fn entries(&self) -> Vec<StateEntry> {
        self.ring().and_then(|r| r.list().ok()).unwrap_or_default()
    }

    /// An empty ring shakes rather than opening an empty screen, per spec section 4.
    fn open_polaroids(&mut self) {
        // Opening the switcher pauses the core — `Session::sync_speed` maps
        // `Phase::Polaroids` straight to `Speed::Paused` — one of the exact manipulations
        // libretro's netpacket contract forbids while a session is live, whether or not the
        // player means to load anything once inside. Checked ahead of even looking for
        // states to show, the same way `load_newest` already checked ahead of looking for
        // one to load.
        if self.link_active() {
            return self.refuse();
        }
        let entries = self.entries();
        if entries.is_empty() {
            return self.refuse();
        }
        let Phase::Playing { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        let mut p = Polaroids::new(entries);
        p.set_undo(self.undo_label());
        self.polaroids = Some(p);
        self.phase = Phase::Polaroids { cart };
        self.push_hint_faces();
    }

    fn close_polaroids(&mut self) {
        let Phase::Polaroids { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.polaroids = None;
        self.phase = Phase::Playing { cart };
    }

    fn load_selected(&mut self) {
        let state = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.state.clone());
        // `None` means nothing was selected, not a refusal, and still closes exactly as
        // before. `Some(false)` means `load_file` refused (a live session, most reachably —
        // see its own doc comment) and already shook the screen for it; closing the switcher
        // on top of that shake would read as the pick landing and then being dismissed, when
        // nothing happened at all. Unreachable today, since `open_polaroids` already refuses
        // to open a switcher a session forbids picking from — but wrong the moment that guard
        // moves, and cheap to keep correct regardless of where it lives.
        let refused = state.map(|state| self.load_file(&state)) == Some(false);
        if !refused {
            self.close_polaroids();
        }
    }

    /// Not undoable, and deliberately so. The undo slot holds one save or one load, and a
    /// third kind in it would be an undo whose meaning depended on what you did last. A state
    /// chosen off a screen showing you exactly which one is a decision, not a slip.
    fn delete_selected(&mut self) {
        let stamp = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.clone());
        let (Some(stamp), Some(ring)) = (stamp, self.ring()) else {
            return;
        };
        if let Err(e) = ring.remove(&stamp) {
            eprintln!("slot: delete: {e}");
            return;
        }
        // An offer left pointing at a file that is gone would remove nothing and then put the
        // evicted entry back, which is not what undoing that save means any more.
        if self.undo_targets(&stamp) {
            self.pending = None;
        }
        let Some(p) = &mut self.polaroids else {
            return;
        };
        p.remove_selected();
        if p.is_empty() {
            self.close_polaroids();
        }
    }

    fn undo_targets(&self, stamp: &str) -> bool {
        match self.pending.as_ref() {
            Some((PendingUndo::Save { stamp: pending, .. }, _)) => pending == stamp,
            // A load's undo holds the prior state in memory, so no file on the card can
            // invalidate it.
            _ => false,
        }
    }

    fn load_newest(&mut self) {
        let Some(newest) = self.entries().first().map(|e| e.state.clone()) else {
            return self.refuse();
        };
        self.load_file(&newest);
    }

    /// The chokepoint every load-from-disk route funnels through — `load_newest` above and
    /// `load_selected` alike — so the session guard lives here once rather than at each
    /// caller. That used to be `load_newest`'s own job, checked ahead of even looking for a
    /// state to load; `load_selected` never got the same check, which is what let the
    /// switcher's own A-button pick bypass it entirely. Guarding here instead closes that
    /// hole for both today's callers and whatever the next one turns out to be.
    /// Reports whether the load actually happened, so a caller that only means to load —
    /// `load_newest` — can ignore it, and one that has something else riding on the answer —
    /// `load_selected`, which must not close the switcher out from under a refusal it just
    /// drew — can ask rather than repeating the guard above for itself.
    fn load_file(&mut self, state: &Path) -> bool {
        // A state load would desynchronise the other device with no way back to agreement.
        if !self.may_load_state() {
            self.refuse();
            return false;
        }
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let bytes = match std::fs::read(state) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("slot: load: {e}");
                return false;
            }
        };
        // Taken before the load, which is the last moment there is anything to go back to.
        let prior = snapshot.state();
        snapshot.load(bytes);
        self.hud.toast(Toast::StateLoaded, self.now());
        // ★ 2026-10-27：读档也是"闪存动了一下"，和存档同一条（**在这里发一次**，
        //   `load_newest` / 槽位表 / Polaroids 三条路都经过这个 chokepoint）。
        self.sfx = Some(Sfx::Save);
        if let Some(prior) = prior {
            self.pending = Some((PendingUndo::Load { prior }, self.now()));
        }
        true
    }

    /// `SELECT+A` and nothing else reaches here. A state with no picture is still worth
    /// keeping: the switcher draws a blank card rather than losing the save.
    ///
    /// Declines outright when the live core refused the resume it was opened with — the same
    /// condition `trusted_write` withholds from `flush`/`eject` for. This is the one durable
    /// sink that guard does not reach, because it is not a write-back over an existing file:
    /// `ring.push` is a deliberate ring buffer, and once it holds `RING_MAX` entries, pushing
    /// an eleventh evicts the oldest to make room. A core running on its own default machine
    /// has nothing worth keeping in that slot, so pushing it would not just waste an entry —
    /// it would delete a real one to make room for a placeholder. Refused the same way every
    /// other "nothing to do here" action in this file is, via `refuse()`: the player gets the
    /// same shake `load_newest`/`open_polaroids` already answer with, rather than a save that
    /// silently did not happen.
    fn save_state(&mut self) {
        let (Some(ring), Some(snapshot)) = (self.ring(), &self.snapshot) else {
            return;
        };
        if !snapshot.resume_trusted() {
            eprintln!("slot: save: the core refused the resume it was given, not pushing a state");
            return self.refuse();
        }
        let Some(state) = snapshot.state() else {
            eprintln!("slot: save: the core gave up no state");
            return;
        };
        let thumb = snapshot.thumb().unwrap_or_default();
        let stamp = free_stamp(&ring, self.wall_secs());
        let evicted = doomed(&ring);
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: save: {e}");
            return;
        }
        self.hud.toast(Toast::StateSaved, self.now());
        // ★ 2026-10-27：同上，`ring.push` 成功了才有这一声。
        self.sfx = Some(Sfx::Save);
        self.pending = Some((PendingUndo::Save { stamp, evicted }, self.now()));
    }

    pub fn undo_available(&self, now: Millis) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(_, at)| now.saturating_sub(*at) <= UNDO_GRACE_MS)
    }

    /// What the offer says, or `None` when there is nothing on offer. The binary rasterises
    /// it; the grace period is read off the app's own clock so the two cannot disagree.
    pub fn undo_label(&self) -> Option<&'static str> {
        if !self.undo_available(self.now()) {
            return None;
        }
        match self.pending.as_ref()?.0 {
            PendingUndo::Save { .. } => Some(lang::UNDO_SAVE),
            PendingUndo::Load { .. } => Some(lang::UNDO_LOAD),
        }
    }

    /// In `LEGEND` order, and uploaded once: none of the three ever changes what it says.
    pub fn set_legend_faces(&mut self, faces: Vec<TexId>) {
        self.legend_faces = faces;
        self.push_hint_faces();
    }

    pub fn set_undo_face(&mut self, face: Option<TexId>) {
        self.undo_face = face;
        self.push_hint_faces();
    }

    /// The undo goes last because `hints` puts it last, which is what keeps faces and hints
    /// on the same index.
    fn push_hint_faces(&mut self) {
        let mut faces = self.legend_faces.clone();
        faces.extend(self.undo_face);
        if let Some(p) = &mut self.polaroids {
            p.set_hint_faces(faces);
        }
    }

    /// The mark a favourite wears, and the corner indicator on the ordinary shelf and in the
    /// favourites, each with the size it was rasterised at. Uploaded at boot: none of them changes.
    pub fn set_fav_faces(
        &mut self,
        star: (TexId, u32, u32),
        ind_off: (TexId, u32, u32),
        ind_on: (TexId, u32, u32),
    ) {
        self.fav_star = Some(star);
        self.fav_ind = Some((ind_off, ind_on));
    }

    /// The emoji atlas, if the card has one and any row asks for a glyph out of it. `None`
    /// is the ordinary answer and means every badge on the card goes undrawn.
    pub fn set_badge_atlas(&mut self, atlas: Option<(TexId, u32, u32)>) {
        self.badge_atlas = atlas;
    }

    /// The HUD glyphs, in `Icon::ALL` order. Uploaded once: they never change.
    pub fn set_icon_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_icons(faces);
    }

    /// The two lines the HUD can say, in `Toast::ALL` order.
    pub fn set_toast_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_toasts(faces);
    }

    /// What the HUD is saying, or `None` once it has faded. Only ever set by an action that
    /// happened: a refusal shakes instead.
    pub fn toast(&self) -> Option<Toast> {
        self.hud.said(self.now())
    }

    /// Show a toast from outside `App` — the in-game cheat toggle, which the session drives
    /// because the core it reaches lives on the emulator thread. `now` is read off the app's own
    /// clock so the fade lines up with everything else the HUD says.
    pub fn toast_now(&mut self, t: Toast) {
        self.hud.toast(t, self.now());
    }

    /// One shot, and it hands the game back the way loading does. Undoing an undo would be a
    /// redo, and the switcher is not a place to sit and shuffle.
    pub fn undo(&mut self, now: Millis) {
        if !self.undo_available(now) {
            self.pending = None;
            return;
        }
        // Undoing a load moves the core to a moment the peer never agreed to — the exact
        // hazard `load_file` guards against, and the one route into it that never passes
        // through `load_file` at all: the bytes are already in hand from when the load
        // happened, not read fresh off disk. Refused without consuming the offer, the same
        // way a refused rewind or state load leaves the player able to try again once the
        // session that refused it is gone — an undo's own save-file cleanup, `undo_save`
        // below, touches no core state at all, so only this arm needs the check.
        if matches!(&self.pending, Some((PendingUndo::Load { .. }, _))) && !self.may_load_state() {
            return self.refuse();
        }
        let Some((what, _)) = self.pending.take() else {
            return;
        };
        match what {
            PendingUndo::Save { stamp, evicted } => self.undo_save(&stamp, evicted),
            PendingUndo::Load { prior } => {
                if let Some(snapshot) = &self.snapshot {
                    snapshot.load(prior);
                }
            }
        }
        self.close_polaroids();
    }

    fn undo_save(&self, stamp: &str, evicted: Option<(String, Vec<u8>, Vec<u8>)>) {
        let Some(ring) = self.ring() else {
            return;
        };
        if let Err(e) = ring.remove(stamp) {
            eprintln!("slot: undo: {e}");
            return;
        }
        let Some((stamp, state, thumb)) = evicted else {
            return;
        };
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: undo: {e}");
        }
    }

    /// Entries in the switcher's order, newest first. The binary reads these to build the
    /// faces, since only the compositor can mint a `TexId`.
    pub fn polaroid_entries(&self) -> &[StateEntry] {
        match &self.polaroids {
            Some(p) => &p.entries,
            None => &[],
        }
    }

    pub fn set_polaroid_faces(&mut self, faces: Vec<TexId>) {
        if let Some(p) = &mut self.polaroids {
            p.set_faces(faces);
        }
    }

    /// Which entry is under the eye. The binary watches this to know when the title has to be
    /// rasterised again. The stamp rather than the index, because a delete leaves the index
    /// where it was and moves a different entry under it.
    pub fn polaroid_stamp(&self) -> Option<&str> {
        self.polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.as_str())
    }

    /// What the top plate says. `now` is a stamp rather than the app's clock: the entries
    /// are named by their filenames and the title is relative to the wall clock.
    pub fn polaroid_title(&self, now: &str) -> String {
        self.polaroids
            .as_ref()
            .map_or_else(String::new, |p| p.title(now))
    }

    pub fn set_polaroid_title_face(&mut self, face: TexId) {
        if let Some(p) = &mut self.polaroids {
            p.set_title_face(Some(face));
        }
    }
}

/// The write-back half of the guard `EmuSnapshot` records. `state` is the bytes the live core
/// actually holds; `snapshot.resume_trusted()`/`save_ram_trusted()` say whether the core that
/// produced them actually accepted the resume/save-ram it was opened with. A region it
/// refused is withheld here — turned into `None` rather than passed on to `persist::flush`/
/// `eject` — because a core running with its own default state has nothing worth writing back
/// over the file that refusal left alone. `verb` names the caller only for the log line
/// ("flush" or "eject"), so a withheld region reads the same as everything else either one
/// already prints.
fn trusted_write(
    snapshot: &dyn Snapshot,
    state: Vec<u8>,
    verb: &str,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let state = if snapshot.resume_trusted() {
        Some(state)
    } else {
        eprintln!(
            "slot: {verb}: the core refused the resume it was given, not overwriting the saved one"
        );
        None
    };
    let sav = snapshot.save_ram();
    let sav = if snapshot.save_ram_trusted() {
        sav
    } else {
        if sav.is_some() {
            eprintln!(
                "slot: {verb}: the core refused the save ram it was given, not overwriting the saved one"
            );
        }
        None
    };
    (state, sav)
}

fn up(level: u8, step: u8, max: u8) -> u8 {
    level.saturating_add(step).min(max)
}

/// The host's own clock, which is all there is before `set_power` hands over the device's.
fn system_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The entry the next push will evict, read out while it is still there. `None` until the
/// ring is full, which is where most of a cart's life is spent.
fn doomed(ring: &StateRing) -> Option<(String, Vec<u8>, Vec<u8>)> {
    let entries = ring.list().ok()?;
    let oldest = entries.get(RING_MAX - 1)?;
    let (state, thumb) = ring.read(&oldest.stamp).ok()?;
    Some((oldest.stamp.clone(), state, thumb))
}

/// The stamp is the filename, so two saves inside one second would be one save. The second
/// one moves on by a second, which keeps the ring in order without a finer format that the
/// polaroid captions would then have to read.
fn free_stamp(ring: &StateRing, now: i64) -> String {
    let taken: Vec<String> = ring
        .list()
        .map(|l| l.into_iter().map(|e| e.stamp).collect())
        .unwrap_or_default();
    // Local, from the same wall clock the captions are read against. A stamp in utc would
    // name every state an hour or several from the time the polaroid says it was taken.
    let mut secs = now;
    let mut stamp = format_stamp(secs);
    while taken.contains(&stamp) {
        secs += 1;
        stamp = format_stamp(secs);
    }
    stamp
}

/// Where a block of `rows` menu rows starts, so it sits in the middle of the panel.
fn centred_top(rows: usize, pitch: f32) -> f32 {
    (OUT_H as f32 - pitch * rows as f32) / 2.0
}

/// ★ 2026-10-21：**网格里走一步**。`drow`/`dcol` 只有一个非零（方向键一次只按一个方向）。
///
/// * **左右**在同一条网格行里换格，到头就停（不环绕）—— 一排的尽头就是尽头，越过去回到
///   另一头只会让人怀疑自己按错了方向。
/// * **上下**换网格行，**环绕**：第一排再往上落到最后一排，最后一排再往下回到第一排。
///   和单列时"上下环绕"是同一个手感，只是步子从"一行"变成"一排"。
///
/// 最后一行通常不满（九项两列 ⇒ 第五排只有一格），所以落点统一 `min(len-1)` 夹一下，
/// 免得算出一个不存在的下标。
fn grid_step(row: usize, len: usize, cols: usize, drow: isize, dcol: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let cols = cols.max(1);
    let grid_rows = len.div_ceil(cols);
    let cur_r = row / cols;
    let cur_c = row % cols;
    if dcol != 0 {
        let c = (cur_c as isize + dcol).clamp(0, cols as isize - 1) as usize;
        return (cur_r * cols + c).min(len - 1);
    }
    let r = (cur_r as isize + drow).rem_euclid(grid_rows as isize) as usize;
    (r * cols + cur_c).min(len - 1)
}


/// One face out of a list, as the one-row list the row drawer takes. `None` for an index
/// that has no face, which is a screen with nothing to say rather than a blank panel with a
/// highlight on it.
fn one(faces: &[(TexId, u32, u32)], index: Option<usize>) -> Vec<(TexId, u32, u32)> {
    index
        .and_then(|i| faces.get(i))
        .copied()
        .into_iter()
        .collect()
}

/// The rows of a menu, at the menu pitch from `top`, with a bar behind the one in hand and
/// none at all when nothing is. Shared by all three menus on the device: the power menu, the
/// core picker and the in-game menu are the same object, and a device this small has no
/// business carrying three copies of the loop that draws one.
///
/// The bar is `edge` — the lightest thing in the theme — because it has to read at a glance.
/// `recess` was tried first and is the right idea and the wrong value: it and `housing` are
/// adjacent dark greys by design, which is correct for a slot you look into and far too quiet
/// for a selection.
///
/// A rect rather than a second face per row: the labels are rastered once at boot and never
/// again, and a device about to lose its GPU is not the place to be uploading textures.
///
/// ★ 2026-10-21：一行菜单「被选中」的两种画法。
///
/// 工具箱那一版 UI 的结论是：**实心底只留给整屏唯一那个主操作，其余一律幽灵描边。**
/// 一行菜单不是主操作 —— 它是列表里"手里拿着的那一条"，所以它该描边，不该是实心色块。
///
/// 供电菜单和主菜单仍是 `Fill`（它们的行是一块块实体按键，不是列表），
/// 游戏菜单换 `Ghost`：它盖在暂停的画面上，实心色块在那里读起来像一块补丁。
#[derive(Copy, Clone, PartialEq)]
#[allow(dead_code)] // `Ghost` 是游戏菜单 WIP（2026-10-21）预留的画法，尚未接线。
enum MenuBar {
    /// 旧的实心底条。
    Fill,
    /// 1px 琥珀发丝边（`slot_ui::light_hairline`）。
    Ghost,
    /// ★ 2026-10-29（六六）：**恒定色**实心底条（RGBA），不随主题/明暗。电源菜单选中行用
    /// 半透明白压在这张固定背景图上。
    Constant([f32; 4]),
}

fn draw_menu_rows(
    faces: &[(TexId, u32, u32)],
    index: Option<usize>,
    top: f32,
    pitch: f32,
    bar: MenuBar,
    out: &mut Vec<Draw>,
) {
    for (row, (tex, w, h)) in faces.iter().copied().enumerate() {
        let y = top + pitch * row as f32;
        let x = ((OUT_W as f32 - w as f32) / 2.0).round();
        if index == Some(row) {
            let (by, bh) = (y + POWER_MENU_BAR_INSET, pitch - 2.0 * POWER_MENU_BAR_INSET);
            match bar {
                MenuBar::Fill => out.push(Draw::Rect {
                    x,
                    y: by,
                    w: w as f32,
                    h: bh,
                    colour: slot_ui::edge(),
                }),
                // ★ 2026-10-29（六六）：恒定色条（电源菜单）。RGBA 写死，不随主题/明暗。
                MenuBar::Constant(colour) => out.push(Draw::Rect {
                    x,
                    y: by,
                    w: w as f32,
                    h: bh,
                    colour,
                }),
                // 发丝边往里缩 4 px：贴着字的包围盒画框，字会显得被挤住；
                // 缩进之后框是"托着"这一行，不是"框住"这行字。
                MenuBar::Ghost => slot_ui::light_hairline(
                    out,
                    x - 4.0,
                    by,
                    w as f32 + 8.0,
                    bh,
                    0.55,
                ),
            }
        }
        out.push(Draw::Tex {
            x,
            y: y + (POWER_MENU_PITCH - h as f32) / 2.0,
            w: w as f32,
            h: h as f32,
            tex,
            alpha: 1.0,
        });
    }
}

#[cfg(test)]
mod cheat_tests {
    use super::*;
    use crate::root::CheatEntry;

    /// A list whose rows carry the given on/off, so a test can talk about what is live without
    /// going through a card and a boot.
    fn items(flags: &[bool]) -> Vec<CheatEntry> {
        flags
            .iter()
            .enumerate()
            .map(|(i, on)| CheatEntry {
                code: format!("AAAAAAAA+000{i}"),
                desc: format!("code {i}"),
                enabled: *on,
            })
            .collect()
    }

    /// `App` without a card behind it. `set_cheats` and `live_cheats` touch nothing but the two
    /// fields, so a boot into a temp root would only be noise.
    fn bare() -> App {
        let d = tempfile::tempdir().expect("tempdir");
        for sub in crate::root::DIRS {
            std::fs::create_dir(d.path().join(sub)).expect("create dir");
        }
        App::boot(d.path())
    }

    #[test]
    fn native_suspend_transfers_backlight_ownership_to_baseos() {
        let mut a = bare();
        a.baseos_sleep = None;
        assert!(a.frontend_owns_backlight(), "generic frontend owns its lid backlight");
        a.baseos_sleep = Some(std::path::PathBuf::from("/usr/sbin/rg353ps-sleep"));
        a.native_suspend_active = true;
        assert!(!a.frontend_owns_backlight(), "active BaseOS sleep helper owns backlight blank/restore");
        a.native_suspend_active = false;
        assert!(a.frontend_owns_backlight(), "music display-off remains frontend-owned even with the helper installed");
    }

    #[test]
    fn music_playback_keeps_the_power_press_out_of_native_suspend() {
        assert!(!App::should_use_native_suspend(false, false));
        assert!(!App::should_use_native_suspend(false, true));
        assert!(!App::should_use_native_suspend(true, true));
        assert!(App::should_use_native_suspend(true, false));
    }

    /// ★ 2026-10-31：**这是这次改动真正的行为**。mGBA 的 `retro_cheat_set` 是
    /// `UNUSED(enabled)` —— 把一条 `false` 发过去，它照样应用。所以「关掉」只有在**不发送**
    /// 的时候才成立，`live_cheats` 是唯一做这件事的地方。
    #[test]
    fn a_code_switched_off_is_not_sent_at_all() {
        let mut a = bare();
        a.set_cheats(items(&[true, false, true]));
        let live = a.live_cheats();
        assert_eq!(live.len(), 2, "the off code must not be in the payload");
        assert_eq!(live[0].1, "AAAAAAAA+0000");
        assert_eq!(live[1].1, "AAAAAAAA+0002");
        assert!(
            live.iter().all(|(on, _)| *on),
            "every code that is sent is sent switched on — that is all a core like mGBA reads"
        );
    }

    /// A file that switched nothing on sends nothing. This is 「默认关」: a `.cht` with no
    /// `cheatN_enable = true` in it means a cart that boots clean.
    #[test]
    fn a_file_with_nothing_on_sends_an_empty_list() {
        let mut a = bare();
        a.set_cheats(items(&[false, false]));
        assert!(a.live_cheats().is_empty());
    }

    /// The master switch still gates the lot, and it gates on top of each row rather than
    /// replacing it: switching the master back on returns exactly the codes that were on.
    #[test]
    fn the_master_switch_gates_every_row() {
        let mut a = bare();
        a.set_cheats(items(&[true, false]));
        assert_eq!(a.live_cheats().len(), 1);
        a.toggle_all_cheats();
        assert!(a.live_cheats().is_empty(), "master off means nothing is sent");
        a.toggle_all_cheats();
        assert_eq!(
            a.live_cheats().len(),
            1,
            "and back on, the rows are where they were"
        );
    }

    /// Flipping a row in the table is what a player does mid-game; the row decides from then on,
    /// not the card.
    #[test]
    fn a_row_flipped_in_the_table_changes_the_payload() {
        let mut a = bare();
        a.set_cheats(items(&[false]));
        assert!(a.live_cheats().is_empty());
        a.open_cheat_menu();
        a.cheat_menu_input(Action::GbaDown(Btn::A));
        assert_eq!(a.live_cheats().len(), 1);
        assert!(a.take_cheats_dirty(), "and the session was told to re-push");
        a.cheat_menu_input(Action::GbaDown(Btn::A));
        assert!(a.live_cheats().is_empty(), "flipped back off again");
    }

    /// A cart with no file still gets a table that opens — on the NO CHEATS panel, with the
    /// payload empty rather than stale codes from the cart before it.
    #[test]
    fn a_new_cart_with_no_cheats_clears_the_payload() {
        let mut a = bare();
        a.set_cheats(items(&[true]));
        assert_eq!(a.live_cheats().len(), 1);
        a.set_cheats(Vec::new());
        assert!(a.live_cheats().is_empty());
    }
}

/// ★ 2026-11-03（六六）：「字体有点小」—— Wi-Fi 配置页与直连提示的整页文字从状态行那档
/// （`word_face`，24px / 20 高的带子）换成了 `page_face`（32px / **44** 高的带子），
/// 行距跟着从 34 走到 48。这一页的纵位全是常量，放大后最可能出的事是**字块互相压**或
/// **脚注滑出屏外**；而 `text::coverage` 对超出带子的墨是"丢掉"不是"缩小"，所以屏幕上
/// 真正被吃掉的就是"每个字块占掉的那一段"，量它即可。
///
/// 这些断言钉的是**排版不打架**，不是像素好看：字号再调大时，这条会先红，提醒把行距、
/// 行数、脚注位一起重算，而不是等真机上字压在一起才发现。
#[cfg(test)]
mod wifi_layout_tests {
    use super::*;
    use slot_ui::PAGE_H;

    fn bottom(y: f32) -> f32 {
        y + PAGE_H as f32
    }

    /// 标题、六行正文（列表视图最满的一屏）、脚注：四段依次落进屏里，且上下不相压。
    #[test]
    fn the_wifi_page_stacks_without_overlap() {
        assert!(
            bottom(WIFI_TITLE_Y) <= WIFI_BODY_Y,
            "title {}..{} runs into the body at {}",
            WIFI_TITLE_Y,
            bottom(WIFI_TITLE_Y),
            WIFI_BODY_Y
        );
        let last_row = WIFI_BODY_Y
            + WIFI_PITCH
            + (WIFI_LIST_MAX.saturating_sub(1)) as f32 * WIFI_PITCH;
        assert!(
            bottom(last_row) <= WIFI_FOOT_Y,
            "the last of {WIFI_LIST_MAX} rows ends at {}, past the foot at {WIFI_FOOT_Y}",
            bottom(last_row)
        );
        assert!(
            bottom(WIFI_FOOT_Y) <= OUT_H as f32,
            "the foot {}..{} is off the {} px screen",
            WIFI_FOOT_Y,
            bottom(WIFI_FOOT_Y),
            OUT_H
        );
    }

    /// 状态视图：当前网络、SSID、三个动作、外加那行「已保存」的注脚，都排在脚注之上。
    #[test]
    fn the_status_view_leaves_the_foot_alone() {
        // 注脚落在 `WIFI_BODY_Y + WIFI_PITCH * 6`，比三个动作再低三行。
        let note = WIFI_BODY_Y + WIFI_PITCH * 6.0;
        assert!(
            bottom(note) <= WIFI_FOOT_Y,
            "the saved-note at {note} runs under the foot at {WIFI_FOOT_Y}"
        );
    }

    /// 打字页自己有四个纵位（标签 / 输进去的串 / 密码提示 / 网格），这一页不借 `WIFI_PITCH`
    /// —— 它被网格上沿夹住，48 的行距塞不进去。这里量的是那四段接着排、谁也不压谁。
    #[test]
    fn the_typing_view_fits_above_the_grid() {
        assert!(
            bottom(WIFI_EDIT_Y) <= WIFI_EDIT_TEXT_Y,
            "the field's label runs into the string under it"
        );
        // 输进去的串是二十高的字形（`wifi_glyph` 还是 `word_face` 那一档），不是 `PAGE_H`。
        assert!(
            WIFI_EDIT_TEXT_Y + slot_ui::STATUS_H as f32 <= WIFI_EDIT_NOTE_Y,
            "the typed string runs into the open-network note"
        );
        assert!(
            bottom(WIFI_EDIT_NOTE_Y) <= WIFI_GRID_Y,
            "the open-network note runs into the grid"
        );
        let rows = WIFI_LAYERS.iter().map(|l| l.rows).max().unwrap_or(0) as f32;
        let grid_bottom = WIFI_GRID_Y + rows * (WIFI_CELL + WIFI_CELL_GAP) - WIFI_CELL_GAP;
        assert!(
            grid_bottom <= WIFI_FOOT_Y,
            "the grid ends at {grid_bottom}, under the foot at {WIFI_FOOT_Y}"
        );
    }
}
