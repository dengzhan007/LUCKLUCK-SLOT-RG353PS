//! Every word this frontend says, in one place, twice.
//!
//! The fork began as a translation: upstream's English was replaced with Chinese, line by line, in
//! whichever file each line lived in. That was the right shape for one language and the wrong shape
//! for two — with the strings scattered through ten files there is nothing to keep in step, and a
//! change made in one language is invisible in the other until someone sees it on a screen. So the
//! words are gathered here, where the two languages sit as two columns of the same list.
//!
//! **Two tables chosen at compile time, not one table chosen at runtime.** A running switch would
//! put both languages in every binary, and for the English build that is the one thing it must not
//! be: the point of that build is to be free of the other language. It would also mean every
//! release carries whichever language it is not.
//!
//! Chinese is the default, so the build that ships today is the build that shipped yesterday.
//! English is `--features lang-en`.
//!
//! **Where the English came from.** Most of it is not a translation. Upstream is an English program
//! and the fork replaced its words, so the original wording is recoverable from the commit the fork
//! was taken from — and it has been recovered phrase by phrase rather than reinvented: "State
//! Saved", "Power Off", "Restarting", "Bringing the radio up", "Back / Delete / Load" are all
//! upstream's own. What is genuinely new is what the fork added — the toasts for the cheat toggle
//! and the three audio profiles, the clock hint — and the shortcut card, which lives in
//! `shortcuts.rs` because its words are inseparable from its layout.
//!
//! Two things are deliberately *not* here. A key's own name ("A", "SELECT") is a button rather than
//! a word and never changes with the language. And a string the game supplies — a cart's title, a
//! cheat's description — belongs to the card, not to this build.

#[cfg(feature = "lang-en")]
mod words {
    /// The one key the clock screen names, and what pressing it does. The key is a button rather
    /// than a word, but it lives beside its label: a column with a gap in it is a column someone
    /// will fill wrongly.
    pub const SET_CLOCK_LABEL: &str = "set the clock";

    pub const TOAST_STATE_SAVED: &str = "State Saved";
    pub const TOAST_STATE_LOADED: &str = "State Loaded";
    pub const TOAST_CHEATS_ON: &str = "Cheats On";
    pub const TOAST_CHEATS_OFF: &str = "Cheats Off";
    pub const TOAST_AUDIO_STABLE: &str = "Audio: Stable";
    pub const TOAST_AUDIO_BALANCED: &str = "Audio: Balanced";
    pub const TOAST_AUDIO_STRICT: &str = "Audio: Strict";
    /// 慢放（L2 双击）开/关。半速的画面和全速的画面一眼看不出区别，所以这两句是那颗键
    ///   唯一的回执。
    pub const TOAST_SLOW_ON: &str = "Slow Motion: On";
    pub const TOAST_SLOW_OFF: &str = "Slow Motion: Off";
    /// A custom image or the generated BaseOS default was selected. The boot artwork changes
    /// on the next startup, not immediately in the running frontend.
    pub const TOAST_BOOT_SET: &str = "Boot screen set — next power-on";
    /// It was not written, and the row went back to the picture it was on. One line rather than
    /// seven: which of the ways it can fail is in `System/boot.err`, and a device in the hand
    /// only needs to know to look at the card.
    pub const TOAST_BOOT_FAIL: &str = "Boot screen not changed";

    pub const POWER_RESTART: &str = "Restart";
    pub const POWER_OFF: &str = "Power Off";
    pub const SHUTDOWN_RESTART: &str = "Restarting";
    pub const SHUTDOWN_POWER_OFF: &str = "Powering Down";

    pub const SWITCHER_LEGEND: [(&str, &str); 3] = [("B", "Back"), ("Y", "Delete"), ("A", "Load")];

    pub const CORE_CANCEL: &str = "Cancel";
    pub const CORE_SWAP: &str = "Swap";
    pub const CORE_CHOOSE: &str = "Choose";

    /// ★ 2026-10-26（六六）：长按 Y 打开的**机种选择面板**。标题一句话，脚下一条键位：
    /// 方向键走格子、A 换到那一台、B 留在这台。（十台之后"下一台"已经不是一个问题，
    /// 见 `sys_picker` 模块头。）
    pub const SYS_PICKER_TITLE: &str = "Choose Machine";
    pub const SYS_PICKER_MOVE: &str = "Move";

    /// 游戏菜单的行：即时存档 / 即时读档（各开一张 Auto + 01–99 的槽位表）、
    /// 操作键映射、色板浏览器、网格/遮罩档位、保存设置、联机、重启模拟器。
    ///
    /// ★ 2026-10-21：按六六 10-20 晚的完整规格补四行 —— 键位映射、色板浏览器、
    /// 网格/遮罩档位、保存设置。前两行 10-20 就有了，后四行今天补上。
    pub const GAME_ROW_SAVE: &str = "Save";
    pub const GAME_ROW_LOAD: &str = "Load";
    pub const GAME_ROW_SHORTCUTS: &str = "Key map";
    pub const GAME_ROW_PALETTE: &str = "Palettes";
    /// ★ 2026-10-21：**同一行两个词，跟机种走** —— GBA 切的是 LCD3x 网格与扫描线（`Screen
    /// grid`），GB/GBC 切的是遮罩艺术环（`Overlay` 那条）。由 `GameRow::text_for` 挑。
    pub const GAME_ROW_GRID: &str = "Screen grid";
    pub const GAME_ROW_OVERLAY: &str = "Overlay";
    pub const GAME_ROW_SAVE_SETTINGS: &str = "Save settings";
    /// ★ 2026-10-21：「重开本局」→ **「重启模拟器」**。它做的正是 `retro_reset` —— 模拟器
    /// 从开机那一刻重来，卡一直在槽里，机器一直开着。
    pub const GAME_ROW_RESTART: &str = "Restart core";
    /// 保存设置弹窗的四个键位：A 全局保存 / X 按游戏保存 / Y 恢复默认 / B 取消。
    pub const SAVE_SET_A: &str = "All machines";
    pub const SAVE_SET_X: &str = "This game";
    pub const SAVE_SET_Y: &str = "Defaults";
    pub const SAVE_SET_B: &str = "Cancel";
    /// 保存设置弹窗的标题。
    pub const SAVE_SET_TITLE: &str = "Save these settings?";
    /// 槽位行：`03 · empty` / `03 · 10-20 22:31`。
    pub const SLOT_EMPTY: &str = "empty";
    pub const LINK_ROW: &str = "Link";
    pub const LINK_HOST: &str = "Host";
    pub const LINK_JOIN: &str = "Join";

