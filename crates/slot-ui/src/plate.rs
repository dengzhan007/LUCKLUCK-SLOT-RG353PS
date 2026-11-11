use crate::draw::{Draw, TexId};
use crate::text;

/// A hint is a key cap and what that key does, sized to what it says: four of them share the
/// bottom plate with the dots, and a fixed width wide enough for the longest would leave the
/// short ones swimming.
pub const HINT_H: u32 = 24;
/// The smallest a cap gets, and the size of every single letter one, so B and X sit
/// identically on theirs.
pub const CAP: u32 = 20;
/// Blank either side of a key that needs more room than the square.
const CAP_PAD: u32 = 4;
/// A cap is never wider than this. Only the layout uses it; nothing names a key this long.
const CAP_MAX_W: f32 = 64.0;
/// Tight. Any more and the cap and its word read as two separate things rather than one
/// label.
/// Between one hint and the next. Lives here rather than with either screen, so the shelf
/// and the switcher cannot drift apart.
pub const HINT_GAP: f32 = 14.0;

/// Between a key cap and the word it belongs to. Small, but it has to stay clearly smaller
/// than the gap between one hint and the next, or the row reads as an alternating run of
/// caps and words rather than as pairs. `grouping_reads_as_pairs` holds that ratio.
pub const CAP_GAP: u32 = 5;
/// Blank column past the type, so a label that filled its band cannot touch the next hint.
const EDGE: u32 = 2;
/// The transparent strip every hint face carries after its label, so type never touches the
/// face's last column. Whoever lines a hint up against something should not count it.
pub const HINT_EDGE: u32 = EDGE;
/// The longest a label may rasterise to before the fitter shrinks it. The undo is the only
/// one that comes from outside this file.
const LABEL_MAX_W: f32 = 140.0;

pub const TITLE_W: u32 = 360;
pub const TITLE_H: u32 = 24;

/// A cap is a filled square with its key cut out of it, which is what a key looks like and the
/// only thing separating the key from its label at this size. The fill is the ink and the cut
/// is the ink's opposite, in whichever mode — see `palette::panel_ink`.
fn cap_ink() -> [u8; 3] {
    crate::palette::panel_ink()
}

fn ink() -> [u8; 3] {
    crate::palette::ink()
}

/// 顶栏上那行游戏名的墨色 —— **不是 `ink()`**，见 `palette::plate_ink`（2026-10-24：
/// 亮色模式下部分 look 的游戏名压在顶栏灰底上只有 1.6–2.8:1）。
fn plate_ink() -> [u8; 3] {
    crate::palette::plate_ink()
}

const KEY_PX: f32 = 14.0;
const LABEL_PX: f32 = 16.0;
const LABEL_MIN_PX: f32 = 10.0;
/// The status row's own band height: the clock and the battery's percent share it.
///
/// The two readings are set at `CLOCK_PX`/`PCT_PX`, sized so the face's own line box fits inside
/// this band. `text::coverage` places the baseline from the box's height and the face's own line
/// metrics, and then its bounds check simply *drops* any ink past the edge — so a font too large
/// for the band loses the bottom of its digits rather than shrinking to fit. `text::fit` has an
/// opinion about width only, and none at all about height.
///
/// Only the **box** is this tall. Where the ink lands inside it is decided by the face's own line
/// metrics (`text::coverage`).
pub const STATUS_H: u32 = 20;

/// The clock's size. Set to sit inside the `STATUS_H` band: twenty-four keeps the baseline just
/// inside the box's bottom, where thirty pushed it past and clipped the digits' lower edge.
const CLOCK_PX: f32 = 24.0;
const CLOCK_MIN_PX: f32 = 12.0;

/// The battery's percent, set at the clock's own size: they are the two readings on the status
/// row and were asked to grow together. Its own pair rather than a share of `LABEL_PX`, which
/// belongs to hint labels and must stay where the legends were measured.
const PCT_PX: f32 = 24.0;
const PCT_MIN_PX: f32 = 12.0;
const TITLE_PX: f32 = 20.0;
const TITLE_MIN_PX: f32 = 12.0;

