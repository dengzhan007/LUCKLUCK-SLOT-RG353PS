use slot_power::{Battery, Charge};
use slot_store::Cart;
use slot_ui::{
    cluster_h, draw_status, label_colour, Draw, Idle, Printed, Shelf, TexId, CART_W, CENTER_SCALE,
    OUT_W,
    STATUS_H, TOP_BAND_H,
};

/// A machine folder spelled one of the ways `System::from_dir` also takes (`NES` for `FC`,
/// `SNES` for `SFC`) is still that machine's own folder, not a subfolder inside it. Matching
/// the canonical name alone files every rom under a folder named after its own machine, and
/// the whole shelf then sits behind a subfolder switch nobody picked.
#[test]
fn an_aliased_machine_folder_is_not_a_subfolder() {
    let s = Shelf::new(vec![
        Cart {
            initial: 'S',
            stem: "SMB".into(),
            rom: "Games/NES/SMB.nes".into(),
            label: None,
            art: None,
            code: String::new(),
            cgb: 0,
            title: "SUPER MARIO BROS".into(),
        },
        Cart {
            initial: 'Z',
            stem: "Zelda".into(),
            rom: "Games/SNES/Zelda.smc".into(),
            label: None,
            art: None,
            code: String::new(),
            cgb: 0,
            title: "ZELDA".into(),
        },
        Cart {
            initial: 'F',
            stem: "Real".into(),
            rom: "Games/Favorites/Real.gba".into(),
            label: None,
            art: None,
            code: String::new(),
            cgb: 0,
            title: "REAL".into(),
        },
    ]);
    // Both aliased folders read as their machine's own shelf, so nothing is hidden.
    assert_eq!(s.machine_view(slot_store::System::Fc).len(), 1);
    assert_eq!(s.machine_view(slot_store::System::Sfc).len(), 1);
    // A folder that names no machine really is a subfolder, and stays behind the switch.
    assert_eq!(s.machine_view(slot_store::System::Gba).len(), 0);
}

fn shelf_with(n: usize) -> Shelf {
    Shelf::new(
        (0..n)
            .map(|i| Cart {
                initial: 'G',
                stem: format!("Game {i}"),
                rom: format!("Games/Game {i}.gba").into(),
                label: None,
                art: None,
                code: String::new(),
                cgb: 0,
                title: format!("GAME {i}"),
            })
            .collect(),
    )
}

fn placed(s: &Shelf) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    out.iter()
        .map(|d| match *d {
            Draw::Rect { x, w, .. } => (x, w),
            Draw::Tex { x, w, .. } => (x, w),
            Draw::Turned { x, w, .. } => (x, w),
            Draw::Yaw { x, w, .. } => (x, w),
            // The card's marks are quads with their own geometry, placed against a cart —
            // the same reading as a face, not a marker on the list.
            Draw::Glyph { x, w, .. } | Draw::Foil { x, w, .. } => (x, w),
            Draw::Game | Draw::Glow | Draw::Shot { .. } => (0.0, OUT_W as f32),
        })
        .collect()
}

fn xw(d: &Draw) -> (f32, f32) {
    match *d {
        Draw::Rect { x, w, .. }
        | Draw::Tex { x, w, .. }
        | Draw::Turned { x, w, .. }
        | Draw::Yaw { x, w, .. }
        | Draw::Glyph { x, w, .. }
        | Draw::Foil { x, w, .. } => (x, w),
        Draw::Game | Draw::Glow | Draw::Shot { .. } => (0.0, OUT_W as f32),
    }
}

fn settle(s: &mut Shelf) {
    for _ in 0..600 {
        s.update(1.0 / 60.0);
    }
}

