//! Everything the binary does with a compositor except own one. The window is the only
//! difference between the host and the device, so it is the only thing left above this.

use std::time::{Duration, Instant};

use slot_gfx::{Compositor, Draw, TexId, OUT_H, OUT_W};
use slot_input::{InputSource, Millis};
use slot_power::{Platform, Power};
use slot_store::{
    format_stamp, System, DOZE_MIN_CHOICES, DOZE_MIN_DEFAULT, FF_SPEED_CHOICES,
    REWIND_SPEED_CHOICES,
};
use slot_ui::lang;
use slot_ui::{
    arrows_hint_face, cart_face, cart_placeholder, cheat_row_face, chip_face,
    dialog_line_face,
    chip_shadow_face, clock_face, hhmm, hint_face, icon_face, letters, menu_face,
    menu_face_at, menu_face_white,
    palette_cell_w, palette_name_face, palette_name_h, page_face, photo_face,
    set_clock_hint_face,
    shelf_title_face, shortcut_hint_face, shortcut_row_face, socket_face, sticker_face,
    title_face, toast_face, wallpaper_face, word_face, CartFace, Icon, PowerChoice,
    Toast, ALERT_PX, BOLT_PX, HUD_ICON_PX, LEGEND, PALETTE_FOOT_PX, SHORTCUT_HINT, SHORTCUT_ROWS,
};

use crate::app::{App, Coin, GameRow, LinkRow, MenuButton, Phase, ShelfRow};
use crate::face_builder::{shelf_window, FaceBuilder, ShelfFaceFiller};
use crate::link_start::{LinkFail, LinkStep};
use crate::root;
use crate::session::Session;
use crate::wallpaper;

/// How long a dark panel waits before the machine actually stops. The dark is immediate —
/// the lid or the button kills the backlight on the edge — but the device is still running
/// flat out behind it at 400-700 mA, so this is the window in which the user might come
/// straight back, not a power saving.
///
/// Five minutes, and then the device powers off rather than sleeping. It cannot wake itself
/// from a sleep — the RTC alarm never fires on this board — so a standby would be a leak with
/// no end, and a power off is the honest version of putting it down.
///
/// ★ **Only the seed, and now literally the card's own default** — spelled from
///   `DOZE_MIN_DEFAULT` rather than repeated, so the two cannot drift. The number the device
///   actually waits is the card's — MENU → Settings → 「息屏关机时间」, zero / five / ten /
///   fifteen (`slot_store::DOZE_MIN_CHOICES`), where zero means the timer is off — and
///   `App::set_power` hands that over before the first frame, so this value is never the one in
///   force on a card that has a state file at all. It stays as the fallback for `Power::new`'s
///   own signature and as the answer to "what does a brand-new card do".
const DOZE_TIMEOUT: Duration = Duration::from_secs(DOZE_MIN_DEFAULT as u64 * 60);

/// Amber. The only warning colour in the tree, and the reason it is not the HUD's ink: a
/// refusal that looks like a volume glyph is a refusal nobody reads as one.
const ALERT_INK: [u8; 3] = [0xf0, 0xb4, 0x3c];

/// Carts either side of the caret that keep a face texture resident.
///
/// The shelf draws three. Twelve leaves room for a flick's overshoot and for the caret's own
/// neighbours to already be there when it lands, while bounding the shelf's memory at about
/// sixteen megabytes whatever the card holds — which is what makes a nine-hundred-game card
/// the same proposition as a hundred-game one.
const RESIDENT: usize = 12;

/// How far past `RESIDENT` a cart may sit before its texture is released. The slack is what
/// stops a caret parked on a boundary from releasing and rebuilding the same cart every step.
const RESIDENT_HYSTERESIS: usize = 3;

pub struct Frontend {
    session: Session,
    start: Instant,
    last: Instant,
    draws: Vec<Draw>,
    /// 上电第二屏收尾：货架首帧之上叠一层动画末帧，逐帧把 alpha 拉到 0。
    /// 只在开机后若干帧内非 None，之后 render 一点额外开销都没有。
    boot_fade: Option<(TexId, f32)>,
    /// One texture per ring slot, reused every time the switcher opens.
    polaroid_texes: Vec<TexId>,
    /// The top plate's line of type, re-rasterised whenever the selection moves.
    title_tex: Option<TexId>,
    /// The shelf's own line of type, and the name it was built for. The same deal as the top
    /// plate's, on the screen that needs it most: one texture, rebuilt when the cart under the
    /// eye changes and at no other time.
    shelf_title_tex: Option<TexId>,
    shelf_titled: Option<String>,
    /// ★ 2026-10-21：槽位表的两样东西 —— 那行字（"03 · 10-20 22:31"）和那张 2 倍截图。
    ///
    /// `slot_built` 是**它们上一次为谁而建**：`slot_caption()` 的整串文字加缩略图的路径。
    /// 用文字本身做键而不是用槽位号，是因为同一槽位上"存过"和"空的"caption 不同、
    /// 而删除之后又变回空的 —— 号没变，内容变了，用号做键就会把旧的一直画着。
    slot_cap_tex: Option<TexId>,
    slot_thumb_tex: Option<TexId>,
    slot_built: Option<(Option<String>, Option<std::path::PathBuf>)>,
    /// ★ 2026-10-21：保存设置弹窗开着没有（上一帧）。开着的那一帧建面、关了放走，
    /// 和 `sync_switcher` 的 `open` 同一个模式。
    save_settings_open: bool,
    /// ★ 2026-10-21：**这一排游戏菜单面是按哪种机器印的。** GBA 印「网格切换」，GB/GBC 印
    /// 「遮罩切换」，同一格两个词 —— 于是插上 GB/GBC 卡之后整排要重印一次。`None` 是"还没
    /// 印过"（`upload_late_furniture` 印第一遍时按货架的空槽读作 GBA，会记下 `Some(Gba)`）。
    game_menu_system: Option<System>,
    /// ★ 2026-10-21：**操作键映射弹窗**这次建面时的"键"—— 开着没有，以及七行各自显示的值。
    /// 只在这两者之一变了的时候重烘：改一行只重烘那一行，卷动清单什么都不用重建。
    keymap_built: Option<Vec<String>>,
    /// The deck's two lines of type and what each was built for: the disc's name in the top
    /// window, and `3/12` in the corner. Both change as the disc changes, and neither is worth a
    /// texture until the deck is actually up — which is why they are counted here rather than
    /// minted at boot with the rest of the furniture. Grouped in `DeckDisc` below for the same
    /// reason `DeckLines` is: `sync_deck` takes one parameter per face it owns, and six loose
    /// options in a row was a list no reader could hold.
    deck_disc: DeckDisc,
    /// The deck's track counter and its lyric line, each with the text it was last built for.
    /// Struct'd rather than four more parameters on `sync_deck`, which already takes one per
    /// face it owns and was at the point where the next reader could not hold the list in their
    /// head. They are here rather than in the group above because these two change **while the
    /// deck is up** — the others only change when the disc does.
    deck_track: DeckLines,
    /// **The panel's two faces have been read off the card.** `false` until the first screen
    /// that actually draws with them comes up — see [`Frontend::ensure_panel_fonts`] for why
    /// that is not at boot.
    panel_fonts: bool,
    /// **The furniture every screen but the shelf draws with has been baked.** `false` for the
    /// first frame — see [`Frontend::upload_late_furniture`] for why that is a frame late rather
    /// than at boot.
    late_faces: bool,
    /// **信息页的行表，和它被烘成的那一版。**
    ///
    /// **一整帧一个 slot** —— 与面板的 `lcd_panel` 是同一个东西的另一个实例
    /// （同一个尺寸、同一套色板、同一张格网）。
    ///
    /// 上一版是"每行一个 slot"的表（二十来张 `word_face`），而那一版连渲染都不是同一套。
    info_tex: Option<TexId>,
    /// 烘焙门槛的三个输入：**说的东西、第几页、哪个色板**。少一个就有一处"变了没重烘"。
    info_built: Option<(crate::app::DeckInfo, usize, usize)>,
    /// Builds the open cart's faces off the frame loop.
    faces: FaceBuilder,
    /// Builds the shelf's remaining cart faces off the frame loop, nearest the caret first.
    shelf_faces: ShelfFaceFiller,
    /// Carts whose face is not on the shelf yet. Ordered by the request, not by the list.
    shelf_missing: Vec<usize>,
    /// The caret position the resident window was last built for.
    resident_at: Option<usize>,
    /// Whether the boot log has the row's own account of itself yet: which cart is in each
    /// slot, and which of those is holding a face rather than the stand-in.
    row_logged: bool,
    /// The cart last asked for.
    core_asked: Option<String>,
    /// The open cart and its lid, and which cart they were built for.
    core_board_tex: Option<TexId>,
    core_lid_tex: Option<TexId>,
    core_built: Option<String>,
    /// The undo cap's label, which changes with what is on offer.
    undo_tex: Option<TexId>,
    switcher: Switcher,
    clocks: Clocks,
    about: AboutFace,
    /// The palette browser's names. A page is read one of two ways and the fields are that pair:
    /// a colour machine's nine grades get a face each, so the page can be read across; a Game Boy's
    /// 343 palettes keep the one line at the foot of the panel, because their names are longer
    /// than a cell (see `App::palette_names`). Never both — the two are keyed differently and each
    /// clears itself on the machine that does not use it.
    palette_texes: Vec<Option<TexId>>,
    palette_tex: Option<TexId>,
    /// The pages both were last built for: the cell faces and the foot line change on a page turn
    /// and at no other time, so a caret moving inside a page mints nothing.
    palette_paged: Option<(System, usize)>,
    palette_named: Option<String>,
    /// Whether the first frame's uptime has been written to `boot.log`. One line, once, so the
    /// log says how long the device took to reach the shelf and which base it did it on.
    frame_logged: bool,
    /// ★ 2026-10-23（六六）：代码层生成的桌面图，按主题烘。**跟踪上次烘的版本**，
    /// `render()` 每帧比对 `(mode, look)`，变了就原地重烘 —— 底色是机器自己的塑料
    /// （`slot_chrome::housing`），色板轮换也会让它换色。卡自带的 `Desktop.png` 是张
    /// 静图、跟不了主题，`desktop_from_card` 为真时不重烘。
    desktop_theme: Option<(slot_ui::palette::Mode, usize)>,
    /// 屏上那张桌面图是不是卡自带的 `Desktop.png`（静图，不随主题重烘）。
    desktop_from_card: bool,
    /// 本前端上传的那张桌面图纹理 —— 重烘前先释放，别让每次换色板都漏一张 1.4MB。
    desktop_tex: Option<TexId>,
    /// ★ 2026-10-28（六六）：屏上铺的是**内置默认壁纸**（卡上没有 `Wallpapers/`）。换主题要
    /// 重烘那圈洋红光（色跟着主题走），卡自己的图没有这回事 —— 所以要记住这一位。
    wall_default: bool,
    /// ★ 2026-10-29（调试）：SFC 反射层"左侧偶尔缺失"追踪。每帧读 `compositor.reflect_diag()`，
    /// 只在 (drew, skip-reason, blur_w, blur_h) 变化时写 boot.log，避免刷屏。
    reflect_last: Option<(bool, Option<&'static str>, u32, u32, i32, i32, i32, i32, u32, u32)>,
    /// 上面那条里累计跳过的帧数，露出"偶尔"的频率（每次 SKIP 行都带上）。
    reflect_total_skips: u32,
}

/// The about label, and what it was last built for. The gauge is the only thing on it that
/// moves, so the reading is what decides whether it is rebuilt.
#[derive(Default)]
struct AboutFace {
    tex: Option<TexId>,
}

/// The clock screen's two faces and the shelf's one, with what each was last built for. The
/// picker's line changes under the caret; the shelf clock changes once a minute; the battery
/// percent changes whenever the reading does.
#[derive(Default)]
struct Clocks {
    line: Option<TexId>,
    hint: Option<TexId>,
    shelf: Option<TexId>,
    picked: Option<String>,
    shown: String,
    battery: String,
    battery_tex: Option<TexId>,
}

/// What the switcher's textures were built for. The photos and the undo cap are per opening;
/// the title is per selection.
#[derive(Default)]
struct Switcher {
    open: bool,
    titled: Option<String>,
}

impl Frontend {
    /// ★ 2026-10-26：**开机那一声**，只由真机入口（`device_app::run`）调。
    ///
    /// 为什么不放在 `Session::boot` 里：那个函数被几十个测试反复调用，每次都会往 ring 里
    /// 塞一秒音频，把所有"环里只有这一条"的断言搅乱。放这里，测试永远碰不到。
    pub fn play_boot_sound(&mut self) {
        self.session.play_sfx(crate::audio::Sfx::Boot);
    }

    /// 上电第二屏把逐帧 PCM 直接喂进 ring，需要拿到 Session 拥有的音频句柄。
    pub fn audio_ring(&self) -> std::sync::Arc<crate::audio::Ring> {
        self.session.audio_ring()
    }

    /// ★ 2026-11-07：动画播完后让末帧溶进货架，而不是一帧硬切。
    /// 由 `device_app` 在主循环的前若干帧驱动 alpha；`None` 时 render 什么都不多做，
    /// 所以这是开机专属的成本，不进稳态帧。`tex` 的创建与释放都归调用方管。
    pub fn set_boot_fade(&mut self, tex: Option<TexId>, alpha: f32) {
        self.boot_fade = tex.map(|t| (t, alpha.clamp(0.0, 1.0)));
    }

    pub fn boot(platform: Box<dyn Platform>) -> Self {
        let now = Instant::now();
        let root = platform.root().to_path_buf();
        let session = Session::boot(root);
        Self::assemble(now, platform, session)
    }

    /// 2026-11-07 并行启动入口：`sink` 已在真机入口的主线程打开（动画要用它的 ring），
    /// 这里复用同一个设备。整个 `Frontend` 在后台线程里构造完再 join 回主线程。
    pub fn boot_with_sink(
        platform: Box<dyn Platform>,
        sink: std::sync::Arc<crate::audio::Device>,
        audio: crate::audio::Profile,
    ) -> Self {
        let now = Instant::now();
        let root = platform.root().to_path_buf();
        let session = Session::boot_with_sink(root, sink, audio);
        let frontend = Self::assemble(now, platform, session);
        // 界面字体（读盘 + 解析约 1.7 s）同样是不碰 GL 的纯 CPU 活，跟扫卡一起塞进
        // 后台窗口；主线程的 upload_faces 只剩纹理上传。
        frontend.load_font();
        frontend
    }

    /// `start` 仍取自构造开始之前：它是前端时钟的基准，不能因为后台化就晚几秒。
    fn assemble(now: Instant, platform: Box<dyn Platform>, mut session: Session) -> Self {
        session
            .app_mut()
            .set_power(Power::new(platform, Some(DOZE_TIMEOUT)));
        Frontend {
            session,
            start: now,
            last: now,
            draws: Vec::new(),
            polaroid_texes: Vec::new(),
            title_tex: None,
            shelf_title_tex: None,
            shelf_titled: None,
            slot_cap_tex: None,
            slot_thumb_tex: None,
            slot_built: None,
            save_settings_open: false,
            game_menu_system: None,
            keymap_built: None,
            deck_disc: DeckDisc::default(),
            deck_track: DeckLines::default(),
            panel_fonts: false,
            late_faces: false,
            info_tex: None,
            info_built: None,
            faces: FaceBuilder::spawn(),
            shelf_faces: ShelfFaceFiller::spawn(),
            shelf_missing: Vec::new(),
            resident_at: None,
            row_logged: false,
            core_asked: None,
            core_board_tex: None,
            core_lid_tex: None,
            core_built: None,
            undo_tex: None,
            switcher: Switcher::default(),
            clocks: Clocks::default(),
            about: AboutFace::default(),
            palette_texes: Vec::new(),
            palette_tex: None,
            palette_paged: None,
            palette_named: None,
            frame_logged: false,
            desktop_theme: None,
            desktop_from_card: false,
            desktop_tex: None,
            wall_default: false,
            reflect_last: None,
            reflect_total_skips: 0,
            boot_fade: None,
        }
    }

    /// The card's own typeface, if it has one. `System/fonts/` belongs to the user, and the
    /// whole point of reading it rather than baking a face in is that a script the embedded one
    /// does not carry — CJK, most obviously — should not need a rebuild to appear.
    ///
    /// Silent on both failure paths on purpose. A card with no font file is the shipped
    /// configuration; a card with a broken one is that same configuration a moment later.
    /// Neither is worth a line on a device with no console and a boot budget to protect.
    fn load_font(&self) {
        // ★ 2026-11-07：设备入口已把字体装填放到后台线程（与开机动画并行）。
        //   卡上那张 7.8 MB 界面字的读盘 + 解析实测 1710 ms，是 upload_faces 里最大的
        //   一块纯 CPU 活；装好了就别在主线程再读一遍（set_font 的 OnceLock 也会忽略
        //   第二次，纯属白等）。
        if slot_ui::text::has_user_font() {
            self.session
                .boot_note("interface font 0 ms (preloaded on boot thread)");
            return;
        }
        let Some(root) = self.session.app().root() else {
            return;
        };
        // ★ **The player's two faces first, and separately.** They are read by name out of
        //   `Music/Fonts/` and fed to their own slots, so the deck's panel is set in its own
        //   faces rather than in whatever the card's interface happens to be. A card without
        //   them still boots: the panel falls back to the interface face.
        // ★★ **面板那两张面**不在这里读**（2026-10-07 实测改动）。** 它们合计 12.75 MB
        //   （`unifont-16px.otf` 一张 5.3 MB），而**货架上没有一个字是它们画的**——
        //   只有卡座面板与信息页用。放在这里等于为"第一帧根本不用的两张面"花掉开机时间：
        //   实测 `font load` 共 **3194 ms**，其中面板那一份约 **2.0 s**
        //   （按这张卡约 6.5 MB/s 的读速）。改由 [`Self::ensure_panel_fonts`] 在
        //   **第一个需要它们的屏幕出现时**才读。详见
        //   `slot-开机慢-诊断与最小加载方案-2026-10-07.md`。
        let iface_at = Instant::now();
        match root::font_file(root) {
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => {
                    // ★ 打出**文件名与大小**：菜单按钮、货架标题、按键卡都用这张字，
                    //   而"屏上没字"与"字没居中"这两种现象，光看画面分不出来 ——
                    //   得先知道到底装上了哪一张。`System/fonts/` 里混着 `.bak` 备份，
                    //   `is_font()` 只认 ttf/otf/ttc，所以**只有真后缀的那个算数**。
                    eprintln!(
                        "slot: font: {} ({} bytes)",
                        path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
                        bytes.len()
                    );
                    slot_ui::text::set_font(bytes);
                }
                Err(e) => eprintln!("slot: font: {} 读不了：{e}", path.display()),
            },
            None => eprintln!("slot: font: 卡上没有（退回内嵌 label.ttf，**它没有汉字**）"),
        }
        self.session.boot_note(&format!(
            "interface font {} ms",
            iface_at.elapsed().as_millis()
        ));
    }

