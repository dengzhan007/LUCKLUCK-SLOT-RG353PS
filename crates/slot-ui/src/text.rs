use std::sync::OnceLock;

use fontdue::{Font, FontSettings};

const LABEL_TTF: &[u8] = include_bytes!("../assets/label.ttf");

/// The small pixel face, **baked into the binary**. Ark Pixel **10px**, OFL-1.1 — same family
/// as the body face, one design size down.
///
/// **Baked in rather than read off the card**, unlike the body face. It exists for one job — the
/// Latin labels under the transport keys — and its absence would have to degrade into something
/// (12px type crammed into a 14-row band, or no labels at all), which is worse than the 370 KB
/// it costs. The card keeps ownership of the *body* face: that one has to carry whatever scripts
/// the user's music does, and a subset baked in could not.
///
/// ⚠️ **It is a reduced charset: 1076 CJK ideographs against the 12px face's 18299.** It is for
/// Latin, digits and punctuation; a Chinese label at this size would come out with holes.
const SMALL_TTF: &[u8] = include_bytes!("../assets/ark-pixel-10px-proportional-zh_hans.otf");

/// The size at or below which a glyph is drawn with [`SMALL_TTF`]. **The panel only ever asks
/// for 10, 12 and 24**, so 11 separates "the small face" from "the body face" cleanly.
pub const SMALL_PX_MAX: f32 = 11.0;

pub struct Layout {
    pub lines: Vec<String>,
    pub px: f32,
    pub tracking: f32,
}

/// The card's face, if it supplied a usable one. Set once at boot by `set_font`; an empty slot
/// means "no card font", and every glyph then falls back to the embedded face.
static USER_FONT: OnceLock<Font> = OnceLock::new();
/// The face baked into the binary. Always present; the fallback for any glyph a card face lacks
/// (most CJK faces ship without Latin), so a Chinese title and an English code both render
/// instead of one blanking the other.
static EMBEDDED_FONT: OnceLock<Font> = OnceLock::new();
/// The 10px face; see [`SMALL_TTF`].
static SMALL_FONT: OnceLock<Font> = OnceLock::new();

/// Set the interface's typeface from the bytes of a TTF, OTF or TTC. Called once at boot.
/// Only a parse that succeeds takes effect; a file that will not parse is ignored and the
/// embedded face remains the fallback. A `.ttc` collection works via `FontSettings::default()`
/// (face 0). A second call is ignored.
/// True once a card face has been installed. The device boots the face on a worker thread
/// (see `Frontend::boot_with_sink`) while the opening animation plays, so the GL thread
/// must not read and parse the same multi-megabyte file a second time.
pub fn has_user_font() -> bool {
    USER_FONT.get().is_some()
}

pub fn set_font(bytes: Vec<u8>) -> bool {
    // Leaked on purpose. `Font::from_bytes` borrows the bytes for as long as the font lives,
    // and the font lives as long as the process; the alternative is a self-referential struct
    // for a single allocation that would never be freed either way.
    let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    match Font::from_bytes(data, FontSettings::default()) {
        Ok(font) => USER_FONT.set(font).is_ok(),
        Err(_) => false,
    }
}

/// The embedded face, initialised on first need.
fn embedded_font() -> &'static Font {
    EMBEDDED_FONT.get_or_init(|| {
        Font::from_bytes(LABEL_TTF, FontSettings::default()).expect("embedded label.ttf must parse")
    })
}

/// The 10px face, initialised on first need; see [`SMALL_TTF`].
fn small_font() -> &'static Font {
    SMALL_FONT.get_or_init(|| {
        Font::from_bytes(SMALL_TTF, FontSettings::default()).expect("baked 10px face must parse")
    })
}