/// ★★ 2026-10-28（六六第二次报同一件事）：「换了壁纸才发现又犯了一个老问题：货架上的卡带是
/// 半透明的」。
///
/// 侧位的 `alpha` 就是**壁纸能透出来的比例** —— 它是合成的账，不是"把卡压暗"的旋钮。底是黑的
/// 时候两者恰好等价（`0.7×卡面 + 0.3×黑`），这张脸七个月都被当成压暗在用；换成亮壁纸那天，
/// 同一个 0.3 就原形毕露。所以这条钉的是**泄漏量**，不是那个具体数字。
#[test]
fn a_side_cart_does_not_let_the_wallpaper_read_through() {
    let s = shelf_with(5);
    let rects = s.slot_rects(None, 0.0, 0.0);
    let side: Vec<f32> = rects
        .iter()
        .filter(|r| !r.selected)
        .map(|r| r.alpha)
        .collect();
    assert!(!side.is_empty(), "一个侧位都没有，这条测不到东西");
    for a in side {
        assert!(
            (1.0 - a) <= 0.12,
            "侧位卡让 {:.0}% 的壁纸透出来 —— 换上一张亮壁纸就又是「半透明」",
            (1.0 - a) * 100.0
        );
    }
    // 中位那张仍然是整张面。
    assert!(
        rects.iter().any(|r| r.selected && (r.alpha - 1.0).abs() < 1e-6),
        "中位卡不是满不透明"
    );
}

/// Which cart each quad in the row belongs to. No faces are uploaded, so every cart draws
/// as a rect in its own label colour, and that colour is the only identity on offer. The
/// colour comes from the cleaned stem, not the header title.
fn drawn_cart_indices(out: &[Draw]) -> Vec<usize> {
    let keys: Vec<[u8; 3]> = (0..16)
        .map(|i| label_colour(&format!("Game {i}")))
        .collect();
    out.iter()
        .map(|d| {
            let Draw::Rect { colour, .. } = d else {
                panic!("a cart with no face should draw as a rect");
            };
            let rgb = [0, 1, 2].map(|c| (colour[c] * 255.0).round() as u8);
            keys.iter()
                .position(|k| *k == rgb)
                .unwrap_or_else(|| panic!("quad {rgb:?} belongs to no cart"))
        })
        .collect()
}

#[test]
fn three_carts_fit_across_the_shelf() {
    let row = CART_W * 3;
    assert!(
        row <= OUT_W,
        "three carts are {row} px across a {OUT_W} px row, so it cannot show one either \
         side of the selection"
    );
}

#[test]
fn the_shelf_wraps_at_both_ends() {
    let mut s = shelf_with(4);
    s.left();
    assert_eq!(
        s.index, 3,
        "going left from the first cart should reach the last"
    );
    s.right();
    assert_eq!(
        s.index, 0,
        "going right from the last cart should reach the first"
    );
}

/// The spring chases `scroll`. If wrapping is a bare index change it unwinds the whole row.
#[test]
fn wrapping_animates_one_step_not_the_long_way_back() {
    let mut s = shelf_with(8);
    for _ in 0..7 {
        s.right();
    }
    settle(&mut s);
    let before = s.scroll;
    s.right();
    let travel = (s.scroll_target() - before).abs();
    assert!(
        travel < 1.5,
        "the spring is travelling {travel} slots to move one"
    );
}

#[test]
fn a_settled_wrap_still_lands_on_the_selected_cart() {
    let mut s = shelf_with(5);
    s.left();
    settle(&mut s);
    assert_eq!(s.index, 4);
    assert!(
        (s.scroll.rem_euclid(5.0) - 4.0).abs() < 0.01,
        "scroll {} did not settle",
        s.scroll
    );
}

#[test]
fn the_neighbour_of_the_last_cart_is_the_first() {
    let s = shelf_with(4);
    assert_eq!(
        s.cart_at_offset(-1),
        Some(3),
        "left of the first is the last"
    );
    assert_eq!(s.cart_at_offset(1), Some(1));
}

/// Two carts would otherwise appear on both sides of the selection at once.
#[test]
fn no_cart_is_drawn_twice_in_one_row() {
    for n in [2usize, 3, 4] {
        let s = shelf_with(n);
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        let drawn = drawn_cart_indices(&out);
        let mut uniq = drawn.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(drawn.len(), uniq.len(), "{n} carts: one is on screen twice");
    }
}

#[test]
fn shelf_scroll_settles_on_the_selected_index() {
    let mut s = shelf_with(5);
    s.right();
    s.right();
    for _ in 0..600 {
        s.update(1.0 / 60.0);
    }
    assert!(
        (s.scroll - 2.0).abs() < 0.01,
        "scroll {} did not settle",
        s.scroll
    );
}

#[test]
fn scroll_never_overshoots_the_cart_it_lands_on() {
    let mut s = shelf_with(5);
    s.right();
    for _ in 0..600 {
        s.update(1.0 / 60.0);
        assert!(s.scroll <= 1.0 + 1e-4, "overshot to {}", s.scroll);
    }
}