    pub const LINK_RADIO: &str = "Bringing the radio up";
    pub const LINK_WAITING: &str = "Looking for the other player";
    pub const LINK_FAIL_RADIO: &str = "The radio did not come up";
    pub const LINK_FAIL_NOBODY: &str = "Nobody arrived";
    pub const LINK_FAIL_PEER: &str = "The other player vanished";
    pub const LINK_FAIL_CANCELLED: &str = "Cancelled";

    pub const UNDO_SAVE: &str = "undo save";
    pub const UNDO_LOAD: &str = "undo load";

    /// The menu behind MENU. Three buttons since 2026-10-21: the two reference screens on the
    /// first row and the player across the second.
    /// 与中文列同步（2026-10-06）：那张卡是**全的**，"Shortcuts" 只说了"快捷"。
    pub const MENU_SHORTCUTS: &str = "All Keys";
    /// ★ 2026-10-06：播放器改名「像素随身听」。英文列走 **Pixel Player** 而不是 Walkman ——
    /// 那个词是索尼的商标，而这个前端是往外发的。
    pub const MENU_DISCS: &str = "Pixel Player";
    /// 2026-10-10 六六：新增设置入口（合盖关机时长 + 日期时间）。★ 同日定名**「参数调整页」/
    /// Parameters** —— 这屏调的全是"机器本身的参数"，"Settings" 太泛。
    pub const MENU_SETTINGS: &str = "Parameters";

    // ── The settings screen ──────────────────────────────────────────────────────────────────
    /// ★ 2026-10-10: the two speed rows. **Named for the key they adjust rather than for the
    /// mechanism** — the player holds R2 or L2, and this is how fast that goes. The value
    /// beside it is the multiplier, so the name must not repeat it.
    /// ★ 2026-10-28 六六：label 改「快进倍率 / 回溯倍率」—— 说的是**档位**（倍率），不是动作，
    ///   原来只写"快进/回溯"读起来像在说按 R2/L2 干什么。
    pub const SETTINGS_FF: &str = "Fast Forward Ratio";
    pub const SETTINGS_REWIND: &str = "Rewind Ratio";
    /// The unit on a speed row's values. **Latin `x`, not the multiplication sign**: `U+00D7` is
    /// not guaranteed to be in the label face, and `2x` reads the same either way.
    pub const SPEED_UNIT: &str = "x";
    /// The lid timer. Named for what the user sees happen rather than for the mechanism: the
    /// screen goes dark the moment the lid shuts, and *this* is how long the dark lasts before
    /// the machine actually stops. "Doze" is this codebase's word for it, not the player's.
    /// ★ 2026-10-10 六六：「这一项叫**息屏关机时间**」—— Chinese renamed, English follows it.
    /// ★ 2026-10-28 六六：label 改「息屏后关机」—— 原来的「息屏关机时间」读起来像"息屏
    ///   关机的时长"，而这一行说的是"息屏之后多久才关机"，词序反了。英文已是对的意思
    ///   （Power Off After Screen Off），不动。
    pub const SETTINGS_SLEEP: &str = "After Screen Off";
    pub const SETTINGS_SLEEP_SYSTEM: &str = "Deep Suspend";
    /// ★ 2026-10-16: whether fast forward and rewind keep playing the game's own audio. **A
    /// switch, like the badge** — and the first row on this screen about how the frontend
    /// *sounds* rather than about what the device does. Off is the shipped behaviour: the
    /// worker drops the core's audio while speeding. On is two things, one per speed — a
    /// rewind plays the frame it landed on at its natural rate, and a fast forward resamples
    /// the gear's worth of audio into one present, the pitch rising with it, which is what a
    /// tape does. Keeping the pitch instead was measured and does not work: see `Worker::run`.
    pub const SETTINGS_FF_SOUND: &str = "Sound While Speeding";
    /// The two words an on/off row's value is one of. **A switch reads as a word, not as a
    /// number**: `1`/`0` beside a name would have to be decoded, and the lid timer already taught
    /// this screen that a value the user has to translate is a value they get wrong.
    pub const SWITCH_ON: &str = "On";
    pub const SWITCH_OFF: &str = "Off";
    pub const SETTINGS_DATETIME: &str = "Date & Time";
    /// ★ 2026-10-28 第二轮（六六）：日期与时间行去掉「校准」描述项 —— 它成了"只留名字"的行，
    ///   和刷新同类，`A` 开门；`SETTINGS_CALIBRATE` 随之删除（底行 `HINT_OPEN` 已说"怎么进"）。
    /// ★ 2026-10-17: read the card again, now. The one row on this screen that is not a setting
    /// at all — it changes nothing about the machine and a great deal about what the machine can
    /// see. A card filled over the network (or by pulling it and putting games on it) is
    /// otherwise only noticed at the next boot, because the boot is the only scan there is.
    pub const SETTINGS_REFRESH: &str = "Refresh Games";
    /// The value that row wears for a moment after the press. **A word rather than a progress
    /// bar**: the scan is a couple of hundred milliseconds on a full card and there is no frame
    /// to draw a bar on, so the honest report is the one that arrives.
    pub const SETTINGS_REFRESHED: &str = "Refreshed";
    /// The BaseOS-generated wordmark is the first, always-available boot-screen choice.
    /// Custom BMP images found under Bootlogos/ follow it.
    pub const SETTINGS_BOOT: &str = "Boot Screen";
    pub const SETTINGS_BOOT_DEFAULT: &str = "BaseOS default";
    /// The unit on the lid timer's values.
    pub const MINUTE_UNIT: &str = "min";
    /// ★ The lid timer's zero choice: the panel still darkens, but the machine never stops by
    /// itself. **A word rather than "0 min"**, because zero minutes reads as "stop at once" —
    /// the exact opposite of what the choice does.
    pub const MINUTES_NEVER: &str = "Screen Off Only";