/// The shelf's own line of type: which cart is under the eye, set below the row. Its own size
/// rather than the top plate's, because it is the only thing on that screen naming the
/// selection — the cart carries a printed label, but that label is a 240 px cart's, and a
/// second reading at the size of a caption on the case would not be a second reading at all.
///
/// One line, never two: a title that wraps moves its own bottom line every time the row
/// moves, and the row is the thing being read.
///
/// The box grew with the type when the type was asked to. `H` is the box the string is
/// rasterised into and `PX` is what it is set at, so a size that outgrew its box would be a
/// clipped title rather than a failing test: the fitter shrinks for *width* and has no opinion
/// about height at all. Forty is thirty plus five of air either side, which is what a CJK glyph
/// at this size needs to keep its top and bottom strokes.
/// 名称栏（铭牌）自己的几何：360 宽、54 深，挂在屏幕顶边正中。
///
/// ★★ 2026-10-29（640 树）：**400 宽不变，只换居中位置。**
/// 400 = 左 40（收藏）/ 中 320（名称）/ 右 40（电量），是六六 2026-10-24 给的切法，
/// 由内容决定而非屏幕决定；720 上 `(720−400)/2 = 160`，本树 `(640−400)/2 = 120`。
/// ⚠️ `desktop.rs` 画桌面图时有一份自己的 `NP_X0/NP_X1`（同一块板的两处），改这里必须同步。
pub const PLATE_X0: f32 = 120.0;
pub const PLATE_X1: f32 = 520.0;
pub const PLATE_H: f32 = 54.0;
/// ★ 2026-10-24（六六）：「顶部名称栏有 360px，名称限定宽度为 300px；收藏符号移至名称栏
///   左边 30px 区域，按 24px 居中；电量改成竖放的电池图标，移至名称栏右边 30px 区域，
///   按 24px 居中」。30 + 300 + 30 = 360，正好把栏内切成三段。
pub const PLATE_MARK: f32 = 24.0;
/// ★ 2026-10-24（六六）：顶栏 400 宽 = 左 40 信息 / 中 320 名称 / 右 40 信息。
pub const PLATE_MARK_ZONE: f32 = 40.0;
/// 两条分割线：信息区与名称区之间，各 1px。画在栏内、上下留边。
pub fn plate_divider_x() -> [f32; 2] {
    [
        PLATE_X0 + PLATE_MARK_ZONE,
        PLATE_X1 - PLATE_MARK_ZONE,
    ]
}
/// 左段（收藏）与右段（电量）里 24px 角标的左上角。
pub fn plate_fav_x() -> f32 {
    PLATE_X0 + (PLATE_MARK_ZONE - PLATE_MARK) / 2.0
}
pub fn plate_battery_x() -> f32 {
    PLATE_X1 - PLATE_MARK_ZONE + (PLATE_MARK_ZONE - PLATE_MARK) / 2.0
}
/// 分割线的矩形 `(x, y, w, h)`：1px 宽，顶栏里上下各留 10px。
pub fn plate_divider_rect(i: usize) -> (f32, f32, f32, f32) {
    (plate_divider_x()[i], 10.0, 1.0, PLATE_H - 20.0)
}

/// 两枚角标同一行：竖直居中在名称栏里。
pub fn plate_mark_y() -> f32 {
    PLATE_H / 2.0 - PLATE_MARK / 2.0
}

/// ★ 2026-10-24（六六）：名称显示区 320px 宽（400 − 两侧各 40），字号 26。
pub const SHELF_TITLE_W: u32 = 320;
pub const SHELF_TITLE_H: u32 = 40;
const SHELF_TITLE_PX: f32 = 26.0;