#[test]
fn an_empty_shelf_is_inert() {
    let mut s = shelf_with(0);
    s.right();
    s.left();
    assert_eq!(s.index, 0);
    s.update(1.0 / 60.0);
    assert!(placed(&s).is_empty());
}

/// The row draws its selection larger than the rest, so the cart in hand is the one the eye
/// lands on, and it sits dead centre.
#[test]
fn the_selected_cart_is_centred_and_drawn_as_the_hero() {
    let mut s = shelf_with(5);
    s.right();
    s.right();
    settle(&mut s);
    let (x, w) = placed(&s)
        .into_iter()
        .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    let hero = CART_W as f32 * CENTER_SCALE;
    assert!(
        (w - hero).abs() < 0.5,
        "selected cart is {w} wide, not {hero}"
    );
    let centre = x + w / 2.0;
    assert!(
        (centre - 360.0).abs() < 0.5,
        "selected cart centre is {centre}"
    );
}

#[test]
fn holding_a_direction_repeats_after_a_delay() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    assert_eq!(s.index, 1, "the first press did not move");
    s.tick(399);
    assert_eq!(s.index, 1, "it repeated before the delay");
    s.tick(400);
    assert_eq!(s.index, 2, "it never repeated");
    s.tick(510);
    assert_eq!(s.index, 3);
    s.release_right();
    s.tick(2_000);
    assert_eq!(s.index, 3, "it kept repeating after release");
}

/// Letting go of one direction while the other is held is a change of direction, not a stop.
#[test]
fn the_other_direction_letting_go_does_not_stop_the_repeat() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    s.release_left();
    s.tick(400);
    assert_eq!(s.index, 2, "releasing left stopped a held right");
}

/// The gauge takes the right end of the band at the margin the row leaves beside its outer
/// carts, so what is printed on the case lines up with what is above it. Charging, with a bolt
/// supplied: the cluster is anchored by the tip of the nub, so its outermost point is the
/// margin whatever is or is not drawn inside it.
#[test]
fn the_gauge_takes_the_right_end_of_the_band() {
    let mut out = Vec::new();
    draw_status(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(1)),
        Printed::default(),
        None,
        &mut out,
    );
    let rightmost = out
        .iter()
        .map(|d| match *d {
            Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => x + w,
            _ => 0.0,
        })
        .fold(0.0, f32::max);
    assert_eq!(
        rightmost,
        OUT_W as f32 - 24.0,
        "the case margin is the case margin"
    );
}

/// `draw_gauge`'s own suite proves the capsule holds still in isolation; the test above only
/// ever calls the band while charging, so nothing here was exercising the discharging path
/// through the call the app actually makes. This is that path, at both charge states, at the
/// same percent: everything but the bolt itself has to come back identical.
#[test]
fn the_band_does_not_move_the_gauge_when_the_charge_state_changes() {
    let mut idle = Vec::new();
    draw_status(
        Some(Battery {
            percent: 68,
            charge: Charge::Discharging,
        }),
        Printed { face: None, w: 30 },
        None,
        Printed { face: None, w: 40 },
        None,
        &mut idle,
    );
    let mut charging = Vec::new();
    draw_status(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(2)),
        Printed { face: None, w: 40 },
        None,
        &mut charging,
    );
    for d in &idle {
        assert!(
            charging.contains(d),
            "{d:?} moved or vanished when charging started"
        );
    }
}

/// The clock is the other half of the swap: it had the right-hand margin of the case and now
/// holds the left one, at the same distance in, so the two readings still face each other
/// across the band.
#[test]
fn the_clock_stays_at_the_left_margin() {
    let mut out = Vec::new();
    draw_status(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        None,
        &mut out,
    );
    let leftmost = out
        .iter()
        .map(|d| match *d {
            Draw::Rect { x, .. } | Draw::Tex { x, .. } => x,
            _ => f32::MAX,
        })
        .fold(f32::MAX, f32::min);
    assert_eq!(leftmost, 24.0, "the case margin is the case margin");
}

/// A device with no gauge shows a band with a clock on it, not a band with a hole in it.
#[test]
fn a_band_with_no_gauge_still_draws_its_clock() {
    let mut out = Vec::new();
    draw_status(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        None,
        &mut out,
    );
    assert_eq!(out.len(), 1);
}