    // ── The operation keymap ─────────────────────────────────────────────────────────────────
    /// ★ 2026-10-28：设置页那一行「操作键映射」随机种多了、游戏里另有入口而删除（六六）。
    ///   键位弹窗本身（游戏内）还在，所以下面这些常量仍留着；只是设置页不再有这一行。
    pub const KEYMAP_TITLE: &str = "Key Mapping";
    pub const KEYMAP_PRESS: &str = "Pick a target";
    pub const KEYMAP_FOOT: &str = "Up and down to choose      A to change      Left and right for the rate      B to go back";
    pub const KEYMAP_FOOT_CAPTURE: &str = "Press the key to bind      Same key again to leave";
    pub const TARGET_NONE: &str = "None";
    pub const TARGET_GAME_A: &str = "Game A";
    pub const TARGET_GAME_B: &str = "Game B";
    pub const TARGET_GAME_L: &str = "Game L";
    pub const TARGET_GAME_R: &str = "Game R";
    /// ★ 2026-10-25：MD 的 `C`/`X`（三键与六键），PS1 的 `□`/`△` —— 两个 libretro 位
    /// `JOYPAD_X`/`JOYPAD_Y` 的名字。和 `Game L`/`Game R` 一样的写法：名字跟**位**走，
    /// 不跟某台机器的印字走（MD 叫 `C`、PS1 叫 `□`，写哪个都会对另一台撒谎）。
    pub const TARGET_GAME_X: &str = "Game X";
    pub const TARGET_GAME_Y: &str = "Game Y";
    /// ★ 2026-10-27（六六）：机种专属游戏键。同一个 libretro 位（`GAME_*` 文件键）在不同机种
    /// 上印的字母不同 —— MD 的 `C`/`Z`、NeoGeo 的 `D`、PS1 的 `L2`/`R2`。这些是标签，
    /// 不是新文件键：`face_label(system, target)` 按机种挑其中一个。
    pub const TARGET_GAME_C: &str = "Game C";
    pub const TARGET_GAME_Z: &str = "Game Z";
    pub const TARGET_GAME_D: &str = "Game D";
    pub const TARGET_GAME_L2: &str = "Game L2";
    pub const TARGET_GAME_R2: &str = "Game R2";
    /// ★ 2026-10-27：PS1 面键用**英文词**而不是 `○×△□` 符号 —— 标签面（`label.ttf`）没有
    /// `○`(U+25CB)、`△`(U+25B3)、`□`(U+25A1) 这三个字形（只有 `×`(U+00D7)），直接印符号
    /// 会是三个空位。四个词在两种语言里都一样（PS1 按键名是国际词，不翻译）。
    pub const TARGET_PS1_CIRCLE: &str = "Circle";
    pub const TARGET_PS1_CROSS: &str = "Cross";
    pub const TARGET_PS1_TRIANGLE: &str = "Triangle";
    pub const TARGET_PS1_SQUARE: &str = "Square";
    /// ★ 2026-10-27：PS1 肩键四颗的标签（位 10/11/12/13）。
    pub const TARGET_PS1_L1: &str = "L1";
    pub const TARGET_PS1_R1: &str = "R1";
    pub const TARGET_PS1_L2: &str = "L2";
    pub const TARGET_PS1_R2: &str = "R2";
    pub const TARGET_AB: &str = "A+B";
    pub const TARGET_AL: &str = "A+L";
    pub const TARGET_AR: &str = "A+R";
    pub const TARGET_BL: &str = "B+L";
    pub const TARGET_BR: &str = "B+R";
    pub const TARGET_LR: &str = "L+R";
    /// ★ 2026-10-27：NeoGeo 的两条机内组合（投币+服务杆那种三键和弦）：机内 A+B+C 与 B+C+D。
    /// 只有 NEOGEO/ARCADE 的候选池里出这两条，别处不给。
    pub const TARGET_ABC: &str = "A+B+C";
    pub const TARGET_BCD: &str = "B+C+D";
    pub const TARGET_TURBO_A: &str = "Turbo A";
    pub const TARGET_TURBO_B: &str = "Turbo B";
    /// ★ 2026-11-01：FBNeo（NEOGEO / ARCADE）的四键连发。`Turbo C` / `Turbo D` 是这台机器
    /// 自己的 C、D 两颗（文件键 `TURBO_C` / `TURBO_D`，位见 `keys::TARGETS`）。
    pub const TARGET_TURBO_C: &str = "Turbo C";
    pub const TARGET_TURBO_D: &str = "Turbo D";
    /// ★ 2026-11-01：MD 六键手柄的 `X`/`Z`。连发目标与游戏键一样按机种换名 ——
    /// `TURBO_L`(bit10) 这一台上是 MD 的 `X`、`TURBO_R`(bit11) 是 `Z`。
    pub const TARGET_TURBO_MD_X: &str = "Turbo X";
    pub const TARGET_TURBO_MD_Z: &str = "Turbo Z";
    pub const TARGET_TURBO_L: &str = "Turbo L";
    pub const TARGET_TURBO_R: &str = "Turbo R";
    pub const TURBO_RATE: &str = "Turbo rate";
    /// ★ 2026-10-21：改完键位之后那句「要保存吗」，和两行按键。`A` 落笔进卡，`B` 丢掉。
    pub const KEYMAP_SAVE_TITLE: &str = "Save the key mapping?";
    pub const KEYMAP_SAVE_A: &str = "A     Save";
    pub const KEYMAP_SAVE_B: &str = "B     Discard";

