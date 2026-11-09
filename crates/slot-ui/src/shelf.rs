use std::path::Path;

use slot_gfx::{Draw, TexId, OUT_W};
use slot_store::Cart;

use crate::cart::{label_colour, label_text, size_for, CartSize};
use crate::hud::Millis;
use crate::slot_chrome::{draw_slot_inside, draw_slot_shell};
use slot_store::System;

/// The centre cart is drawn larger than the row so it reads as the one in hand; the
/// neighbours keep their full size but are pushed to the edges (see the `CartSize` pitch). 1.5
/// makes the selection clearly the hero without dwarfing the neighbours off screen.
///
/// Public because it is not the shelf's business alone: the app scales the cart into the slot
/// from this size, and a cart drawn at rest is a `CENTER_SCALE` cart to anything looking for
/// one.
pub const CENTER_SCALE: f32 = 1.5;
/// How much of its face a side cart keeps at full recede. Public for the same reason as
/// `CENTER_SCALE`: the app derives how much further to dim the neighbours from it, and that
/// arithmetic has to follow this number rather than be written down against an old one.
///
/// ★★ 2026-10-28（六六第二次报同一件事：「换了壁纸才发现又犯了一个老问题：货架上的卡带是半
/// 透明的」）—— **0.7 → 0.90**。
///
/// 0.7 这个数一直是按「压暗侧位」的意思在用，可 alpha 合成的结果取决于底下是什么：底是黑的
/// （默认壁纸中央、暗态 `ground`）时 `0.7 × 卡面 + 0.3 × 黑` 恰好就是"卡面乘 0.7"，看起来是
/// 压暗——**这七个月的观感一直是这么来的**。底一旦换成卡自己的亮壁纸，同一份 0.3 就原形毕露：
/// 透出来的是壁纸。
///
/// 2026-10-17 用一块剪影黑影去补过，2026-10-25 又因为"贴图不透明、影子只能从 alpha 推、推
/// 出来是方角矩形"把它删了 —— 删得没错（那推理只对了下半句：贴图不透明**不等于**合成结果
/// 不透明），但删完没人把那 0.3 处理掉，于是问题第二次冒头。
///
/// 真正的杠杆就是这个数：0.90 时壁纸最多透 10%，而它还要先过壁纸自己那层纱（暗态没有、亮态
/// 0.74、卡自己的图 0.62），实际落到眼睛里 2~4%，看不出。压暗的含义随之退到
/// 「侧位就是小一号的卡」—— 之前那个"暗"本来就是黑底给的假象，不是这张面自己的颜色。
///
/// ★★ 2026-11-05（第三次，六六：「把两侧卡带改不透明」）—— **0.90 → 1.0**。0.90 时壁纸仍
/// 最多透 10%，真机上还是有人看得出两侧是「半透明的」。抬到 1.0，侧位就是一张完全不透明的
/// 贴图，底下是什么都不透。「压暗侧位」这个含义到此彻底退掉 —— 之前那个"暗"本来就是黑底给的
/// 假象，不是这张面自己的颜色，侧位和小一号的卡之间不再有任何透明度差。
pub const SIDE_ALPHA: f32 = 1.0;
/// Carts stand on the row rather than float: the foot stays put as a cart shrinks away.
/// Where the row's feet stand — the carts stand on the row rather than float on it. Derived
/// from the cart's height, so a taller shell stands lower and the row keeps its proportions.
pub(crate) fn foot_y(size: CartSize) -> f32 {
    size.foot_y()
}
/// Critically damped, so a flick lands on a cart instead of bouncing past and returning.
const OMEGA: f32 = 16.0;
/// How far the cart next to the selection is pushed aside as the chosen one goes in. Enough
/// to clear the frame from where it stands.
const PART: f32 = 130.0;
/// How much a face-less cart is stretched along the row while the ring glides. Small on
/// purpose: a cart stretched far enough to notice as a *shape* stops reading as a cart, and the
/// point is speed rather than distortion. Exactly 1 at rest, so a still row is untouched.
const SMEAR: f32 = 1.12;

/// Slots considered either side of the selection. Two reach the edges of a 720 row, the
/// third covers the lag while the spring is still catching up with a flick.
const SLOTS: i32 = 3;

/// How far a cart is drawn from its resting place at the top of a system-shelf swap beat, in
/// offscreen pixels. Well past the top of the panel, so a risen cart is off screen and not
/// merely high.
const SWAP_RISE: f32 = 560.0;
/// The stagger between neighbouring slots, as a share of the beat: the centre cart leads and
/// each cart further out follows it, so the row is drawn away in order rather than at once.
const SWAP_STAGGER: f32 = 0.16;

/// The row's vertical motion while the system shelf is being swapped.
///
/// `p` is 0..1 through the current beat; `ascend` is true while the old shelf is being drawn
/// up and away (the carts climb and fade), and false while the new shelf drops back into place
/// (the carts fall from above and fade in). Absent at rest, so a still row is laid out exactly
/// as it always was — this is the one thing a swap changes about the layout, and it is opt-in.
#[derive(Copy, Clone)]
pub struct Motion {
    pub p: f32,
    pub ascend: bool,
}

/// Before the first repeat. Long enough that a press meaning one cart cannot become two.
const REPEAT_DELAY_MS: Millis = 400;
/// Between repeats after that. Fast enough to cross a thirty cart library, slow enough to
/// stop on one.
const REPEAT_MS: Millis = 110;

/// A letter jump's own travel, played by hand rather than by the spring.
///
/// The spring's own travel is what a single cart step wants, but a letter jump can cross
/// hundreds of carts and the spring's pace does not scale with distance — it settles in the
/// same few hundred milliseconds whatever it is asked to cover, which at range is a blur the
/// eye cannot read and at one cart is exactly right. So a jump glides instead: `scroll` is
/// swept from where the row stands to the cart the dial chose, over a duration that grows a
/// little with the distance and then stops growing. The sweep is a smoothstep, so it eases in
/// and out — the row *accelerates* away from the letter it is leaving and settles onto the
/// one it is arriving at, and every cart it skips passes through the middle on the way.
#[derive(Copy, Clone)]
struct Glide {
    from: f32,
    to: f32,
    t: f32,
    dur: f32,
}

/// One cart's place on screen this frame, addressed by its index in `Shelf::carts`.
///
/// The row's layout, stated once and read twice: `draw_row` draws the faces from it, and the
/// favourites star is placed from it, so a mark on a cart cannot drift off the cart it marks.
pub struct SlotRect {
    pub cart: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The cart's own opacity: 1.0 at the selection, `SIDE_ALPHA` at and past one slot out,
    /// then cut by the `recede` the caller asked for.
    pub alpha: f32,
    /// ★ 2026-10-21：这张是不是**手里拿的那张**（环上的 0 号位）。
    ///
    /// 从 `alpha` 反推是不行的：`recede` 会把中心那张也压下去，插卡途中中心位的 alpha
    /// 可以低到 0.2，而侧位永远是 `SIDE_ALPHA`。焦点光是**位置**的事实，不是透明度的事实，
    /// 所以它得自己带一个布尔。
    pub selected: bool,
}

/// The glide's length in seconds, as a function of the carts it has to cross. Short jumps are
/// near a spring's own settle so a single-letter step does not feel slower than an arrow; the
/// cap keeps a four-hundred cart jump from becoming a slideshow.
fn glide_seconds(distance: f32) -> f32 {
    (0.22 + distance * 0.004).clamp(0.22, 0.85)
}