// ── The deck panel's two faces, and the plan that pairs them ─────────────────────────────────
//
// ★ **Why the panel needs faces the rest of the UI does not.**
//
// The panel is the only screen that sets Chinese and Latin **on the same line at different
// sizes**: the name block reads `artist - title`, and a Chinese title beside an English artist
// is the normal case for a music card. The interface's own faces cannot do that: the body face
// is one size for the whole line, so a line's Chinese and its Latin come out the same height,
// and the one size that suits Chinese (16) is the one size the 12px-designed face cannot be
// drawn at (70.6% antialiased — see the note on `line_height`).
//
// So the panel gets its own pair, chosen together in [`Plan`]:
//
// | | face | design size | ink | built for |
// |---|---|---|---|---|
// | Chinese | [`CJK_TTF`] | 16px | 15 | names, artists, lyrics |
// | Latin | [`LATIN_TTF`] | 8px | 8 | clock, transport word, EQ tag, mode |
//
// ★ **Both sizes are the fonts' own design sizes.** 16px is what the Chinese face was drawn
// for and 8px is what the Latin face was drawn for; both rasterise to hard pixels with **no
// half-tones at all** (measured, 0.0% over the panel's whole Latin string), and both are clean
// at every whole multiple too (8/16/24/32/40 and 16/32), which is what lets the strip and the
// clock ever grow. Scaling either face to a size it was not drawn for is the one thing that
// must not happen — that is the "dirty font" bug this project already spent a round on.

/// The panel's Chinese face: **GNU Unifont 16px** (dual-licensed — SIL OFL-1.1 **or** GPLv2+ with
/// the GNU Font Embedding Exception; slot takes the OFL option, and `unifont-16px.otf.LICENSE.txt`
/// ships beside it, quoting the font's own `name` table).
///
/// **Read off the card, not baked in.** It used to be `include_bytes!` — 12.6 MB of binary for a
/// font that only one screen ever draws — and 六六's call is that it ships as a file instead:
/// `<card>/Music/Fonts/`, put there by the release package. The card already owns the
/// interface's typeface for exactly this reason (the user's music may be in any script, so the
/// face cannot be a subset baked at build time).
///
/// ★ **It replaced Chill Bitmap 16px on 2026-10-07** (六六: 「换 UNIFONT 吧感觉好看些」).
/// Two things made that a swap rather than a rework: **its `line_h` at 16px is 16, the same as
/// the face it replaces**, so the panel's whole vertical layout — every number derived from
/// `line_h()` — is untouched; and it is **5.3 MB against 12.6**, so the read-and-parse is about
/// half. It also covers more: 20992/20992 of the CJK block and none of 5896 distinct hanzi
/// taken from real novels missing, against Chill's identical block figure but no measured
/// corpus. See `slot-点阵中文字体-候选与实测-2026-10-07.md`.
///
/// ⚠️ **Absent is a degraded panel, not a crash.** [`cjk_font`] falls back to the interface's own
/// face, which draws something readable at the wrong size rather than nothing; a card missing the
/// files says so in the boot log and carries on.
const CJK_FILE: &str = "unifont-16px.otf";

/// The panel's Latin face: **Press Start 2P** (OFL-1.1; `press-start-2p.ttf.OFL.txt` ships
/// beside it). Drawn on an 8×8 design grid, all-caps by temperament, and the only face measured
/// that gets Latin clean at a size below 12px. Card-supplied, like [`CJK_FILE`].
const LATIN_FILE: &str = "press-start-2p.ttf";

/// The two file names the panel looks for, for a caller that has to find them on a card.
pub const PANEL_FONT_FILES: [&str; 2] = [CJK_FILE, LATIN_FILE];

static CJK_FONT: OnceLock<Font> = OnceLock::new();
static LATIN_FONT: OnceLock<Font> = OnceLock::new();

/// Install the panel's two faces from a card. Called once at boot, beside [`set_font`].
///
/// Either file may be missing or unparseable on its own: each is installed only if it parses, and
/// whichever is absent falls back to the interface face. Two files rather than a folder scan
/// because the panel asks for two *specific* faces — a folder that happened to hold a third font
/// must not change what the deck looks like.
pub fn set_panel_fonts(cjk: Option<Vec<u8>>, latin: Option<Vec<u8>>) -> (bool, bool) {
    // Leaked on purpose, for the same reason `set_font` leaks: the font borrows the bytes and
    // both live as long as the process.
    let cjk_ok = cjk.is_some_and(|bytes| {
        let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        Font::from_bytes(data, FontSettings::default())
            .is_ok_and(|font| CJK_FONT.set(font).is_ok())
    });
    let latin_ok = latin.is_some_and(|bytes| {
        let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        Font::from_bytes(data, FontSettings::default())
            .is_ok_and(|font| LATIN_FONT.set(font).is_ok())
    });
    (cjk_ok, latin_ok)
}

/// The panel's Chinese face, or the interface's if the card did not supply one.
fn cjk_font() -> &'static Font {
    CJK_FONT.get().unwrap_or_else(|| label_font().expect("embedded"))
}