pub struct UndoFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// A key cap and what it does. Every screen names its own buttons; the binary rasterises what
/// it is told and hands back a texture.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hint {
    pub key: &'static str,
    pub label: String,
}

/// A legend as hints, in the order it will be drawn.
pub fn hint_row(legend: &[(&'static str, &'static str)]) -> Vec<Hint> {
    legend
        .iter()
        .map(|(key, label)| Hint {
            key,
            label: label.to_string(),
        })
        .collect()
}

/// How wide the cap has to be for the key on it. Square for a single letter; wider for one
/// that names two buttons, since shrinking `L / R` into the square would make the shelf's only
/// legend the smallest type on screen.
pub fn cap_width(key: &str) -> u32 {
    let Some(font) = text::label_font() else {
        return CAP;
    };
    let layout = text::fit(font, key, CAP_MAX_W, 1, KEY_PX, KEY_PX);
    let ink = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    CAP.max(ink.ceil() as u32 + 2 * CAP_PAD)
}

/// What `hint_face` will rasterise to. The screens lay their legends out from these, so the
/// two have to agree or every hint after the first sits beside someone else's type.
pub fn hint_width(key: &str, label: &str) -> u32 {
    cap_width(key) + CAP_GAP + band_width(label) + EDGE
}

/// Transparent apart from the cap and the type: both plates are translucent over a
/// screenshot, and a filled face would be a solid block sitting on one.
pub fn hint_face(key: &str, label: &str) -> UndoFace {
    let w = hint_width(key, label);
    let mut rgba = vec![0u8; (w * HINT_H * 4) as usize];

    let cap_w = cap_width(key);
    let mut cap = Vec::with_capacity((cap_w * CAP * 4) as usize);
    for _ in 0..cap_w * CAP {
        cap.extend_from_slice(&[ink()[0], ink()[1], ink()[2], 255]);
    }
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, key, cap_w as f32, 1, KEY_PX, KEY_PX);
        text::draw_centred(&mut cap, cap_w, CAP, &layout, cap_ink());
    }
    blit(&mut rgba, w, &cap, cap_w, CAP, 0, (HINT_H - CAP) / 2);

    let text_w = band_width(label);
    let mut band = vec![0u8; (text_w * HINT_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, label, text_w as f32, 1, LABEL_PX, LABEL_MIN_PX);
        text::draw_centred(&mut band, text_w, HINT_H, &layout, ink());
    }
    blit(&mut rgba, w, &band, text_w, HINT_H, cap_w + CAP_GAP, 0);

    UndoFace { rgba, w, h: HINT_H }
}

/// The same type as a hint's label, with no key cap in front of it. What the shelf prints on
/// the case: the wordmark and the time, in the font the buttons are labelled in.
pub fn word_width(text: &str) -> u32 {
    band_width_at(text, PCT_PX, PCT_MIN_PX)
}

/// The battery's percent. Named `word_*` for history and now the only caller; it is set at
/// `PCT_PX` in a `STATUS_H` band, so it and the clock are the same size in the same box.
pub fn word_face(text: &str) -> UndoFace {
    let w = word_width(text);
    let mut rgba = vec![0u8; (w * STATUS_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, w as f32, 1, PCT_PX, PCT_MIN_PX);
        text::draw_centred(&mut rgba, w, STATUS_H, &layout, ink());
    }
    UndoFace {
        rgba,
        w,
        h: STATUS_H,
    }
}

/// One Wi-Fi keyboard cell, rasterised without the uppercase conversion used by
/// word_face and the rest of the interface's display labels. A keyboard's lower-case and
/// upper-case layers are distinct input choices, so their glyphs must stay distinct too.
pub fn wifi_key_face(glyph: char) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: vec![0; 4],
            w: 1,
            h: 1,
        };
    };
    let value = glyph.to_string();
    let width = (text::line_width(font, &value, PCT_PX, 0.0).ceil().max(1.0) as u32) + 4;
    let height = (text::line_height(PCT_PX).ceil().max(1.0) as u32) + 4;
    let layout = text::Layout {
        lines: vec![value],
        px: PCT_PX,
        tracking: 0.0,
    };
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    text::draw_centred(&mut rgba, width, height, &layout, ink());
    UndoFace {
        rgba,
        w: width,
        h: height,
    }
}