pub struct Shelf {
    pub carts: Vec<Cart>,
    /// Which machine's shelf is up. Every cart on the row belongs to it, and so does the row's
    /// geometry: a Game Boy cart is portrait and a Game Boy shelf is its own layout.
    system: System,
    /// The row's geometry — the cart's size, the pitch between neighbours, how much bigger the
    /// selected one is.
    ///
    /// A field rather than a call to `size_for` at the point of use, because the row's layout is
    /// a function of this and of nothing else (`slot_rects` reads `w`, `h`, `pitch`, `middle`,
    /// `side` and `foot_y` and never asks what a cart *is*). Keeping it here is what lets a shelf
    /// be laid out for something that is not a machine's cartridge at all: `set_system` derives
    /// it, and `set_size` overrides it for a shelf whose things are not cartridges.
    size: CartSize,
    /// The carts the ring actually walks, as indices into `carts`. The full library when
    /// nothing is filtered; the starred ones while the favourites shelf is up. A list of
    /// indices rather than a rebuilt cart list, and that is the whole point: `faces` stays
    /// indexed by the library, so every texture the row was showing is the same texture after
    /// the view changes. Rebuilding the shelf instead would drop those faces and put blank
    /// placeholders on screen while they were rasterised again.
    view: Vec<usize>,
    /// Each machine's own shelf, in the order that shelf shows — the three libraries the row
    /// can walk. `carts` is the card; these are the shelves, and the distinction is the whole
    /// reason this exists: a machine shelf is a *scattered* subset of the card's sorted list
    /// (this card files its three Game Boy carts at 27, 127 and 637 among nine hundred Advance
    /// ones), so any measurement of "how far away is this cart" that walks `carts` is measuring
    /// the wrong thing.
    machines: [Vec<usize>; MACHINES],
    /// Each machine's own subfolders, sorted and distinct — the folders `folder_pick` chooses
    /// among. A cart loose in its machine's folder contributes none; the folders are read off the
    /// roms' own paths (`Games/<machine>/<folder>/<rom>`), so a card that files its games per
    /// machine and then per language needs no second index to be switchable.
    folders: [Vec<String>; MACHINES],
    /// Which of a machine's subfolders its shelf is showing, or `None` for all of them. Per
    /// machine, so a folder picked on one shelf is still picked when the user comes back to it.
    folder_pick: [Option<usize>; MACHINES],
    /// One entry per cart: its position in its own machine's shelf, or `u32::MAX` for a cart no
    /// shelf shows. Where a switch to that machine lands is the *front* of this list, which is
    /// what makes the front of every machine the set worth keeping faces for.
    pos_in_machine: Vec<u32>,
    /// One entry per cart: its position in `view`, or `u32::MAX` when the view does not hold it.
    /// A table rather than a search, because the filler asks this of every cart it could build,
    /// every frame.
    pos_in_view: Vec<u32>,
    /// Position in `view`, not an index into `carts`. `current()` is what turns it back.
    pub index: usize,
    pub scroll: f32,
    /// One slot per cart, `None` until its face has been rasterised and uploaded. The
    /// shelf is born showing placeholders and fills in behind the caret: rasterising every
    /// cart up front costs about 70 ms each on this hardware, which is seven seconds on a
    /// hundred-game card spent before the first frame.
    faces: Vec<Option<TexId>>,
    /// A cart with a blank label, drawn for a cart whose own face is not built yet. One texture
    /// for the whole row — until the label goes on they are all the same cart — and it is what a
    /// jump across the alphabet shows sliding past.
    placeholder: Option<TexId>,
    /// ★ 2026-10-21：手里那张卡脚下的柔光池。一张纹理，整行共用 ——
    /// 每个选中卡带的脚下都是同一滩光，只是宽度跟着卡带走。
    focus: Option<TexId>,
    /// ★ 2026-10-22（六六）：机种 logo 高光区，画在卡座背层（卡带之下）。
    /// 空槽时可见，卡带一插就盖住。按机种（GB/GBC/GBA/SFC/FC）各一张，`draw` 取
    /// `self.system` 那一张。`None` 是素材没到位 —— 货架照常画，只是没 logo。
    logo: [Option<(TexId, u32, u32)>; MACHINES],
    vel: f32,
    /// Whether the case's own slot is drawn under the row.
    ///
    /// **The disc row does not draw it.** The slot is a cartridge bay: it is there so the cart
    /// the user picked has a visible place to go, and it is the bottom of the screen on every
    /// cartridge screen. A row of sleeves has no bay and nothing is pushed into one, so drawing
    /// a mouth under it promises a mechanism the screen does not have (2026-10-07, 六六). Set
    /// with `set_mouth`, and reset by `set_size`'s callers rather than here.
    mouth: bool,
    /// The direction being held and when it next repeats. Repeat lives here rather than in
    /// the gesture layer so nothing in game starts auto firing.
    held: Option<(i32, Millis)>,
    /// A letter jump in flight. While one is running it owns `scroll` and the spring stands
    /// down; a cart step drops it so the arrows always answer immediately.
    glide: Option<Glide>,
    /// The system-shelf swap in flight, if any. See `Motion`. `None` at rest.
    motion: Option<Motion>,
}

/// 静置时中间那张卡的**待机动效**：竖轴偏转角（弧度）与脚下柔光的倍数。
///
/// ★ 2026-10-25（六六：「1 sway，幅度再大一点点，叠加柔光」）——
/// 从五种离机预演里挑了这一档：**左右轻摆 + 脚下那池光跟着呼吸**。
///
/// ★★ 2026-10-25 00:50（六六：「摆动看起来还是有点奇怪，能朝着玩家晃吗（就是轴是从卡带
/// 纵向穿过的，卡带左右两侧交替往玩家方向摆）」）——
/// 第一版是 `Draw::Turned`，也就是**在屏幕平面里转**：卡带看起来像被拧了一下，
/// 不是"转过来给你看"。改成 **`Draw::Yaw`：绕穿过卡带的竖直轴偏转**，
/// 于是画出来是**梯形**（近的一侧更高更宽，远的一侧更矮更窄），
/// 读作"这卡立在桌上，正被左右转着看"。
///
/// 为什么是这两个数字：
/// * **9°** —— 平面内转 1.2° 时边缘位移 7.6px；竖轴偏转要的是**近远两侧的高度差**，
///   同一个"看得出来"的量落在 9° 上（`YAW_DEPTH = 0.36` ⇒ 近边比远边高 12%）。
///   比它小就只在中间几像素动，比它大就开始像广角贴脸。
/// * 柔光取 `light::breathe`（3400 ms，0.55..1.0）—— 那是这台机器上**早就定好**的呼吸节奏
///   （发光件那颗灯的），不另造一个；两件事同频，观感才是一件东西。
///
/// `Idle::NONE` 是**恒等元**：`yaw = 0`、`glow = 1`。所有既有调用点（插卡、退卡、
/// 选核心、静态出图）都走它，所以这个功能对它们**一个字节都没改**。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Idle {
    /// 绕**竖轴**偏转的角度，弧度，正 = 右侧朝观察者。
    pub yaw: f32,
    /// 脚下柔光池的强度倍数。
    pub glow: f32,
}

impl Idle {
    /// 不动的那个值。默认即此。
    pub const NONE: Idle = Idle {
        yaw: 0.0,
        glow: 1.0,
    };