/// The panel's Latin face, or the interface's if the card did not supply one.
fn latin_font() -> &'static Font {
    LATIN_FONT.get().unwrap_or_else(|| label_font().expect("embedded"))
}

/// **Which face the Latin runs are set in.** The panel needs two answers, and which is right
/// depends on whether the Latin stands *alone* or *inside a sentence*.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Latin {
    /// **The panel's own Latin face**, at [`Plan::latin_px`]. For text that is all Latin and read
    /// as a label — `PLAY`, `[REPEAT]`, `0:37`. Compact and blocky, which is what a deck's
    /// displayed numbers and words look like.
    Own,
    /// **The same face the Chinese is set in.** For a Latin word sharing a line with ideographs —
    /// `五月天 - 突然好想你`, `CrazySix - 算` — and for lyrics. 六六: 「歌手名，歌名，歌词里的
    /// 英文和数字沿用 16px 的字体观感才统一」. A face swap mid-line reads as two typefaces pasted
    /// together, and the 16px face carries Latin at 16px with **0.0% half-tones** (measured), so
    /// the swap costs nothing in sharpness.
    Cjk,
}

/// **The size pairing the panel sets a line in**: which face each script is drawn with, and at
/// which size. The two are one decision, because each face is only clean at its own design size.
///
/// A plan is per-render, not per-glyph: a line's Chinese and its Latin share a baseline, and the
/// baseline comes from whichever face is taller. See [`plan_coverage`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Plan {
    pub cjk_px: f32,
    pub latin_px: f32,
    pub latin: Latin,
}

/// Whether `font` carries a *glyph slot* for `ch`.
///
/// Choose a face by its character map, not by rasterising at an arbitrary 1 px and
/// inferring presence from the resulting bitmap. A glyph may exist even when a tiny
/// rasterisation has no ink. This is also the same test used by `missing_in`, so the
/// diagnostic and drawing paths agree about which characters a face supports.
fn carries(font: &Font, ch: char) -> bool {
    font.lookup_glyph_index(ch) != 0
}

/// The face and size one character is drawn with, under `plan`.
///
/// **The script decides the face, not the reverse** — and the fallback only fires for a
/// character the chosen face does not carry (a Cyrillic artist name, say), where drawing it from
/// the other face at *its* size is still better than drawing nothing.
fn plan_face(plan: &Plan, ch: char) -> (&'static Font, f32) {
    if is_cjk(ch) {
        return if carries(cjk_font(), ch) {
            (cjk_font(), plan.cjk_px)
        } else {
            (latin_font(), plan.latin_px)
        };
    }
    // Latin (or anything outside the CJK blocks): the plan says which face, and the *other* one
    // is the fallback for a character the chosen one cannot set at all.
    let (chosen, other) = match plan.latin {
        Latin::Own => (latin_font(), cjk_font()),
        Latin::Cjk => (cjk_font(), latin_font()),
    };
    if carries(chosen, ch) {
        (chosen, plan.latin_px)
    } else {
        (other, plan.cjk_px)
    }
}

/// A face's ascent at `px`, or the nominal size when it has no metrics at all.
fn face_ascent(font: &Font, px: f32) -> f32 {
    font.horizontal_line_metrics(px).map(|m| m.ascent).unwrap_or(px)
}

/// The blank between two neighbours under `plan`.
///
/// * **Chinese to Chinese keeps a hair of space** — the faces are drawn on a full square, and
///   two of them side by side touch, which reads as one wide glyph rather than two.
/// * **Anything else Chinese takes none** — the square carries its own rhythm.
/// * **Latin to Latin takes a tenth of the em**, which is the interface's own letter-spacing.
fn plan_gap(plan: &Plan, prev: Option<char>, ch: char) -> f32 {
    let Some(prev) = prev else { return 0.0 };
    match (is_cjk(prev), is_cjk(ch)) {
        (true, true) => (plan.cjk_px / 12.0).round().max(1.0),
        (true, false) | (false, true) => 0.0,
        (false, false) => (plan.latin_px * 0.10).round(),
    }
}