    // ── The deck's LCD panel ─────────────────────────────────────────────────────────────────
    // Short, uppercase words: this is a monochrome player of 1999–2004, and its labels are the
    // ones a real one used. **The Chinese column is the same set rather than a translation of
    // the English**, because the panel is a fixed 240-pixel screen and a long label does not fit
    // it — `频谱` is two ideographs where "SPECTRUM" is eight characters.
    //
    // ⚠️ **这里现在只剩一个词了**，而且它还是可选的（分区框的标签已经去掉）。面板上其余的字
    //    要么是键自己的名字（`Key::label`，拉丁、不进这张表），要么是**符号**（四个播放模式）。
    //    "NOW PLAYING" 抬头、`DECK_TRACK`、`DECK_MODE_*`、`DECK_SPECTRUM` 都删掉了 ——
    //    每一条都是"屏上已经说了的事再说一遍"。
    /// The deck's own name for itself, for a caller that wants a caption.
    pub const DECK_SPECTRUM: &str = "SPECTRUM";

    // ── The player's info page (SELECT, 2026-10-06) ───────────────────────────────────────────
    /// The two labels that are not a value. **Not "name" / "tracks" as bare words** — the disc's
    /// name line is the name itself, and a line reading just `12` under it would be a fact with
    /// nothing saying what it counts.
    pub const INFO_TRACKS: &str = "Tracks";
    pub const INFO_LENGTH: &str = "Length";
    /// The section head above the key guide, so the block of key lines below it does not read as
    /// more track metadata.
    pub const INFO_KEYS_TITLE: &str = "Keys";
    /// What a card with no `Music/` says on that page. Its own sentence rather than an empty
    /// sheet: an empty page would be indistinguishable from "still loading".
    pub const INFO_NO_DISC: &str = "No albums on this card";
    /// The direct-connect dialog's five lines — the prompt, what A does, the wait, the ready
    /// word, and what to do with the address once it is there. `System/slotnet.sh` does the
    /// work; these are only what it says on the way through.
    pub const DIRECT_TITLE: &str = "Direct connect";
    pub const DIRECT_PROMPT: &str = "A: WiFi + SSH    B: cancel";
    pub const DIRECT_WAITING: &str = "Connecting...";
    pub const DIRECT_READY: &str = "Ready";
    pub const DIRECT_HINT: &str = "sftp to root@the address above    B: leave";
    /// The Wi-Fi row's two words, and the config screen's own. The row is a fixed-width
    /// plate, so the SSID itself is never printed there — the name lives in the screen.
    pub const SETTINGS_WIFI: &str = "Wi-Fi";
    /// ★ 2026-10-28：美术资源那一行已删（六六：「美术资源…已经迭代不需要这功能了」）。
    ///   卡带脸的来源（整体贴图 vs 贴纸）仍是 `art_source` 状态在定，只是屏上不再有这一行。
    pub const SETTINGS_WIFI_UNSET: &str = "Not set";
    pub const SETTINGS_WIFI_SET: &str = "Set";
    /// 2026-10-28: the two new gameplay-style toggles on the parameters page. "Resume on Insert"
    /// loads the resume snapshot when a cart is seated or the device boots with one in; "Save on
    /// Eject" writes it when the cart is pulled. Neither touches the native `.sav`, and lid /
    /// power / autosave are outside their scope.
    pub const SETTINGS_INSERT_RESUME: &str = "Resume on Insert";
    pub const SETTINGS_EJECT_SAVE: &str = "Save on Eject";
    pub const WIFI_CURRENT: &str = "Network";
    pub const WIFI_NONE: &str = "Not configured yet";
    pub const WIFI_SCAN: &str = "Scan for networks";
    pub const WIFI_SCANNING: &str = "Scanning...";
    pub const WIFI_MANUAL: &str = "Enter the name by hand";
    pub const WIFI_PICK: &str = "Choose a network";
    pub const WIFI_SSID_LABEL: &str = "Name";
    pub const WIFI_PSK_LABEL: &str = "Password";
    pub const WIFI_OPEN: &str = "Open network (no password)";
    pub const WIFI_NO_NETS: &str = "No networks found";
    pub const WIFI_SAVED: &str = "Saved -- hold B on the shelf to connect";
    pub const WIFI_DEL: &str = "Delete the config";
    /// The screen's three foot lines. Measured against the 660 px plate before writing:
    /// a line that is too long is cut off rather than squeezed.
    pub const WIFI_HINT_STATUS: &str = "A scan    X by hand    Y delete    B back";
    pub const WIFI_HINT_LIST: &str = "Up and down to choose    A pick    B back";
    pub const WIFI_HINT_EDIT: &str = "A types    B erases    L/R layer    START done";
}

#[cfg(not(feature = "lang-en"))]
mod words {
    pub const SET_CLOCK_LABEL: &str = "设置时钟";

    pub const TOAST_STATE_SAVED: &str = "存档已保存";
    pub const TOAST_STATE_LOADED: &str = "存档已读取";
    pub const TOAST_CHEATS_ON: &str = "金手指已开启";
    pub const TOAST_CHEATS_OFF: &str = "金手指已关闭";
    pub const TOAST_AUDIO_STABLE: &str = "音频：稳定";
    pub const TOAST_AUDIO_BALANCED: &str = "音频：均衡";
    pub const TOAST_AUDIO_STRICT: &str = "音频：严格";
    pub const TOAST_SLOW_ON: &str = "慢放：开";
    pub const TOAST_SLOW_OFF: &str = "慢放：关";
    /// 已选择自定义图片或 BaseOS 默认画面。开机画面在下一次启动时生效，
    /// 当前正在运行的前端不会立即重绘。
    pub const TOAST_BOOT_SET: &str = "已设置，下次开机生效";
    /// 没写成，这一行退回原来那张。用一句而不是七句：具体是哪一步失败的记在
    /// `System/boot.err` 里，手上这台机器只需要知道"去看卡"。
    pub const TOAST_BOOT_FAIL: &str = "开机画面没换";