/// The mode badge: what kind of a screen this is, printed beside what time it is.
///
/// Three things it has to do, and the first is the one that would be easy to lose. It goes
/// *after* the clock and clear of it, because a clock that gains a digit must push the badge
/// rather than be sat on by it. It waits for the clock rather than arriving first, so nothing on
/// the band jumps when the time's face lands. And it stays well inside the middle of the screen:
/// the letters are printed between the two ends, and a badge reaching the window would be
/// competing with the one reading on this row that is actually being looked at.
#[test]
fn the_mode_badge_follows_the_clock_and_keeps_out_of_the_middle() {
    let mut out = Vec::new();
    let clock_w = 40;
    draw_status(
        None,
        Printed::default(),
        None,
        Printed {
            face: None,
            w: clock_w,
        },
        Some(TexId::from_raw(7)),
        &mut out,
    );
    let badge = out
        .iter()
        .find_map(|d| match *d {
            Draw::Tex {
                x, y, w, h, tex, ..
            } if tex == TexId::from_raw(7) => Some((x, y, w, h)),
            _ => None,
        })
        .expect("the badge was not drawn");
    assert!(
        badge.0 >= 24.0 + clock_w as f32,
        "the badge overlaps the clock: {}",
        badge.0
    );
    assert!(
        badge.0 < 720.0 / 2.0 - badge.2,
        "the badge reaches the middle of the screen: {}",
        badge.0
    );
    // And it is on the band's line rather than floating above or below it: centred on the same
    // `STATUS_H` line the clock is set on, which is the line the gauge's own cluster occupies
    // too. All three share one middle, and this is the badge's end of that agreement.
    assert!(
        badge.1 > 0.0 && badge.1 + badge.3 < 58.0,
        "the badge left the band: {}..{}",
        badge.1,
        badge.1 + badge.3
    );
}

/// The two ends of the band are centred on the same line, which is the band's own middle.
///
/// Worth a test because the two are placed by different arithmetic: the clock and the badge are
/// `(TOP_BAND_H - STATUS_H) / 2`, and the gauge is centred as a cluster of its own. Nothing in
/// either expression mentions the other, so the agreement rests on `cluster_h()` being
/// `STATUS_H` — which is the one thing that keeps a change to either end from leaving the other
/// sitting high.
///
/// The cluster is one band of type now, so the highest quad at the right-hand end *is* the
/// percent and its top is the cluster's top. That is a simpler shape than the capsule-plus-number
/// this test was written against, and the assertion is the same one.
#[test]
fn both_ends_of_the_band_are_centred_on_the_same_line() {
    let mut out = Vec::new();
    draw_status(
        Some(Battery {
            percent: 68,
            charge: Charge::Discharging,
        }),
        Printed { face: None, w: 30 },
        None,
        Printed { face: None, w: 40 },
        None,
        &mut out,
    );
    // The clock's middle, from its own box.
    let clock = out
        .iter()
        .find_map(|d| match *d {
            Draw::Rect { x, y, w, .. } if (x - 24.0).abs() < 0.01 && w >= 40.0 => {
                Some(y + STATUS_H as f32 / 2.0)
            }
            Draw::Tex { x, y, w, .. } if (x - 24.0).abs() < 0.01 && w >= 40.0 => {
                Some(y + STATUS_H as f32 / 2.0)
            }
            _ => None,
        })
        .expect("the clock was not drawn at the left margin");
    // The gauge cluster's middle, from its own top edge — the highest quad at the right-hand
    // end, which with no capsule above the number is the percent itself.
    let capsule_top = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, y, .. } | Draw::Tex { x, y, .. } if x > OUT_W as f32 / 2.0 => Some(y),
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    let cluster_middle = capsule_top + cluster_h() / 2.0;
    assert!(
        (clock - cluster_middle).abs() < 0.01,
        "the clock is at {clock} and the gauge at {cluster_middle}"
    );
    assert!(
        (clock - TOP_BAND_H / 2.0).abs() < 0.01,
        "the shared line is not the band's middle: {clock}"
    );
}