    /// Read the panel's two faces off the card — **once, and only when a screen that draws with
    /// them is about to be baked.**
    ///
    /// # Why this is not in `load_font`
    ///
    /// The pair is **5.4 MB** (`unifont-16px.otf` 5.3 MB + `press-start-2p.ttf` 118 KB)
    /// and **the shelf draws not one glyph with either** — the deck panel and the info page are
    /// the only two screens that do. Read at boot they cost about **0.6 s** of it (measured
    /// 2026-10-07 on the device: `font load` was 3194 ms in total, at the ~6.5 MB/s this card
    /// delivers), for faces the first frame never touches. The cost belongs on the screen that
    /// actually wants them, which is also the screen the user is already waiting on.
    ///
    /// # It is idempotent, and a failure is the same failure as before
    ///
    /// `set_panel_fonts` installs each face independently and `OnceLock::set` refuses a second
    /// one, so calling this twice is harmless. A missing or unparseable file leaves the panel on
    /// the interface face — which is exactly what it did when this ran at boot, and the reason
    /// the boot log's "缺 …" line still exists on this path.
    fn ensure_panel_fonts(&mut self) {
        if self.panel_fonts {
            return;
        }
        self.panel_fonts = true;
        let Some(root) = self.session.app().root() else {
            return;
        };
        let at = Instant::now();
        let (cjk, latin) = root::panel_fonts(root);
        if cjk.is_none() || latin.is_none() {
            eprintln!(
                "slot: {}: 缺 {} —— 播放面板退回界面字体",
                root.join(root::PANEL_FONT_DIR).display(),
                if cjk.is_none() {
                    slot_ui::text::PANEL_FONT_FILES[0]
                } else {
                    slot_ui::text::PANEL_FONT_FILES[1]
                }
            );
        }
        slot_ui::text::set_panel_fonts(cjk, latin);
        // Its own line, and not part of `font load`: this is now a *deck* cost, not a boot one,
        // and a number that shows up at boot would be a lie about where the time went.
        self.session.boot_note(&format!(
            "panel font (on demand) {} ms",
            at.elapsed().as_millis()
        ));
    }

    /// Everything that never changes: the carts, the HUD glyphs and the key caps. All of it
    /// needs a live context, so it happens after the compositor and not at boot.
    pub fn upload_faces(&mut self, compositor: &mut Compositor) {
        // Timed: this is the one part of the boot that scales with the number of games on the
        // card, and the number that says whether a card's covers or the shelf's faces are what
        // the wait is being spent on.
        let faces_at = Instant::now();
        // Before anything is rasterised, and so before anything has an opinion about type: the
        // card's face has to be in place for the first cart's label, not for the second frame.
        let font_at = Instant::now();
        self.load_font();
        // Its own line, because it is the one cost here that does not scale with anything:
        // the whole card font is read off the SD card and parsed before a glyph can be drawn.
        self.session
            .boot_note(&format!("font load {} ms", font_at.elapsed().as_millis()));
        // Every title on the card, checked once against both faces: a subset font goes stale
        // the moment a game is added, and a missing glyph fails silently as a blank.
        //
        // ⚠️ **This is O(carts × chars × 2 faces) and `has_glyph` is not a cmap lookup** —
        //    it is `font.rasterize(ch, 1.0)`, a real rasterisation. Timed since 2026-10-07
        //    because it is the one boot cost that scales with the library and has no line of
        //    its own: on a nine-hundred-cart card it is ~2×10⁴ rasterisations, all before the
        //    first frame.
        let miss_at = Instant::now();
        let mut missing: Vec<char> = Vec::new();
        for cart in self.session.app().carts() {
            for ch in slot_ui::text::missing_in(&slot_ui::label_text(cart)) {
                if !missing.contains(&ch) {
                    missing.push(ch);
                }
            }
        }
        self.session.boot_note(&format!(
            "font missing check {} ms ({} carts)",
            miss_at.elapsed().as_millis(),
            self.session.app().carts().len()
        ));
        if !missing.is_empty() {
            self.session.boot_note(&format!(
                "font missing {} chars: {}",
                missing.len(),
                missing.iter().collect::<String>()
            ));
        }
                let carts_at = Instant::now();
        // The carts the row is actually drawing — not a contiguous run of library indices around
        // the caret. On a machine shelf those two sets are not the same one: this card files its
        // three Game Boy carts at 27, 127 and 637 among nine hundred Advance ones, so a window
        // that walks the library builds nine hundred Advance carts and misses the three on
        // screen, and the row comes up as the blank stand-in. See `Shelf::draw_window`.
        let index = self.session.app().shelf_index();
        let count = self.session.app().carts().len();
        let window = self.session.app().shelf_draw_window();
        let window = if window.is_empty() {
            shelf_window(index, count)
        } else {
            window
        };
        let mut faces: Vec<Option<TexId>> = vec![None; count];
        for &i in &window {
            let f = self.face_for(i);
            faces[i] = Some(compositor.create_texture(f.w, f.h, &f.rgba));
        }
        let fixed_at = Instant::now();
        self.session.boot_note(&format!(
            "cart faces ({} built of {}) {} ms",
            window.len(),
            count,
            carts_at.elapsed().as_millis()
        ));
        self.session.app_mut().set_faces(faces);
        self.shelf_missing = (0..count).filter(|i| !window.contains(i)).collect();
        // The furniture, and the one group that has to be built again when the mode moves:
        // see `upload_fixed`.
        self.upload_fixed(compositor);
        // Everything above is one texture per fixed string or glyph: none of it scales with
        // the card, all of it has to be rasterised before the first frame, and it is the last
        // large block left in the boot once the font and the cart faces are accounted for.
        self.session.boot_note(&format!(
            "fixed faces {} ms",
            fixed_at.elapsed().as_millis()
        ));
        let wall_at = Instant::now();
        self.upload_wallpaper(compositor);
        // The card's wallpaper, decoded and scaled. A card whose wallpaper is a photograph
        // pays for it here, once, in the same way a label used to.
        self.session
            .boot_note(&format!("wallpaper {} ms", wall_at.elapsed().as_millis()));
        // ★ 2026-10-23（六六）：货架页桌面图，画在壁纸之上、卡带之下，替代程序化顶条/底条。
        let desk_at = Instant::now();
        self.upload_desktop(compositor);
        self.session
            .boot_note(&format!("desktop {} ms", desk_at.elapsed().as_millis()));
        self.upload_faces_note(faces_at);
    }

    /// How tall the star a starred cart wears is drawn. Larger than the indicator: it is read
    /// at the size of the cart it sits above, where the indicator is a lamp in a corner.
    const FAV_STAR_PX: f32 = 36.0;
    /// How tall the shelf's indicator is drawn.
    const FAV_IND_PX: f32 = 36.0;
    /// The dark theme's star, and the only thing on the shelf that is neither the case's ink nor
    /// the palette's. A favourite is a fact about the cart rather than about which mode the
    /// frontend is in, so it stays the same colour in both — this one is what it is in the dark,
    /// where a star is drawn and not a heart.
    const FAV_INK: [u8; 3] = [0xff, 0xc4, 0x1e];
    /// The light theme draws a heart instead, because that is what the light cart's own sticker
    /// carries in its corner and the indicator has to match it.
    const HEART_INK: [u8; 3] = [0xf4, 0x79, 0x83];
    /// The same heart unlit: solid, and the case's grey rather than an outline, which on a pale
    /// shelf read as a hole rather than as a shape.
    const HEART_INK_QUIET: [u8; 3] = [0x3c, 0x3c, 0x3c];