    pub const POWER_RESTART: &str = "重启";
    pub const POWER_OFF: &str = "关机";
    pub const SHUTDOWN_RESTART: &str = "正在重启";
    pub const SHUTDOWN_POWER_OFF: &str = "正在关机";

    pub const SWITCHER_LEGEND: [(&str, &str); 3] = [("B", "返回"), ("Y", "删除"), ("A", "读取")];

    pub const CORE_CANCEL: &str = "取消";
    pub const CORE_SWAP: &str = "切换";
    pub const CORE_CHOOSE: &str = "选择";

    /// ★ 2026-10-26（六六）：机种选择面板的标题与"怎么走"那一条。
    pub const SYS_PICKER_TITLE: &str = "选择机种";
    pub const SYS_PICKER_MOVE: &str = "移动";

    pub const GAME_ROW_SAVE: &str = "即时存档";
    pub const GAME_ROW_LOAD: &str = "即时读档";
    pub const GAME_ROW_SHORTCUTS: &str = "操作键映射";
    pub const GAME_ROW_PALETTE: &str = "色板浏览器";
    /// ★ 2026-10-21 六六：「屏幕档位」应该改成「网格切换」—— 它切的是 LCD3x 网格与扫描线，
    /// 不是"屏幕"这个整体（亮度/色板另有其路），叫"屏幕档位"会让人以为去改面板本身。
    pub const GAME_ROW_GRID: &str = "网格切换";
    /// ★★ 2026-10-21 六六：「GB GBC 里『网格切换』应该为『遮罩切换』」。同一行、同一个位置，
    /// **跟机种换一个词** —— GBA 没有遮罩艺术环（那台机器只有网格与扫描线），GB/GBC 反过来
    /// 没有 LCD3x 网格。挑哪一个由 `GameRow::text_for` 按这一局的机种决定。
    pub const GAME_ROW_OVERLAY: &str = "遮罩切换";
    pub const GAME_ROW_SAVE_SETTINGS: &str = "保存设置";
    /// ★★ 2026-10-21 六六：「重开本局」改为「重启模拟器」。改的是**名字跟上了它实际做的事**：
    /// 它不复位机器、不拔卡，只让核心从开机那一刻重来（`retro_reset`）。
    pub const GAME_ROW_RESTART: &str = "重启模拟器";
    /// 保存设置弹窗的四个键位：A 全局保存 / X 按游戏保存 / Y 恢复默认 / B 取消。
    pub const SAVE_SET_A: &str = "按机种保存";
    pub const SAVE_SET_X: &str = "按游戏保存";
    pub const SAVE_SET_Y: &str = "恢复默认";
    pub const SAVE_SET_B: &str = "取消";
    /// 保存设置弹窗的标题。
    pub const SAVE_SET_TITLE: &str = "要保存这些设置吗？";
    /// 槽位行：`03 · empty` / `03 · 10-20 22:31`。
    pub const SLOT_EMPTY: &str = "空";
    pub const LINK_ROW: &str = "联机";
    pub const LINK_HOST: &str = "主机";
    pub const LINK_JOIN: &str = "加入";

    pub const LINK_RADIO: &str = "正在启动无线连接";
    pub const LINK_WAITING: &str = "正在寻找其他玩家";
    pub const LINK_FAIL_RADIO: &str = "无线连接未能启动";
    pub const LINK_FAIL_NOBODY: &str = "无人加入";
    pub const LINK_FAIL_PEER: &str = "对方已断开";
    pub const LINK_FAIL_CANCELLED: &str = "已取消";

    pub const UNDO_SAVE: &str = "撤销存档";
    pub const UNDO_LOAD: &str = "撤销读取";

    /// 菜单里的按钮。2026-10-21 起是三颗：上排两颗参考屏，播放器整行。
    ///
    /// ★ 2026-10-06 六六：「MENU 页的"按键指南"改成"全键位一览"」——那张卡是**列全的**
    ///   （39 行，`shortcuts.rs` 的 `ROWS`），"指南"听着像挑重点，名字要对得上内容。
    pub const MENU_SHORTCUTS: &str = "全键位一览";
    /// ★ 2026-10-06 六六：「播放器名字改为**像素随身听**」。
    /// 这是**播放器本身的名字**，不是"磁带列表"——所以是名字，不是描述。
    pub const MENU_DISCS: &str = "像素随身听";
    /// 2026-10-10 六六：新增设置入口（合盖关机时长 + 日期时间）。★ 同日定名**「参数调整页」**
    /// （英文列 Parameters）—— 这屏调的全是"机器本身的参数"，"设置"太泛。
    pub const MENU_SETTINGS: &str = "参数调整页";