/// How tall a planned line is, in logical pixels — from the **Chinese** face, which is the
/// taller of the two and therefore the one the line box has to hold.
///
/// ⚠️ **Measured on the baked Chinese face, not through [`line_height`]**: that one answers for
/// the *interface's* face — the card's, or the embedded fallback — and the panel's Chinese is
/// neither. Going through it would silently size the panel's rows off a font it never draws.
pub fn plan_line_height(plan: &Plan) -> f32 {
    cjk_font()
        .horizontal_line_metrics(plan.cjk_px)
        .map(|m| m.new_line_size - m.line_gap)
        .unwrap_or(plan.cjk_px)
}

/// The width `line` sets to under `plan`, measured the same way [`plan_coverage`] draws it.
pub fn plan_line_width(line: &str, plan: &Plan) -> f32 {
    let mut w = 0.0f32;
    let mut prev: Option<char> = None;
    for ch in line.chars() {
        w += plan_gap(plan, prev, ch);
        let (font, px) = plan_face(plan, ch);
        w += font.metrics(ch, px).advance_width;
        prev = Some(ch);
    }
    w
}

/// Where `line`'s ink sits, as `[top, bottom)` rows **relative to the line's em box top**.
///
/// ★ **Layout arithmetic has to be done on the ink, not on the em box.** The em box is
/// `ascent + descent` and is mostly empty: an 8px Latin word puts its ink in rows 0..8 of a box
/// the Chinese face wants 16 rows of, so a row centred by its em box lands visibly high (or, for
/// the strip's clock, visibly low). Every "centre this between those two things" in the panel
/// goes through here.
///
/// ⚠️ **The arithmetic must be the same one [`plan_coverage`] draws with**, and that is
/// `asc - (height + ymin)` — *not* `asc - face_ascent(...) - (height + ymin)`. The extra
/// `face_ascent` term looks plausible (it is how the browser reference shifts a short face down
/// onto a tall one) but it is already inside `height + ymin`: subtracting it again moved the
/// meta row and the strip's clock ~9 rows out of place, which is exactly the kind of error that
/// reads as "the type is not centred" and gets blamed on the font.
pub fn plan_ink_span(line: &str, plan: &Plan) -> (i32, i32) {
    let mut asc = 0.0f32;
    for ch in line.chars() {
        let (font, px) = plan_face(plan, ch);
        asc = asc.max(face_ascent(font, px));
    }
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for ch in line.chars() {
        let (font, px) = plan_face(plan, ch);
        let (m, _) = font.rasterize(ch, px);
        if m.width == 0 || m.height == 0 {
            continue;
        }
        let top = asc - (m.height as f32 + m.ymin as f32);
        lo = lo.min(top.round() as i32);
        hi = hi.max((top + m.height as f32).round() as i32);
    }
    if lo > hi {
        (0, 0)
    } else {
        (lo, hi)
    }
}

/// Lay `text` out under `plan`, wrapped to `max_lines` — the plan's [`fit`].
///
/// **Upper-cased, like [`fit`]** — the panel is a deck's display and it has always set its type
/// in caps; dropping that here would change the case of every Latin title on the same commit
/// that changes the face, and two changes at once is how a regression gets blamed on the wrong
/// one.
pub fn plan_fit(text: &str, max_w: f32, max_lines: usize, plan: &Plan) -> Layout {
    let in_plan = |s: &str| plan_line_width(s, plan);
    let upper = text.to_uppercase();
    let px = plan.cjk_px;
    let lines = wrap(&in_plan, &upper, max_w, false);
    if lines.len() <= max_lines && lines.iter().all(|l| in_plan(l) <= max_w) {
        return Layout {
            lines,
            px,
            tracking: 0.0,
        };
    }
    Layout {
        lines: wrap(&in_plan, &upper, max_w, true)
            .into_iter()
            .take(max_lines)
            .collect(),
        px,
        tracking: 0.0,
    }
}