/// The time. Same face and same band as the battery's percent — the two halves of one status row.
/// ★ 2026-11-03（六六）：「字体有点小」。状态行那档（`word_face`，24px 挤在 20 高的带子里）
/// 原是给顶栏电池/时钟那个窄仓位定的，Wi-Fi 配置页和直连提示整页挂在它身上就偏小。
/// 整页的文字另开一档：三十二，带高四十四（正好装下 `ascent - descent` 的整 em 框，
/// 见 `STATUS_H` 上那段关于「超高的墨会被丢而不是缩」的注释）。
///
/// 带高必须跟着字号走，不能沿用 `STATUS_H`：`text::fit` 只管宽度，`text::coverage` 把
/// 基线按 `(dst_h - block_h)/2 + ascent` 摆，带子矮了就把字顶（或字底）裁掉。
pub const PAGE_PX: f32 = 32.0;
pub const PAGE_MIN_PX: f32 = 20.0;
pub const PAGE_H: u32 = 44;
/// 整页一行的最宽：屏宽六百四减左右各四十的留白。
pub const PAGE_MAX_W: f32 = 560.0;

/// A line of type on a full page rather than in a status slot — the same face, at the size a
/// page is read at.
pub fn page_face(text: &str) -> UndoFace {
    let w = band_width_capped(text, PAGE_PX, PAGE_MIN_PX, PAGE_MAX_W);
    let mut rgba = vec![0u8; (w * PAGE_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, w as f32, 1, PAGE_PX, PAGE_MIN_PX);
        text::draw_centred(&mut rgba, w, PAGE_H, &layout, ink());
    }
    UndoFace {
        rgba,
        w,
        h: PAGE_H,
    }
}

pub fn clock_width(text: &str) -> u32 {
    band_width_at(text, CLOCK_PX, CLOCK_MIN_PX)
}

pub fn clock_face(text: &str) -> UndoFace {
    let w = clock_width(text);
    let mut rgba = vec![0u8; (w * STATUS_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, w as f32, 1, CLOCK_PX, CLOCK_MIN_PX);
        text::draw_centred(&mut rgba, w, STATUS_H, &layout, ink());
    }
    UndoFace {
        rgba,
        w,
        h: STATUS_H,
    }
}

/// A hint's slot is held whether or not its face arrived, for the same reason a cart with no
/// face still holds its place on the shelf: a missing affordance is worse than a blank one.
pub fn hint_quad(x: f32, y: f32, w: f32, face: Option<TexId>) -> Draw {
    let h = HINT_H as f32;
    match face {
        Some(tex) => Draw::Tex {
            x,
            y,
            w,
            h,
            tex,
            alpha: 1.0,
        },
        None => Draw::Rect {
            x,
            y,
            w,
            h,
            colour: [1.0, 1.0, 1.0, 0.12],
        },
    }
}

/// How wide the type alone comes out, which is what the hint is sized around. A label the
/// fitter had to break lands at `LABEL_MAX_W`, since that is the width it was broken to.
fn band_width(label: &str) -> u32 {
    band_width_at(label, LABEL_PX, LABEL_MIN_PX)
}

/// The same, at a stated size. The clock and the battery's percent are one band of type rendered
/// twice at two sizes, and this is the one place the size is a parameter rather than a constant —
/// which is what keeps their widths, their shrink thresholds and their fitter rules from drifting
/// apart the moment one of the two sizes is touched.
fn band_width_at(label: &str, px: f32, min_px: f32) -> u32 {
    band_width_capped(label, px, min_px, LABEL_MAX_W)
}