    // ── 设置页 ───────────────────────────────────────────────────────────────────────────────
    /// ★ 2026-10-10：两行倍速。**按它调的那根键命名，不按机制命名** —— 玩家按住 R2 / L2，
    ///   这一项说的是那个"多快"。右边那个值是倍数，所以行名不再重复说一遍。
    /// ★ 2026-10-28 六六：label 改「快进倍率 / 回溯倍率」—— 说的是**档位**（倍率），不是动作，
    ///   原来只写"快进/回溯"读起来像在说按 R2/L2 干什么。
    pub const SETTINGS_FF: &str = "快进倍率";
    pub const SETTINGS_REWIND: &str = "回溯倍率";
    /// 倍速数值后面的单位。**拉丁 `x`，不用乘号**：`U+00D7` 不保证标签字体里有，而 `2x` 一样读。
    pub const SPEED_UNIT: &str = "x";
    /// ★ **按用户看见的事命名，不按机制命名**（2026-10-10）：合盖那一刻屏就暗了，这一项说的
    ///   是"暗屏之后多久机器才真的停"。"doze" 是这堆代码的内部叫法，不是玩家的词。
    /// ★ 2026-10-28 六六：label 改「息屏后关机」—— 原来的「息屏关机时间」读起来像"息屏关机
    ///   的时长"，而这一行说的是"息屏之后多久才关机"，词序反了。
    pub const SETTINGS_SLEEP: &str = "息屏后处理";
    pub const SETTINGS_SLEEP_SYSTEM: &str = "系统深度休眠";
    /// ★ 2026-10-16：快进与回溯的时候，还要不要听见游戏自己的声音。**和标识一样是个开关**，
    ///   也是这一屏第一行关于"听起来"而不是"机器干什么"的设置。默认「关」，也就是出货以来的
    ///   做法：加速时 worker 直接丢掉核心的音频。开到「开」是两回事，按速度分开——回溯放的是
    ///   落定那一帧的声音，自然速率；快进把这一档的音频重采样压进一个 present，音高随档位一起
    ///   上去，磁带快进就是这样。**保音调的做法实测不可行**（2× 时 440 Hz 只剩 37% 周期性，
    ///   能量全被轰到 ±512 Hz 边带上，其中一个还是 72 Hz 的低频摆动），推导与数字见
    ///   `emu::Worker::run` 的注释。
    pub const SETTINGS_FF_SOUND: &str = "加速时声音";
    /// 开/关两个词。**开关要写成词，不写成数字**：名字旁边一个 `1`/`0` 得先解码才读得懂，
    ///   而这一屏已经从合盖时长那一档学到过 —— 要让用户自己翻译的值，就是会被读错的值。
    pub const SWITCH_ON: &str = "开";
    pub const SWITCH_OFF: &str = "关";
    pub const SETTINGS_DATETIME: &str = "日期与时间";
    /// ★ 2026-10-28 第二轮（六六）：日期与时间行去掉「校准」描述项 —— 它成了"只留名字"的行，
    ///   和刷新同类，`A` 开门；`SETTINGS_CALIBRATE` 随之删除（底行 `HINT_OPEN` 已说"怎么进"）。
    /// ★ 2026-10-17 六六：「MENU页设置添加刷新本机ROM功能」。**这一屏唯一不是"设置"的一行** ——
    ///   它不改机器的任何参数，改的是机器能看见什么。卡上的游戏是随时可以换的（拔卡拷进去、
    ///   或者网络直连拷进去），而在此之前**只有开机那一次扫描**会认。
    pub const SETTINGS_REFRESH: &str = "刷新游戏库";
    /// 按下去之后那一小会儿这一行显示的值。**用词不用进度条**：满卡的扫描也就几百毫秒，
    ///   屏上根本没有一帧能画进度条，所以老实的报告就是"做完了"这一句。
    pub const SETTINGS_REFRESHED: &str = "已刷新";
    /// BaseOS 生成式启动字样是第一项；用户放在 Bootlogos/ 里的 BMP 随后列出。
    pub const SETTINGS_BOOT: &str = "开机画面";
    pub const SETTINGS_BOOT_DEFAULT: &str = "BaseOS 默认";
    /// 合盖时长数值后面的单位。
    pub const MINUTE_UNIT: &str = "分钟";
    /// ★ 时长那一档的 0：屏照样息，但机器**永不**自己停。**用词不用「0 分钟」**——那读起来是
    ///   "立刻停"，正好是这一档的反面。
    pub const MINUTES_NEVER: &str = "仅息屏";

    // ── 操作键映射（2026-10-20 规格，组合键保持不开放）──────────────────────────────────
    /// ★ 2026-10-28：设置页那一行「操作键映射」随机种多了、游戏里另有入口而删除（六六）。
    ///   键位弹窗本身（游戏内）还在，所以下面这些常量仍留着；只是设置页不再有这一行。
    /// 弹窗标题。
    pub const KEYMAP_TITLE: &str = "操作键映射";
    /// 正在选目标时右侧显示的值。
    pub const KEYMAP_PRESS: &str = "请选择目标";
    /// 弹窗底行：上下选行、A 修改、左右改 Turbo 频率、B 返回。
    pub const KEYMAP_FOOT: &str = "上下选择    A 修改    左右改档    B 返回";
    /// 选目标时底行。
    pub const KEYMAP_FOOT_CAPTURE: &str = "选择目标    B 退出";
    /// 17 个目标文案。
    pub const TARGET_NONE: &str = "无";
    pub const TARGET_GAME_A: &str = "游戏 A";
    pub const TARGET_GAME_B: &str = "游戏 B";
    pub const TARGET_GAME_L: &str = "游戏 L";
    pub const TARGET_GAME_R: &str = "游戏 R";
    /// ★ 2026-10-25：MD 的 `C`/`X`、PS1 的 `□`/`△`，见英文那份的同名注释。
    pub const TARGET_GAME_X: &str = "游戏 X";
    pub const TARGET_GAME_Y: &str = "游戏 Y";
    /// ★ 2026-10-27：机种专属游戏键标签（同英文那份）—— MD `C`/`Z`、NeoGeo `D`、PS1 `L2`/`R2`。
    pub const TARGET_GAME_C: &str = "游戏 C";
    pub const TARGET_GAME_Z: &str = "游戏 Z";
    pub const TARGET_GAME_D: &str = "游戏 D";
    pub const TARGET_GAME_L2: &str = "游戏 L2";
    pub const TARGET_GAME_R2: &str = "游戏 R2";
    /// ★ 2026-10-27：PS1 面键用词不用符号，理由同英文列（标签面无 `○△□` 字形）。
    /// 四个词是国际按键名，两种语言都不翻译。
    pub const TARGET_PS1_CIRCLE: &str = "Circle";
    pub const TARGET_PS1_CROSS: &str = "Cross";
    pub const TARGET_PS1_TRIANGLE: &str = "Triangle";
    pub const TARGET_PS1_SQUARE: &str = "Square";
    /// ★ 2026-10-27：PS1 肩键四颗的标签（位 10/11/12/13），同英文列。
    pub const TARGET_PS1_L1: &str = "L1";
    pub const TARGET_PS1_R1: &str = "R1";
    pub const TARGET_PS1_L2: &str = "L2";
    pub const TARGET_PS1_R2: &str = "R2";
    pub const TARGET_AB: &str = "A+B";
    pub const TARGET_AL: &str = "A+L";
    pub const TARGET_AR: &str = "A+R";
    pub const TARGET_BL: &str = "B+L";
    pub const TARGET_BR: &str = "B+R";
    pub const TARGET_LR: &str = "L+R";
    /// ★ 2026-10-27：NeoGeo 两条机内组合（同英文列），只有 NEOGEO/ARCADE 出。
    pub const TARGET_ABC: &str = "A+B+C";
    pub const TARGET_BCD: &str = "B+C+D";
    pub const TARGET_TURBO_A: &str = "连发 A";
    pub const TARGET_TURBO_B: &str = "连发 B";
    /// ★ 2026-11-01：FBNeo（NEOGEO / ARCADE）的四键连发，同英文列。
    pub const TARGET_TURBO_C: &str = "连发 C";
    pub const TARGET_TURBO_D: &str = "连发 D";
    /// ★ 2026-11-01：MD 六键手柄的 `X`/`Z`（同英文列）。
    pub const TARGET_TURBO_MD_X: &str = "连发 X";
    pub const TARGET_TURBO_MD_Z: &str = "连发 Z";
    pub const TARGET_TURBO_L: &str = "连发 L";
    pub const TARGET_TURBO_R: &str = "连发 R";
    /// 最下面频率行。
    pub const TURBO_RATE: &str = "连发频率";
    /// ★ 2026-10-21：改完键位之后那句「要保存吗」，和两行按键。`A` 落笔进卡，`B` 丢掉。
    pub const KEYMAP_SAVE_TITLE: &str = "要保存这次键位改动吗？";
    pub const KEYMAP_SAVE_A: &str = "A     保存";
    pub const KEYMAP_SAVE_B: &str = "B     丢弃";