    /// 一帧的待机姿态，`now` 是开机以来的毫秒。
    ///
    /// 偏转与柔光**同相位**：`breathe` 在 `phase = .5` 最亮，而 `sin` 在 `.25` 转到底 ——
    /// 于是卡转到最侧时灯在中值，卡转回正时灯最亮。两条曲线错开四分之一拍，正好谁也不压谁。
    pub fn at(now: u32) -> Idle {
        let period = crate::light::BREATHE_MS;
        let phase = (now % period) as f32 / period as f32;
        // ⚠️ 光池**不能直接乘 `breathe`**：`breathe` 是 0.55..1.0，可光池自己的峰值
        //    alpha 只有 **0.22**（`light::pool` 里写着），再乘 0.55 就是 0.12 ——
        //    在近黑的底上那等于**熄掉**。离机实测：谷底帧里一个琥珀像素都找不到，
        //    峰值帧 173 个。那是"闪"不是"呼吸"，而 `light::breathe` 的下限本来就是为了
        //    不闪才不取 0。
        //    所以把呼吸**映射到 0.72..1.0**：幅度还是那条余弦，但谷底仍是一层看得见的暖光。
        //    （0.6 试过：谷底 0.6 × 0.22 = 0.13，离机图上仍读作"熄了"；0.72 是
        //     0.16，同一张对比图里能看出是"暗下去"而不是"没了"。）
        let b = crate::light::breathe(now);
        let t = ((b - 0.55) / 0.45).clamp(0.0, 1.0);
        Idle {
            yaw: YAW_DEG.to_radians() * (std::f32::consts::TAU * phase).sin(),
            glow: 0.72 + 0.28 * t,
        }
    }
}

impl Default for Idle {
    fn default() -> Self {
        Idle::NONE
    }
}

/// **竖轴偏转角**的峰值，度。见 [`Idle`] 的注释：平面内转 1.2° 只挪 7.6px，
/// 竖轴偏转要让近远两侧差出可见的高度，得 9°。
const YAW_DEG: f32 = 9.0;

/// **How many machines the shelf keeps a row for** — one per `slot_store::System`, and it is
/// the length of every per-machine array in this file.
///
/// It is a constant rather than the literal `7` it used to be so that adding a machine is one
/// edit here plus one arm in `machine_slot`, instead of a hunt for every `[..; 7]`. The two
/// cannot drift: `machine_slot`'s last arm is `MACHINES - 1` by construction, and a machine that
/// forgot its arm is a compile error rather than an index panic on a card.
///
/// ⚠️ **The number itself is load-bearing and may only grow.** A machine's slot is the key
/// `System/overlay.txt`, `System/display.txt` and `System/shelf.txt` store their settings by,
/// so renumbering would silently repoint every card already in the field. See `machine_slot`.
pub const MACHINES: usize = 10;

impl Shelf {
    pub fn new(carts: Vec<Cart>) -> Self {
        let view: Vec<usize> = (0..carts.len()).collect();
        let mut machines: [Vec<usize>; MACHINES] = Default::default();
        let mut pos_in_machine = vec![u32::MAX; carts.len()];
        for (i, c) in carts.iter().enumerate() {
            let list = &mut machines[Self::machine_slot(c.system())];
            pos_in_machine[i] = list.len() as u32;
            list.push(i);
        }
        let mut folders: [Vec<String>; MACHINES] = Default::default();
        for c in &carts {
            let slot = Self::machine_slot(c.system());
            if let Some(f) = folder_of(&c.rom, c.system().dir_name()) {
                if !folders[slot].contains(&f) {
                    folders[slot].push(f);
                }
            }
        }
        for f in folders.iter_mut() {
            f.sort();
        }
        let mut pos_in_view = vec![u32::MAX; carts.len()];
        for (p, &i) in view.iter().enumerate() {
            pos_in_view[i] = p as u32;
        }
        Shelf {
            carts,
            machines,
            folders,
            folder_pick: [None; MACHINES],
            pos_in_machine,
            pos_in_view,
            system: System::Gba,
            size: size_for(System::Gba),
            view,
            index: 0,
            scroll: 0.0,
            faces: Vec::new(),
            placeholder: None,
            focus: None,
            logo: [None; MACHINES],
            vel: 0.0,
            mouth: true,
            held: None,
            glide: None,
            motion: None,
        }
    }

    /// Sets the swap motion, or clears it. Driven by the app, which owns the timing; the row
    /// only reads it when laying out. `None` restores the resting layout exactly.
    pub fn set_motion(&mut self, motion: Option<Motion>) {
        self.motion = motion;
    }

    /// Which machine the row is showing. Set with the view, because the view is the machine's
    /// carts: a Game Boy shelf is laid out for portrait carts and an Advance shelf for
    /// landscape ones, and the row has to be told which it is looking at.
    ///
    /// The geometry comes with it. A caller that wants a row which is not a machine's says so
    /// afterwards, with `set_size`.
    pub fn set_system(&mut self, system: System) {
        self.system = system;
        self.size = size_for(system);
    }

    pub fn system(&self) -> System {
        self.system
    }

    /// Lay the row out for a size of its own, rather than the one the machine implies.
    ///
    /// The row's layout is a function of `CartSize` alone, so a shelf of something that is not a
    /// cartridge is a shelf with a different size and nothing else: the ring, the spring, the
    /// glide, the favourites view and the folder switch all carry over untouched.
    pub fn set_size(&mut self, size: CartSize) {
        self.size = size;
    }

    /// Whether the case's own slot is drawn under the row. See the `mouth` field for why the
    /// disc row turns it off. It is a separate call from `set_size` because it is a separate
    /// fact: a row can be laid out for something that is not a cartridge and still want the
    /// slot, and the two are decided by different things.
    pub fn set_mouth(&mut self, on: bool) {
        self.mouth = on;
    }

    /// The row's geometry: the cart's own size, the pitch between neighbours and how much
    /// bigger the selected one is.
    pub fn size(&self) -> CartSize {
        self.size
    }

    /// Face textures in `carts` order. The caller uploads them because only the compositor
    /// can mint a `TexId`.

    /// The blank cart a not-yet-built face stands in as. Set once at boot.
    ///
    /// Returns the one it replaced: the frame is rebuilt whenever the row's machine changes,
    /// because the stand-in is cut to that machine's cart, and a caller that cannot release
    /// the old texture leaks half a megabyte per shelf switch.
    pub fn set_placeholder(&mut self, face: TexId) -> Option<TexId> {
        self.placeholder.replace(face)
    }

    /// ★ 2026-10-21：选中卡带脚下的柔光池。`Option` 是因为它允许被收回去 ——
    /// 烘焙失败时货架照常画，只是脚下不亮。
    pub fn set_focus(&mut self, face: Option<TexId>) {
        self.focus = face;
    }

    /// ★ 2026-10-22（六六）：机种 logo 高光区，画在卡座背层（卡带之下）。
    ///
    /// `Option` 是因为素材没到位时货架照常画，只是没 logo。前端在上传 logo 时用
    /// `slot_chrome::fill_logo_highlight` 把洋红填成当前主题的 `bright` 色 —— 换主题时
    /// `rebake()` 重新上传，高光色跟着走。按机种各一张，`draw` 取 `self.system` 那一张。
    ///
    /// ⚠️ 数组长度是 `MACHINES`（9），不是"有素材的机种数"：下标就是 `machine_slot`，
    /// 而那是卡上设置的键。素材缺的机种留 `None`，不能靠缩短数组表达 —— 缩短过一次，
    /// 结果是加到第七个机种时 `set_logo` 直接越界。
    pub fn set_logo(&mut self, system: slot_store::System, tex: Option<(TexId, u32, u32)>) {
        self.logo[Self::machine_slot(system)] = tex;
    }

    /// Face textures in `carts` order. The caller uploads them because only the compositor
    /// can mint a `TexId`.
    pub fn set_faces(&mut self, faces: Vec<Option<TexId>>) {
        self.faces = faces;
    }

    /// The texture a cart is holding, if its face has been built and not since released.
    pub fn face_of(&self, i: usize) -> Option<TexId> {
        self.faces.get(i).copied().flatten()
    }