/// [`coverage`], for a planned line: each glyph drawn **by its own face at its own design
/// size**, all of them on one baseline.
///
/// The baseline is the line's common one — `em_top + max(ascent)` across the glyphs — so a
/// Chinese word and an English one beside it stand on the same line rather than each floating
/// in its own box.
pub fn plan_coverage(dst_w: u32, dst_h: u32, layout: &Layout, plan: &Plan) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w * dst_h) as usize];
    let line_h = plan_line_height(plan);
    let block_h = line_h * layout.lines.len() as f32;
    let top0 = (dst_h as f32 - block_h) / 2.0;

    for (i, line) in layout.lines.iter().enumerate() {
        let em_top = top0 + i as f32 * line_h;
        let asc = line
            .chars()
            .map(|ch| {
                let (font, px) = plan_face(plan, ch);
                face_ascent(font, px)
            })
            .fold(0.0f32, f32::max);
        let baseline = em_top + asc;
        let mut pen = (dst_w as f32 - plan_line_width(line, plan)) / 2.0;
        let mut prev: Option<char> = None;
        for ch in line.chars() {
            pen += plan_gap(plan, prev, ch);
            let (font, px) = plan_face(plan, ch);
            let (m, cov) = font.rasterize(ch, px);
            stamp(
                &mut out,
                dst_w,
                dst_h,
                (pen + m.xmin as f32).round() as i32,
                (baseline - (m.height as f32 + m.ymin as f32)).round() as i32,
                &cov,
                m.width as u32,
                m.height as u32,
            );
            pen += m.advance_width;
            prev = Some(ch);
        }
    }
    out
}

/// **Which face a glyph is drawn with — and the size decides.**
///
/// The panel has two pixel faces: the body one (design size 12px, from the card) and the small
/// one (design size 10px, baked in; see [`SMALL_TTF`]). **Each is only clean at its own design
/// size** — 12px type rasterised at 10 is 99.8% antialiased, which a 1-bit panel turns into a
/// smudge — so the size and the face are the same decision, and making it here means no caller
/// has to pass a flag through five functions to say which one it wants.
///
/// **A glyph the small face lacks falls back to the body face**, which is what keeps the ~1000
/// ideographs it does carry from being a trap: a Chinese label at 10px renders from the 12px
/// face rather than vanishing.
fn face_for(ch: char, px: f32) -> &'static Font {
    if px <= SMALL_PX_MAX && carries(small_font(), ch) {
        small_font()
    } else {
        font_for(ch)
    }
}

/// The face a **whole line** is set in, chosen by size alone — for metrics that belong to the
/// line rather than to a glyph (its height, above all).
///
/// [`face_for`] answers "which face has this character"; this answers "which face is this size
/// set in", and the two differ exactly when a glyph falls back. A line's height must follow the
/// face it is *set* in, not whichever face happened to supply one character.
fn face_by_size(px: f32) -> &'static Font {
    if px <= SMALL_PX_MAX {
        small_font()
    } else {
        label_font().unwrap_or_else(|| embedded_font())
    }
}

/// How tall one line of type is at `px`, in pixels — `new_line_size - line_gap`, from the face
/// that size is set in.
///
/// **Public because the panel's whole vertical plan is built from it.** The line box is not the
/// nominal size: Ark Pixel 12px has a 16-pixel line, and its 10px face has 14. A caller that
/// reserved `px` rows would find the second line overlapping the first.
pub fn line_height(px: f32) -> f32 {
    face_by_size(px)
        .horizontal_line_metrics(px)
        .map(|m| m.new_line_size - m.line_gap)
        .unwrap_or(px)
}

/// The characters in `text` that neither face can draw, in order of first appearance.
///
/// The card's face is a subset, cut to the titles the card carries; a card that gains a game
/// with a character outside it loses that character silently, because a missing glyph draws as
/// nothing rather than as an error. Checking is per character and does not rasterise a whole
/// title, so every title on the card can be asked at boot.
pub fn missing_in(text: &str) -> Vec<char> {
    let mut out = Vec::new();
    for ch in text.chars() {
        if ch == '\n' || ch.is_whitespace() {
            continue;
        }
        // ★ **`carries` uses the cmap glyph index**: ask whether the face can set this
        //   character, not whether a 1 px rasterisation happened to leave any ink. This
        //   check runs across every title before the first frame and must agree with drawing.
        let user_has = USER_FONT.get().is_some_and(|f| carries(f, ch));
        if !user_has && !carries(embedded_font(), ch) && !out.contains(&ch) {
            out.push(ch);
        }
    }
    out
}

/// The face to draw one glyph with: the card's if it has that glyph, otherwise the embedded one.
/// This is what lets a card carry a CJK face (for the Chinese title) without also blanking the
/// Latin UI — codes and labels — that the embedded face still covers.
fn font_for(ch: char) -> &'static Font {
    if let Some(user) = USER_FONT.get() {
        // Keep selection consistent with missing_in(): ask whether the font's cmap
        // contains a glyph slot, not whether it happens to rasterise at 1 px.
        if carries(user, ch) {
            return user;
        }
    }
    embedded_font()
}