    // ── 卡座液晶面板 ────────────────────────────────────────────────────────────────────────
    // **短、全大写**：这是一块 1999–2004 的单色播放器，标签就用当年真机上的那些词。
    // 中文一栏是**同一套词的对应写法，不是英文的逐字翻译** —— 面板只有 240 像素宽，
    // 「频谱」两个汉字放得下，"SPECTRUM" 八个字母也放得下，两边都得是短的。
    //
    // ⚠️ **这里现在只剩一个词了**，而且它还是可选的（分区框的标签已经去掉）。面板上其余的字
    //    要么是键自己的名字（`Key::label`，拉丁、不进这张表），要么是**符号**（四个播放模式）。
    //    「正在播放」抬头、`DECK_TRACK`、`DECK_MODE_*`、`DECK_SPECTRUM` 都删掉了 ——
    //    每一条都是"屏上已经说了的事再说一遍"。
    /// 这块面板自己的名字，留给想要个标题的调用方。
    pub const DECK_SPECTRUM: &str = "频谱";

    // ── 信息页（SELECT，2026-10-06）────────────────────────────────────────────────────────
    /// 两个不是值的标签。**不是光秃秃的「曲目数」/「时长」** —— 碟片名那一行就是碟片名，
    /// 它下面孤零零一个 `12`，读的人得回头看上面那行才知道那是什么东西在数。
    pub const INFO_TRACKS: &str = "曲目";
    pub const INFO_LENGTH: &str = "时长";
    /// 键位指南那一段的抬头。少了它，下面那十几行键位会被当成又一堆曲目信息。
    pub const INFO_KEYS_TITLE: &str = "键位";
    /// 卡上没有 `Music/` 时这一页说的话。**给一句整话而不是给一张白纸** ——
    /// 白纸和"还在读卡"在屏上长得一模一样。
    pub const INFO_NO_DISC: &str = "这张卡上没有专辑";
    /// 直连对话框的五句 —— 提示、A 干什么、等待、就绪、地址到手之后怎么办。
    /// `System/slotnet.sh` 干活，这几句只是它路上说的话。
    pub const DIRECT_TITLE: &str = "直连模式";
    pub const DIRECT_PROMPT: &str = "A：开 WiFi 与 SSH    B：取消";
    pub const DIRECT_WAITING: &str = "正在连接…";
    pub const DIRECT_READY: &str = "已就绪";
    pub const DIRECT_HINT: &str = "sftp 连 root@上面的地址    B：收回";
    /// Wi-Fi 行的两个词，和配置屏自己的台词。那一行是定宽盘子，SSID 不打印在行上 ——
    /// 名字在配置屏里看，跟键位表一个道理。
    pub const SETTINGS_WIFI: &str = "无线网络";
    /// ★ 2026-10-28：美术资源那一行已删（六六：「美术资源…已经迭代不需要这功能了」）。
    ///   卡带脸的来源（整体贴图 vs 贴纸）仍是 `art_source` 状态在定，只是屏上不再有这一行。
    /// ★ 2026-10-28：参数调整页新增的两个开关。插卡续玩 = 插卡 / 开机卡槽有卡时自动读 resume 接着玩；
    /// 拔卡续存 = 拔卡时把当前画面写进 resume（即时存档快照）。都不动原生 `.sav`，也不管
    /// lid / 关机 / 自动存档那条。
    pub const SETTINGS_INSERT_RESUME: &str = "插卡续玩";
    pub const SETTINGS_EJECT_SAVE: &str = "拔卡续存";
    pub const SETTINGS_WIFI_UNSET: &str = "未配置";
    pub const SETTINGS_WIFI_SET: &str = "已配置";
    pub const WIFI_CURRENT: &str = "当前网络";
    pub const WIFI_NONE: &str = "尚未配置";
    pub const WIFI_SCAN: &str = "扫描附近网络";
    pub const WIFI_SCANNING: &str = "正在扫描…";
    pub const WIFI_MANUAL: &str = "手动输入名称";
    pub const WIFI_PICK: &str = "选择一个网络";
    pub const WIFI_SSID_LABEL: &str = "名称";
    pub const WIFI_PSK_LABEL: &str = "密码";
    pub const WIFI_OPEN: &str = "开放网络（无密码）";
    pub const WIFI_NO_NETS: &str = "附近没有找到网络";
    pub const WIFI_SAVED: &str = "已保存 —— 货架长按 B 即可直连";
    pub const WIFI_DEL: &str = "删除配置";
    /// 配置屏的三条页脚。写之前用 `_tools/_font_width.py` 量过 660 px 那条线：
    /// 超长是截断，不是缩小。
    pub const WIFI_HINT_STATUS: &str = "A 扫描    X 手动输入    Y 删除    B 返回";
    pub const WIFI_HINT_LIST: &str = "上下选择    A 确定    X 手动    B 返回";
    pub const WIFI_HINT_EDIT: &str = "A 输入    B 退格    L/R 换层    START 完成";
}