    /// Drops one face back to its placeholder. The caller releases the texture; this only
    /// forgets the handle, so the shelf stops pointing at something that is no longer there.
    pub fn clear_face(&mut self, i: usize) {
        if let Some(slot) = self.faces.get_mut(i) {
            *slot = None;
        }
    }

    /// One face, as it arrives. The list is grown to the cartridge count first, because the
    /// background filler answers out of order and an index can land before its turn.
    pub fn set_face(&mut self, i: usize, tex: Option<TexId>) {
        if self.faces.len() < self.carts.len() {
            self.faces.resize(self.carts.len(), None);
        }
        if let Some(slot) = self.faces.get_mut(i) {
            *slot = tex;
        }
    }

    /// The cart filed under `stem`, **looked for on this shelf's own machine first**.
    ///
    /// ★ 2026-11-09：名字不是唯一的键，而全库 `position` 只会答**排序靠前的那条**。
    ///   `Games/FC/魂斗罗.nes` 与 `Games/MD/魂斗罗.md` 的 stem 都是 `魂斗罗`，库里不去重
    ///   （`slot_store::scan::build` 逐条 push），`order` 又以路径兜底 —— 于是 `Games/FC/…`
    ///   恒排在 `Games/MD/…` 前面（`F` < `M`）。玩家实报的症状就是这个：「加载 MD 游戏时
    ///   显示的是 FC 卡带且进去也是 FC」—— MD 货架上按下 A 之后，插卡动画画的
    ///   （`App::chrome_with`）、核心跑的都是（`Session::spawn_core`）FC 那张卡。
    ///
    ///   所以先在同一机种的货架 `machines[]` 里找，找不到才退到全库。退到全库这一步要留着：
    ///   调用方拿到的 stem 未必属于当前货架的机种（换机种、退卡途中都不该落空）。
    ///
    ///   ⚠️ 只修**跨机种**串档。同一台机器里重号（两个子文件夹各放一份同名 rom）仍取靠前
    ///   那条 —— 那是卡自己的排法，六六 2026-11-09 拍板「同机种不串就行」。
    pub fn find(&self, stem: &str) -> Option<(&Cart, Option<TexId>)> {
        let i = self.find_index(stem)?;
        Some((&self.carts[i], self.faces.get(i).copied().flatten()))
    }

    /// 名字 → 库索引，**当前机种的货架优先**。见 [`Self::find`]。
    fn find_index(&self, stem: &str) -> Option<usize> {
        let own = &self.machines[Self::machine_slot(self.system)];
        if let Some(&i) = own.iter().find(|&&i| self.carts[i].stem == stem) {
            return Some(i);
        }
        self.carts.iter().position(|c| c.stem == stem)
    }

    /// The position of a cart in the current *view*, by stem — i.e. what `index` counts in.
    /// `None` when the view does not hold it, which for a system shelf means a cart belonging
    /// to another machine. `find` searches the whole library; this searches only what the row
    /// is actually showing, which is what a caller about to set `index` needs.
    pub fn index_of_stem(&self, stem: &str) -> Option<usize> {
        self.view.iter().position(|&i| self.carts[i].stem == stem)
    }

    /// The position in the view of the first cart filed under letter `slot`. The letter strip
    /// counts the view (see `retally_letters`), so a seat on a letter has to be found in the
    /// view too: a library index would be the wrong cart on a shelf that holds a subset of it.
    pub fn view_position_of_letter(&self, slot: usize) -> Option<usize> {
        self.view
            .iter()
            .position(|&i| crate::letters::slot_of(self.carts[i].initial) == slot)
    }

    /// Send the row to the cart the letter dial has just chosen, sweeping it across everything
    /// in between rather than flicking through only the last cart. Called after `index` has
    /// been set, since that is what the row is measured towards.
    ///
    /// The direction is not chosen here. The row is a ring, so the cart the dial arrived at has
    /// an image on either side, and the sweep takes the one nearest where the row already
    /// stands: stepping the dial backward runs the row backward. That is the whole of what
    /// keeps the dial and the row agreeing about which way is "down" — a seat that always
    /// played forward would have the two controls contradict each other on every backward step.
    ///
    /// A card with no more carts than the row has slots is left to the spring: there is nothing
    /// to glide across, and a sweep one cart wide would only be a slower version of the single
    /// step the row already draws cleanly.
    pub fn glide_to_target(&mut self) {
        let rows = SLOTS * 2 + 1;
        if (self.view.len() as i32) < rows {
            self.glide = None;
            self.vel = 0.0;
            return;
        }
        let from = self.scroll;
        let to = self.scroll_target();
        self.glide = Some(Glide {
            from,
            to,
            t: 0.0,
            dur: glide_seconds((to - from).abs()),
        });
        self.vel = 0.0;
    }

    /// Which way the ring is travelling while it glides, or `None` at rest. The smear and its
    /// trail hang off this: a still row must be drawn exactly as it always was.
    pub(crate) fn glide_dir(&self) -> Option<f32> {
        self.glide
            .map(|g| (g.to - g.from).signum())
            .filter(|d| *d != 0.0)
    }

    /// The cart the caret is on, as an index into `carts`. `None` only for an empty view.
    pub fn current(&self) -> Option<usize> {
        self.view.get(self.index).copied()
    }

    /// The cart itself, which is what most callers actually want.
    pub fn current_cart(&self) -> Option<&Cart> {
        self.carts.get(self.current()?)
    }

    /// Whether the view is narrower than the library — i.e. the favourites shelf is up.
    pub fn filtered(&self) -> bool {
        self.view.len() != self.carts.len()
    }

    /// **Read the library again, in place** — the shelf the settings screen's refresh row leaves
    /// behind (2026-10-17).
    ///
    /// Everything derived from the cart list is rebuilt from the new one: the three machine
    /// shelves, the folder lists, the two position tables. What is *not* touched is everything
    /// about how the row is drawn rather than what is on it — the system, the size and the mouth
    /// are the caller's decisions and the new list has no opinion about them, and a refresh must
    /// not silently reset the machine the user was browsing.
    ///
    /// The two things that *are* reset are the ones the new list can invalidate: `folder_pick`,
    /// because a folder that existed a moment ago may not any more and an index into a list that
    /// moved is a wrong cart rather than a missing one; and the ring itself, which is parked back
    /// on the front of the view (`keep` if it still names a cart, the first otherwise) — the
    /// caret cannot be trusted across an edit that may have removed the cart it was on.
    ///
    /// **The old faces are handed back rather than dropped.** Half a megabyte each, and only the
    /// caller has the compositor that can release them; a shelf that forgot them without saying so
    /// would leak the whole resident window on every refresh. The returned vector is exactly the
    /// ones that were live, in `carts` order, so the caller can walk it and release each.
    pub fn replace_library(&mut self, carts: Vec<Cart>, keep: Option<&str>) -> Vec<TexId> {
        let mut machines: [Vec<usize>; MACHINES] = Default::default();
        let mut pos_in_machine = vec![u32::MAX; carts.len()];
        for (i, c) in carts.iter().enumerate() {
            let list = &mut machines[Self::machine_slot(c.system())];
            pos_in_machine[i] = list.len() as u32;
            list.push(i);
        }
        let mut folders: [Vec<String>; MACHINES] = Default::default();
        for c in &carts {
            let slot = Self::machine_slot(c.system());
            if let Some(f) = folder_of(&c.rom, c.system().dir_name()) {
                if !folders[slot].contains(&f) {
                    folders[slot].push(f);
                }
            }
        }
        for f in folders.iter_mut() {
            f.sort();
        }

        let view: Vec<usize> = (0..carts.len()).collect();
        let mut pos_in_view = vec![u32::MAX; carts.len()];
        for (p, &i) in view.iter().enumerate() {
            pos_in_view[i] = p as u32;
        }

        // The live faces come off first, before anything is overwritten: after `self.carts`
        // moves, a `TexId` in the old table is a handle to a cart that is no longer there and
        // nothing would be able to line them up again.
        let released: Vec<TexId> = self.faces.iter().filter_map(|f| *f).collect();

        self.carts = carts;
        self.machines = machines;
        self.folders = folders;
        self.folder_pick = [None; MACHINES];
        self.pos_in_machine = pos_in_machine;
        self.pos_in_view = pos_in_view;
        self.view = view;
        self.faces = Vec::new();
        self.index = keep
            .and_then(|stem| self.view.iter().position(|&i| self.carts[i].stem == stem))
            .unwrap_or(0);
        self.scroll = self.index as f32;
        self.vel = 0.0;
        self.glide = None;
        self.held = None;
        self.motion = None;
        released
    }