/// The interface's nominal typeface: the card's if one was supplied at boot, else the embedded
/// face (always present). Returns `None` only if even the embedded face failed to parse, which
/// should never happen. Per-glyph rasterisation inside `coverage` may still switch to the other
/// face for a glyph this one lacks, so a Chinese title and a Latin code render from one baseline.
pub fn label_font() -> Option<&'static Font> {
    USER_FONT.get().or_else(|| Some(embedded_font()))
}

/// Largest whole pixel size at which `text` wraps into `max_lines` or fewer whole words per
/// line. Shrinking is preferred to hyphenless mid-word breaks, so a long title reads smaller
/// rather than scrambled. Only a title that will not fit even at `min_px` is broken between
/// characters and clipped to `max_lines`.
pub fn fit(
    font: &Font,
    text: &str,
    max_w: f32,
    max_lines: usize,
    max_px: f32,
    min_px: f32,
) -> Layout {
    let upper = text.to_uppercase();
    let mut px = max_px;
    while px > min_px {
        let tracking = tracking_for(px);
        let w = |s: &str| line_width(font, s, px, tracking);
        let lines = wrap(&w, &upper, max_w, false);
        if lines.len() <= max_lines && lines.iter().all(|l| w(l) <= max_w) {
            return Layout {
                lines,
                px,
                tracking,
            };
        }
        px -= 1.0;
    }
    let tracking = tracking_for(min_px);
    let w = |s: &str| line_width(font, s, min_px, tracking);
    Layout {
        lines: wrap(&w, &upper, max_w, true)
            .into_iter()
            .take(max_lines)
            .collect(),
        px: min_px,
        tracking,
    }
}

pub fn draw_centred(dst: &mut [u8], dst_w: u32, dst_h: u32, layout: &Layout, colour: [u8; 3]) {
    for (i, a) in coverage(dst_w, dst_h, layout).into_iter().enumerate() {
        if a > 0 {
            blend(&mut dst[i * 4..i * 4 + 4], a as u32, colour);
        }
    }
}

/// [`draw_centred`], but centred on the **ink** instead of on the em box.
///
/// ★ **为什么需要第二个**（2026-10-06，菜单按钮）：`draw_centred` 把 em 框摆正，而 em 框是
///   `ascent + descent`，两者的比例是字体自己定的。Ark Pixel 在 30px 上是 32.5 / 7.5 ——
///   **不对称**，于是字在框里天然偏下。实测 `按键指南` 的墨迹中心落在 36.5 而按钮中心是 34.5，
///   **低 2px**，肉眼就是"名称没居中"。
///   这一课在液晶面板上已经吃过一次（见 [`plan_ink_span`]）：**要摆正的是墨，不是框。**
///
/// `>= 100` 才算墨：抗锯齿的边（1..99）不上算，否则一笔的"毛边"会把包围盒撑大，
/// 于是量出来的是"笔画外沿的包围盒"，又回到偏。
pub fn draw_centred_ink(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    layout: &Layout,
    colour: [u8; 3],
) {
    let cov = coverage(dst_w, dst_h, layout);
    let (mut x0, mut y0, mut x1, mut y1) = (dst_w, dst_h, 0u32, 0u32);
    for y in 0..dst_h {
        for x in 0..dst_w {
            if cov[(y * dst_w + x) as usize] >= 100 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 || y1 < y0 {
        return; // 一个字都没画出来
    }
    let dx = (dst_w as i32 - (x1 - x0 + 1) as i32) / 2 - x0 as i32;
    let dy = (dst_h as i32 - (y1 - y0 + 1) as i32) / 2 - y0 as i32;
    for y in 0..dst_h {
        for x in 0..dst_w {
            let a = cov[(y * dst_w + x) as usize];
            if a == 0 {
                continue;
            }
            let (tx, ty) = (x as i32 + dx, y as i32 + dy);
            if tx < 0 || ty < 0 || tx >= dst_w as i32 || ty >= dst_h as i32 {
                continue; // 位移把边上的墨推出去就丢掉，不绕回来
            }
            let i = ((ty as u32 * dst_w + tx as u32) * 4) as usize;
            blend(&mut dst[i..i + 4], a as u32, colour);
        }
    }
}

/// The same type as `draw_centred` lays down, as one byte of ink per pixel and no colour.
/// What a caller needs to dilate a halo out of the type before tinting it.
pub fn coverage(dst_w: u32, dst_h: u32, layout: &Layout) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w * dst_h) as usize];
    let Some(font) = label_font() else {
        return out;
    };
    let Some(vm) = face_by_size(layout.px).horizontal_line_metrics(layout.px) else {
        return out;
    };
    let _ = font;
    // Centre on the true em box — ascent minus descent — not on `new_line_size`, which also
    // carries the font's hhea line gap. The gap is blank space *between* lines and is not part
    // of the ink a line draws; some faces pad it by a full extra em (Plix ships `line_gap == upm`).
    // Feeding it into the line box pushes the baseline down until the line no longer fits the band
    // it is rasterised into, so the type is clipped at the top and reads as if shifted upward.
    // Subtracting the gap leaves `ascent - descent`, identical to `new_line_size` when the gap is
    // zero (label.ttf, the embedded fallback), so the long-standing layout for that face is
    // unchanged and any card face a player drops in — whatever its metrics — stays centred.
    let line_h = vm.new_line_size - vm.line_gap;
    let block_h = line_h * layout.lines.len() as f32;
    let mut baseline = (dst_h as f32 - block_h) / 2.0 + vm.ascent;

    for line in &layout.lines {
        let mut pen = (dst_w as f32 - line_width(font, line, layout.px, layout.tracking)) / 2.0;
        let mut prev: Option<char> = None;
        for ch in line.chars() {
            // Spaced before the glyph, not after, so the two metric paths stay the same
            // arithmetic and cannot drift apart.
            pen += pair_tracking(prev, ch, layout.tracking);
            let g = face_for(ch, layout.px);
            let (m, cov) = g.rasterize(ch, layout.px);
            stamp(
                &mut out,
                dst_w,
                dst_h,
                (pen + m.xmin as f32).round() as i32,
                (baseline - (m.height as f32 + m.ymin as f32)).round() as i32,
                &cov,
                m.width as u32,
                m.height as u32,
            );
            pen += m.advance_width;
            prev = Some(ch);
        }
        baseline += line_h;
    }
    out
}