/// [`band_width_at`] with the width ceiling stated too. It is a parameter rather than the
/// constant because a full page's line is as wide as the screen, not as wide as a hint's label:
/// 140 would break a Wi-Fi SSID across two lines the moment it arrived.
fn band_width_capped(label: &str, px: f32, min_px: f32, max_w: f32) -> u32 {
    let Some(font) = text::label_font() else {
        return max_w as u32;
    };
    let layout = text::fit(font, label, max_w, 1, px, min_px);
    let ink = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    (ink.ceil() as u32).clamp(1, max_w as u32)
}

/// One line of type for the top plate, naming the entry under the eye.
pub fn title_face(text: &str) -> UndoFace {
    let mut rgba = vec![0u8; (TITLE_W * TITLE_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, TITLE_W as f32, 1, TITLE_PX, TITLE_MIN_PX);
        text::draw_centred(&mut rgba, TITLE_W, TITLE_H, &layout, ink());
    }
    UndoFace {
        rgba,
        w: TITLE_W,
        h: TITLE_H,
    }
}

/// One line of the direct-connect dialog's own type. Its own builder rather than a share of a
/// screen's, because the dialog is a screen of its own and asks for sizes (36/24) nothing else
/// prints at. The box is twice the type's height so the font's own line box fits rather than being
/// clipped — `text::coverage` drops ink past the box's edge instead of shrinking it.
pub fn dialog_line_face(text: &str, px: f32) -> UndoFace {
    let max_w = 640.0f32;
    let box_h = (px * 2.0).ceil() as u32;
    let mut w = 1u32;
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, max_w, 1, px, px * 0.5);
        let ink = layout
            .lines
            .iter()
            .map(|l| text::line_width(font, l, layout.px, layout.tracking))
            .fold(0.0, f32::max);
        w = (ink.ceil() as u32).clamp(1, max_w as u32);
    }
    let mut rgba = vec![0u8; (w * box_h * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, w as f32, 1, px, px * 0.5);
        text::draw_centred(&mut rgba, w, box_h, &layout, ink());
    }
    UndoFace { rgba, w, h: box_h }
}

/// The shelf's line of type: the game under the eye, for the screen below the row. Cut from
/// the cart's own name rather than the raw filename, so what is set here and what is printed
/// on the cart are the same words — `slot_store::clean_label` is the one place that decides which
/// part of a dumped filename is the game and which part is a fact about the dump.
pub fn shelf_title_face(text: &str) -> UndoFace {
    // ★ 2026-10-24（六六）：「长名字不缩字号，循环滚动就行」——
    //   一律按设计字号（`SHELF_TITLE_PX`）单行排，**不再为了塞进 300px 而缩号**；
    //   面宽 = max(300, 实测宽)。超宽的那部分由 `app.rs` 在 300px 的窗里循环推着走
    //   （`Draw::Glyph` 采样一条滑动的 uv 子矩形），所以这里的宽度是"整条字"的宽度。
    let mut face_w = SHELF_TITLE_W;
    let mut layout = None;
    if let Some(font) = text::label_font() {
        let l = text::fit(
            font,
            text,
            f32::MAX,
            1,
            SHELF_TITLE_PX,
            SHELF_TITLE_PX,
        );
        let w = l
            .lines
            .first()
            .map(|line| text::line_width(font, line, l.px, l.tracking))
            .unwrap_or(0.0);
        face_w = (w.ceil() as u32).max(SHELF_TITLE_W);
        layout = Some(l);
    }
    let mut rgba = vec![0u8; (face_w * SHELF_TITLE_H * 4) as usize];
    if let Some(layout) = layout {
        text::draw_centred(&mut rgba, face_w, SHELF_TITLE_H, &layout, plate_ink());
    }
    UndoFace {
        rgba,
        w: face_w,
        h: SHELF_TITLE_H,
    }
}