    /// Swap the set of carts the ring walks. The frame — the placeholder — is the shelf's
    /// rather than the list's, so it is left alone; `faces` is left alone too, for the reason
    /// `view` is a list of indices. The caret is parked on `keep` when the new view
    /// still holds it, and on the first cart otherwise.
    pub fn set_view(&mut self, view: Vec<usize>, keep: Option<&str>) {
        for slot in self.pos_in_view.iter_mut() {
            *slot = u32::MAX;
        }
        for (p, &i) in view.iter().enumerate() {
            self.pos_in_view[i] = p as u32;
        }
        self.view = view;
        // ★★ 2026-10-27 23:1x（六六）：「按一比一像素就行了」。
        //
        // 盒子除了机种还要看**这一行卡自己的目录**：`Games/FC` 与 `Games/NES` 都是
        // `System::Fc`，可两张缺省图一个是 360×229、一个是 300×334 —— 同一个盒子必然把
        // 其中一张拉变形。货架本来就是按文件夹分的，所以一行里的卡通常同目录；只有这种情况
        // 才换盒子，混合行（收藏夹那一行跨机种）仍然走 `set_system` 给机种盒子。
        let mut dirs = self.view.iter().filter_map(|&i| self.carts[i].dir_name());
        if let Some(first) = dirs.next() {
            let same = dirs.all(|d| d.eq_ignore_ascii_case(first));
            if same {
                self.size = crate::cart::size_for_dir(Some(first), self.system);
            }
        }
        self.index = keep
            .and_then(|stem| self.view.iter().position(|&i| self.carts[i].stem == stem))
            .unwrap_or(0);
        self.scroll = self.index as f32;
        self.vel = 0.0;
        self.glide = None;
        self.held = None;
    }

    /// The carts the row is actually drawing, as indices into `carts`.
    ///
    /// This is the set a build has to cover, and it is **not** the same thing as a contiguous
    /// run of indices around the caret. A machine shelf and the favourites shelf are both
    /// *subsets* of the library — this card files its three Game Boy carts at 48..50 among nine
    /// hundred Advance ones — so a window that walks the library lands on nine hundred Advance
    /// carts and misses the three the row is showing. That is what "the side carts are still the
    /// placeholder" looked like on the device: the boot had built seven carts nobody was looking
    /// at.
    ///
    /// A cart's distance from the caret **in the row's own order**, or `None` when the view
    /// does not hold it.
    ///
    /// Every measurement of "worth keeping a face for" has to be this one and not a library
    /// distance. A machine shelf is a scattered subset of the library — this card's three Game
    /// Boy carts sit at 27, 127 and 637 among nine hundred Advance ones, because the library is
    /// sorted by title and the row is not — so the two carts either side of the caret are a
    /// hundred and three hundred indices away. A resident window measured in the library
    /// therefore releases exactly the faces the user is looking at, and the filler spends its
    /// time on carts that are on no shelf at all.
    pub fn view_distance(&self, cart: usize) -> Option<usize> {
        let n = self.view.len();
        if n == 0 {
            return None;
        }
        let pos = *self.pos_in_view.get(cart)?;
        if pos == u32::MAX {
            return None;
        }
        let d = (pos as usize).abs_diff(self.index);
        Some(d.min(n - d))
    }

    /// Which of the seven shelves a machine's carts live on. The order is the enum's
/// (`Gba, Gb, Gbc, Sfc, Fc, Neogeo, Arcade`), and it only grows: a shelf's index is
/// what `System/overlay.txt` and `System/display.txt` slot by, so reordering it would
/// silently repoint every card's saved settings at another machine.
    fn machine_slot(s: System) -> usize {
        match s {
            System::Gba => 0,
            System::Gb => 1,
            System::Gbc => 2,
            System::Sfc => 3,
            System::Fc => 4,
            System::Neogeo => 5,
            System::Arcade => 6,
            // ★ 2026-10-25：MD / PS1 追加在末尾 —— 与前七行同一个理由，一个货架的编号
            //   是卡上 `overlay.txt` / `display.txt` / `shelf.txt` 的键，插队会把每一张
            //   已经发出去的卡上的设置静默指到别的机种。
            System::Md => 7,
            System::Ps1 => 8,
            // ★ 2026-10-25：PCE 追加在末尾 —— 与上面同一个理由，货架编号是卡上
            //   `overlay.txt` / `display.txt` / `shelf.txt` 的键，插队会把每一张已经发
            //   出去的卡上的设置静默指到别的机种。
            System::Pce => 9,
        }
    }

    /// One machine's shelf: its carts as library indices, in the order the row shows them, cut to
    /// the subfolder `folder_pick` has chosen for that machine (`None` leaves all of them).
    pub fn machine_view(&self, s: System) -> Vec<usize> {
        let slot = Self::machine_slot(s);
        let all = &self.machines[slot];
        let Some(name) = self.folder_pick[slot].and_then(|p| self.folders[slot].get(p)) else {
            // No subfolder picked: the machine's own shelf, with the games filed inside
            // subfolders kept out of sight. Tapping Y walks into each subfolder in turn and
            // comes back here, so this is the "machine folder" state — top-level only
            // (2026-10-10, 六六).
            return all
                .iter()
                .copied()
                .filter(|&i| folder_of(&self.carts[i].rom, s.dir_name()).is_none())
                .collect();
        };
        all.iter()
            .copied()
            .filter(|&i| {
                folder_of(&self.carts[i].rom, s.dir_name()).as_deref() == Some(name.as_str())
            })
            .collect()
    }

    /// Step one machine's subfolder filter: all -> the first folder -> ... -> all. `false` when
    /// the machine has no subfolders, so the caller can refuse the press rather than let it read
    /// as a key that worked.
    /// One machine's shelf, but **never empty when that machine has subfolders**.
    ///
    /// ★ 2026-10-24（真机上抓到的）：卡上 15 盘 GBA 全在 `Games/GBA/GBA中文游戏甄选/` 里，
    /// 而「机种文件夹只显示顶层」这条规则让 `machine_view` 返回空 —— 开机是一片空白货架，
    /// 连为什么空都看不出来（字母条还在，说明有 15 盘游戏）。
    /// ⇒ 顶层为空而机种有子文件夹时，落到**第一个子文件夹**；那正是按一下 Y 会到的地方，
    /// 所以观感上没有多一层，只是不让空屏出现。
    pub fn machine_view_or_first_folder(&mut self, s: System) -> Vec<usize> {
        let view = self.machine_view(s);
        if !view.is_empty() {
            return view;
        }
        let slot = Self::machine_slot(s);
        if self.folders[slot].is_empty() {
            return view;
        }
        self.folder_pick[slot] = Some(0);
        self.machine_view(s)
    }