    fn upload_fixed(&mut self, compositor: &mut Compositor) {
        // ★ **分组计时**（2026-10-07 六六报"开机变慢"）：这一个函数把**每一个屏幕**的家具
        //   都在第一帧之前烘好，合计 660–970 ms，是开机里最大的一块固定成本。
        //   分成五段记进 `boot.log`，才知道哪一段值得延后 —— 见
        //   `slot-开机慢-诊断与最小加载方案-2026-10-07.md`。
        let g_badges = Instant::now();
        let icons = Icon::ALL
            .iter()
            .map(|i| {
                let f = icon_face(*i, HUD_ICON_PX, slot_ui::palette::ink());
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_icon_faces(icons);
        // The favourites: the star a starred cart wears, and the shelf's indicator, unlit and
        // lit. Uploaded here with the rest of the fixed furniture, because none of the three
        // ever changes — the indicator lights by drawing the other face, not by re-rasterising.
        //
        // Two shapes rather than one, and that is what the mode decides. The dark shelf draws a
        // star, which is what the cart art's own corner carries there; the light shelf draws the
        // heart the light cart art has, so a light-mode cart and its indicator are the same mark.
        // `rebake` runs this again when the mode moves, which is where "fixed" ends.
        let (star, off, on) = match slot_ui::palette::mode() {
            slot_ui::palette::Mode::Dark => {
                let star = icon_face(Icon::Star, Self::FAV_STAR_PX, Self::FAV_INK);
                let star = (
                    compositor.create_texture(star.w, star.h, &star.rgba),
                    star.w,
                    star.h,
                );
                let off = icon_face(Icon::StarOutline, Self::FAV_IND_PX, Self::FAV_INK);
                let off = (
                    compositor.create_texture(off.w, off.h, &off.rgba),
                    off.w,
                    off.h,
                );
                let on = icon_face(Icon::Star, Self::FAV_IND_PX, Self::FAV_INK);
                let on = (compositor.create_texture(on.w, on.h, &on.rgba), on.w, on.h);
                (star, off, on)
            }
            slot_ui::palette::Mode::Light => {
                let star = icon_face(Icon::Heart, Self::FAV_STAR_PX, Self::HEART_INK);
                let star = (
                    compositor.create_texture(star.w, star.h, &star.rgba),
                    star.w,
                    star.h,
                );
                // Solid grey, not an outline: the cart's own corner heart is solid, and an
                // outline beside it read as a different, emptier mark.
                let off = icon_face(Icon::Heart, Self::FAV_IND_PX, Self::HEART_INK_QUIET);
                let off = (
                    compositor.create_texture(off.w, off.h, &off.rgba),
                    off.w,
                    off.h,
                );
                let on = icon_face(Icon::Heart, Self::FAV_IND_PX, Self::HEART_INK);
                let on = (compositor.create_texture(on.w, on.h, &on.rgba), on.w, on.h);
                (star, off, on)
            }
        };
        self.session.app_mut().set_fav_faces(star, off, on);
        // The three machine logos for the shelf indicator, bottom-left, in `GB, GBC, GBA`
        // order. Text for now — the user will bake real machine logos — uploaded here with the
        // rest of the fixed furniture so nothing is rasterised mid-swap.
        // A card that supplies its own badge is used as it is; a card that does not gets the
        // built-in word. Either way the face comes with its size, because the app fits it into
        // the bar's box rather than drawing it at the size it happens to be.
        let carts_dir = self.session.app().root().map(|r| r.join("System/Carts"));
        let shelf_ind: Vec<(TexId, u32, u32)> = [
            ("GB", "type_gb.png"),
            ("GBC", "type_gbc.png"),
            ("GBA", "type_gba.png"),
        ]
        .iter()
        .map(|(word, file)| {
            match carts_dir
                .as_deref()
                .and_then(|d| slot_ui::decode_rgba(&d.join(file)))
            {
                Some((rgba, w, h)) => (compositor.create_texture(w, h, &rgba), w, h),
                None => {
                    let f = word_face(word);
                    (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
                }
            }
        })
        .collect();
        self.session.app_mut().set_shelf_ind_faces(shelf_ind);
        // ★ 2026-10-22（六六）：卡座背层的机种 logo 高光区。素材 `System/Carts/slot_*.png`
        //   里的**洋红** = 要被前端填成主题发光色的高光区；上传前用
        //   `slot_chrome::fill_logo_highlight` 把洋红换成当前主题的 `bright` 色，换主题时
        //   `rebake()` 会重跑这里 ⇒ 高光跟着主题走（这正是六六要的"高光区随主题切换"）。
        //   没有素材的机种就是 `None`，货架照常画、只是没 logo。
        let theme_bright = slot_ui::ridge_ink();
        for (system, file) in [
            (slot_store::System::Gb, "slot_gb.png"),
            (slot_store::System::Gbc, "slot_gbc.png"),
            (slot_store::System::Gba, "slot_gba.png"),
            (slot_store::System::Sfc, "slot_sfc.png"),
            (slot_store::System::Fc, "slot_fc.png"),
        ] {
            let logo = carts_dir
                .as_deref()
                .and_then(|d| slot_ui::decode_rgba(&d.join(file)))
                .and_then(|(mut rgba, w, h)| {
                    // No magenta mark ⇒ no highlight region ⇒ nothing to glow. Treated as
                    // "no logo supplied" rather than uploaded as a flat picture, so a card
                    // that only drew some of the five does not pay for the rest.
                    let marks = slot_ui::fill_logo_highlight(&mut rgba, w, h, theme_bright);
                    (marks > 0).then(|| (compositor.create_texture(w, h, &rgba), w, h))
                });
            self.session.app_mut().set_slot_logo(system, logo);
        }
        // ★★ 2026-10-24（六六）：「**常驻插卡位的各机种背板**」+ 卷闸上的 CHANGE。
        //   背板是六六自己出的不透明成品图（380×44，没有凸起、没有洋红），照原样上传、
        //   不填色；CHANGE 是洋红镂空字，走 `fill_logo_highlight` 换成当前主题的发光色，
        //   于是换主题时（`rebake` 重跑这里）字色跟着走。
        //   没有素材的机种就叫不出背板（`set_slot_back(None)`），洞里照旧只有塑料 ——
        //   现在是 **九张，全机种齐了**（GBA / GB / GBC / SFC / FC / NEOGEO / MD 七张是
        //   10-24～10-25 陆续挂上的，**PS1 与 ARCADE 是 2026-10-26 补齐**）。
        //
        //   ★ 2026-10-25：`slot_back_md.png` 从 10-24 就在卡上了，当时 `System` 还没有 `Md`
        //   而**没有落点**；本轮加机种时把 `DIRS`/`ScreenSystem`/货架槽位/`display.txt` 槽/
        //   货架索引一并补完，这张图**到这一版才真正挂上去**。
        //   ★ 2026-10-26：`slot_back_ps1.png` / `slot_back_arcade.png` 同一个病 —— 六六
        //   「补上 ARCEDE 和 PS1 的插卡位背板（已放进卡里但没显示）」。图早就在卡上
        //   （`System/Carts/`，380×44 RGBA，字节数与桌面原图一致），**缺的只是这张表里的
        //   两行**：`set_slot_back` 没被叫过，`draw_slot_back` 就查不到落点。
        //
        //   ⚠️ 这张表的**键是 `slot_store::System`**，`draw_slot_back` 再按
        //   `self.shelf_system` 去查 —— 两边必须是同一个枚举值。`Arcade` 的货架目录是
        //   `Games/ARCADE`（`root::DIRS`），PS1 是 `Games/PS1`，都对得上。
        for (system, file) in [
            (slot_store::System::Gba, "slot_back_gba.png"),
            (slot_store::System::Gb, "slot_back_gb.png"),
            (slot_store::System::Gbc, "slot_back_gbc.png"),
            (slot_store::System::Sfc, "slot_back_sfc.png"),
            (slot_store::System::Fc, "slot_back_fc.png"),
            (slot_store::System::Neogeo, "slot_back_neogeo.png"),
            (slot_store::System::Md, "slot_back_md.png"),
            (slot_store::System::Ps1, "slot_back_ps1.png"),
            (slot_store::System::Arcade, "slot_back_arcade.png"),
        ] {
            let face = carts_dir
                .as_deref()
                .and_then(|d| slot_ui::decode_rgba(&d.join(file)))
                .map(|(rgba, w, h)| (compositor.create_texture(w, h, &rgba), w, h));
            self.session.app_mut().set_slot_back(system, face);
        }
        // ★★ 2026-10-27（六六）：「这是街机和 NEOGEO 的代币（等同于卡带/光盘）缺省图；
        //   `SLOT_LOAD_*` 是投币动画要用到的素材」。
        //
        //   每台机种一套三张：
        //   * `cab` —— 从插卡口升起的街机箱体。洋红区走 `fill_logo_highlight` 换成本主题
        //     亮色（六六：「洋红区发光，随主题色板」），跟 CHANGE 字、卷闸缝高光同一个色源。
        //   * `glow` —— **只跟洋红区走**的一圈柔光。不能拿整张箱体去 `glow_face_spread`：
        //     那是按 alpha 糊的，光会顺着箱子的外轮廓绕一圈，读成"箱子在发光"而不是
        //     "它的屏幕/招牌亮着"。所以先 `mark_only` 切出洋红遮罩再烘。
        //
        //   ★ 2026-10-27 晚：**代币不在这里烘了**。六六「换随机色吧」⇒ 代币直接复用货架
        //     那张卡面（缺省图按游戏自己的随机色填），进槽那枚和货架上那枚是同一个纹理。
        let mut coins = Vec::new();
        if let Some(dir) = carts_dir.as_deref() {
            for (system, cab_file) in [
                (slot_store::System::Neogeo, "SLOT_LOAD_NEOGEO.png"),
                (slot_store::System::Arcade, "SLOT_LOAD_ARCADE.png"),
            ] {
                let Some((mut cab, w, h)) = slot_ui::decode_rgba(&dir.join(cab_file)) else {
                    continue;
                };
                let (mark, marks) = slot_ui::mark_only(&cab, w, h);
                let glow = if marks > 0 {
                    slot_ui::glow_face_spread(&mark, w, h, theme_bright, 12, 4, 0.55)
                        .map(|(g, gw, gh)| (compositor.create_texture(gw, gh, &g), gw, gh))
                } else {
                    None
                };
                slot_ui::fill_logo_highlight(&mut cab, w, h, theme_bright);
                coins.push((
                    system,
                    Coin {
                        cab: (compositor.create_texture(w, h, &cab), w, h),
                        glow,
                    },
                ));
            }
        }
        self.session.app_mut().set_coin_assets(coins);
        let mut change = None;
        let mut change_glow = None;
        if let Some((mut rgba, w, h)) = carts_dir
            .as_deref()
            .and_then(|d| slot_ui::decode_rgba(&d.join("slot_change.png")))
        {
            // 洋红 → 本主题的亮色（字与光同一个色）。
            slot_ui::fill_logo_highlight(&mut rgba, w, h, theme_bright);
            // ★ 2026-10-24 20:42（六六）：「CHANGE 出现的时候能发光吗」—— 开销全放在开机：
            //   字的 alpha 糊开、着成同一个亮色，烘成一张比字大一圈（`GLOW_PAD`）的贴图。
            change_glow = slot_ui::glow_face(&rgba, w, h, theme_bright)
                .map(|(g, gw, gh)| (compositor.create_texture(gw, gh, &g), gw, gh));
            change = Some((compositor.create_texture(w, h, &rgba), w, h));
        }
        self.session.app_mut().set_change_face(change, change_glow);
        // ★ 2026-10-26（六六）：「给所有机种的插卡/插光盘动画加上货架区的呼吸发光文字
        //   （颜色随主题）」。
        //   `slot_loading.png` 是 **720×240 的成品图**（"GAME START / LOADING..." 空心字），
        //   纯洋红 #FF00FF、带抗锯齿（alpha 119 那圈），所以走和 CHANGE 完全一样的两步：
        //   ① `fill_logo_highlight` 把洋红像素换成本主题亮色（保留 alpha ⇒ 边缘不毛）；
        //   ② `glow_face_spread` 烘一圈柔光 —— 但这张是 720 宽的大字，
        //      CHANGE 那套 `GLOW_PAD` 6 / 半径 2 只糊出 4px，等于没有光，所以开大：
        //      留白 14、半径 4、上限 0.55。
        //   不按机种分：所有机种（含 PS1 的光盘）共用这一张。
        let mut loading = None;
        let mut loading_glow = None;
        if let Some((mut rgba, w, h)) = carts_dir
            .as_deref()
            .and_then(|d| slot_ui::decode_rgba(&d.join("slot_loading.png")))
        {
            slot_ui::fill_logo_highlight(&mut rgba, w, h, theme_bright);
            loading_glow = slot_ui::glow_face_spread(&rgba, w, h, theme_bright, 14, 4, 0.55)
                .map(|(g, gw, gh)| (compositor.create_texture(gw, gh, &g), gw, gh));
            loading = Some((compositor.create_texture(w, h, &rgba), w, h));
        }
        self.session.app_mut().set_loading_face(loading, loading_glow);
        // The direct-connect dialog's five lines, baked with the fixed furniture rather than on
        // the way in: the dialog is opened by a hold, and a hold is not a moment to rasterise
        // text. `System/slotnet.sh` does the work — this is only what it says while it does.
        let direct_faces: Vec<(TexId, u32, u32)> = [
            lang::DIRECT_TITLE,
            lang::DIRECT_PROMPT,
            lang::DIRECT_WAITING,
            lang::DIRECT_READY,
            lang::DIRECT_HINT,
        ]
        .iter()
        .map(|line| {
            let f = page_face(line);
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        })
        .collect();
        self.session.app_mut().set_direct_faces(direct_faces);
        // ★ 2026-10-21：Wi-Fi 配置屏的字符网格字形 —— 四层每一个字符一张脸，外加密码的
        //   掩码点。开机烤好永不变：网格每帧都要画，而输进去的串也是拿这些字形拼的，
        //   按一个键不该 rasterise 任何东西。
        let mut glyph_chars = String::new();
        for layer in crate::app::WIFI_LAYERS {
            for c in layer.chars.chars() {
                if c != ' ' && !glyph_chars.contains(c) {
                    glyph_chars.push(c);
                }
            }
        }
        glyph_chars.push('•');
        let wifi_glyphs: Vec<(char, TexId, u32, u32)> = glyph_chars
            .chars()
            .map(|c| {
                let f = word_face(&c.to_string());
                (c, compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_wifi_glyphs(wifi_glyphs);
        // The badge atlas: one PNG holding a grid of emoji, which `carts.tsv` indexes by
        // number. Decoded only when the table actually names a badge — it is the one upload
        // on the shelf measured in megabytes, and a card that asks for no glyph should not
        // pay for the whole set. Absent, a rule that names one asks for nothing and its cart
        // goes unmarked.
        let badge_atlas = carts_dir
            .as_deref()
            .filter(|_| slot_ui::cart_rules_wants_badges())
            .and_then(|d| slot_ui::decode_rgba(&d.join("emoji.png")))
            .map(|(rgba, w, h)| (compositor.create_texture(w, h, &rgba), w, h));
        self.session.app_mut().set_badge_atlas(badge_atlas);

        // The disc row's badge: the word "Disc", or the card's own `type_disc.png`. Built here
        // beside the machine logos because that is where the compositor is, and because the row's
        // badge must land in the same box as theirs — a badge that shifted when the row came up
        // would read as the whole panel moving.
        //
        // **A disc has no machine, so this is one face and not three.** The machine logos are a
        // per-machine set indexed by `System`; there is no index a disc could be, and borrowing
        // `shelf_system`'s put the word GBA under a row of music.
        let mut disc_badge = |file: &str| -> (TexId, u32, u32) {
            let from_card = carts_dir
                .as_deref()
                .and_then(|d| slot_ui::decode_rgba(&d.join(file)));
            match from_card {
                Some((rgba, w, h)) => (compositor.create_texture(w, h, &rgba), w, h),
                // The same built-in word the machine logos fall back to, so a card with no
                // `Carts/type_*.png` at all is unchanged rather than blank.
                None => {
                    let f = word_face("Disc");
                    (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
                }
            }
        };
        self.session.app_mut().set_disc_ind_faces(
            Some(disc_badge("type_disc.png")),
            Some(disc_badge("type_disc_light.png")),
        );
        // The light theme's three, drawn separately by the card rather than tinted from the dark
        // ones. A card without them falls back to the built-in word as well, so a switch to
        // light mode on such a card changes nothing but the rest of the palette.
        let shelf_ind_light: Vec<(TexId, u32, u32)> = [
            ("GB", "type_gb_light.png"),
            ("GBC", "type_gbc_light.png"),
            ("GBA", "type_gba_light.png"),
        ]
        .iter()
        .map(|(word, file)| {
            match carts_dir
                .as_deref()
                .and_then(|d| slot_ui::decode_rgba(&d.join(file)))
            {
                Some((rgba, w, h)) => (compositor.create_texture(w, h, &rgba), w, h),
                None => {
                    let f = word_face(word);
                    (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
                }
            }
        })
        .collect();
        self.session.app_mut().set_shelf_ind_light(shelf_ind_light);
        // Its own upload rather than one of the HUD's: it is drawn on a cart, at its own
        // size, and in a warning colour the level glyphs have no business borrowing.
        let alert = icon_face(Icon::Alert, ALERT_PX, ALERT_INK);
        let alert = compositor.create_texture(alert.w, alert.h, &alert.rgba);
        self.session.app_mut().set_alert_face(alert);
        self.session
            .boot_note(&format!("fixed/badges {} ms", g_badges.elapsed().as_millis()));
        // ⚠️ **The menus / power / core / link group used to be baked here** (85 ms measured on
        //    the device) and now lives in `upload_late_furniture`, one frame later — nothing the
        //    shelf draws with is in it.
        let g_shelf = Instant::now();
        // Built for the shelf the device came up on. A shelf switch later in the session changes
        // the row's geometry under them, which `set_shelf_geometry` re-cuts.
        let sys = self.session.app().shelf_system();
        let size = self.session.app().shelf_size();
        // The blank cart every not-yet-built face is drawn as. A jump across the alphabet
        // crosses carts no filler will ever reach in time, and this is what slides past in
        // their place — a cartridge, rather than the label's colour as a bar of paint.
        let blank = cart_placeholder(size, sys);
        let blank = compositor.create_texture(blank.w, blank.h, &blank.rgba);
        if let Some(old) = self.session.app_mut().set_cart_placeholder(blank) {
            compositor.release_texture(old);
        }
        // ★ 2026-10-21：**发光件**，三张，一次烘完。
        //
        //   尺寸都是定值（720×2 / 640×48 / 400×20），在这段 660–970 ms 的固定成本里可以
        //   忽略不计，但它们是货架和游戏菜单"通着电"的全部来源。走 `create_texture` 而不是
        //   `_nearest`：渐变的每一级都要插值，最近邻会把一条平滑的衰减切成台阶。
        //
        //   颜色是 `slot_ui::light` 里那一支琥珀，**不跟 look 走** —— 换壳不换灯珠。
        let (pw, ph, pbuf) = slot_ui::power_line(slot_ui::OUT_W);
        let (gw, gh, gbuf) = slot_ui::title_glow(slot_ui::SHELF_TITLE_W, slot_ui::SHELF_TITLE_H + 8);
        // 脚下柔光池：横向烘宽一点（400），画的时候按卡带宽度拉 —— 横向是线性衰减，
        // 拉宽只是把同一段衰减摊开，不会变形；竖向只有 20，竖向一拉就成墨点了。
        let (ow, oh, obuf) = slot_ui::pool(400, 20);
        let power = (compositor.create_texture(pw, ph, &pbuf), pw, ph);
        let glow = (compositor.create_texture(gw, gh, &gbuf), gw, gh);
        let pool = compositor.create_texture(ow, oh, &obuf);
        self.session
            .app_mut()
            .set_light_faces(crate::app::LightFaces {
                power: Some(power),
                glow: Some(glow),
                pool: Some(pool),
            });
        self.session
            .boot_note(&format!("fixed/shelf frame {} ms", g_shelf.elapsed().as_millis()));
        let g_letters = Instant::now();
        // `draw_gauge` now draws the bolt beside the capsule, on the housing, in its own
        // reserved slot rather than over the fill. The housing tint was only ever needed to
        // hide the bolt inside the fill it sat on; out here it sits where every other HUD
        // glyph does, so it takes the same ink they do.
        let bolt = icon_face(Icon::Charging, BOLT_PX, slot_ui::palette::ink());
        let bolt_id = compositor.create_texture(bolt.w, bolt.h, &bolt.rgba);
        self.session.app_mut().set_bolt_face(bolt_id);
        // The letter strip: one face per slot of a fixed alphabet — twenty-seven small
        // textures against the cart faces' one apiece — and the one piece of metal the strip
        // repeats between them. None of them can ever change, so they are built here with the
        // rest of the fixed furniture rather than lazily the way a cart's is.
        //
        // The ridge is the only *texture* the strip carries: the band the letters stand in is
        // the machine's own, drawn from the cart bay's numbers every frame by `slot_chrome`.
        let letter_faces = letters::SLOTS
            .iter()
            .map(|ch| {
                let f = letters::letter_face(*ch);
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_letter_faces(letter_faces);
        let ridge = letters::ridge_face();
        let ridge = (
            compositor.create_texture(ridge.w, ridge.h, &ridge.rgba),
            ridge.w,
            ridge.h,
        );
        self.session.app_mut().set_letter_ridge_face(ridge);
        // ★ 2026-10-24（六六）：字母轮的**胶囊背板**（两端半圆），与 ridge 同一批烘。
        let pill = letters::pill_face();
        let pill = (
            compositor.create_texture(pill.w, pill.h, &pill.rgba),
            pill.w,
            pill.h,
        );
        self.session.app_mut().set_letter_pill_face(pill);
        // ⚠️ **The shortcut card, the menu's buttons and the deck's own furniture used to be
        //    baked here** and now live in `upload_late_furniture`, one frame later — the shelf
        //    draws none of them.
        self.session
            .boot_note(&format!("fixed/letters {} ms", g_letters.elapsed().as_millis()));
    }

    /// **The furniture every screen but the shelf draws with**, baked one frame after the first.
    ///
    /// # Why this is a frame late rather than at boot
    ///
    /// `upload_fixed` bakes everything this frontend can ever draw, and it did so *before* the
    /// first frame — measured on the device at **910 ms** (2026-10-07, a 968-game card). Most of
    /// that is the shelf's own and has to stay: the HUD icons (which pay for parsing a 2.5 MB
    /// symbol font), the machine badges, the cart frame, the letter strip. What did **not** have
    /// to be there is the group below — the menus, the power menu, the core picker, the link
    /// screens, the toasts, the shortcut card, the menu's buttons and the deck's own furniture —
    /// every one of them a screen the shelf never shows.
    ///
    /// **One frame is enough to make them free.** By the second frame the shelf is on screen and
    /// none of these is on it, so the bake is invisible; and no hand reaches a key before it
    /// finishes.
    ///
    /// ⚠️ **Deliberately not a phase gate.** The power menu and a toast both come up *over the
    /// shelf*, so "the user has left the shelf" is not the condition that makes these safe to
    /// defer — being one frame behind the first frame is. A phase gate would leave those two
    /// blank on the one screen they are most likely to be used from.
    ///
    /// Called from `render` on the frame after the first, and from `rebake` — a theme change has
    /// to reprint all of it and cannot wait for a frame.
    fn upload_late_furniture(&mut self, compositor: &mut Compositor) {
        let at = Instant::now();
        // A shutdown is the one moment there is no time to rasterise anything, and the GPU is
        // about to be taken away. One line per choice, in `PowerChoice::ALL` order, at the
        // menu's own size so the screen that follows a choice is set in the same voice as the
        // row that was chosen.
        let lines = PowerChoice::ALL
            .iter()
            .map(|c| {
                let f = menu_face(match c {
                    PowerChoice::Restart => lang::SHUTDOWN_RESTART,
                    PowerChoice::PowerOff => lang::SHUTDOWN_POWER_OFF,
                });
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_shutdown_faces(lines);
        // ★ 2026-10-29（六六）：电源菜单的选项**恒定白色**（`menu_face_white`），不随主题/
        // 明暗 —— 背景是固定的霓虹隧道图，选项若还随主题，亮色主题下深色字会压在图中央的
        // 黑上看不见。`shutdown_faces` 上面那组仍走 `menu_face`（主题墨），那是关机后的
        // 整屏提示、不在这个固定背景上。
        let menu = PowerChoice::ALL
            .iter()
            .map(|c| {
                let f = menu_face_white(c.text());
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_power_menu_faces(menu);
        // ★ 2026-10-29（六六）：电源菜单的背景图，整屏固定图。它不随主题，所以只需上传一次
        // —— 但 `upload_late_furniture` 在换主题时也会跑（`rebake`），重新上传同一张图没有
        // 成本以外的坏处，且幂等（同样的字节、同样的纹理）。
        if let Some(bg) = slot_ui::power_bg_face() {
            let id = compositor.create_texture(bg.w, bg.h, &bg.rgba);
            self.session.app_mut().set_power_bg_face(id);
        }
        // The open cart's parts that never change: each socket, the chip seated in each, the
        // blank chip in flight and its shadow, in `Core::ALL` order. Baked ahead of the picker
        // so the first frame of a lid coming off is not spent in a rasteriser.
        let sockets = slot_store::Core::ALL
            .iter()
            .map(|c| {
                let f = socket_face(*c);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        let chips = slot_store::Core::ALL
            .iter()
            .map(|c| {
                let f = chip_face(Some(*c));
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        let blank = chip_face(None);
        let blank = compositor.create_texture(blank.w, blank.h, &blank.rgba);
        let shadow = chip_shadow_face();
        let shadow = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
        self.session
            .app_mut()
            .set_core_part_faces(sockets, chips, blank, shadow);
        // Every action the picker takes, the way out first and the choice last, as the
        // switcher's legend is ordered.
        let legend = [
            hint_face("B", lang::CORE_CANCEL),
            arrows_hint_face(lang::CORE_SWAP),
            hint_face("A", lang::CORE_CHOOSE),
        ]
        .into_iter()
        .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
        .collect();
        self.session.app_mut().set_core_legend_faces(legend);
        // The in-game menu, its two link rows, and the sentences the screen says while a link is
        // coming up or after it did not — the same size and the same rasteriser as the two menus
        // above, because they are the same object.
        // ★ 2026-10-21：**这一排的字跟机种走** —— `Grid` 那一行在 GBA 是「网格切换」、在
        // GB/GBC 是「遮罩切换」。这一遍印的时候还在货架上（`seated_system` 读作 GBA），插上
        // GB/GBC 卡之后由 `sync_game_menu_rows` 整排重印一次。
        //
        // ★★ 傍晚第二版：每行印**两张** —— 静止的一张、手里那张亮底的一张
        // （`slot_ui::game_row_face(label, w, hot)`）。宽度是格子的宽，一排按钮因此一样宽。
        let system = self.session.app().seated_system();
        let texts: Vec<&'static str> = GameRow::ALL.iter().map(|r| r.text_for(system)).collect();
        let (idle, hot) = game_menu_rows(compositor, &texts);
        self.session.app_mut().set_game_menu_faces(idle, hot);
        self.game_menu_system = Some(system);
        let link = menu_faces(compositor, LinkRow::ALL.iter().map(|r| r.text()));
        self.session.app_mut().set_link_menu_faces(link);
        let steps = menu_faces(compositor, LinkStep::ALL.iter().map(|s| s.line()));
        self.session.app_mut().set_link_step_faces(steps);
        let fails = menu_faces(compositor, LinkFail::SHOWN.iter().map(|f| f.line()));
        self.session.app_mut().set_link_fail_faces(fails);
        let toasts = Toast::ALL
            .iter()
            .map(|t| {
                let f = toast_face(*t);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_toast_faces(toasts);
        // The cheat table's "this cart has no codes" panel, so opening it never waits on a font.
        let empty = menu_face("NO CHEATS");
        let empty = (
            compositor.create_texture(empty.w, empty.h, &empty.rgba),
            empty.w,
            empty.h,
        );
        self.session.app_mut().set_cheat_empty_face(empty);
        let legend = legend_faces(compositor, &LEGEND);
        self.session.app_mut().set_legend_faces(legend);
        let menus_ms = at.elapsed().as_millis();

        // The shortcut card: twenty-odd rows of fixed string, none of which can ever change, and
        // opening a help screen is the worst moment to be asking a font for them.
        let rows = SHORTCUT_ROWS
            .iter()
            .map(|r| {
                let f = shortcut_row_face(*r);
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        let hint = shortcut_hint_face(SHORTCUT_HINT);
        let hint = (
            compositor.create_texture(hint.w, hint.h, &hint.rgba),
            hint.w,
            hint.h,
        );
        self.session.app_mut().set_shortcut_faces(rows, Some(hint));

        // The menu's buttons. A button is a rounded plate with a word on it, and rounding it is a
        // per-pixel pass that has no business running while the user is deciding something.
        // **Both faces of every button are baked**, because which one is in hand is not known
        // until it is pressed and a button rasterised on the press would arrive a frame late.
        let mut buttons: [Option<MenuButton>; slot_ui::menu::SLOTS] = [None; slot_ui::menu::SLOTS];
        for (i, slot) in slot_ui::menu::Item::GRID.iter().enumerate() {
            let Some(item) = slot else {
                continue;
            };
            let b = slot_ui::menu::button(*item);
            buttons[i] = Some(MenuButton {
                idle: (
                    compositor.create_texture(b.idle.w, b.idle.h, &b.idle.rgba),
                    b.idle.w,
                    b.idle.h,
                ),
                chosen: (
                    compositor.create_texture(b.chosen.w, b.chosen.h, &b.chosen.rgba),
                    b.chosen.w,
                    b.chosen.h,
                ),
            });
        }
        // One recess for the whole grid: every empty slot is the same empty slot.
        let empty = slot_ui::menu::empty();
        let empty = (
            compositor.create_texture(empty.w, empty.h, &empty.rgba),
            empty.w,
            empty.h,
        );
        let menu_hint = shortcut_hint_face(slot_ui::menu::HINT);
        let menu_hint = (
            compositor.create_texture(menu_hint.w, menu_hint.h, &menu_hint.rgba),
            menu_hint.w,
            menu_hint.h,
        );
        self.session
            .app_mut()
            .set_menu_faces(buttons, Some(empty), Some(menu_hint));

        // The settings screen's rows. Same bargain the buttons above make and for the same
        // reason: **every value a row can take is baked, not the one it has**. The row is a
        // 660-wide rounded plate, so changing a value on a keypress would be a per-pixel corner
        // pass on the frame the user is looking at; walking a four-long list would do it four
        // times. Two sets of three gears, four lid values, one word and one switch is thirteen
        // plates, which is less than the shelf's own furniture. The two foot lines are the menu's
        // renderer, so they cost the same as its one.
        //
        // Every list is the card's (`slot_store`'s three `*_CHOICES`), handed in rather than
        // written here — `settings::values` has no idea what a card is.
        // ★ 2026-10-20：开机画面那一行的值**是卡上的目录**，不是一张写死的表 ——
        // 和三个档位表一样从外面递进去（`settings::values` 从没听说过卡）。
        // boot 时扫一次就够：图是用户从电脑上放的，slot 跑着的时候不会多出来。
        let logos = self.session.app().boot_labels();
        let rows = slot_ui::settings::Row::ALL
            .iter()
            .map(|row| {
                slot_ui::settings::values(
                    *row,
                    &DOZE_MIN_CHOICES,
                    &FF_SPEED_CHOICES,
                    &REWIND_SPEED_CHOICES,
                    &logos,
                )
                    .iter()
                    .map(|value| {
                        let f = slot_ui::settings_value_face(*row, value);
                        [
                            (
                                compositor.create_texture(f.idle.w, f.idle.h, &f.idle.rgba),
                                f.idle.w,
                                f.idle.h,
                            ),
                            (
                                compositor.create_texture(f.chosen.w, f.chosen.h, &f.chosen.rgba),
                                f.chosen.w,
                                f.chosen.h,
                            ),
                        ]
                    })
                    .collect()
            })
            .collect();
        let change = shortcut_hint_face(slot_ui::SETTINGS_HINT_CHANGE);
        let change = (
            compositor.create_texture(change.w, change.h, &change.rgba),
            change.w,
            change.h,
        );
        let open = shortcut_hint_face(slot_ui::SETTINGS_HINT_OPEN);
        let open = (
            compositor.create_texture(open.w, open.h, &open.rgba),
            open.w,
            open.h,
        );
        // ★ **The third line** (2026-10-17), for the refresh row: `A` does the thing rather than
        // opening a screen, so neither of the other two would say the truth. Uploaded here with
        // the rest of the fixed furniture — a line of type that is the same on every boot has no
        // business being rasterised while a key is down.
        let act = shortcut_hint_face(slot_ui::SETTINGS_HINT_ACT);
        let act = (
            compositor.create_texture(act.w, act.h, &act.rgba),
            act.w,
            act.h,
        );
        // ★ **The fourth line** (2026-10-20), for the boot row: `←`/`→` look, `A` is what
        // writes the picture into the machine, `B` is what throws the looking away. The row's
        // keys are not this screen's keys, so none of the other three lines fits it.
        let pick = shortcut_hint_face(slot_ui::SETTINGS_HINT_PICK);
        let pick = (
            compositor.create_texture(pick.w, pick.h, &pick.rgba),
            pick.w,
            pick.h,
        );
        self.session
            .app_mut()
            .set_settings_faces(rows, Some(change), Some(open), Some(act), Some(pick));

        // The deck's furniture, so play and pause never wait on a raster pass.
        //
        // **The sleeve comes from `cd_art`, not from `cd::placeholder`.** This line used to be
        // the placeholder, and the card's own art was loaded, reported correctly at boot
        // (`disc art: disc 340x203+label`) and then never drawn — the deck was showing the
        // built-in cassette the whole time. Nothing said so: the placeholder is a plausible
        // cassette, so a wrong face looks like a working one.
        //
        // The size is the deck's own, not the shelf's: `deck::FACE_W/H` is how big the sleeve is
        // on the card at 1:1 with the art, which is what "a deck's window" means.
        let disc = slot_ui::cd_face(&[], &slot_ui::cd::SHELL, None);
        let disc = (
            compositor.create_texture(disc.w, disc.h, &disc.rgba),
            disc.w,
            disc.h,
        );
        let name_u = slot_ui::word_face(lang::MENU_DISCS);
        let name = CartFace {
            rgba: name_u.rgba,
            w: name_u.w,
            h: name_u.h,
        };
        let name = (
            compositor.create_texture(name.w, name.h, &name.rgba),
            name.w,
            name.h,
        );
        let key = slot_ui::deck::key_face();
        let key = (
            compositor.create_texture(key.w, key.h, &key.rgba),
            key.w,
            key.h,
        );
        self.session
            .app_mut()
            .set_deck_faces(disc, Some(name), key);

        // ★★ 2026-10-26（六六）：**机种面板**（长按 Y 打开的那一块）。
        //
        // 十张标识来自卡上（`System/Carts/SLOT_SYS_*.png`，见 `root::sys_badge_file`），
        // 两张按钮底 + 一圈光在这里画（帘子本身每一帧现画，没有可烘的东西）。烘在**这一批**
        // 而不是开机那一批：长按 Y 至少要按够 500 ms，而这一批在开机后第一帧就跑，时间上
        // 永远追得上；塞进开机路径只会从那 4.4 秒里挖走几十毫秒。
        //
        // **标识按 `icon_px`（放大态的 74）预缩，不是原样上传。** 原图 120 直接画到 62 是
        // 一条 0.5 倍降采样，双线性在降采样里会啃掉细笔画，而这十张全是细笔画的白标识。
        // 预缩用的是 `art::stretch` 的盒式平均 —— 它缩出来的 74 再由 GPU 缩到 62，
        // 剩下的 0.84 倍几乎无损。
        //
        // 每个机种烘**两份**：卡上那张白图给深色主题，同一个形状染成墨色给亮色主题。
        // 白标识压在亮色主题的浅塑料格子上等于没有标识（0xCF 的底），而卡上本来就同时
        // 带着 `type_gb.png` 与 `type_gb_light.png` 这样一对 —— 机种标识从来是两套。
        let icon_px = slot_ui::sys_picker_icon_px();
        let carts_dir = self.session.app().root().map(|r| r.join("System/Carts"));
        let ink = slot_ui::palette::ink();
        let sys_icons: Vec<Option<(crate::app::Upload, crate::app::Upload)>> = root::MACHINES
            .iter()
            .map(|system| {
                let path = carts_dir.as_deref()?.join(root::sys_badge_file(*system));
                let rgba = slot_ui::art_stretch(&path, icon_px, icon_px)?;
                let dark = (
                    compositor.create_texture(icon_px, icon_px, &rgba),
                    icon_px,
                    icon_px,
                );
                let lit = slot_ui::sys_picker_ink_icon(&rgba, ink);
                let light = (
                    compositor.create_texture(icon_px, icon_px, &lit),
                    icon_px,
                    icon_px,
                );
                Some((dark, light))
            })
            .collect();
        let idle = slot_ui::sys_picker_button(false);
        let idle = (
            compositor.create_texture(idle.w, idle.h, &idle.rgba),
            idle.w,
            idle.h,
        );
        let hot = slot_ui::sys_picker_button(true);
        let hot = (
            compositor.create_texture(hot.w, hot.h, &hot.rgba),
            hot.w,
            hot.h,
        );
        let (gw, gh, gbuf) = slot_ui::sys_picker_glow();
        let glow = (compositor.create_texture(gw, gh, &gbuf), gw, gh);
        // ★★ 2026-10-27（六六）：标题用卡上的贴图（`System/Carts/SLOT_SELECT.png`，品红
        // 标记 → 主题墨色）。没有那张图就回退到渲染文字 —— 老卡不背这张图时标题照旧，
        // 而一张有图的卡永远用图（用户要的就是这个像素字，不是系统渲染的笔画）。
        // 贴图是 720×120 全宽铺在顶部，画的时候 1:1，不需要预缩（与机种标识相反：标识要
        // 缩到格子里，标题是固定全宽）。
        let title = carts_dir
            .as_deref()
            .and_then(|d| slot_ui::sys_picker_title_face(d, ink))
            .map(|(rgba, w, h)| (compositor.create_texture(w, h, &rgba), w, h))
            .unwrap_or_else(|| {
                // 标题比货架标题大一档（36），因为它是这一屏唯一的句子，而它下面那十格
                // 全是图形 —— 字号不给够，一行标题会和第一排格子读成同一层。
                let t = menu_face_at(lang::SYS_PICKER_TITLE, 36.0, 46, 18);
                (compositor.create_texture(t.w, t.h, &t.rgba), t.w, t.h)
            });
        // 脚下一排，与核心选择器**同序**（出口在前、选择在后）：这台机器上两块"选一件东西"
        // 的屏，手的走法应该一样。
        let foot: Vec<(TexId, u32)> = [
            hint_face("B", lang::CORE_CANCEL),
            arrows_hint_face(lang::SYS_PICKER_MOVE),
            hint_face("A", lang::CORE_CHOOSE),
        ]
        .into_iter()
        .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
        .collect();
        self.session.app_mut().set_sys_picker_faces(
            sys_icons,
            slot_ui::SysPickerFaces {
                idle,
                hot,
                glow,
                title: Some(title),
                foot,
            },
        );

        self.session.boot_note(&format!(
            "late furniture {} ms (menus+power+core+link {} ms)",
            at.elapsed().as_millis(),
            menus_ms
        ));
    }

    /// The mode moved: re-rasterise everything whose ink is burned in, and drop the caches of
    /// everything that will be rasterised again on demand.
    ///
    /// Two halves, and both are needed. `upload_fixed` + `upload_late_furniture` between them
    /// cover the faces that only exist once — the glyphs, the menus, the letters, the shortcut
    /// card. The caches below cover the faces that are built lazily as the screens need them:
    /// the shelf's title, the clock and the charge on the band, the about card, the switcher's
    /// caption. Clearing them is what makes the *next* frame rebuild them rather than the last
    /// one; the four defaults are the value each compares against to decide it has nothing worth
    /// keeping.
    ///
    /// The cart faces are deliberately not in either half. A label is a picture and a shell is a
    /// colour off the game's code: nothing about a cartridge is printed in the palette, which is
    /// the same reason `upload_faces` can put nine hundred of them outside the boot's critical
    /// path.
    ///
    /// Rasterise the fixed furniture again, in whatever mode is now set, and rebuild the cart
    /// faces the row is drawing.
    ///
    /// The whole reason a theme change does not need a restart: the type is a texture, so the
    /// ink cannot move under it, so the textures have to be made again. What it deliberately
    /// does *not* touch is the font and the labels — a label is a picture and has no palette in
    /// it, and asking for nine hundred of them again would be a second of boot budget spent on
    /// colours they never had.
    fn rebake(&mut self, compositor: &mut Compositor) {
        let at = Instant::now();
        self.upload_fixed(compositor);
        // ★ **换主题要把全部固定家具重印，包括延后那一批。** 开机时它们晚一帧没关系
        //   （货架已经在屏上、且用不到它们），但**换主题是用户正看着的一次变化**，
        //   晚一帧就是肉眼可见的"一半新一半旧"。`late_faces` 一并置真，免得下一帧再烘一遍。
        self.late_faces = true;
        self.upload_late_furniture(compositor);
        // ★ 2026-10-28：默认壁纸的**底图和光**都是按主题亮色做的 —— 换主题要整条重做，不然
        //   底图还留着上一个主题的色、光也是。旧贴图由 `bake_wall_default` 自己释放。
        if self.wall_default {
            self.bake_wall_default(compositor);
        }
        // The row's frame bakes a palette colour too — the blank cart is the housing's on the
        // light theme — so a theme change re-cuts it exactly as a machine swap does. Without
        // this the stand-in kept the other theme's colour until something else rebuilt it, and
        // a letter jump is mostly stand-ins: that is where it showed.
        self.rebake_row_frame(compositor);
        self.title_tex = None;
        self.shelf_title_tex = None;
        self.shelf_titled = None;
        self.switcher = Switcher::default();
        self.clocks = Clocks::default();
        self.about = AboutFace::default();
        // Cart faces: the visible window is rebuilt in place; everything outside it is released
        // and cleared so the resident pump rebuilds it on demand. `resident_at = None` forces
        // `sync_resident_faces` to rescan on the next frame and re-queue the dropped faces.
        let count = self.session.app().carts().len();
        let index = self.session.app().shelf_index();
        let window = self.session.app().shelf_draw_window();
        let window = if window.is_empty() {
            shelf_window(index, count)
        } else {
            window
        };
        for i in 0..count {
            if window.contains(&i) {
                let f = self.face_for(i);
                let tex = compositor.create_texture(f.w, f.h, &f.rgba);
                self.session.app_mut().set_face(i, tex);
            } else if let Some(tex) = self.session.app().face_of(i) {
                compositor.release_texture(tex);
                self.session.app_mut().clear_face(i);
            }
        }
        self.resident_at = None;
        self.session.boot_note(&format!(
            "reprinted for the mode in {} ms",
            at.elapsed().as_millis()
        ));
    }

    /// A machine shelf swapped in mid-session: re-cut the row's frame for the machine that is up
    /// and build the carts the new row draws.
    ///
    /// Three things were minted for the shelf the device came up on and are wrong the moment
    /// another one is swapped in: the shadow under a dimmed cart, the blank cart a not-yet-built
    /// face stands in as (an Advance cart is landscape and a Game Boy cart portrait, so the
    /// stand-in is visibly the wrong object), and the set of faces worth having. All three are
    /// rebuilt here, in the one place with a compositor.
    ///
    /// The faces are the reason this exists rather than being cosmetic. A swap changes the view
    /// to a *subset* of the library, and the filler orders its work by ring distance in the
    /// library — so the carts on screen are not necessarily the ones it would reach first, and
    /// until it gets there the row shows the stand-in. Building the seven the row draws, here,
    /// means the shelf is right on the frame it appears rather than at some point after.
    fn set_shelf_geometry(&mut self, compositor: &mut Compositor) {
        let at = Instant::now();
        let sys = self.session.app().shelf_system();
        self.rebake_row_frame(compositor);
        // The faces need nothing here, and that is the point of measuring in the shelf rather
        // than in the library. `Shelf::shelf_distance` counts from the front of *every*
        // machine's shelf as well as from the live caret, so the row this swap is about to show
        // has been resident the whole time. Measuring in the library evicted it as "too far
        // away", and rebuilding it here cost 129-179 ms on the render thread, in the middle of
        // a 0.68 s swap animation. Nothing to build now, so nothing to stall on.
        let count = self.session.app().carts().len();
        // A rescan is still wanted, so the queue matches the new view rather than the old one.
        self.resident_at = None;
        // The row, as the draw sees it: which carts are in the slots, and which of those are
        // holding a face rather than the stand-in. A slot with `N` here is the stand-in on
        // screen, and a slot index that is not in `window` means the two are answering
        // different questions.
        let slots: Vec<usize> = self.session.app().shelf_slot_carts();
        let marks: String = slots
            .iter()
            .map(|&i| {
                if self.session.app().face_of(i).is_some() {
                    'Y'
                } else {
                    'N'
                }
            })
            .collect();
        self.session.boot_note(&format!(
            "shelf {} lib {} view {} idx {} slots {:?} faces {}",
            sys.dir_name(),
            count,
            self.session.app().shelf_view_len(),
            self.session.app().shelf_index(),
            slots,
            marks
        ));
        self.session.boot_note(&format!(
            "shelf geometry {} in {} ms",
            sys.dir_name(),
            at.elapsed().as_millis()
        ));
    }

    /// The row's own frame — the shadow under a dimmed cart, and the blank cart a not-yet-built
    /// face stands in as. Both bake a colour the palette owns: the light theme repaints the
    /// built-in shell in the housing's colour (`cart::themed_shell`), so the blank cart is a
    /// texture that goes stale the instant the mode flips. That is why this is one function —
    /// `rebake` (theme) and `set_shelf_geometry` (machine) are two different reasons to want
    /// the same two textures minted, and only the first knew about it before.
    fn rebake_row_frame(&mut self, compositor: &mut Compositor) {
        let sys = self.session.app().shelf_system();
        let size = self.session.app().shelf_size();
        let blank = cart_placeholder(size, sys);
        let blank = compositor.create_texture(blank.w, blank.h, &blank.rgba);
        if let Some(old) = self.session.app_mut().set_cart_placeholder(blank) {
            compositor.release_texture(old);
        }
    }

    /// The third boot line: how long the card's faces took, which is the only part of the boot
    /// that grows with the library. Split out so the timing call does not need a borrow of the
    /// compositor that `upload_faces` has already given back.
    fn upload_faces_note(&self, since: Instant) {
        self.session.boot_note(&format!(
            "shelf faces total {} ms",
            since.elapsed().as_millis()
        ));
        // Where that time went, by pass. A hundred-game card and a seventy-millisecond face
        // is a number worth splitting before anyone decides what to optimise.
        self.session.boot_note(&format!(
            "cart face phases: {}",
            slot_ui::face_profile::line()
        ));
    }

    /// One decode, at boot. **A card's own picture wins**; a card with no `Wallpapers`, no
    /// readable picture in it, or one the decoder will not take, gets the built-in default
    /// wallpaper — the neon tunnel, re-inked in the theme's colour, with the breathing glow on
    /// top of its wires.
    fn upload_wallpaper(&mut self, compositor: &mut Compositor) {
        let app = self.session.app();
        let seed = app.wall_secs().unsigned_abs();
        let card = app
            .root()
            .and_then(|root| wallpaper::pick(root, seed))
            .and_then(|path| wallpaper_face(&path));
        if let Some(rgba) = card {
            let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
            if let Some(old) = self.session.app_mut().set_wallpaper(id) {
                compositor.release_texture(old);
            }
            self.session.app_mut().set_wall_glow(None);
            self.wall_default = false;
            return;
        }
        // ★ 2026-10-28（六六）：「加入默认壁纸（洋红区加呼吸光效，跟主题色板走）」。
        //   底图与光**都跟主题**，所以都归 `bake_wall_default` 一条路：开机走一次，
        //   换主题再走一次。
        self.wall_default = true;
        self.bake_wall_default(compositor);
    }

    /// 默认壁纸的两层：底图（网线换成主题亮色）+ 那圈**只跟着网线走**的呼吸光。
    ///
    /// ★ 2026-10-28 第二轮（六六：「洋红区还是比较明显」「颜色盖不太住」）：底图**要重新上色**。
    /// 上一版只让光去盖品红，可光糊过 1px 的线只剩 5% 的覆盖，盖不住 —— 见
    /// `slot_ui::wallpaper_ink` 的注释。所以这条会在**每次换主题时**重传一次底图（旧的释放掉），
    /// 光和底图同一个色源（`ridge_ink`，跟 CHANGE 字、箱体洋红区一致）。
    fn bake_wall_default(&mut self, compositor: &mut Compositor) {
        let Some((rgba, w, h)) = slot_ui::default_wallpaper() else {
            return;
        };
        if let Some(ink) = slot_ui::wallpaper_ink(rgba, *w, *h, slot_ui::ridge_ink()) {
            let id = compositor.create_texture(*w, *h, &ink);
            if let Some(old) = self.session.app_mut().set_wallpaper(id) {
                compositor.release_texture(old);
            }
        }
        let glow = slot_ui::wallpaper_glow(rgba, *w, *h, slot_ui::ridge_ink())
            .map(|(g, gw, gh)| (compositor.create_texture(gw, gh, &g), gw, gh));
        self.session.app_mut().set_wall_glow(glow);
    }

    /// ★ 2026-10-23（六六）：货架页桌面图，放在壁纸之上、卡带之下。
    ///
    /// **A picture on the card wins over the one this code draws.** `System/Desktop.png` is
    /// read first, and only if the card carries no such file — or carries one of the wrong
    /// size — does the built-in rasteriser (`slot_ui::desktop_rgba`) step in. That is what
    /// makes the frame a thing that can be *drawn*: a picture made in a paint program, put on
    /// the card, is what the shelf shows, with no rebuild in between.
    ///
    /// Either way the theme is recorded, so a mode change re-bakes it in place.
    fn upload_desktop(&mut self, compositor: &mut Compositor) {
        let drawn = self
            .session
            .app()
            .root()
            .and_then(|root| slot_ui::decode_rgba(&root.join("System").join("Desktop.png")));
        let (rgba, from_card) = match drawn {
            Some((buf, w, h)) if w == OUT_W && h == OUT_H => (buf, true),
            _ => (slot_ui::desktop_rgba(), false),
        };
        let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
        self.session.app_mut().set_desktop(id);
        self.desktop_tex = Some(id);
        self.desktop_from_card = from_card;
        self.desktop_theme = Some((slot_ui::palette::mode(), slot_ui::palette::look()));
    }

    /// Upload whatever the shelf filler finished, then ask it for the cart nearest the caret
    /// that is still missing a face.
    ///
    /// One request in flight at a time: the answer is always wanted, and a queue would only
    /// let the worker fall further behind where the caret actually is.
    fn pump_shelf_faces(&mut self, compositor: &mut Compositor) {
        self.sync_resident_faces(compositor);
        for (i, f) in self.shelf_faces.drain() {
            let id = compositor.create_texture(f.w, f.h, &f.rgba);
            self.session.app_mut().set_face(i, id);
        }
        if self.shelf_faces.busy() || self.shelf_missing.is_empty() {
            return;
        }
        // Only within the resident window, and the window is the row's own order — the carts
        // the caret could reach by stepping, not by index arithmetic. Without the cap the
        // filler would keep working outward and the cap would be a treadmill; without the view
        // it works outward through carts no shelf is showing.
        let Some(pos) = self
            .shelf_missing
            .iter()
            .enumerate()
            .filter_map(|(p, &i)| {
                self.session
                    .app()
                    .shelf_distance(i)
                    .filter(|&d| d <= RESIDENT)
                    .map(|d| (d, p))
            })
            .min_by_key(|&(d, _)| d)
            .map(|(_, p)| p)
        else {
            return;
        };
        let i = self.shelf_missing.swap_remove(pos);
        let cart = self.session.app().carts()[i].clone();
        self.shelf_faces.request(i, cart);
    }

    /// Keeps the textures resident to a window around the caret, and the queue of carts still
    /// wanting one.
    ///
    /// A face is `FACE_W * FACE_H * 4` — a little over half a megabyte — and costs about 26 ms
    /// to rasterise, so a card of nine hundred games would hold nearly five hundred megabytes
    /// of shelf if every cart kept its own. The shelf draws three either side; this keeps
    /// twelve, releases what falls outside, and re-queues it for the build it will need if the
    /// caret ever comes back.
    ///
    /// Runs on the caret's edge rather than every frame: it walks the whole library, and the
    /// caret moves at most a few times a second.
    fn sync_resident_faces(&mut self, compositor: &mut Compositor) {
        let count = self.session.app().carts().len();
        if count == 0 {
            return;
        }
        let index = self.session.app().shelf_index();
        if self.resident_at == Some(index) {
            return;
        }
        self.resident_at = Some(index);

        self.shelf_missing.clear();
        let mut released = Vec::new();
        for i in 0..count {
            // Kept by *view* distance: see `Shelf::view_distance`. A cart the shelf up does not
            // hold is not worth a face at all, and one it does hold is worth one however far
            // away it sits in the library.
            let held = self
                .session
                .app()
                .shelf_distance(i)
                .is_some_and(|d| d <= RESIDENT + RESIDENT_HYSTERESIS);
            match (self.session.app().face_of(i), held) {
                (Some(tex), false) => released.push((i, tex)),
                (None, _) => self.shelf_missing.push(i),
                _ => {}
            }
        }
        for (i, tex) in released {
            compositor.release_texture(tex);
            self.session.app_mut().clear_face(i);
            self.shelf_missing.push(i);
        }
    }

    /// One frame into the offscreen target and out to a surface of `window` pixels. The
    /// caller swaps: only it knows what presenting costs.
    pub fn render(&mut self, compositor: &mut Compositor, window: (u32, u32)) {
        // ★ **Every screen's furniture except the shelf's, baked one frame after the first**
        //   (2026-10-07). `upload_fixed` used to do all of it *before* the first frame; measured
        //   on the device, the part no shelf screen draws with is the menu/power/core/link group
        //   (85 ms), the shortcut card, the menu buttons and the deck's own faces.
        //
        //   **One frame late is invisible**, which is why this is not a phase gate: the shelf is
        //   already on screen and none of these is on it, and by the time a hand can reach a key
        //   the bake is long done. A phase gate would have been the other way to do it and the
        //   wrong one — the power menu and a toast can both come up *over the shelf*, so "the
        //   user has left the shelf" is not the condition that makes these safe to defer.
        if self.frame_logged && !self.late_faces {
            self.late_faces = true;
            self.upload_late_furniture(compositor);
        }
        // The one number that can be compared across base OSes, logged once on the frame the
        // user first sees. Everything else in `boot.log` is a duration, and a duration cannot
        // tell you which base you booted — nor does it carry what the kernel and initramfs cost
        // before this process started, which is exactly what a faster base trims.
        //
        // Device uptime is the right clock: it starts at kernel boot, so it includes the kernel
        // and initramfs, and the bootloader before it is the same whichever base is underneath.
        // The base's own version comes with it so the log says what it was measured on.
        if !self.frame_logged {
            self.frame_logged = true;
            let uptime = std::fs::read_to_string("/proc/uptime")
                .ok()
                .and_then(|s| s.split_whitespace().next().map(str::to_string))
                .unwrap_or_else(|| "?".into());
            let base = std::fs::read_to_string("/etc/os-release")
                .ok()
                .and_then(|s| {
                    s.lines().find(|l| l.starts_with("VERSION_ID=")).map(|l| {
                        l["VERSION_ID=".len()..]
                            .trim()
                            .trim_matches('"')
                            .to_string()
                    })
                })
                .unwrap_or_else(|| "unknown".into());
            self.session.boot_note(&format!(
                "first frame at {} s of uptime  (base VERSION_ID={})",
                uptime, base
            ));
        }
        // The mode, before the frame is built: it is the one thing the app can change that the
        // textures cannot follow on their own, so the app raises a flag and this is what takes
        // it. Done here rather than in `advance` because this is the only place with a
        // compositor, and a texture cannot be minted without one.
        if self.session.app_mut().take_mode_dirty() {
            self.rebake(compositor);
        }
        // The other thing the app can change that a texture cannot follow on its own: which
        // machine's shelf is up. Same reason, same place.
        if self.session.app_mut().take_shelf_dirty() {
            self.set_shelf_geometry(compositor);
        }
        // ★ **The refresh row's handover** (2026-10-17). Two steps, and the order is the order
        // they have to happen in: the retired faces are **released first**, because a `TexId` is
        // a handle into a fixed pool and minting the new window before giving back the old one
        // would have both live at once — on a card of nine hundred games that is the whole
        // resident window's worth of textures asked for in the same breath as it is handed back.
        // Then the frame is rebuilt exactly as a machine switch rebuilds it.
        //
        // The shelf's own list has already been swapped by the time this runs (`App::refresh_library`
        // does it on the press); nothing here reads the card. This is only the GPU catching up.
        if self.session.app_mut().take_library_dirty() {
            for tex in self.session.app_mut().take_stale_faces() {
                compositor.release_texture(tex);
            }
            self.set_shelf_geometry(compositor);
        }
        // First, so a face that finished since the last frame is on screen this frame.
        self.pump_shelf_faces(compositor);
        if !self.row_logged {
            self.row_logged = true;
            let (sys, lib, view, idx, slots, marks, win) = {
                let app = self.session.app();
                let slots = app.shelf_slot_carts();
                let marks: String = slots
                    .iter()
                    .map(|&i| if app.face_of(i).is_some() { 'Y' } else { 'N' })
                    .collect();
                (
                    app.shelf_system().dir_name().to_string(),
                    app.carts().len(),
                    app.shelf_view_len(),
                    app.shelf_index(),
                    slots,
                    marks,
                    app.shelf_draw_window(),
                )
            };
            self.session.boot_note(&format!(
                "shelf row {} lib {} view {} idx {} slots {:?} faces {} window {:?} cc {}",
                sys,
                lib,
                view,
                idx,
                slots,
                marks,
                win,
                self.session.app().display_palette_name()
            ));
        }
        // Set every frame rather than on the edge: the grade is part of the final blit, so
        // it has to be right whether or not anything just changed it.
        compositor.set_blue_light(self.session.app().blue_light());
        compositor.set_shake(self.session.app().screen_shake());
        compositor.set_screen_power(self.session.app().screen_power());
        // The card's display filter, every frame so a SELECT+X (that machine's own screen look)
        // or SELECT+Y (colour) press lands on the next one.
        compositor.set_panel_mask(&self.session.app().display_mask());
        compositor.set_color_correction(
            &self.session.app().display_cc(),
            &self.session.app().display_cc_bias(),
        );
        // The Game Boy's colour, which is a palette lookup rather than a matrix: four shades,
        // four colours, and the table does the rest. `None` on every other machine and on
        // 原生灰, which leaves the matrix path — and its two `pow`s — switched off.
        let palette = self.session.app().display_palette();
        compositor.set_palette(palette.as_ref());
        // The pixel-art lookup, the other texture-driven grade: `None` unless the pixel mode is
        // the one in force on a colour machine, so it costs nothing the rest of the time.
        compositor.set_pixel_lut(self.session.app().display_pixel_lut());
        // ...and the lattice over it, on the two machines that draw one: a Game Boy's is the
        // palette's own lightest shade and a Game Boy Color's is white or black and rides the
        // overlay ring. An Advance has no overlay, so it is on the aperture table instead
        // (`display_mask`) and pushes nothing here.
        match self.session.app().display_grid() {
            Some((colour, mix, scanline, step)) => {
                // 间距与浓度分开推：`mix` 是一条线多黑，`step` 是一屏多少条。
                compositor.set_grid(&colour, mix, scanline);
                compositor.set_grid_step(step);
            }
            None => compositor.set_grid(&[0.0, 0.0, 0.0], 0.0, false),
        }
        compositor.set_cc_gamma(self.session.app().display_cc_gamma());
        // Every frame, like the rest: the machine whose frame these filters are about is the
        // cart under the highlight, and walking the row changes it. Cheap — the pass keeps its
        // own answer and does nothing at all until it differs.
        compositor.set_system(self.session.app().screen_system());
        compositor.begin_frame();
        // ★★ 2026-10-26 00:4x（PS1 偶发整帧黑的定案修复）：**这个 `FrameRef` 必须活到本帧
        //   画完为止**，不能像原来那样让 `if let` 的作用域在 `upload_game` 之后就把它放掉。
        //
        //   `Frames` 是三缓冲移交（`frames.rs`）：`latest()` 把 `Vec` 的所有权交给渲染器，
        //   `FrameRef` 一 drop 就 `recycle` 回空闲池，模拟线程立刻可以拿它写下一次的画面。
        //   而 `glTexSubImage2D` 传的是**客户端内存指针** —— Mali 的延迟路径上，驱动不一定
        //   在调用返回时就把 300KB 拷完。缓冲一被放回池子，模拟线程往里写新帧（或 PCSX 清
        //   `vout_buf`），驱动读到的就是被覆盖/清空的内容 ⇒ 偶发整帧黑。
        //
        //   实测症状完全吻合：`slot: frame` 打出来一直是亮的、`slot: up` 每次都是亮的，
        //   而面板中心像素连续几十上百帧是 (0,0,0)、偶尔一两帧正常。PS1 帧 320×240×4 =
        //   300KB，比 GBA 的 154KB 大一倍，竞态窗口也大一倍 —— 这也解释了为什么别的机种
        //   从来没报过这个问题。
        let frame = self.session.frame();
        if let Some(frame) = &frame {
            // The frame's own dimensions, not the active machine's: an arcade board hands over
            // whatever its hardware produces, and this is where that number reaches the pass.
            compositor.upload_game(frame, frame.width(), frame.height());
        }
        sync_clock(self.session.app_mut(), compositor, &mut self.clocks);
        sync_about(self.session.app_mut(), compositor, &mut self.about);
        sync_shelf_title(
            self.session.app_mut(),
            compositor,
            &mut self.shelf_title_tex,
            &mut self.shelf_titled,
        );
        sync_slot_sheet(
            self.session.app_mut(),
            compositor,
            &mut self.slot_cap_tex,
            &mut self.slot_thumb_tex,
            &mut self.slot_built,
        );
        sync_core_picker(
            self.session.app_mut(),
            compositor,
            &self.faces,
            &mut self.core_asked,
            &mut self.core_board_tex,
            &mut self.core_lid_tex,
            &mut self.core_built,
        );
        sync_switcher(
            self.session.app_mut(),
            compositor,
            Faces {
                pool: &mut self.polaroid_texes,
                title: &mut self.title_tex,
                undo: &mut self.undo_tex,
            },
            &mut self.switcher,
        );
        sync_cheat_faces(self.session.app_mut(), compositor);
        // ★ 2026-10-21：游戏菜单的两件懒建面 —— 标题行（游戏名，跟着 stem）和保存设置弹窗
        //   （弹窗开的那一帧建、关了放走）。都挂在 cheat 之后：只在游戏里才非空。
        sync_game_title(self.session.app_mut(), compositor);
        // ★ 2026-10-26：PS1 光盘封面贴图 —— 只在 PS1 插卡/游戏/弹出时非空，跟着 stem 走。
        sync_disc_face(self.session.app_mut(), compositor);
        sync_save_settings(
            self.session.app_mut(),
            compositor,
            &mut self.save_settings_open,
        );
        // ★ 2026-10-21：操作键映射弹窗的面。七行各有自己的一句值（"游戏 A"/"连发 B"/"8 Hz"），
        //   所以按"七行值的快照"当键：改哪行重烘哪行。
        sync_keymap(
            self.session.app_mut(),
            compositor,
            &mut self.keymap_built,
        );
        // ★ 2026-10-21：Wi-Fi 配置浮层的面。定句每开一次重烘（便宜，十六句），会变的
        //   两样（当前 SSID、扫描结果）跟着脏标记走；关了整族放走。
        sync_wifi(self.session.app_mut(), compositor);
        // ★ 2026-10-21：**这一排菜单面按这一局的机种重印。** 开机那一遍是在货架上印的
        //   （空槽读作 GBA），插上 GB/GBC 之后「网格切换」得变成「遮罩切换」—— 不加这一步，
        //   Game Boy 卡上那一行会一直写着它没有的东西。
        sync_game_menu_rows(&mut self.session, compositor, &mut self.game_menu_system);
        // ★ **面板那两张面在这里才读**（2026-10-07）：只有卡座与信息页用它们，而它们合计
        //   12.75 MB。开机时读等于为第一帧根本不用的两张面花掉约 2.0 s。见 `ensure_panel_fonts`。
        if self.session.app().needs_panel_fonts() {
            self.ensure_panel_fonts();
        }
        // Read before the mutable borrow, not after: `app_mut` and `root` are two hands on the
        // same session, and the order the arguments are written in is not the order they are
        // evaluated in.
        let deck_root = self.session.root().to_path_buf();
        sync_deck(
            self.session.app_mut(),
            compositor,
            &deck_root,
            &mut self.deck_disc,
            &mut self.deck_track,
        );
        sync_info_panel(
            self.session.app_mut(),
            compositor,
            &mut self.info_tex,
            &mut self.info_built,
        );
        sync_palette_names(
            self.session.app_mut(),
            compositor,
            &mut self.palette_texes,
            &mut self.palette_paged,
        );
        sync_palette_name(
            self.session.app_mut(),
            compositor,
            &mut self.palette_tex,
            &mut self.palette_named,
        );
        sync_overlay(self.session.app_mut(), compositor);
        // ★ 2026-10-23（六六）：桌面图按主题 + 色板烘。底色是机器自己的塑料
        //   （`slot_chrome::housing`），所以明暗切换、色板轮换都要重烘。每帧比对
        //   `(mode, look)`，变了先释放旧纹理再建新的 —— 不释放的话每换一次色板就漏
        //   一张 1.4MB。卡自带的 `Desktop.png` 是静图，跟不了主题，不重烘。
        let dm = slot_ui::palette::mode();
        let dl = slot_ui::palette::look();
        if !self.desktop_from_card && self.desktop_theme != Some((dm, dl)) {
            if let Some(old) = self.desktop_tex {
                compositor.release_texture(old);
            }
            let rgba = slot_ui::desktop_rgba();
            let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
            self.session.app_mut().set_desktop(id);
            self.desktop_tex = Some(id);
            self.desktop_theme = Some((dm, dl));
        }
        // The screen reflection: handed the overlay texture as its zone mask when the overlay
        // up is a reflective one, `None` otherwise. Set every frame with the rest of the state.
        compositor.set_reflection(if self.session.app().overlay_reflects() {
            self.session.app().overlay_tex()
        } else {
            None
        });
        self.draws.clear();
        self.session.app().draw(&mut self.draws);
        compositor.draw_list(&self.draws);
        // 收尾淡出：货架已经完整画好，最后再盖一层渐隐的动画末帧。
        // 放在 draw_list 之后、end_frame 之前，这样两层都在同一个 offscreen FBO 里合成，
        // blit 一次出屏 —— 不额外多一次 swap，也不额外占开机时间。
        if let Some((tex, alpha)) = self.boot_fade {
            if alpha > 0.0 {
                compositor.draw_list(&[Draw::Tex {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    tex,
                    alpha,
                }]);
            }
        }
        // ★ 调试（2026-10-29）：SFC 反射层"左侧偶尔缺失"。每帧读合成器的反射诊断，仅 SFC、
        // 且仅状态变化时落笔到 boot.log（见 `log_reflection_diag`）。玩家跑一遍回传即可定位
        // 是整帧跳过（mask 未就绪/窗过小）还是窗/模糊尺寸跳变。临时诊断，定位后删除。
        self.log_reflection_diag(compositor);
        compositor.end_frame(window);
        // 到这里（`end_frame` 已把 offscreen 目标 blit 出去）才允许缓冲回到空闲池。
        drop(frame);
    }

    /// 临时调试（2026-10-29）：把 SFC 反射层的每帧状态变化写进 `System/boot.log`。
    ///
    /// 只在 `(drew, skip_reason, blur_w, blur_h)` 变化时落笔，避免 60fps 刷屏；每次跳过附带
    /// 累计跳过计数，露出"偶尔"的频率。非 SFC 完全不写。定位到根因后整段删除（连同 `frame()`
    /// 里的调用与 `Frontend` 的 `reflect_last`/`reflect_total_skips` 字段）。
    fn log_reflection_diag(&mut self, compositor: &Compositor) {
        const ON: bool = true;
        if !ON {
            return;
        }
        let d = compositor.reflect_diag();
        if d.sys != slot_gfx::System::Sfc {
            return;
        }
        let key = (
            d.drew,
            d.skip,
            d.blur.0,
            d.blur.1,
            d.win.0 as i32,
            d.win.1 as i32,
            d.win.2 as i32,
            d.win.3 as i32,
            d.src.0,
            d.src.1,
        );
        let changed = self.reflect_last != Some(key);
        if !d.drew {
            self.reflect_total_skips += 1;
        }
        if changed {
            if d.drew {
                self.session.boot_note(&format!(
                    "SFC reflect OK: win=({:.0},{:.0},{:.0},{:.0}) blur=({},{}), src=({},{}), mask={}",
                    d.win.0, d.win.1, d.win.2, d.win.3,
                    d.blur.0, d.blur.1,
                    d.src.0, d.src.1,
                    if d.mask_present { "on" } else { "off" }
                ));
            } else {
                self.session.boot_note(&format!(
                    "SFC reflect SKIP (cumulative {}): reason={:?} win=({:.0},{:.0},{:.0},{:.0}) blur=({},{}), src=({},{}), mask_present={} mask_resolved={}",
                    self.reflect_total_skips,
                    d.skip,
                    d.win.0, d.win.1, d.win.2, d.win.3,
                    d.blur.0, d.blur.1,
                    d.src.0, d.src.1,
                    d.mask_present, d.mask_resolved
                ));
            }
            self.reflect_last = Some(key);
        }
    }

    /// The face for row element `i`, from whichever pipeline the row belongs to.
    ///
    /// **This is the one place that knows a music row is drawn differently**, and it is one place
    /// on purpose: the two callers that build faces both ask "the row's element `i`", and the
    /// answer is `cd_face` on the music row and `cart_face` on the game row. A first version
    /// left `cart_face` in both, which draws a game's shape and a game's shell for a music file —
    /// the row's geometry was right and every face was wrong, and nothing said so.
    fn face_for(&mut self, i: usize) -> CartFace {
        let (kind, cart) = {
            let app = self.session.app();
            (app.shelf_kind(), app.carts()[i].clone())
        };
        match kind {
            ShelfRow::Games => cart_face(&cart),
            // **The cover path is the one `disc_row` already resolved**, not a path built here.
            // This rebuilt it from the stem with no extension — the same mistake the deck made, and
            // it is silent in exactly the same way: no cover found, no error, a sleeve with a blank
            // face, which is what a sleeve with no art is supposed to look like. There was one
            // lookup rule and now there is one, done where the row was built.
            //
            // `cart.label` is still the field it arrives in: a sleeve is presented to the row as a
            // `Cart` (see `App::disc_row`), and renaming that field would touch the game's own path
            // for no gain.
            ShelfRow::Discs => {
                let (cw, ch) = slot_ui::cd_cover_size();
                let cover = cart
                    .label
                    .as_deref()
                    .and_then(|p| slot_ui::cd_cover(p, cw, ch))
                    .unwrap_or_default();
                slot_ui::cd_face(&cover, &slot_ui::cd::SHELL, None)
            }
        }
    }

    /// Input and time, after the frame is on screen. The gesture windows expire on this
    /// whether or not anything was pressed, so it is called every frame.
    pub fn advance(&mut self, input: &mut dyn InputSource) {
        let now = self.now();
        let events = input.poll(now);
        self.session.feed(events, now);
        let dt = self.last.elapsed().as_secs_f32();
        self.last = Instant::now();
        self.session.update(dt);
    }

    fn now(&self) -> Millis {
        self.start.elapsed().as_millis() as Millis
    }

    pub fn powering_off(&self) -> bool {
        self.session.app().ready_to_power_off()
    }

    pub fn restarting(&self) -> bool {
        self.session.app().ready_to_restart()
    }

    pub fn restart(&mut self) {
        self.session.app_mut().restart();
    }

    /// The state was flushed on the edge that set `powering_off`, so there is nothing left to
    /// do but go.
    pub fn poweroff(&mut self) {
        self.session.app_mut().poweroff();
    }
}

/// A line of menu type per label, in the order they were handed over, each with the size it
/// was rastered at. Every menu on the device is drawn from a list shaped exactly like this,
/// so the four the in-game menu needs are built through one function rather than four copies
/// of the same three lines.
fn menu_faces<'a>(
    compositor: &mut Compositor,
    labels: impl Iterator<Item = &'a str>,
) -> Vec<(TexId, u32, u32)> {
    menu_faces_at(
        compositor,
        labels,
        slot_ui::MENU_PX,
        slot_ui::MENU_H,
        slot_ui::MENU_PAD,
    )
}

/// ★ 2026-10-21：任意尺寸的行。游戏菜单用大号那一档 —— 六六的原话是「小气巴拉的」。
/// 参数全摊开而不是给一个"大/小"的开关：还有别处会想用中间一档，一个布尔很快不够用。
fn menu_faces_at<'a>(
    compositor: &mut Compositor,
    labels: impl Iterator<Item = &'a str>,
    px: f32,
    h: u32,
    pad: u32,
) -> Vec<(TexId, u32, u32)> {
    labels
        .map(|label| {
            let f = menu_face_at(label, px, h, pad);
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        })
        .collect()
}

/// A screen's key caps, in the order the legend names them. None of them ever changes what
/// it says, so they are uploaded once and outlive every visit to that screen.
fn legend_faces(compositor: &mut Compositor, legend: &[(&str, &str)]) -> Vec<TexId> {
    legend
        .iter()
        .map(|(key, label)| {
            let f = hint_face(key, label);
            compositor.create_texture(f.w, f.h, &f.rgba)
        })
        .collect()
}

/// The switcher's textures, which outlive any one opening.
struct Faces<'a> {
    pool: &'a mut Vec<TexId>,
    title: &'a mut Option<TexId>,
    undo: &'a mut Option<TexId>,
}

/// Photos and the undo cap are built once per opening, on the way in, while the game is
/// already paused. Rebuilt each time rather than cached because the ring changes underneath
/// them. The title names the selection, so it follows a flick instead.
fn sync_switcher(app: &mut App, compositor: &mut Compositor, texes: Faces, state: &mut Switcher) {
    if !matches!(app.phase(), Phase::Polaroids { .. }) {
        state.open = false;
        return;
    }
    if !state.open {
        state.open = true;
        state.titled = None;
        let faces: Vec<_> = app.polaroid_entries().iter().map(photo_face).collect();
        let ids = faces
            .iter()
            .enumerate()
            .map(|(i, f)| match texes.pool.get(i) {
                Some(id) => {
                    compositor.update_texture(*id, f.w, f.h, &f.rgba);
                    *id
                }
                None => {
                    let id = compositor.create_texture_nearest(f.w, f.h, &f.rgba);
                    texes.pool.push(id);
                    id
                }
            })
            .collect();
        app.set_polaroid_faces(ids);

        // An offer can expire while the switcher is up but it cannot change into the other
        // kind, so the cap only has to be rasterised on the way in. Whether it is drawn at
        // all is the app's call.
        let label = app
            .undo_label()
            .map(|l| upload(compositor, texes.undo, hint_face("X", l)));
        app.set_undo_face(label);
    }
    if state.titled.as_deref() != app.polaroid_stamp() {
        state.titled = app.polaroid_stamp().map(str::to_string);
        let face = title_face(&app.polaroid_title(&format_stamp(app.wall_secs())));
        let id = upload(compositor, texes.title, face);
        app.set_polaroid_title_face(id);
    }
}

/// The picker is rasterised on every change under the caret, which is once per press. The
/// shelf clock follows the wall clock, so it is rebuilt when the minute turns and not on the
/// fifty nine seconds either side of it.
fn sync_clock(app: &mut App, compositor: &mut Compositor, clocks: &mut Clocks) {
    let picked = app.picker().map(|p| p.text());
    if picked != clocks.picked {
        clocks.picked = picked;
        if let Some(face) = app.picker().map(|p| p.face()) {
            let line = upload(compositor, &mut clocks.line, face);
            let hint = upload(compositor, &mut clocks.hint, set_clock_hint_face());
            app.set_clock_faces(line, hint);
        }
    }
    let shown = hhmm(app.wall_secs());
    if shown != clocks.shown {
        // The clock and the battery's percent are the two halves of one status row and are set at
        // the same size now, in the same `STATUS_H`-tall band — `clock_face` and `word_face` own
        // those two numbers, so nothing here has to say what they are.
        let face = clock_face(&shown);
        clocks.shown = shown;
        let w = face.w;
        let id = upload(compositor, &mut clocks.shelf, face);
        app.set_shelf_clock_face(id, w);
    }
    let battery_shown = app
        .battery()
        .map(|b| format!("{}%", b.percent))
        .unwrap_or_default();
    if battery_shown != clocks.battery {
        clocks.battery = battery_shown.clone();
        if !battery_shown.is_empty() {
            let face = word_face(&battery_shown);
            let w = face.w;
            let id = upload(compositor, &mut clocks.battery_tex, face);
            app.set_battery_percent_face(id, w);
        }
    }
    // The direct-connect address: baked when the connect script has left one behind, because DHCP
    // decides it and the dialog's baked-at-boot lines cannot carry it.
    if app.direct_ip_dirty() {
        match app.direct_ip_str().map(str::to_string) {
            Some(ip) if !ip.is_empty() => {
                let face = slot_ui::dialog_line_face(&ip, 32.0);
                let (w, h) = (face.w, face.h);
                let id = compositor.create_texture(w, h, &face.rgba);
                app.set_direct_ip_face(Some((id, w, h)));
            }
            _ => app.set_direct_ip_face(None),
        }
        app.clear_direct_ip_dirty();
    }
}

/// Built only once the screen is up: it is a 660 by 228 rasterisation and most sessions never
/// open it. The label is on the menu screen and nowhere else — the card that used to hang under
/// it is a screen of its own now, and it carries no label.
///
/// ★ 2026-10-29（六六）：牌子上的字全删了，这张面**不再随任何东西变** —— 它原来跟着电量
/// 百分比重烘（`BATTERY : LI-ION (62%)` 那一行），现在牌子只显示图，电量在顶栏那枚电池上。
/// 所以要烘的条件只剩「还没有」。`sticker_face()` 返回 `None`（图不是本面板的尺寸、
/// 或图还没换）时什么也不建，且**每帧都会再试** —— 这正是想要的：图一到位就自动出现。
fn sync_about(app: &mut App, compositor: &mut Compositor, state: &mut AboutFace) {
    if !matches!(app.phase(), Phase::Menu { .. }) {
        return;
    }
    if state.tex.is_some() {
        return;
    }
    let Some(face) = sticker_face() else {
        return;
    };
    let id = upload(compositor, &mut state.tex, face);
    app.set_sticker_face(id);
}

/// The open cart's faces, asked for as soon as the caret lands on a cart and uploaded when the
/// worker hands them back, so they are normally on the GPU before START. The worker is the only
/// place they are built: rasterised on the frame loop, a board freezes the shelf for the better
/// part of half a second on the H700.
/// The game under the eye on the shelf, rebuilt only when the cart under the eye changes.
/// ★ 2026-10-21：槽位表的两样东西 —— 那行字和那张 2 倍截图。
///
/// # 为什么这函数以前不存在
///
/// `App::set_slot_faces` 从 2026-10-20 起就在那里，**但没有任何人调它**：槽位表画出来
/// 只有标题和一个空面板。`slot_caption()` / `slot_thumb_path()` 两个访问器同样没人读。
/// 存档、读档、左右换槽全都通着，只有"看得见"这一半没接上 —— 这就是"游戏菜单做完"
/// 里最实的一块。
///
/// # 键为什么是文字和路径，不是槽位号
///
/// 同一个槽位上：空的 → 存过 → 又删了，号一次都没变，内容变了三次。用号做键，
/// 第三种状态会把第一种的旧图一直画着。文字里带着时间，路径里带着文件名，
/// 两样加起来就是"这一格现在到底是什么"。
fn sync_slot_sheet(
    app: &mut App,
    compositor: &mut Compositor,
    cap: &mut Option<TexId>,
    thumb: &mut Option<TexId>,
    built: &mut Option<(Option<String>, Option<std::path::PathBuf>)>,
) {
    let want = (app.slot_caption(), app.slot_thumb_path());
    if built.as_ref() == Some(&want) {
        return;
    }
    *built = Some(want.clone());

    // 那行字。`dialog_line_face` 是这一族里唯一按句子排版的（会自动缩），
    // 而这行字里带着游戏名和日期，长度不由我们决定。
    let caption = match &want.0 {
        Some(text) => {
            let f = slot_ui::dialog_line_face(text, 18.0);
            let (w, h) = (f.w, f.h);
            Some((upload(compositor, cap, f), w, h))
        }
        None => None,
    };
    // 那张图。空槽位没有文件，`None` 让它保持空着 —— 画一块黑矩形比留白更像坏了。
    let picture = want
        .1
        .as_ref()
        .and_then(|p| slot_ui::decode_rgba(p))
        .map(|(rgba, w, h)| (upload_rgba(compositor, thumb, w, h, &rgba), w, h));
    app.set_slot_faces(caption, picture);
}

/// One rasterise per arrow press, and none at all on a frame where nothing moved.
fn sync_shelf_title(
    app: &mut App,
    compositor: &mut Compositor,
    slot: &mut Option<TexId>,
    shown: &mut Option<String>,
) {
    // ★ 2026-10-24：缓存键是**印出来的名字**，不是 stem —— 街机的 stem 是 romset
    // （`kof97`），名字是「拳皇 '97」。键存显示名，换一张卡就必然重烘；用 stem 当键而
    // 拿显示名去画，会出现"键没变于是不重烘"、屏幕上留着上一张卡的中文名的错。
    let want = app.selected_name();
    if *shown == want {
        return;
    }
    *shown = want.clone();
    match want {
        Some(name) => {
            // 名字由 `slot_store::shown_name` 给出 —— 街机查内置中文表，其余机种就是
            // 清洗过的文件名。**和贴纸、芯片刻字、菜单抬头是同一个投递口**，所以四处
            // 不可能各说各话。
            let face = shelf_title_face(&name);
            let (w, h) = (face.w, face.h);
            let id = upload(compositor, slot, face);
            app.set_shelf_title_face(Some((id, w, h)));
        }
        None => app.set_shelf_title_face(None),
    }
}

/// The three faces the deck bakes **when the disc changes**: its sleeve, its name and its
/// `3/12` counter — each with the text it was last built for.
///
/// Grouped for the same reason [`DeckLines`] is: `sync_deck` takes one parameter per face it
/// owns, and six loose `Option`s in a row was a list the next reader could not hold. Split from
/// `DeckLines` because these three change only when the disc does, while those two change while
/// it plays.
#[derive(Default)]
struct DeckDisc {
    name_tex: Option<TexId>,
    named: Option<String>,
    count_tex: Option<TexId>,
    counted: Option<String>,
    disc_tex: Option<TexId>,
    discd: Option<String>,
}

/// The deck's two lines that change while it is up: the track counter and the lyric.
///
/// **Each carries the text it was last built for, which is the whole caching story.** A frame
/// where the song has not reached a new line compares two strings and uploads nothing; the frame
/// a line changes on rasterises once. Without the `built` half of each pair this would mint a
/// texture per frame, and a lyric line changes on a human timescale — a few times a minute —
/// against a panel that draws sixty.
#[derive(Default)]
struct DeckLines {
    track: Option<TexId>,
    tracked: Option<String>,
    lyric: Option<TexId>,
    sung: Option<String>,
    /// The LCD panel's texture, and **everything the frame it would bake depends on**.
    ///
    /// **Six fields and not a hash of the picture**, because a hash would have to be taken over
    /// the composed frame — which is the expensive part. These are the inputs, and comparing
    /// them is free. The spectrum is the awkward one: it changes every frame by definition, so
    /// it is *not* in here, and `panelled` below is what keeps that from baking sixty textures a
    /// second.
    panel: Option<TexId>,
    panel_face: u8,
    panel_ramp: usize,
    panel_track: String,
    panel_sung: Option<String>,
    /// The title's marquee offset the panel was last baked at. **It is in the gate because a
    /// long name scrolls on the clock rather than on a change**: without it the panel would bake
    /// once and then hold a still frame with half the name missing.
    panel_scroll: i32,
    /// The whole second the strip's clock was last baked at; see the gate in `sync_lcd_panel`.
    panel_sec: u32,
    /// The panel is up and the spectrum is live, so **the bake is on a clock rather than on a
    /// change.** A still panel mints nothing; a panel with a moving spectrum is re-baked
    /// `PANEL_BAKE_MS` apart, which is fast enough to look continuous and slow enough that a
    /// deck left playing costs a fraction of a core.
    panel_baked_at: Millis,
    panel_has_signal: bool,
    /// **The bars' own memory.** [`slot_ui::lcd::ballistics`] needs each band's previous height,
    /// so that history lives here rather than in the composer (see `lcd::Content::levels`). It is
    /// the one panel input that is *state* rather than a fact about the track, which is why it is
    /// deliberately **not** in the bake gate above: it changes on every bake by definition.
    panel_levels: [f32; slot_ui::lcd::BANDS],
}


/// How often the panel is re-baked while the spectrum is moving. **300 ms** — a little over three
/// frames a second. It was 100, then 200 (六六 2026-10-07:「采样可以再少一半」), then 300 in the
/// same session (「我要的采样少一半应该是刷新率慢一点，现在算是太灵敏所以观感跳跃很快」).
///
/// ⚠️ **The cadence is not what makes the bars jump, and this is the second time that has been
/// worth saying.** A slower clock means fewer updates, but each one still draws a fresh
/// independent snapshot of a 46 ms window — so on its own it makes every jump *bigger*, not
/// smaller. What actually calms the panel is the **pair** `sync_lcd_panel` now runs on this
/// loop: [`slot_ui::lcd::bands`] riding a fixed dB scale instead of re-normalising to each
/// frame's own peak, and [`slot_ui::lcd::ballistics`] (both directions smoothed) on top of it.
/// The two belong together: slowing down without either would have made the original complaint
/// worse.
const PANEL_BAKE_MS: Millis = 300;

/// Compose and upload the deck's LCD panel, if anything about it has changed.
///
/// **The gate is the six inputs plus the clock, and it is the whole reason this function is not
/// simply called every frame.** A 240×120 frame is 28 800 pixels to compose, threshold, mask
/// and scale; doing that sixty times a second to produce the same picture would be the most
/// expensive thing in the frontend. So: a frame where the panel, the ramp, the track and the
/// lyric line are all the same and the spectrum has nothing to say bakes nothing.
fn sync_lcd_panel(
    app: &mut App,
    compositor: &mut Compositor,
    lines: &mut DeckLines,
) {
    let face = app.deck_panel();
    let ramp_at = app.deck_ramp_index();
    let track = app.deck_title_now();
    let artist = app.deck_artist_now();
    // ★ **The name block is one line — `artist - title`** (2026-10-06). The composer is
    //   `lcd::title_text`, and the panel draws exactly what this returns: measuring a different
    //   string here than the panel draws is how a title gets shown as "fits" and drawn clipped.
    //   It is also what the bake gate below compares, so a change to *either* half re-bakes.
    let name = slot_ui::lcd::title_text(&artist, &track);
    let sung = app.deck_lyric_window().join("\n");
    let (elapsed, length) = app.deck_position();
    // A signal is what makes the spectrum move. **`peak` over a threshold rather than "is the
    // tap non-zero"**: a tap that has been written to but holds only silence is not a spectrum
    // that needs re-baking sixty times a second.
    let mut tap = [0i16; crate::music::TAP_LEN];
    let n = app.deck_tap(&mut tap);
    let has_signal = slot_ui::lcd::peak(&tap[..n]) > 0.01;
    let now = app.clock_ms();
    // **The marquee is computed here rather than inside the panel**, because it is the one input
    // that changes with the clock instead of with the music — and the gate below is built from
    // the inputs, so it has to be able to see this one. `marquee_scroll` returns 0 for a title
    // that fits, so a short name is still and costs nothing.
    let title_w = slot_ui::lcd::title_width_px(&name);
    let col_w = slot_ui::lcd::title_col_w();
    let scroll = slot_ui::lcd::marquee_scroll(title_w, col_w, now);
    // ★ **The strip shows the clock, so the whole seconds are an input too.** Without this the
    //   panel would only re-bake when the music changed — and the clock would sit frozen through
    //   a quiet passage, which is exactly when a listener looks at it. One re-bake a second.
    let sec = elapsed.max(0.0) as u32;

    let same = lines.panel.is_some()
        && lines.panel_face == face as u8
        && lines.panel_ramp == ramp_at
        && lines.panel_track.as_str() == name.as_str()
        && lines.panel_sung.as_deref() == Some(sung.as_str())
        && lines.panel_scroll == scroll
        && lines.panel_sec == sec
        && lines.panel_has_signal == has_signal;
    if same && (!has_signal || now.saturating_sub(lines.panel_baked_at) < PANEL_BAKE_MS) {
        return;
    }

    let lyric = app.deck_lyric_window();
    let rate = app.deck_rate();
    // ★ **弹道在这里，不在 composer 里。** 面板是一帧接一帧画的，所以它是唯一能跨帧记住
    //   每根柱子高度的调用者：`bands` 给出的一次读数按"快升、慢降"过一次再交给 composer。
    //   见 `slot_ui::lcd::bands`（固定 dB 刻度）与 `slot_ui::lcd::ballistics`
    //   （六六 2026-10-07 的「太灵敏所以观感跳跃很快」）。
    let raw = slot_ui::lcd::bands(&tap[..n], rate);
    let levels = slot_ui::lcd::ballistics(&mut lines.panel_levels, raw);
    let content = slot_ui::lcd::Content {
        face: match face {
            crate::app::Panel::Name => slot_ui::lcd::Face::Name,
            crate::app::Panel::Lyrics => slot_ui::lcd::Face::Lyrics,
        },
        title: &track,
        artist: &artist,
        lines: &lyric,
        tap: &tap[..n],
        rate,
        // 平滑过的那一份；composer 不再自己从 `tap` 算（见上面那段）。
        levels: Some(levels),
        elapsed,
        length,
        scroll,
        // The equaliser preset, printed at the right end of the meta row (see
        // `slot_ui::lcd::meta_row`). `NULL` until the user cycles it with START.
        eq: app.deck_eq(),
        // The transport bar is **part of the frame**, not a second texture — see
        // `slot_ui::lcd::LOGICAL_H`. It comes along with the picture because it changes with it:
        // the play key's glyph is the transport's state and the strip is the track's times.
        bar: Some(app.deck_bar()),
    };
    let (_, ramp) = app.deck_ramp();
    // The mask is on. **A card can turn it off** — a panel without a pixel grid is a legitimate
    // thing to want — but the default is the grid, because that is what makes the frame read as
    // a 240×120 screen rather than as a picture of one.
    let rgba = slot_ui::lcd::panel(&content, ramp, true);
    let (w, h) = (slot_ui::lcd::PANEL_W, slot_ui::lcd::PANEL_H);
    let id = upload_rgba(compositor, &mut lines.panel, w, h, &rgba);
    app.set_lcd_panel(Some((id, w, h)));
    lines.panel_face = face as u8;
    lines.panel_ramp = ramp_at;
    lines.panel_track = track;
    lines.panel_sung = Some(sung);
    lines.panel_scroll = scroll;
    lines.panel_sec = sec;
    lines.panel_baked_at = now;
    lines.panel_has_signal = has_signal;
}

/// The deck's faces and the card's music behind them.
///
/// **Three jobs, and the order is the point.** The walk comes first, because it is the one that
/// can take real time — it opens every file under `Music/` to read its length — and it is asked
/// for by the deck being opened rather than by anything changing, so it happens on the first
/// frame of the first visit and never again. The two rasterisations come after it, keyed on the
/// text they would draw, so a frame where the disc has not changed mints nothing.
///
/// The walk is on the frame loop and that is a deliberate, bounded choice rather than an
/// oversight: it is a directory walk plus a container header read per file, it happens once per
/// session, and a worker thread would cost more in plumbing than it saves on the one screen
/// where the user is already waiting for something. If a card ever carries enough music for it
/// to be felt, this is the line to move.
fn sync_deck(
    app: &mut App,
    compositor: &mut Compositor,
    root: &std::path::Path,
    disc: &mut DeckDisc,
    lines: &mut DeckLines,
) {
    if !app.is_listening() {
        return;
    }
    if app.discs_dirty() {
        // **The two walks happen together, on the same frame, for the same reason:** both are
        // answers to "what is on this card", and a deck that could see the tracks but not the
        // lyrics would be a card whose words are on the volume and cannot be shown. The lyric
        // walk is far cheaper — directory names only, no file is opened.
        // 启动/换卡时，给「有音频但没播放列表」的子文件夹自动补一张 `list.m3u`（按序号/歌名排序），
        // 让它们作为一整张碟出现，而不是每首歌散成一张。best-effort：只读目录、不解码音频，失败静默。
        slot_media::playlist::ensure_music(root);
        app.set_discs(slot_media::discs(root));
        app.set_lyrics(slot_media::lrc::Index::scan(&root.join(slot_media::DIR)));
        // **第三趟同一次走**：碟片、歌词、封面三样都是"这张卡上有什么"的回答，
        // 且封面扫描只读本目录名（和歌词同价），所以和上面两样在同一帧一起做。
        // 漏掉这一行 → `self.covers` 永远是 `default()`（空），`Music/Covers/` 永远不被查，
        // 碟片行与卡座都只能落到内建空袖套，用户放进 `Music/Covers/` 的封面一张都看不见。
        app.set_covers(slot_media::Covers::scan(&root.join(slot_media::DIR)));
    }
    // **The sleeve's own face, re-baked when the disc changes** — the cover is a per-album thing
    // and the face is where it shows. Baked once at boot with no cover, the deck showed a
    // correct-looking sleeve with a blank face and nothing anywhere said why.
    //
    // The cover is looked up through the same index the shelf row uses (`Music/Covers/`, keyed by
    // album and then flat), so the row and the deck cannot disagree about which picture an album
    // has. A sleeve with no art of its own gets the built-in face, and one whose cover is missing
    // gets the face with no cover — both are the truthful answer.
    let want_disc = app.deck_disc().map(|t| t.stem.clone()).unwrap_or_default();
    if disc.discd != Some(want_disc.clone()) {
        let face = if let Some(path) = app.deck_cover() {
            let (cw, ch) = slot_ui::cd_cover_size();
            let cover = slot_ui::cd_cover(&path, cw, ch).unwrap_or_default();
            eprintln!(
                "slot: deck: cover area {}x{} -> {} bytes from {:?}",
                cw,
                ch,
                cover.len(),
                path.display().to_string()
            );
            slot_ui::cd_face(&cover, &slot_ui::cd::SHELL, None)
        } else {
            slot_ui::cd_face(&[], &slot_ui::cd::SHELL, None)
        };
        let (w, h) = (face.w, face.h);
        let id = upload_rgba(compositor, &mut disc.disc_tex, w, h, &face.rgba);
        app.set_deck_face(id, w, h);
        disc.discd = Some(want_disc);
    }
    // The name in the top window. `deck_title` answers the menu's own word for the screen when
    // the card has no music, so a card with nothing in it still says what this place is.
    let want_name = app.deck_title();
    if disc.named != Some(want_name.clone()) {
        let face = word_face(&want_name);
        let (w, h) = (face.w, face.h);
        let id = upload(compositor, &mut disc.name_tex, face);
        app.set_deck_name_face(Some((id, w, h)));
        disc.named = Some(want_name);
    }
    // The counter, `3/12`. `None` for a card with fewer than two discs: there is nothing to
    // count between, and a `1/1` in the corner would be a fact about the user's filing rather
    // than about the deck.
    let (len, at) = app.deck_disc_count();
    let want_count = (len > 1).then(|| format!("{}/{}", at + 1, len));
    if disc.counted != want_count {
        match &want_count {
            Some(text) => {
                let face = word_face(text);
                let (w, h) = (face.w, face.h);
                let id = upload(compositor, &mut disc.count_tex, face);
                app.set_deck_count(Some((id, w, h)));
            }
            None => app.set_deck_count(None),
        }
        disc.counted = want_count;
    }
    // The track counter, `2/9`. Same rule as the disc's own: a disc of one track has nothing to
    // count between, and a `1/1` is a fact about how the card was filed rather than about where
    // the transport is.
    let (at, of) = app.deck_track();
    let want_track = (of > 1).then(|| format!("{}/{}", at + 1, of));
    if lines.tracked != want_track {
        match &want_track {
            Some(text) => {
                let face = word_face(text);
                let (w, h) = (face.w, face.h);
                let id = upload(compositor, &mut lines.track, face);
                app.set_deck_track_count(Some((id, w, h)));
            }
            None => {
                app.set_deck_track_count(None);
                lines.track = None;
            }
        }
        lines.tracked = want_track;
    }
    // The line being sung. `None` for the majority of cards, and that is the ordinary case
    // rather than a missing feature: the deck draws its meters either way.
    //
    // **The LRC's own spacing is thrown away** (`split_whitespace().join(" ")`). The files
    // carry per-character spaces — 「情 绪 小 集 怀 旧 季」 — which is a karaoke convention for
    // highlighting one character at a time. Nothing here highlights, so keeping them would space
    // a twelve-character line across half the panel for no reason a reader could see.
    let want_lyric = app
        .deck_lyric()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|text| !text.is_empty());
    if lines.sung != want_lyric {
        match &want_lyric {
            Some(text) => {
                let face = word_face(text);
                let (w, h) = (face.w, face.h);
                let id = upload(compositor, &mut lines.lyric, face);
                app.set_deck_lyric_face(Some((id, w, h)));
            }
            None => {
                app.set_deck_lyric_face(None);
                lines.lyric = None;
            }
        }
        lines.sung = want_lyric;
    }
    // The panel last, because it reads the disc and the lyric the two above resolved.
    sync_lcd_panel(app, compositor, lines);
}

/// The info page, composed as **one panel-shaped frame** — ★ 2026-10-06 重做，
/// 上一版是逐行 `word_face`（货架/菜单那一套渲染：矢量字、抗锯齿、没有格网），
/// 所以它和播放屏并排放的时候不像同一台机器。现在它与 `sync_lcd_panel` 走**同一条路**：
/// `lcd::info_frame` / `lcd::info_keys_frame` 合成一张 1-bit 帧，`render` 用**同一套色板、
/// 同一张格网、同一个倍率**变成 720×480。
///
/// **三道门，第一道最要紧**：
///  1. `is_disc_info()` —— 不在屏上就**一帧都不做**。这一页要合成一整张
///     240×160 的 1-bit 帧（38 400 像素，逐像素阈值化 + 上采样），
///     每帧都做一次是纯浪费，而这一页一天里绝大多数帧都不该存在。
///  2. `same_as` —— 这一页说的东西（碟片 / 曲目 / 标签）没变就不重烘。
///     **`← →` 翻页也在这个门外**：所以门里必须把 `page` 也算进去，否则翻页不重烘。
///  3. 色板档位 —— `Y` 键在信息页上照样换色板，所以它是第三个输入。
///
/// **纹理不回收**：与面板同一个道理（`lines.panel` 也就是留着），再来一次是一次比较而不是
/// 一次合成；留着只占一张 720×480，而释放它意味着第二次进来要整个重画。
fn sync_info_panel(
    app: &mut App,
    compositor: &mut Compositor,
    tex: &mut Option<TexId>,
    built: &mut Option<(crate::app::DeckInfo, usize, usize)>,
) {
    if !app.is_disc_info() {
        return;
    }
    let info = app.deck_info();
    let page = app.info_page();
    let ramp_at = app.deck_ramp_index();
    if built
        .as_ref()
        .is_some_and(|(b, p, r)| *p == page && *r == ramp_at && b.same_as(&info))
    {
        return;
    }
    let frame = info.frame(page);
    let (_, ramp) = app.deck_ramp();
    // ★ **格网开着，与面板一致**：这一页是"这块屏在讲另一件事"，而格网是这块屏的物理属性
    //   （见 `lcd::grid_bottom` 的注释）。关掉它就等于说"这一页不是这块屏"。
    let rgba = slot_ui::lcd::render(&frame, ramp, true);
    let (w, h) = (slot_ui::lcd::PANEL_W, slot_ui::lcd::PANEL_H);
    let id = upload_rgba(compositor, tex, w, h, &rgba);
    app.set_info_panel(Some((id, w, h)));
    *built = Some((info, page, ramp_at));
}

/// The palette browser's row of names: one face per cell, so every cell on the panel says what
/// it is rather than only the one under the caret. A colour machine's page, whose nine grades are
/// named in four to six characters each.
///
/// Mirrors `sync_shelf_title` in what it costs and how: the key is the page, so a turn of the
/// page is the only thing here that rasterises anything, and a caret walking inside a page —
/// which is nearly every frame the browser is up — is a comparison and nothing else. The
/// textures come from a pool and go back into it, and every face is the cell's own width, so
/// the pool neither grows nor has to be resized after the first page.
///
/// Empty on a Game Boy, whose names do not fit a cell: the faces are cleared and
/// `sync_palette_name` sets the foot line instead.
fn sync_palette_names(
    app: &mut App,
    compositor: &mut Compositor,
    pool: &mut Vec<Option<TexId>>,
    shown: &mut Option<(System, usize)>,
) {
    let key = app.palette_names_key();
    if *shown == key {
        return;
    }
    *shown = key;
    if key.is_none() {
        app.set_palette_names(Vec::new());
        return;
    }
    let (w, h) = (palette_cell_w(), palette_name_h());
    let names = app.palette_names();
    let mut faces = Vec::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        // The author's own name, number and all: `PS40 Sunburst` is what the same palette is
        // called in `gbcpalettes.h`, so a name read off the device can be looked up there
        // without a translation table in between. Fixed-size boxes, so a long one is set smaller
        // rather than clipped and the pool below is reused instead of rebuilt.
        let face = palette_name_face(name, w, h);
        while pool.len() <= i {
            pool.push(None);
        }
        let id = upload_rgba(compositor, &mut pool[i], w, h, &face.rgba);
        faces.push((id, w, h));
    }
    app.set_palette_names(faces);
}

/// The palette browser's one line of type: the name of the entry under the caret, for the tables
/// whose names are longer than a cell. Mirrors `sync_shelf_title` — one rasterise per move of the
/// caret, and none at all on a frame where nothing moved.
///
/// The box is `dialog_line_face`'s, 640 pixels wide and twice the type size tall, so a name that
/// no cell could hold is set here whole, at the size the rest of the interface uses.
fn sync_palette_name(
    app: &mut App,
    compositor: &mut Compositor,
    slot: &mut Option<TexId>,
    shown: &mut Option<String>,
) {
    let want = app.palette_name_want();
    if *shown == want {
        return;
    }
    *shown = want.clone();
    match want {
        Some(name) => {
            let face = dialog_line_face(&name, PALETTE_FOOT_PX);
            let (w, h) = (face.w, face.h);
            let id = upload(compositor, slot, face);
            app.set_palette_name_face(Some((id, w, h)));
        }
        None => app.set_palette_name_face(None),
    }
}

/// (Re)build the cheat table's row faces when the seated cart changes. Mirrors `sync_shelf_title`:
/// only rasterises when the highlight's stem differs from the one the faces were last built for,
/// so a long list is rasterised once per cart, not once per frame.
fn sync_cheat_faces(app: &mut App, compositor: &mut Compositor) {
    let want = app.selected_stem().map(str::to_string);
    let n = app.cheats().len();
    // Rebuild when the cart changes OR when the code count changes. Cheats load from the card
    // after the first build (when the core spawns), so an early build would otherwise cache empty
    // faces and the table would show chips with no text.
    if app.cheat_face_stem() == want.as_deref() && app.cheat_face_count() == n {
        return;
    }
    match want {
        Some(stem) => {
            let rows: Vec<(String, String)> = app
                .cheats()
                .iter()
                .map(|c| (c.desc.clone(), c.code.clone()))
                .collect();
            let faces: Vec<(TexId, u32, u32)> = rows
                .iter()
                .map(|(desc, code)| {
                    let f = cheat_row_face(desc, code);
                    (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
                })
                .collect();
            app.set_cheat_faces(Some(stem), faces);
        }
        None => app.set_cheat_faces(None, Vec::new()),
    }
}

/// ★ 2026-10-21：菜单标题行的脸 —— 游戏名（`selected_stem` 的清洗版，和货架标题同一套
/// 清洗），跟着 stem 走：换一张卡重烘一次，同一张卡上一帧零成本。缓存键存 `App` 侧
/// （`game_title_built_for`，和 `cheat_face_stem` 同一款先例）。
fn sync_game_title(app: &mut App, compositor: &mut Compositor) {
    let want = app.selected_name();
    let built = app.game_title_built_for().map(str::to_string);
    if built == want {
        return;
    }
    match want {
        Some(name) => {
            // ★★ 2026-10-21：菜单的抬头用**自己那一档字号**（38），不是货架标题的 30 ——
            // 它下面是一排 26 的按钮，标题只大 4 像素就只剩"一行略大的选项"。
            let face = slot_ui::game_title_face(&name);
            let id = compositor.create_texture(face.w, face.h, &face.rgba);
            app.set_game_title_face(Some((id, face.w, face.h)));
            app.set_game_title_built_for(Some(name));
        }
        None => {
            app.set_game_title_face(None);
            app.set_game_title_built_for(None);
        }
    }
}

/// ★ 2026-10-26：PS1 光盘封面贴图 —— 从游戏封面 PNG 加载后上传 GPU。
/// 和 `sync_game_title` 同一模式：跟着 stem 走，换一张卡重烘一次。
fn sync_disc_face(app: &mut App, compositor: &mut Compositor) {
    let want_stem = app.disc_face_stem();
    let built = app.disc_face_built_for().map(str::to_string);
    if built == want_stem {
        return;
    }
    match app.disc_face_path() {
        Some(path) => {
            if let Some(rgba) = slot_ui::cover(&path, 300, 300) {
                let id = compositor.create_texture(300, 300, &rgba);
                app.set_disc_face(Some((id, 300, 300)));
            } else {
                app.set_disc_face(None);
            }
        },
        None => {
            app.set_disc_face(None);
        }
    }
    app.set_disc_face_built_for(want_stem);
}

/// ★ 2026-10-21：**保存设置弹窗**的三族面 —— 弹窗开着的那一帧建好，关了放走
/// （和 `sync_switcher` 同一模式）。标题、四个键帽、四行字，开机时这些字还不存在
/// （`SAVE_SET_*` 是菜单行），所以这里是懒建：第一次打开重烘五张小图，之后开着不动。
fn sync_save_settings(app: &mut App, compositor: &mut Compositor, open: &mut bool) {
    let is_open = matches!(app.game_menu(), Some(crate::app::GameMenu::SaveSettings));
    if *open == is_open {
        return;
    }
    *open = is_open;
    if !is_open {
        app.set_save_settings_faces(None);
        return;
    }
    // 标题 + 四行（键帽 + 行字），一次建齐。
    let title = slot_ui::title_face(slot_ui::lang::SAVE_SET_TITLE);
    let title_face = (
        compositor.create_texture(title.w, title.h, &title.rgba),
        title.w,
        title.h,
    );
    let rows = [
        (slot_ui::lang::SAVE_SET_A, "A"),
        (slot_ui::lang::SAVE_SET_X, "X"),
        (slot_ui::lang::SAVE_SET_Y, "Y"),
        (slot_ui::lang::SAVE_SET_B, "B"),
    ];
    let mut caps = Vec::new();
    let mut lines = Vec::new();
    for (text, key) in rows {
        let cap = slot_ui::hint_face(key, text);
        caps.push((
            compositor.create_texture(cap.w, cap.h, &cap.rgba),
            cap.w,
            cap.h,
        ));
        let line = slot_ui::menu_face_at(
            text,
            slot_ui::GAME_ROW_PX,
            slot_ui::GAME_ROW_H,
            slot_ui::GAME_ROW_PAD,
        );
        lines.push((
            compositor.create_texture(line.w, line.h, &line.rgba),
            line.w,
            line.h,
        ));
    }
    app.set_save_settings_faces(Some((title_face, caps, lines)));
}

/// ★ 2026-10-21：**操作键映射弹窗的面**。开着的那一帧建，关了就放走；开着的时候只有在
/// 「七行各自显示的值」变了时才重建 —— 改哪行就重烘哪行是不可能的（`Sync` 里没有按行
/// 缓存），但**同一份 sig 不重建**已经足够让卷动目标清单不花任何成本。
///
/// 面一共五族：标题；七行（六键 + 频率）；高亮（一张）；脚线；十五个目标；保存确认的
/// 标题 + 两行。全部在 `slot_ui` 那边按像素烤，这里只负责上传与释放。
/// ★ 2026-10-21：**这一排游戏菜单面按这一局的机种印。**
///
/// 只有 `Grid` 那一行的字跟机器走：GBA 是「网格切换」（LCD3x 网格 / 扫描线），GB/GBC 是
/// 「遮罩切换」（遮罩艺术环）。开机那一遍在货架上印，空槽读作 GBA，所以插上 Game Boy 卡
/// 必须重印一次 —— 否则那一行会一直写着这台机器根本没有的网格。
///
/// **只在机种变了的时候印**：整排八张，一次几毫秒，但每帧都印就是每帧几毫秒。换卡的时候
/// 印，时机正好落在插入动画里 —— 菜单不开，玩家看不到这一下。
fn sync_game_menu_rows(
    session: &mut Session,
    compositor: &mut Compositor,
    built: &mut Option<System>,
) {
    // 还没印过：等 `upload_late_furniture` 印第一遍，那一边也会把 `built` 填上。
    if built.is_none() {
        return;
    }
    let want = session.app().seated_system();
    if *built == Some(want) {
        return;
    }
    // 旧的一排要放走 —— 拿走而不是借出：换的是一整批，中间不留半新半旧的时刻。
    let old = session.app_mut().take_game_menu_faces();
    for (tex, _, _) in old {
        compositor.release_texture(tex);
    }
    let texts: Vec<&'static str> = GameRow::ALL.iter().map(|r| r.text_for(want)).collect();
    let (idle, hot) = game_menu_rows(compositor, &texts);
    session.app_mut().set_game_menu_faces(idle, hot);
    *built = Some(want);
}

/// ★★ 2026-10-21：**游戏菜单那一排按钮面。** 每行印两张 —— 静止的一张、手里那张亮底的
/// 一张 —— 宽度是**格子的宽**（`App::game_menu_cell_w`），不是按字数伸缩：一排按钮要一样宽
/// 才成排，按字数伸缩是电源菜单那种"一行一个词居中放着"的形状。
fn game_menu_rows(
    compositor: &mut Compositor,
    texts: &[&'static str],
) -> (Vec<crate::app::Upload>, Vec<crate::app::Upload>) {
    let w = crate::app::game_menu_cell_w().round() as u32;
    let mut idle = Vec::with_capacity(texts.len());
    let mut hot = Vec::with_capacity(texts.len());
    for label in texts {
        let off = slot_ui::game_row_face(label, w, false);
        idle.push((compositor.create_texture(off.w, off.h, &off.rgba), off.w, off.h));
        let on = slot_ui::game_row_face(label, w, true);
        hot.push((compositor.create_texture(on.w, on.h, &on.rgba), on.w, on.h));
    }
    (idle, hot)
}

fn sync_keymap(
    app: &mut App,
    compositor: &mut Compositor,
    built: &mut Option<Vec<String>>,
) {
    let open = app.keymap_edit().is_some();
    let system = app.current_machine();
    if !open {
        // 关了：放走所有面。
        if built.is_some() {
            if let Some(old) = app.keymap_faces() {
                let ids = keymap_face_ids(old);
                for id in ids {
                    compositor.release_texture(id);
                }
            }
            app.set_keymap_faces(None);
            *built = None;
        }
        return;
    }
    let sig = app.keymap_row_values();
    if built.as_deref() == Some(sig.as_slice()) {
        return;
    }
    // 和前一帧比变了（或者第一次打开）：旧的全放走，新的全烘一遍。
    if app.keymap_faces().is_some() {
        if let Some(old) = app.keymap_faces() {
            let ids = keymap_face_ids(old);
            for id in ids {
                compositor.release_texture(id);
            }
        }
    }

    // ── 标题 ────────────────────────────────────────────────────────────────────────────
    // ★★ 2026-10-26：**标题带上机种名。** 键位表现在是「每台机种一份」——从货架设置页进来的人
    // 必须先知道自己在改哪一台，否则那一列「默认」是哪个默认就说不清。半角括号，中英文都读得通。
    let title_text = format!(
        "{} ({})",
        slot_ui::lang::KEYMAP_TITLE,
        system.dir_name()
    );
    let title = slot_ui::keymap_title_face(&title_text);
    let title_face = (
        compositor.create_texture(title.w, title.h, &title.rgba),
        title.w,
        title.h,
    );

    // ── 行：物理键（左名，右值）+ 频率。行数按机种（PS1 八行）──
    let mut rows = Vec::new();
    for (p, value) in slot_ui::keys::physical_for(system).iter().zip(sig.iter()) {
        let face = slot_ui::keymap_row_face(slot_ui::keys::name(*p), value);
        rows.push((
            compositor.create_texture(face.w, face.h, &face.rgba),
            face.w,
            face.h,
        ));
    }
    // 频率行：名字是 `TURBO_RATE`，值是 `sig` 的最后一项（`"8 Hz"`）。
    let rate_name = slot_ui::lang::TURBO_RATE;
    let rate_value = sig.last().cloned().unwrap_or_default();
    let rate = slot_ui::keymap_row_face(rate_name, &rate_value);
    rows.push((
        compositor.create_texture(rate.w, rate.h, &rate.rgba),
        rate.w,
        rate.h,
    ));

    // ── 高亮（一张） ──────────────────────────────────────────────────────────────────
    let hl = slot_ui::keymap_highlight_face();
    let highlight = (
        compositor.create_texture(hl.w, hl.h, &hl.rgba),
        hl.w,
        hl.h,
    );

    // ── 脚线 ─────────────────────────────────────────────────────────────────────────
    let foot = slot_ui::keymap_foot_face(slot_ui::lang::KEYMAP_FOOT);
    let foot_face = (
        compositor.create_texture(foot.w, foot.h, &foot.rgba),
        foot.w,
        foot.h,
    );

    // ── 候选目标（按机种裁剪的池子，标签按机种翻译）─────────────────────────────────
    let mut targets = Vec::new();
    for t in slot_ui::keys::targets_for(system).iter() {
        let face = slot_ui::keymap_target_face(t.name, slot_ui::keys::face_label(system, t.name));
        targets.push((
            compositor.create_texture(face.w, face.h, &face.rgba),
            face.w,
            face.h,
        ));
    }

    // ── 保存确认：标题 + 两行（A/B） ───────────────────────────────────────────────────
    let st = slot_ui::keymap_title_face(slot_ui::lang::KEYMAP_SAVE_TITLE);
    let save_title = (
        compositor.create_texture(st.w, st.h, &st.rgba),
        st.w,
        st.h,
    );
    let mut save_lines = Vec::new();
    for line in [slot_ui::lang::KEYMAP_SAVE_A, slot_ui::lang::KEYMAP_SAVE_B] {
        let face = slot_ui::menu_face_at(
            line,
            slot_ui::GAME_ROW_PX,
            slot_ui::GAME_ROW_H,
            slot_ui::GAME_ROW_PAD,
        );
        save_lines.push((
            compositor.create_texture(face.w, face.h, &face.rgba),
            face.w,
            face.h,
        ));
    }

    app.set_keymap_faces(Some(crate::app::KeymapFaces {
        title: title_face,
        rows,
        highlight,
        foot: foot_face,
        targets,
        save_title,
        save_lines,
        sig: sig.clone(),
    }));
    *built = Some(sig);
}

/// ★ 2026-10-21：**Wi-Fi 配置浮层的面。** 定句十六句每开一次重烘（`word_face` 一句一张，
/// 便宜）；会变的两样 —— 当前 SSID、扫描结果 —— 跟着 `wifi_dirty` 走。关了就整族放走，
/// 和键位弹窗同一个纪律：面不留在 App 里让前端以为"还没建"。
fn sync_wifi(app: &mut App, compositor: &mut Compositor) {
    if app.wifi().is_none() {
        if let Some(old) = app.wifi_faces() {
            for id in wifi_face_ids(old) {
                compositor.release_texture(id);
            }
            app.set_wifi_faces(None);
        }
        return;
    }
    if app.wifi_faces().is_some() && !app.wifi_dirty() {
        return;
    }
    if let Some(old) = app.wifi_faces() {
        for id in wifi_face_ids(old) {
            compositor.release_texture(id);
        }
    }
    let mut line = |s: &str| {
        let f = slot_ui::page_face(s);
        (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
    };
    // 当前 SSID：卡上有什么就烤什么，没有就不给这一张（状态视图改画"尚未配置"）。
    let ssid = app.wifi_conf().map(|c| line(&c.ssid));
    let nets: Vec<(TexId, u32, u32)> = app
        .wifi()
        .map(|w| w.nets.iter().map(|n| line(&n.ssid)).collect())
        .unwrap_or_default();
    app.set_wifi_faces(Some(crate::app::WifiFaces {
        title: line(lang::SETTINGS_WIFI),
        label_current: line(lang::WIFI_CURRENT),
        label_ssid: line(lang::WIFI_SSID_LABEL),
        label_psk: line(lang::WIFI_PSK_LABEL),
        label_open: line(lang::WIFI_OPEN),
        act_scan: line(lang::WIFI_SCAN),
        act_manual: line(lang::WIFI_MANUAL),
        act_del: line(lang::WIFI_DEL),
        line_pick: line(lang::WIFI_PICK),
        line_none: line(lang::WIFI_NONE),
        line_scanning: line(lang::WIFI_SCANNING),
        line_no_nets: line(lang::WIFI_NO_NETS),
        line_saved: line(lang::WIFI_SAVED),
        hint_status: line(lang::WIFI_HINT_STATUS),
        hint_list: line(lang::WIFI_HINT_LIST),
        hint_edit: line(lang::WIFI_HINT_EDIT),
        ssid,
        nets,
    }));
}

/// 一族 Wi-Fi 面里所有的 texture id，用于整族释放。`ssid` 和 `nets` 是那两样会变的，
/// 释放时它们可能已经不在族里（`None` / 空），所以逐个 `Option` 着取。
fn wifi_face_ids(faces: &crate::app::WifiFaces) -> Vec<TexId> {
    let mut ids = vec![
        faces.title.0,
        faces.label_current.0,
        faces.label_ssid.0,
        faces.label_psk.0,
        faces.label_open.0,
        faces.act_scan.0,
        faces.act_manual.0,
        faces.act_del.0,
        faces.line_pick.0,
        faces.line_none.0,
        faces.line_scanning.0,
        faces.line_no_nets.0,
        faces.line_saved.0,
        faces.hint_status.0,
        faces.hint_list.0,
        faces.hint_edit.0,
    ];
    if let Some(t) = faces.ssid {
        ids.push(t.0);
    }
    ids.extend(faces.nets.iter().map(|n| n.0));
    ids
}

/// 一族键位面里所有的 texture id，用于整族释放。
fn keymap_face_ids(faces: &crate::app::KeymapFaces) -> Vec<TexId> {
    let mut ids = Vec::new();
    ids.push(faces.title.0);
    ids.extend(faces.rows.iter().map(|r| r.0));
    ids.push(faces.highlight.0);
    ids.push(faces.foot.0);
    ids.extend(faces.targets.iter().map(|t| t.0));
    ids.push(faces.save_title.0);
    ids.extend(faces.save_lines.iter().map(|l| l.0));
    ids
}

/// Reconciles the per-system screen overlay texture with `overlay_pixels`. Called every frame,
/// but does nothing unless `overlay_dirty` is set: on insert the GL resource is minted once,
/// and on a cart with no overlay (GBA, or a missing/corrupt PNG) any previous texture is
/// released. The decoded RGBA is owned by `App`; only the compositor touches the GL name.
fn sync_overlay(app: &mut App, compositor: &mut Compositor) {
    if !app.overlay_dirty() {
        return;
    }
    if let Some(old) = app.take_overlay_tex() {
        compositor.release_texture(old);
    }
    let tex = match app.overlay_pixels() {
        Some((rgba, w, h))
            if *w > 0
                && *h > 0
                && (rgba.len() as u64) >= (*w as u64) * (*h as u64) * 4 =>
        {
            Some(compositor.create_texture(*w, *h, rgba))
        }
        _ => None,
    };
    app.set_overlay_tex(tex);
    app.set_overlay_clean();
}

fn sync_core_picker(
    app: &mut App,
    compositor: &mut Compositor,
    builder: &FaceBuilder,
    asked: &mut Option<String>,
    board: &mut Option<TexId>,
    lid: &mut Option<TexId>,
    built: &mut Option<String>,
) {
    let highlighted = app.selected_stem().map(str::to_string);
    // ★ 2026-11-09：当前机种优先（`App::cart_by_stem`）。全库 `find` 在同名跨机种时会拿到
    //   排序靠前的那条 —— `Games/FC/…` 恒在 `Games/GBA/…` 之前（`F` < `G`），于是这张
    //   GBA 卡的核心板会用 FC 那条的贴纸去烘。
    if let Some(stem) = highlighted.as_deref() {
        if *asked != highlighted {
            if let Some(cart) = app.cart_by_stem(stem) {
                builder.request(cart.clone());
            }
            *asked = highlighted.clone();
        }
    }
    let Some(faces) = builder.take() else {
        return;
    };
    // A build for a cart the caret has since left is dropped; the one it is on is on its way.
    if highlighted.as_deref() != Some(faces.stem.as_str()) || *built == highlighted {
        return;
    }
    let board_id = upload_rgba(
        compositor,
        board,
        faces.board.w,
        faces.board.h,
        &faces.board.rgba,
    );
    let lid_id = upload_rgba(compositor, lid, faces.lid.w, faces.lid.h, &faces.lid.rgba);
    app.set_core_board_faces(board_id, lid_id);
    *built = Some(faces.stem);
}

fn upload(compositor: &mut Compositor, slot: &mut Option<TexId>, face: slot_ui::UndoFace) -> TexId {
    upload_rgba(compositor, slot, face.w, face.h, &face.rgba)
}

/// Into the slot's own texture if it has one, so the pool stops growing after the first time.
fn upload_rgba(
    compositor: &mut Compositor,
    slot: &mut Option<TexId>,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> TexId {
    match *slot {
        Some(id) => {
            compositor.update_texture(id, w, h, rgba);
            id
        }
        None => {
            let id = compositor.create_texture(w, h, rgba);
            *slot = Some(id);
            id
        }
    }
}