pub use words::*;

/// `3 分钟` / `3 min` — one of the settings screen's values, with its unit.
///
/// ★ **Zero is a word, not `0 分钟`.** `0` is the choice where the timer is switched off, and
/// "zero minutes" would read as "stop straight away" — see `words::MINUTES_NEVER`.
///
/// A function rather than a constant because the number *is* the setting, and here rather than
/// in `settings.rs` for the same reason every other word is here: the two languages are two
/// columns of one list, and a unit living beside the layout would be a third place to look.
pub fn minutes(n: u8) -> String {
    if n == 0 {
        return words::MINUTES_NEVER.to_string();
    }
    format!("{n} {}", words::MINUTE_UNIT)
}

/// `2 倍速` / `2x` — one of the two speed rows' values.
///
/// A function rather than a constant for the same reason `minutes` is: **the number is the
/// setting**. Unlike the lid timer there is no zero and no word among these: every gear is a
/// whole multiplier of one core frame per present, and none of them is "off" — fast forward is
/// something the user holds down, and rewind is something they hold down too. There is no state
/// to name with a word.
pub fn speed(n: u8) -> String {
    format!("{n}{}", words::SPEED_UNIT)
}

/// `开` / `关` — "On" / "Off" — for a row that is a switch rather than a list of lengths.
///
/// **Two words and no third possibility**, which is the whole of what a switch is; the row's
/// `←`/`→` walk between them and stop at each end, the same clamp every other list on this screen
/// uses. Nothing here is a number the user could be asked to type.
pub fn switch_word(on: bool) -> &'static str {
    if on {
        words::SWITCH_ON
    } else {
        words::SWITCH_OFF
    }
}

// ── The player's key guide, printed on that page ───────────────────────────────────────────────
//
// **A table of its own rather than four more rows in `shortcuts.rs`**, for two reasons that
// both come from what this table is: those rows are **one key line and a description that must
// fit a fixed 340-pixel column**, rasterised into the 720×480 shortcut card; these are **one
// plain string per row, centred on a 720-wide sheet**. Reusing `Row` would force this guide
// through a column budget it does not have and through a `fit` that would shrink a row rather
// than report it.
//
// ⚠️ **每一格必须短，因为这一页是两栏**（`lcd::info_keys` 把 14 条排成 2×7）。
//   每格 = 一个键 + 一个 6px 间隔 + 一句说明，**两栏各 114 逻辑像素**。
//
// ★★ **2026-10-06 六六「全用 16PX」之后这个约束变紧了**：这一页现在只有 `prose_plan`
//   一套字号，而它的拉丁走的是**汉字那张面** ⇒ `SELECT` 从 8px 的 53 变成 16px 的 **61**，
//   `MENU短` 从 51 变成 **59**。于是在 8px 版里刚好放得下的两条现在**超了 3px**：
//
//   ```text
//   SELECT 信息页  键 61.0 + 6 + 说明 50.0 = 117.0（两栏 114）  ★放不进两栏
//   MENU短 回货架  键 59.0 + 6 + 说明 50.0 = 115.0（两栏 114）  ★放不进两栏
//   ```
//
// ⇒ **缩短那两条的说明**（「信息页」→「本页」、「回货架」→「货架」），
//   **而不是把这两格退回 8px** —— 那一页只有一套字号，正是六六要的东西。
//
// **The keys are buttons, so they do not translate** — same rule as `lang`'s own header says.
// The descriptions do, and they are in the two language tables above.
#[cfg(not(feature = "lang-en"))]
pub const DECK_KEY_GUIDE: &[(&str, &str)] = &[
    ("A", "播放/暂停"),
    ("A长按", "停止"),
    ("B", "货架"),
    ("X", "切面板"),
    ("Y", "换色板"),
    ("←", "快退"),
    ("→", "快进"),
    ("↑", "上一首"),
    ("↓", "下一首"),
    ("L1", "上一碟"),
    ("R1", "下一碟"),
    ("START", "换 EQ"),
    ("MENU短", "信息"),
    ("MENU长", "退出"),
];

#[cfg(feature = "lang-en")]
pub const DECK_KEY_GUIDE: &[(&str, &str)] = &[
    ("A", "play/pause"),
    // A with the "hold" mark (▴, the same mark the MENU long press wears): stop, not leave.
    ("A▴", "stop"),
    ("B", "shelf"),
    ("X", "panel"),
    ("Y", "palette"),
    ("←", "rewind"),
    ("→", "ffwd"),
    ("↑", "prev"),
    ("↓", "next"),
    ("L1", "prev disc"),
    ("R1", "next disc"),
    ("START", "EQ"),
    ("MENU▾", "info"),
    ("MENU▴", "quit"),
];

/// `m:ss`, or `h:mm:ss` past an hour — **the disc and track lengths on the info page**.
///
/// A copy of `lcd::clock`, and deliberately not a call into it: that one is private to the
/// panel and its minutes are deliberately unpadded for a clock a person glances at. This one
/// is the same rule, kept as its own function because a private helper cannot be called from
/// `slot`, and because the two will not stay the same shape forever (a 40-minute podcast wants
/// a different line from a 4-minute song).
pub fn disc_clock(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