pub(crate) fn blit(dst: &mut [u8], dst_w: u32, src: &[u8], src_w: u32, src_h: u32, x: u32, y: u32) {
    for row in 0..src_h {
        let from = ((row * src_w) * 4) as usize;
        let to = (((y + row) * dst_w + x) * 4) as usize;
        dst[to..to + (src_w * 4) as usize].copy_from_slice(&src[from..from + (src_w * 4) as usize]);
    }
}

/// Between the two arrow caps. Tighter than `CAP_GAP`, so the pair reads as one control and
/// its word as belonging to both.
pub const ARROW_GAP: u32 = 3;

/// Font Awesome's carets, as the bundled symbols font carries them. `label.ttf` has no arrows,
/// and a glyph it lacks would rasterise to a blank key.
const LEFT_CARET: char = '\u{f0d9}';
const RIGHT_CARET: char = '\u{f0da}';

pub fn arrows_hint_width(label: &str) -> u32 {
    2 * CAP + ARROW_GAP + CAP_GAP + band_width(label) + EDGE
}

/// Left and right as the two keys they are — a cap each — then the one word they share.
pub fn arrows_hint_face(label: &str) -> UndoFace {
    let w = arrows_hint_width(label);
    let mut rgba = vec![0u8; (w * HINT_H * 4) as usize];
    for (i, glyph) in [LEFT_CARET, RIGHT_CARET].into_iter().enumerate() {
        blit(
            &mut rgba,
            w,
            &glyph_cap(glyph),
            CAP,
            CAP,
            i as u32 * (CAP + ARROW_GAP),
            (HINT_H - CAP) / 2,
        );
    }
    let text_w = band_width(label);
    let mut band = vec![0u8; (text_w * HINT_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, label, text_w as f32, 1, LABEL_PX, LABEL_MIN_PX);
        text::draw_centred(&mut band, text_w, HINT_H, &layout, ink());
    }
    blit(
        &mut rgba,
        w,
        &band,
        text_w,
        HINT_H,
        2 * CAP + ARROW_GAP + CAP_GAP,
        0,
    );
    UndoFace { rgba, w, h: HINT_H }
}

/// A square cap with one symbol glyph centred on it, in the same inks as a lettered cap.
fn glyph_cap(glyph: char) -> Vec<u8> {
    let mut cap = Vec::with_capacity((CAP * CAP * 4) as usize);
    for _ in 0..CAP * CAP {
        cap.extend_from_slice(&[ink()[0], ink()[1], ink()[2], 255]);
    }
    let Some(font) = crate::icon::symbols_font() else {
        return cap;
    };
    let (m, cov) = font.rasterize(glyph, KEY_PX);
    let x0 = (CAP as i32 - m.width as i32) / 2;
    let y0 = (CAP as i32 - m.height as i32) / 2;
    for gy in 0..m.height {
        for gx in 0..m.width {
            let (dx, dy) = (x0 + gx as i32, y0 + gy as i32);
            if dx < 0 || dy < 0 || dx >= CAP as i32 || dy >= CAP as i32 {
                continue;
            }
            let a = cov[gy * m.width + gx] as u32;
            let at = ((dy as u32 * CAP + dx as u32) * 4) as usize;
            for k in 0..3 {
                cap[at + k] =
                    ((cap_ink()[k] as u32 * a + cap[at + k] as u32 * (255 - a)) / 255) as u8;
            }
        }
    }
    cap
}

#[cfg(test)]
mod wifi_key_face_tests {
    use super::wifi_key_face;

    #[test]
    fn wifi_key_face_preserves_lowercase_and_uppercase() {
        let lower = wifi_key_face('a');
        let upper = wifi_key_face('A');

        assert!(lower.w > 0 && lower.h > 0);
        assert!(upper.w > 0 && upper.h > 0);
        assert_ne!(
            lower.rgba, upper.rgba,
            "the keyboard's lowercase and uppercase layers must render different glyphs"
        );
    }
}