/// `font` is the layout's nominal face, kept for call-site compatibility; the measured width
/// uses [`face_for`] per glyph, so a mixed-script line's width matches what `coverage` draws —
/// and so a line measured at 10px is measured with the small face, which is a different design
/// with different advances.
pub fn line_width(_font: &Font, line: &str, px: f32, tracking: f32) -> f32 {
    let mut w = 0.0;
    let mut prev: Option<char> = None;
    for ch in line.chars() {
        w += pair_tracking(prev, ch, tracking) + face_for(ch, px).metrics(ch, px).advance_width;
        prev = Some(ch);
    }
    w
}

fn tracking_for(px: f32) -> f32 {
    (px * 0.10).round()
}

/// Whether a character is set without spaces around it, which is what decides both where a
/// line may break and whether it gets the Latin letter spacing. These are the CJK blocks
/// proper plus hangul and the fullwidth forms, not a general "non-Latin" test: a Greek or
/// Cyrillic title is still spaced type.
pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF   // hangul jamo
        | 0x2E80..=0x2EFF // CJK radicals
        | 0x3000..=0x303F // CJK punctuation
        | 0x3040..=0x30FF // hiragana, katakana
        | 0x3130..=0x318F // hangul compatibility jamo
        | 0x31C0..=0x31EF // CJK strokes
        | 0x3400..=0x4DBF // CJK extension A
        | 0x4E00..=0x9FFF // CJK unified ideographs
        | 0xA960..=0xA97F // hangul jamo extended A
        | 0xAC00..=0xD7FF // hangul syllables
        | 0xF900..=0xFAFF // CJK compatibility ideographs
        | 0xFE30..=0xFE4F // CJK compatibility forms
        | 0xFF00..=0xFF60 // fullwidth forms
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x2FA1F // CJK extensions B onwards
    )
}

/// The letter spacing between two neighbours. The layout's, except where either side is CJK:
/// 10% of the em is set for Latin and reads as a gap wedged between ideographs, which carry
/// their own rhythm on the square they are drawn in.
pub(crate) fn pair_tracking(prev: Option<char>, ch: char, tracking: f32) -> f32 {
    if tracking == 0.0 || prev.is_some_and(is_cjk) || is_cjk(ch) {
        0.0
    } else {
        tracking
    }
}