/// Nothing to hang it off means nothing drawn: the badge is placed from the clock's width, so a
/// frame that arrives before the time's face would put it at the margin and then move it.
#[test]
fn the_mode_badge_waits_for_the_clock() {
    let mut out = Vec::new();
    draw_status(
        None,
        Printed::default(),
        None,
        Printed::default(),
        Some(TexId::from_raw(7)),
        &mut out,
    );
    assert!(
        !out.iter().any(|d| matches!(
            d,
            Draw::Tex {
                tex,
                ..
            } if *tex == TexId::from_raw(7)
        )),
        "the badge was drawn before the clock"
    );
}

/// The carts are what was refused. Nothing else on the screen was: the slot is part of the
/// device and the legend is printed on it, and a screen that shook wholesale would read as a
/// rendering fault rather than as a cart being rejected.
#[test]
fn a_refusal_moves_the_carts_and_leaves_the_device_where_it_is() {
    let s = shelf_with(3);
    let (mut still, mut shaken) = (Vec::new(), Vec::new());
    s.draw(0.0, &mut still);
    s.draw(9.0, &mut shaken);
    assert_eq!(still.len(), shaken.len(), "the shake changed the row");
    // The row draws first, so its quads are the leading ones. Sizes cannot tell the two
    // apart: the carts either side of the selection are drawn scaled down.
    let mut row = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut row);
    let carts = row.len();
    assert!(
        carts > 0 && carts < still.len(),
        "{carts} of {}",
        still.len()
    );
    for (i, (a, b)) in still.iter().zip(&shaken).enumerate() {
        let ((ax, _), (bx, _)) = (xw(a), xw(b));
        if i < carts {
            assert!((bx - ax - 9.0).abs() < 0.01, "a cart stood still");
        } else {
            assert_eq!(ax, bx, "the device moved with the carts");
        }
    }
}

#[test]
fn carts_past_the_edges_of_the_row_are_not_drawn() {
    let mut s = shelf_with(30);
    for _ in 0..8 {
        s.right();
    }
    settle(&mut s);
    let n = placed(&s).len();
    assert!(n > 1, "only {n} carts drawn, the neighbours should peek in");
    assert!(n <= 5, "{n} carts drawn into a 720 px row");
}

/// The row reads as continuing past the screen rather than as three equal carts side by side:
/// the neighbours are deliberately half off each edge, and the only cart wholly on screen is
/// the one in hand.
#[test]
fn the_neighbours_run_off_the_edges_and_the_selection_is_whole() {
    let s = shelf_with(5);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    let mut spans = cart_spans(&out);
    assert_eq!(
        spans.len(),
        3,
        "expected three carts on screen, got {}",
        spans.len()
    );
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert!(
        spans[0].0 < 0.0,
        "the left neighbour runs off the left edge here, not from {}",
        spans[0].0
    );
    assert!(
        spans[2].1 > OUT_W as f32,
        "the right neighbour runs off the right edge here, not to {}",
        spans[2].1
    );
    assert!(
        spans[1].0 >= 0.0 && spans[1].1 <= OUT_W as f32,
        "the selection has to be wholly on screen: {:?}",
        spans[1]
    );
}

/// The row is symmetric about the selection: whatever the neighbour on one side leaves off the
/// screen, its opposite leaves off the other, so the two sides read as the same distance out
/// rather than one crowding the middle.
#[test]
fn the_row_is_symmetric_about_the_selection() {
    let s = shelf_with(5);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    let mut spans = cart_spans(&out);
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let centre = OUT_W as f32 / 2.0;
    let left = centre - (spans[0].0 + spans[0].1) / 2.0;
    let right = (spans[2].0 + spans[2].1) / 2.0 - centre;
    assert!(
        (left - right).abs() < 0.5,
        "the left neighbour sits {left:.1} out and the right {right:.1}: the row is lopsided"
    );
}

/// Carts only. The legend shares the draw list and its plates are short, so height is what
/// separates them.
fn cart_spans(out: &[Draw]) -> Vec<(f32, f32)> {
    out.iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, w, h, .. }
            | Draw::Tex { x, w, h, .. }
            | Draw::Turned { x, w, h, .. }
            | Draw::Yaw { x, w, h, .. }
            | Draw::Glyph { x, w, h, .. }
            | Draw::Foil { x, w, h, .. } => (h > 60.0).then_some((x, x + w)),
            Draw::Game | Draw::Glow | Draw::Shot { .. } => None,
        })
        .filter(|(x0, x1)| *x1 > 0.0 && *x0 < OUT_W as f32)
        .collect()
}