    pub fn cycle_folder(&mut self, s: System) -> bool {
        let slot = Self::machine_slot(s);
        let n = self.folders[slot].len();
        if n == 0 {
            return false;
        }
        self.folder_pick[slot] = match self.folder_pick[slot] {
            None => Some(0),
            Some(p) if p + 1 < n => Some(p + 1),
            Some(_) => None,
        };
        true
    }

    /// The subfolder one machine's shelf is showing, or `None` when it is showing them all.
    pub fn folder_name(&self, s: System) -> Option<&str> {
        let slot = Self::machine_slot(s);
        self.folders[slot]
            .get(self.folder_pick[slot]?)
            .map(String::as_str)
    }

    /// How far a cart is from the caret that would be showing it.
    ///
    /// Two roads to the same cart, and the nearer one wins. If the view up holds it, that is the
    /// distance that matters — the row on screen, favourites included. Otherwise it is counted
    /// from the *front* of its own machine's shelf, because that is where a switch to that
    /// machine parks the caret. Keeping both windows resident is what makes a machine switch
    /// show real carts on the frame it lands rather than a row of stand-ins: by the time the
    /// user reaches for the machine, its first screen is already built.
    ///
    /// O(1), deliberately: the filler asks this of every cart it could build, every frame.
    pub fn shelf_distance(&self, cart: usize) -> Option<usize> {
        let here = self.view_distance(cart);
        let front = self
            .pos_in_machine
            .get(cart)
            .copied()
            .filter(|&p| p != u32::MAX)
            .map(|p| p as usize);
        match (here, front) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Same walk as `slot_rects`, which is what draws them: the slots either side of the caret
    /// in view order, and a cart's representative nearest the middle when the view is too short
    /// to fill the row.
    pub fn draw_window(&self) -> Vec<usize> {
        let n = self.view.len() as i64;
        if n == 0 {
            return Vec::new();
        }
        let base = self.scroll.round() as i64;
        let mut out = Vec::new();
        for slot in -SLOTS..=SLOTS {
            let off = slot as i64;
            let r = off.rem_euclid(n);
            let nearest = if r * 2 > n { r - n } else { r };
            if nearest != off {
                continue;
            }
            let i = self.view[(base + off).rem_euclid(n) as usize];
            if !out.contains(&i) {
                out.push(i);
            }
        }
        out
    }

    /// The carts the current view walks, in view order. A view is a list of indices, so this
    /// is how a caller that needs the carts themselves — the letter tally — reads one.
    pub fn visible(&self) -> Vec<&Cart> {
        self.view.iter().map(|&i| &self.carts[i]).collect()
    }

    /// How many carts the current view holds. The library count and this differ on every
    /// shelf that is a subset of it, which is the distinction the boot's face window has to
    /// respect.
    pub fn view_len(&self) -> usize {
        self.view.len()
    }

    pub fn left(&mut self) {
        self.step(-1);
    }

    pub fn right(&mut self) {
        self.step(1);
    }

    pub fn hold_left(&mut self, now: Millis) {
        self.hold(-1, now);
    }

    pub fn hold_right(&mut self, now: Millis) {
        self.hold(1, now);
    }

    /// The press moves a cart itself, so the repeat is what the delay is measured from
    /// rather than what it produces.
    fn hold(&mut self, by: i32, now: Millis) {
        self.step(by);
        self.held = Some((by, now + REPEAT_DELAY_MS));
    }

    pub fn release_left(&mut self) {
        self.release(-1);
    }

    pub fn release_right(&mut self) {
        self.release(1);
    }

    /// Only the direction that is being held stops it. Letting go of the other one is a
    /// change of direction the shelf has already acted on.
    fn release(&mut self, by: i32) {
        if matches!(self.held, Some((held, _)) if held == by) {
            self.held = None;
        }
    }

    /// Whatever is held, let go of. Nothing on screen is holding it.
    pub fn release_hold(&mut self) {
        self.held = None;
    }

    /// Fires the repeat. Due from `now` rather than from the deadline it passed, so a frame
    /// the app was late for costs one cart instead of a burst of catching up.
    pub fn tick(&mut self, now: Millis) {
        let Some((by, due)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(by);
        self.held = Some((by, now + REPEAT_MS));
    }

    fn step(&mut self, by: i32) {
        let n = self.view.len();
        if n == 0 {
            return;
        }
        // An arrow is a direction of its own, and it has to answer on the press: a jump still
        // gliding is dropped so the spring takes the row from wherever the glide left it.
        self.glide = None;
        self.index = (self.index as i32 + by).rem_euclid(n as i32) as usize;
    }

    /// Where the spring is heading, in the continuous coordinate `scroll` lives in. The row
    /// is a ring, so the selected cart has an image every `n` slots; this is the one nearest
    /// where the row already is, which is what stops a wrap unwinding the whole row.
    pub fn scroll_target(&self) -> f32 {
        let n = self.view.len();
        if n == 0 {
            return 0.0;
        }
        let n = n as f32;
        self.scroll + (self.index as f32 - self.scroll + n / 2.0).rem_euclid(n) - n / 2.0
    }

    /// The cart `off` slots right of the selection. `None` when the row is empty, or when
    /// this slot would repeat a cart another slot is already showing: with two carts the
    /// left and right neighbours are the same one, and a row holding it twice reads as a
    /// bug. The row is left with a gap instead.
    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.view.len() as i32;
        if n == 0 {
            return None;
        }
        let r = off.rem_euclid(n);
        let nearest = if r * 2 > n { r - n } else { r };
        (nearest == off).then(|| self.view[(self.index as i32 + off).rem_euclid(n) as usize])
    }

    pub fn update(&mut self, dt: f32) {
        // A letter jump owns the row while it runs. Its sweep is the movement, so the spring
        // must not also be pulling at `scroll`, or the two would argue over the same cart.
        if let Some(mut g) = self.glide {
            g.t += dt;
            let p = (g.t / g.dur).clamp(0.0, 1.0);
            // Smoothstep: ease in, ease out, so the row accelerates away and settles in.
            let e = p * p * (3.0 - 2.0 * p);
            self.scroll = g.from + (g.to - g.from) * e;
            self.vel = 0.0;
            self.glide = if p >= 1.0 {
                self.scroll = g.to;
                None
            } else {
                Some(g)
            };
            return;
        }
        let accel = -2.0 * OMEGA * self.vel - OMEGA * OMEGA * (self.scroll - self.scroll_target());
        self.vel += accel * dt;
        self.scroll += self.vel * dt;
    }


/// The shelf screen: the row of carts and the slot under it. What is printed on the case
/// is drawn after this, by whoever holds the type.
    ///
    /// ★ 2026-10-22（六六：机种框是顶层、预留插口弧度）：底条拆成两半夹住卡带 ——
    ///   **内侧洞（`draw_slot_inside`：开口/凹腔/缝/拇指弧）先画，压在卡带之下**，卡带插进去
    ///   时遮住它、露出插口弧度；**外壳（`draw_slot_shell`：裁掉卡带下缘的塑料）后画，压在卡带
    ///   之上**。顶条/系统显示由调用方（`app.rs`）在这之后画，整体落在卡带之上。这样堆叠自下而上
    ///   是：壁纸 → 卡槽内侧 → 卡带 → 机种框（顶条+底条外壳）→ 系统显示，不再是整块 `draw_empty_slot`
    ///   把暗洞也压在卡带之上（那让卡带像在洞前面漂浮、插口弧度被卡带遮住）。
    pub fn draw(&self, shake: f32, out: &mut Vec<Draw>) {
        self.draw_idle(shake, Idle::NONE, out)
    }

    /// 同 [`Shelf::draw`]，外加静置时的待机动效。
    pub fn draw_idle(&self, shake: f32, idle: Idle, out: &mut Vec<Draw>) {
        if self.mouth {
            draw_slot_inside(out);
            // ★ 2026-10-22（六六）：机种 logo 在卡座背层（卡带之下）。空槽时可见，卡带一插就盖住。
            if let Some(logo) = self.logo[Self::machine_slot(self.system)] {
                crate::slot_chrome::draw_slot_logo(logo, out);
            }
        }
        self.draw_row_idle(None, shake, 0.0, 1.0, idle, out);
        if self.mouth {
            draw_slot_shell(out);
        }
    }

    /// The row alone. The cart on its way into the slot is drawn by the chrome, at the same
    /// place the row would draw it; leaving it in the row as well puts two of one cart on
    /// screen and the travel then reads as a copy sliding away from the original.
    ///
    /// `shake` displaces the carts and nothing else. On the shelf the frame is mostly
    /// backdrop, so shaking that slides the letterbox in at the edges rather than reading as
    /// a refusal.
    ///
    /// `recede` clears the row for the cart going into the slot: 0.0 leaves it alone, 1.0
    /// has every other cart gone. They part outwards rather than fading in place, so the row
    /// reads as making way for the one that was chosen.
    ///
    /// `dim` darkens the faces further and nothing else: 1.0 leaves them as `recede` has them.
    ///
    /// ★ 2026-10-25：**侧位卡带脚下那块黑影已经不画了**（见 `draw_row_idle` 里那段说明）。
    /// 这里跟着改，是因为旧文档还在描述一个不存在的图层。
    pub fn draw_row(
        &self,
        hidden: Option<&str>,
        shake: f32,
        recede: f32,
        dim: f32,
        out: &mut Vec<Draw>,
    ) {
        self.draw_row_idle(hidden, shake, recede, dim, Idle::NONE, out)
    }

    /// 同 [`Shelf::draw_row`]，外加中间那张卡的待机动效（见 [`Idle`]）。
    pub fn draw_row_idle(
        &self,
        hidden: Option<&str>,
        shake: f32,
        recede: f32,
        dim: f32,
        idle: Idle,
        out: &mut Vec<Draw>,
    ) {
        let dim = dim.clamp(0.0, 1.0);
        for r in self.slot_rects(hidden, shake, recede) {
            let cart = &self.carts[r.cart];
            // ★ 2026-10-17 曾经在这里画过一块"卡带剪影涂黑"，垫在变暗的侧位卡下面，
            //   用来对付 `SIDE_ALPHA = 0.7` 把整张面乘成 70% 透、在壁纸上读作鬼影的问题。
            //   **2026-10-25 整条删掉了**，理由见下面 `Draw::Tex` 之前那一段
            //   （根子是：它的形状只能从贴图 alpha 推，而贴图铺满画布 ⇒ 推出来是一块方角矩形）。
            //
            // ★ 2026-10-21：手里那张卡**脚下**的一池柔光（`light::pool`）。
            //
            //   第一版在这里画的是一圈 1px 琥珀发丝边，六六否了，原话：
            //   「卡带贴图不是死板的矩形，它是有轮廓的，好不容易做的透明底被你一框又是一个矩形。」
            //   完全正确 —— 卡带的贴图是按真实形状裁的透明 PNG，一个矩形框等于把已经做出来的
            //   轮廓重新宣布成矩形。所以改成光池：画在卡带脚下、比卡带宽一截、比卡带矮得多，
            //   **一根线都不压卡带自己的边**。
            //
            //   条件仍是 `dim > 0.99 && r.alpha > 0.9`：换核心、插卡这些时候焦点已经不在
            //   这张卡上，留一池亮光会让人以为它还能按。**侧位一个都不画** —— 都亮就等于都不亮。
            if r.selected && dim > 0.99 && r.alpha > 0.9 {
                if let Some(tex) = self.focus {
                    let pw = r.w * 1.25;
                    let ph = 20.0;
                    out.push(Draw::Tex {
                        x: r.x - (pw - r.w) / 2.0,
                        // 压在落脚线上：光是从货架面往上映的，不是从卡带往下打的。
                        y: r.y + r.h - ph / 2.0,
                        w: pw,
                        h: ph,
                        tex,
                        // ★ 2026-10-25（六六选定 sway + 柔光）：这池光跟着呼吸。
                        //   `Idle::NONE` 的 `glow` 是 1.0 ⇒ 其它调用点（插卡、选核心、
                        //   静态出图）一个字节都不变。
                        alpha: crate::light::gain() * idle.glow,
                    });
                }
            }
            // ★★ 2026-10-25 01:1x（六六：「GBC 的阴影还没拿掉，还是兜底的那个代码画的 svg」）——
            //   **侧位卡带脚下那块黑影，整条链路删掉了。**
            //
            //   它原来的职责是"别让半透明的侧位卡在壁纸上看成鬼影"。但卡带早就是一张带 alpha
            //   的贴图，而这影子的形状只能从**同一个 alpha** 推出来 —— 贴图本身几乎铺满画布
            //   （`cart_gbc.png` 只有 2.6% 是透明的），于是推出来的"轮廓"就是**一块方角矩形**，
            //   比卡带的圆角大一圈，在边沿露出一道直边。那不是影子，是兜底形状。
            //
            //   六六 2026-10-24 就说过「目前已经是卡带贴图了也不怕透光了」—— 这句话现在确实成立：
            //   `SIDE_ALPHA` 只是把整张面压暗，贴图本身不透明，透出来的是它自己的暗面而不是壁纸。
            //   所以影子没有存在的必要，`cart_shadow` / `set_shadow` 一并删除。
            out.push(match self.faces.get(r.cart).copied().flatten() {
                Some(tex) => {
                    // ★★ 2026-10-25（六六：「摆动看起来还是有点奇怪，能朝着玩家晃吗（就是轴
                    //    是从卡带纵向穿过的，卡带左右两侧交替往玩家方向摆）」）——
                    //    静置时**中间那张卡绕着穿过自己的竖直轴转**：`Draw::Yaw`，画出来是
                    //    梯形，近的一侧更高更宽。判据与脚下柔光池同一道（选中 + 不在让位 +
                    //    不透明），所以两者永远一起出现、一起消失，不会"光在呼吸而卡是死的"。
                    //    ⚠️ 这不是屏幕平面内的旋转（`Draw::Turned`）—— 那个看起来像被拧了一下。
                    let yaw = if r.selected && dim > 0.99 && r.alpha > 0.9 {
                        idle.yaw
                    } else {
                        0.0
                    };
                    if yaw == 0.0 {
                        Draw::Tex {
                            x: r.x,
                            y: r.y,
                            w: r.w,
                            h: r.h,
                            tex,
                            alpha: r.alpha * dim,
                        }
                    } else {
                        Draw::Yaw {
                            x: r.x,
                            y: r.y,
                            w: r.w,
                            h: r.h,
                            tex,
                            alpha: r.alpha * dim,
                            yaw,
                        }
                    }
                }
                // A cart whose face has not been uploaded still holds its place, and as a cart.
                // A jump across the alphabet crosses hundreds of carts and hundreds of faces
                // cannot be held at once — but every cart can still carry *its own* colour, so
                // the row reads as a ribbon of labels going past rather than as a row of empty
                // cases. The shell goes down, then one solid quad in the panel `label_panel`
                // defines, in the colour this cart's own title hashes to: no texture is minted
                // and nothing is rasterised, which is the whole reason it can be done at all.
                None => match self.placeholder {
                    Some(tex) => {
                        let c = label_colour(&label_text(cart));
                        let (x0, y0, x1, y1) = self.size().label_panel_at(r.w as u32, r.h as u32);
                        let ink = [
                            c[0] as f32 / 255.0,
                            c[1] as f32 / 255.0,
                            c[2] as f32 / 255.0,
                            r.alpha * dim,
                        ];
                        // While the ring glides, a cart with no face of its own is smeared along
                        // the travel: one fainter copy behind it and both stretched a little. A
                        // jump crosses hundreds of carts in well under a second, and a plain cart
                        // sliding past at that speed reads as a slideshow; the smear is what tells
                        // the eye it is moving fast. Only the placeholder carts get it — a cart
                        // wearing a real face is the game's own picture, and smearing that would be
                        // a lie about the art — and at rest the stretch is exactly 1.
                        let (dir, grow) = match self.glide_dir() {
                            Some(dir) => (dir, r.w * (SMEAR - 1.0)),
                            None => (0.0, 0.0),
                        };
                        if grow > 0.0 {
                            let alpha = r.alpha * dim * 0.30;
                            let tx = r.x - dir * grow * 2.0;
                            out.push(Draw::Tex {
                                x: tx - grow / 2.0,
                                y: r.y,
                                w: r.w + grow,
                                h: r.h,
                                tex,
                                alpha,
                            });
                            out.push(Draw::Rect {
                                x: tx + x0 as f32 - grow / 2.0,
                                y: r.y + y0 as f32,
                                w: (x1 - x0) as f32 + grow,
                                h: (y1 - y0) as f32,
                                colour: [ink[0], ink[1], ink[2], alpha],
                            });
                        }
                        out.push(Draw::Tex {
                            x: r.x - grow / 2.0,
                            y: r.y,
                            w: r.w + grow,
                            h: r.h,
                            tex,
                            alpha: r.alpha * dim,
                        });
                        out.push(Draw::Rect {
                            x: r.x - grow / 2.0 + x0 as f32,
                            y: r.y + y0 as f32,
                            w: (x1 - x0) as f32 + grow,
                            h: (y1 - y0) as f32,
                            colour: ink,
                        });
                        continue;
                    }
                    None => {
                        let c = label_colour(&label_text(cart));
                        Draw::Rect {
                            x: r.x,
                            y: r.y,
                            w: r.w,
                            h: r.h,
                            colour: [
                                c[0] as f32 / 255.0,
                                c[1] as f32 / 255.0,
                                c[2] as f32 / 255.0,
                                r.alpha * dim,
                            ],
                        }
                    }
                },
            });
        }
    }

    /// Where each cart the row would draw sits this frame, in draw order and in offscreen
    /// pixels. The one piece of layout the row and a mark drawn over it — the favourites star —
    /// must agree about, so it is stated once and read twice rather than written twice.
    ///
    /// `hidden` drops one cart by stem, which is how the shelf gets out of the way of the copy
    /// the chrome is sliding into the slot.
    pub fn slot_rects(&self, hidden: Option<&str>, shake: f32, recede: f32) -> Vec<SlotRect> {
        let recede = recede.clamp(0.0, 1.0);
        let n = self.view.len() as i64;
        if n == 0 {
            return Vec::new();
        }
        // The row is laid out around where it *is*, not around the cart it is heading for.
        // While a letter jump glides, `scroll` sweeps across everything between two letters
        // and each of those carts has to be drawn where `scroll` puts it this frame; anchoring
        // on the destination would leave the row blank for the length of the sweep. At rest
        // the two are the same place, so a still row is drawn exactly as it always was.
        let base = self.scroll.round() as i64;
        let mut out = Vec::new();
        for slot in -SLOTS..=SLOTS {
            let off = slot as i64;
            let r = off.rem_euclid(n);
            // Only a cart's representative nearest the middle is drawn, so a row too short to
            // fill its slots does not show the same cart on both sides of the selection.
            let nearest = if r * 2 > n { r - n } else { r };
            if nearest != off {
                continue;
            }
            let coord = base + off;
            let i = self.view[coord.rem_euclid(n) as usize];
            let cart = &self.carts[i];
            if hidden == Some(cart.stem.as_str()) {
                continue;
            }
            let offset = coord as f32 - self.scroll;
            let t = offset.abs().min(1.0);
            let size = self.size();
            let scale = size.middle + (size.side - size.middle) * t;
            let mut alpha = (1.0 + (SIDE_ALPHA - 1.0) * t) * (1.0 - recede);
            let (w, h) = (size.w as f32 * scale, size.h as f32 * scale);
            // Away from the middle, and further the further out it already was, so the row
            // opens rather than sliding sideways.
            let away = offset.signum() * (1.0 + offset.abs());
            let x = OUT_W as f32 / 2.0 + offset * size.pitch - w / 2.0 + away * PART * recede;
            let mut y = foot_y(size) - h;
            // The system-shelf swap, when one is running. The centre cart leads the beat and
            // each cart further out follows it — drawn up and away on the way out, fallen back
            // from above on the way in — so the row moves in order rather than all at once.
            if let Some(m) = self.motion {
                let lead = SWAP_STAGGER * (slot as f32).abs();
                let span = (1.0 - SWAP_STAGGER * SLOTS as f32).max(0.05);
                let f = ((m.p - lead) / span).clamp(0.0, 1.0);
                let e = f * f * (3.0 - 2.0 * f);
                if m.ascend {
                    y -= e * SWAP_RISE;
                    alpha *= 1.0 - e;
                } else {
                    y -= (1.0 - e) * SWAP_RISE;
                    alpha *= e;
                }
            }
            if x + w <= 0.0 || x >= OUT_W as f32 || alpha <= 0.0 {
                continue;
            }
            out.push(SlotRect {
                cart: i,
                x: x + shake,
                y,
                w,
                h,
                alpha,
                selected: slot == 0,
            });
        }
        out
    }
}

/// The directory the scanner reads games out of, spelled as the scanner spells it. Kept here only
/// so `folder_of` can tell a subfolder from the root the subfolders live under.
const GAMES_DIR: &str = "Games";

/// The subfolder a cart is filed under, for the shelf's per-machine folder switch.
///
/// A card may file its games however it likes under `Games/`: loose (`Games/x.gba`), one level of
/// machine folder (`Games/GBA/x.gba`), a folder inside that (`Games/GBA/GBA中文/x.gba`), or with no
/// machine folder at all (`Games/GBA中文/x.gba`) — the machine is read off the *extension*, so all
/// four layouts are the same card to everything else. The subfolder is therefore whatever sits
/// between the machine (when there is one) and the file: the name directly above the rom, and
/// `None` when that name is the machine folder or the `Games` root itself. Both of those are
/// otherwise ordinary directory names, which is exactly why the root is found by name rather than
/// by counting — a loose rom's parent is `Games`, and mistaking that for a folder would offer a
/// "subfolder" on every flat card and switch to a set of carts identical to the one already up.
fn folder_of(rom: &Path, machine: &str) -> Option<String> {
    let parts: Vec<&str> = rom
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    let root = parts.iter().rposition(|p| *p == GAMES_DIR)?;
    let mut rest = &parts[root + 1..];
    // The machine folder is stripped whether it is spelled the canonical way or one of the
    // aliases `System::from_dir` also takes (`NES` for `FC`, `SNES` for `SFC`). Matching the
    // canonical name alone would file every rom under a folder named after its own machine,
    // which hides the whole shelf behind a subfolder switch that was never picked.
    if rest
        .first()
        .is_some_and(|d| slot_store::System::from_dir(d).map(System::dir_name) == Some(machine))
    {
        rest = &rest[1..];
    }
    // Whatever is left in front of the file is the subfolder, if any is left at all.
    (rest.len() >= 2).then(|| rest[0].to_string())
}