/// A breakable piece of a line, and whether a space separated it from the one before.
///
/// Whitespace is the only break opportunity Latin gives; CJK gives none at all. A Chinese
/// title under a whitespace splitter is one token, so the fitter shrinks it all the way to
/// `min_px` before it can break it anywhere, and a name that reads fine at 20 px arrives at
/// 10. Cutting at every ideograph is what the script does anyway.
struct Token {
    text: String,
    space_before: bool,
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !run.is_empty() {
                out.push(Token {
                    text: std::mem::take(&mut run),
                    space_before: pending_space,
                });
            }
            // Set after the run is flushed, not before: every token still to come is separated
            // from the one before it by this space, and the run that just ended was not.
            pending_space = true;
            continue;
        }
        if is_cjk(ch) {
            if !run.is_empty() {
                out.push(Token {
                    text: std::mem::take(&mut run),
                    space_before: pending_space,
                });
                pending_space = false;
            }
            out.push(Token {
                text: ch.to_string(),
                space_before: pending_space,
            });
            pending_space = false;
            continue;
        }
        run.push(ch);
    }
    if !run.is_empty() {
        out.push(Token {
            text: run,
            space_before: pending_space,
        });
    }
    out
}

/// Greedy word wrap. With `break_words`, a word wider than `max_w` on its own is split
/// between characters so every line fits; without it that word overflows its line and the
/// caller sees a line wider than `max_w`.
///
/// **`width` is passed in rather than derived from a font**, because there are now two ways to
/// measure a line — one face at one size ([`line_width`]) and one plan over two faces
/// ([`plan_line_width`]) — and the wrapper must measure exactly the way the renderer will draw.
/// A wrapper that took a `Font` would silently measure a planned line at the wrong size.
fn wrap(width: &dyn Fn(&str) -> f32, text: &str, max_w: f32, break_words: bool) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for token in tokens(text) {
        let parts = if break_words {
            break_word(width, &token.text, max_w)
        } else {
            vec![token.text.clone()]
        };
        for (i, part) in parts.iter().enumerate() {
            // The space belongs between the tokens it separated, so it goes back only where
            // the token it preceded stayed on this line. A continuation of a broken word
            // never gets one.
            let sep = if line.is_empty() {
                ""
            } else if i == 0 && token.space_before {
                " "
            } else {
                ""
            };
            let joined = format!("{line}{sep}{part}");
            if line.is_empty() || width(&joined) <= max_w {
                line = joined;
            } else {
                lines.push(std::mem::take(&mut line));
                line = part.clone();
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn break_word(width: &dyn Fn(&str) -> f32, word: &str, max_w: f32) -> Vec<String> {
    if width(word) <= max_w {
        return vec![word.to_string()];
    }
    let mut parts = Vec::new();
    let mut part = String::new();
    for ch in word.chars() {
        part.push(ch);
        if width(&part) > max_w && part.chars().count() > 1 {
            part.pop();
            parts.push(std::mem::take(&mut part));
            part.push(ch);
        }
    }
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

/// Ink laid into a coverage buffer, taking the greater of what is there and what arrives.
/// Adjacent glyphs only ever touch at their antialiased edges, where a sum would darken the
/// seam into a visible join.
#[allow(clippy::too_many_arguments)]
fn stamp(dst: &mut [u8], dst_w: u32, dst_h: u32, x: i32, y: i32, cov: &[u8], gw: u32, gh: u32) {
    for gy in 0..gh {
        let dy = y + gy as i32;
        if dy < 0 || dy >= dst_h as i32 {
            continue;
        }
        for gx in 0..gw {
            let dx = x + gx as i32;
            if dx < 0 || dx >= dst_w as i32 {
                continue;
            }
            let i = (dy as u32 * dst_w + dx as u32) as usize;
            dst[i] = dst[i].max(cov[(gy * gw + gx) as usize]);
        }
    }
}

/// Source over, carrying the destination's own alpha, so type lands on a transparent buffer
/// as ink rather than as ink faded toward black. Over an opaque buffer it collapses to the
/// plain coverage blend.
fn blend(px: &mut [u8], a: u32, colour: [u8; 3]) {
    let under = px[3] as u32 * (255 - a) / 255;
    let out = a + under;
    for c in 0..3 {
        px[c] = ((colour[c] as u32 * a + px[c] as u32 * under + out / 2) / out) as u8;
    }
    px[3] = out as u8;
}