/// The row makes way for the cart going into the slot: the others part outwards and are gone
/// by the time it is seated. Fading them where they stand reads as the screen dimming rather
/// than as the shelf clearing.
#[test]
fn the_row_parts_for_the_cart_going_in() {
    let s = shelf_with(5);
    let at = |recede: f32| {
        let mut out = Vec::new();
        s.draw_row(Some("Game 0"), 0.0, recede, 1.0, &mut out);
        out
    };
    let start = at(0.0);
    // Partway, not half. At this pitch the neighbours are off the screen entirely by the
    // halfway mark, so the parting has to be sampled while there is still a pair to measure.
    let part = at(0.35);
    assert_eq!(start.len(), part.len(), "a cart left the row early");

    let centre = OUT_W as f32 / 2.0;
    for (a, b) in start.iter().zip(&part) {
        let (ax, aw) = xw(a);
        let (bx, _) = xw(b);
        let side = (ax + aw / 2.0) - centre;
        assert!(
            (bx - ax).signum() == side.signum(),
            "a cart at {ax} moved to {bx}, which is towards the slot, not away from it"
        );
        assert!((bx - ax).abs() > 1.0, "the cart at {ax} did not move");
    }
    assert!(
        at(1.0).is_empty(),
        "the row is still on screen with the cart seated"
    );
}

/// Dimming darkens a side cart's face — and **only** the face: nothing is drawn under it.
///
/// ★ 2026-10-25：这条原来还断言"垫在下面那块黑跟着 recede 走"。那块黑影已经整条删掉了
/// （六六：「GBC 的阴影还没拿掉」），所以现在要钉的是**它确实不在了** ——
/// 否则哪天有人手一抖把影子加回来，没有任何测试会响。
#[test]
fn dim_darkens_a_side_carts_face_and_draws_nothing_under_it() {
    let mut s = shelf_with(3);
    let side = TexId::from_raw(11);
    s.set_faces(vec![
        Some(TexId::from_raw(10)),
        Some(side),
        Some(TexId::from_raw(12)),
    ]);
    let drawn = |dim: f32| {
        let mut out = Vec::new();
        s.draw_row(Some("Game 0"), 0.0, 0.3, dim, &mut out);
        let (x, face) = out
            .iter()
            .find_map(|d| match *d {
                Draw::Tex { x, tex, alpha, .. } if tex == side => Some((x, alpha)),
                _ => None,
            })
            .expect("the side cart is not drawn");
        let under = out
            .iter()
            .filter(|d| match **d {
                Draw::Tex { x: at, tex, .. } => at == x && tex != side,
                _ => false,
            })
            .count();
        assert_eq!(under, 0, "卡面底下不该再垫任何东西");
        face
    };
    let face = drawn(1.0);
    let dimmed = drawn(0.5);
    assert!(
        (dimmed - face * 0.5).abs() < 1e-6,
        "the face went from {face} to {dimmed} at half dim"
    );
}

/// ★★ 2026-10-25（六六：「摆动看起来还是有点奇怪，能朝着玩家晃吗（就是轴是从卡带纵向穿过的，
/// 卡带左右两侧交替往玩家方向摆）」）。
///
/// 这条钉住的是**用哪个图元**，不是角度多大 —— 角度是可以调的，而图元选错了
/// 不管调多少度都是同一个观感：
/// * `Draw::Turned` 在**屏幕平面**里转，卡带看起来像被拧了一下（就是六六说奇怪的那一版）；
/// * `Draw::Yaw` 绕**穿过卡带的竖直轴**偏转，画出梯形，近的一侧更高更宽 ——
///   读作"这卡立在桌上，正被左右转着看"。
///
/// 所以判据是：静置的中间卡必须是 `Draw::Yaw`，而且**全表里一个 `Turned` 都不能有**。
#[test]
fn the_selected_cart_turns_to_face_the_player_rather_than_spinning_flat() {
    let mut s = shelf_with(5);
    s.index = 2;
    s.set_faces((0..5).map(|i| Some(TexId::from_raw(10 + i))).collect());

    let mut out = Vec::new();
    s.draw_row_idle(None, 0.0, 0.0, 1.0, Idle { yaw: 0.1, glow: 1.0 }, &mut out);

    let yaws: Vec<f32> = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Yaw { yaw, .. } => Some(yaw),
            _ => None,
        })
        .collect();
    assert_eq!(
        yaws,
        vec![0.1],
        "只有中间那张卡绕竖轴偏转，且角度就是传进来的那个 —— 现在拿到 {yaws:?}"
    );
    assert!(
        !out.iter().any(|d| matches!(*d, Draw::Turned { .. })),
        "静置的卡不该在屏幕平面里转（那是看起来奇怪的那一版）"
    );
}

/// 上面那条的反面：**不动的时候必须是普通 `Tex`**。
///
/// `Idle::NONE` 是恒等元，插卡、退卡、选核心、静态出图全走它。若为了省事让"yaw = 0"
/// 也发 `Draw::Yaw`，那些画面的绘制列表就全变了（哪怕看起来一样）——
/// 而这条流水线是靠列表内容做断言的，看不见的变化就是回归。
#[test]
fn a_still_cart_is_a_plain_quad() {
    let mut s = shelf_with(5);
    s.index = 2;
    s.set_faces((0..5).map(|i| Some(TexId::from_raw(10 + i))).collect());

    let mut out = Vec::new();
    s.draw_row_idle(None, 0.0, 0.0, 1.0, Idle::NONE, &mut out);

    assert!(
        !out.iter()
            .any(|d| matches!(*d, Draw::Yaw { .. } | Draw::Turned { .. })),
        "Idle::NONE 下不该出现任何转过/偏转过的图元"
    );
    let faces = out
        .iter()
        .filter(|d| matches!(**d, Draw::Tex { .. }))
        .count();
    // 720 宽只放得下三张（侧-中-侧），要看的是**它们全都是普通 quad**，不是数量本身。
    assert_eq!(faces, 3, "中间那一排三张卡都该是普通 quad，现在有 {faces} 张");
}

/// ★ 2026-11-09（玩家实报）：**同名跨机种，反查要先问自己机种的货架。**
///
/// 症状原话：「加载 MD 游戏时显示的是 FC 卡带且进去也是 FC」。库里不去重
/// （`Games/FC/Contra.nes` 与 `Games/MD/Contra.md` 是两条，stem 都是 `Contra`），
/// 而全库 `position` 只会答**排序靠前**的那条 —— `order` 以路径兜底，`Games/FC/…`
/// 恒在 `Games/MD/…` 前面（`F` < `M`）。于是 MD 货架上按下 A 之后，插卡动画画的、
/// 核心跑的全成了 FC 那张卡：`insert` 只把选中的 cart 压成 stem，之后全靠名字反查。
#[test]
fn a_name_shared_across_machines_resolves_on_its_own_shelf() {
    let cart = |stem: &str, rom: &str| Cart {
        initial: 'C',
        stem: stem.into(),
        rom: rom.into(),
        label: None,
        art: None,
        code: String::new(),
        cgb: 0,
        title: stem.to_uppercase(),
    };
    // 库里的顺序就是 `order` 排出来的顺序：路径靠前的那条在前。
    let mut s = Shelf::new(vec![
        cart("Contra", "Games/FC/Contra.nes"),
        cart("Contra", "Games/MD/Contra.md"),
        cart("Metroid", "Games/GBA/Metroid.gba"),
    ]);

    // 两条同名都还在库里 —— 修的是"取对"，不是去重。
    assert_eq!(s.carts.iter().filter(|c| c.stem == "Contra").count(), 2);

    // MD 的货架上，名字答的是 MD 那张。
    s.set_system(slot_store::System::Md);
    let (found, _) = s.find("Contra").expect("MD 货架上该找得到");
    assert_eq!(
        found.system(),
        slot_store::System::Md,
        "MD 货架上按 A，反查到的必须是 MD 那张（从前会答 FC，因为 `Games/FC/…` 排在前面）"
    );

    // FC 的货架上，名字答的是 FC 那张。
    s.set_system(slot_store::System::Fc);
    let (found, _) = s.find("Contra").expect("FC 货架上该找得到");
    assert_eq!(found.system(), slot_store::System::Fc);

    // 当前机种里没这个名字时仍要退到全库 —— 换机种、退卡途中都不该落空。
    s.set_system(slot_store::System::Md);
    let (found, _) = s.find("Metroid").expect("别的机种的卡也要找得到");
    assert_eq!(found.system(), slot_store::System::Gba);
}
