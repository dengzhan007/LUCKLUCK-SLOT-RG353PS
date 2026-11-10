mod art;
mod backdrop;
mod battery;
mod board;
mod cart;
mod cart_art;
mod cart_rules;
pub mod cd;
mod cd_art;
mod clock;
pub mod deck;
mod desktop;
mod draw;
mod hud;
mod icon;
pub mod eq;
pub mod keys;
pub mod keymap;
pub mod lang;
pub mod lcd;
pub mod letters;
pub mod light;
pub mod menu;
pub mod palette;
mod palette_browser;
mod plate;
mod polaroids;
mod power_menu;
mod refusal;
mod rounded;
mod shelf;
pub mod settings;
pub mod shutter;
mod shell;
mod shortcuts;
mod silhouette;
mod slot_chrome;
pub mod tape;
mod tape_art;
mod status;
mod sticker;
// ★ 2026-10-26（六六）：长按 Y 打开的**机种选择面板** —— 两行五列，一格一台。
pub mod sys_picker;
pub mod text;
mod toast;

pub use backdrop::{
    default_wallpaper, draw_backdrop, wallpaper_face, wallpaper_glow, wallpaper_ink,
    WALL_GLOW_PAD,
};
pub use battery::{cluster_h, draw_gauge, BOLT_H, BOLT_PX, BOLT_W};
pub use board::{
    board_at, board_face, board_zoom, chip_face, chip_shadow_face, grown, lid_at, lift_of,
    on_board, padded, rom_marking, rom_marking_face, shelf_cart, slide_of, socket_face, Placed,
    BOARD_H, BOARD_W, BOARD_X, BOARD_Y, CHIP_H, CHIP_TIP, CHIP_U, CHIP_V, CHIP_W, HOP_LIFT,
    LID_TURN, ROM_H, ROM_W, ROM_X, ROM_Y, SHADOW_H, SHADOW_W, SLIDE_SHARE, SLIDE_UP, SOCKET_H,
    SOCKET_U, SOCKET_V, SOCKET_W, TURN_PAD,
};
pub use cart::face_profile;
pub use cart_art::{
    alpha_for as cart_alpha_for, Alpha as CartAlpha, board_art,
    default_art_for as cart_default_art, defaults_summary as cart_defaults_summary,
    install as install_cart_art, install_defaults as install_default_art,
    label_share as cart_label_share, load as load_cart_art, load_defaults as load_default_art,
    summary as cart_art_summary, DefaultArt as CartDefaultArt, DefaultArtSet,
};
// The card's own per-cart rules — the editable half of the shell table, read at boot beside
// the art it can override. `for_code` is what the shelf's marks ask; the rest is the boot
// path and the log line.
pub use cart_rules::{
    for_code as cart_rule_for_code, install as install_cart_rules, load as load_cart_rules,
    marks as cart_marks, summary as cart_rules_summary,
    wants_badges as cart_rules_wants_badges, Foil, FoilKind, Row as CartRuleRow,
};
// The disc's art, the same way and for the same reasons. `tape_art::face` is what the deck calls;
// these are only so the binary can load it at boot and say what it found.
pub use tape_art::{
    face as tape_face, install as install_tape_art, load as load_tape_art, share as tape_label_share, read_sticker as tape_sticker, slot_size as tape_slot_size,
    summary as tape_art_summary,
};
// The sleeve's art, the same way and for the same reasons. **These are what the music row calls
// now**; the tape pair above is what it called until 2026-10-04 and still builds.
pub use cd_art::{
    face as cd_face, install as install_cd_art, load as load_cd_art, read_cover as cd_cover,
    share as cd_cover_share, slot_size as cd_cover_size, summary as cd_art_summary,
    summary_of as cd_art_summary_of,
};
// Re-exported rather than defined here: the rule that turns a file name into a title belongs
// with the scan, because the ring's letter is decided from it and a scan cannot ask the ui.
pub use slot_store::{clean_label, label_tags};

/// ★ 2026-10-26：键位表现在按机种分，所以「机种」这个类型也得能从这里说出口气 ——
/// 调用方一直拿的是 `slot_store::System`，这里只是开一扇门。
pub use slot_store::System;

pub use cart::{
    art_source, cart_face, cart_placeholder, label_colour, label_panel, label_text,
    set_art_source, size_for, size_for_dir, star_at, CartFace, CartSize, CART_H, CART_W, FACE_H,
    FACE_SCALE, FACE_W, LABEL_H, LABEL_W, LABEL_X, LABEL_Y,
};
pub use clock::{clock_label, hhmm, set_clock_hint_face, ClockPicker, Field};
pub use draw::{Draw, TexId, OUT_H, OUT_W};
// The entry point for reading a PNG the user dropped on the card (e.g. a screen overlay).
// Exposed so `slot` can decode `Overlay/<sys>/<sys>.png` without the rest of `art` going public.
// ★ 2026-10-26：`stretch` 也出来了 —— 机种面板那十张 120 的标识要预缩到画上去的尺寸，
// 让 GPU 少做一次 0.5 倍降采样（双线性在降采样里会啃掉细笔画）。
pub use art::{cover, decode_rgba, stretch as art_stretch};
pub use hud::{
    ff_badge, FfState, Hud, HudKind, Millis, Plate, HUD_ICON_PX, HUD_MS, PLATE_H, PLATE_Y,
    SHELF_PLATE_H, SHELF_PLATE_R,
};
pub use icon::{icon_box, icon_face, warm_symbols_font, Icon};
pub use letters::{pill_face, pill_quad, Letters};
pub use keys::{
    current, current_for, face_label, physical_for, rows as keymap_op_rows,
    rows_for as keymap_rows_for, stock as keymap_stock, target_by_name, targets_for, turbo_hz,
    BTN_NAME, PHYSICAL, TARGETS, Target, TargetKind, TURBO_HZ, TURBO_HZ_DEFAULT,
};

/// ★ 2026-10-26：「这台机种的出厂键位」。`keymap_stock` 是 Advance 读法，这一个带机种 ——
/// 名字分开是为了让调用处一眼看出自己拿的是哪一种。
pub use keys::stock_for as keymap_stock_for;
pub use keymap::{
    draw as draw_keymap, foot_face as keymap_foot_face, highlight_face as keymap_highlight_face,
    pick_top as keymap_pick_top, rate_face as keymap_rate_face, row_face as keymap_row_face,
    target_face as keymap_target_face, title_face as keymap_title_face,
    FOOT_Y as KEYMAP_FOOT_Y, PANEL_H as KEYMAP_PANEL_H, PANEL_W as KEYMAP_PANEL_W,
    PANEL_X as KEYMAP_PANEL_X, PANEL_Y as KEYMAP_PANEL_Y, PITCH as KEYMAP_PITCH,
    ROW_H as KEYMAP_ROW_H, ROW_W as KEYMAP_ROW_W, TOP as KEYMAP_TOP, VISIBLE as KEYMAP_VISIBLE,
};
pub use light::{
    breathe as light_breathe, dot_glow, gain as light_gain, hairline as light_hairline, halo,
    pool, power_line, scanlines, title_glow, AMBER, AMBER_DEEP, AMBER_DK, BREATHE_MS,
};
pub use palette::{
    chosen_ink, chosen_rim, look, look_case, look_name, looks, next_look, ridge_ink, set_look, Look, Mode,
    LOOK_COUNT,
};
pub use palette_browser::{
    cell_width as palette_cell_w, draw as draw_palette_browser, name_face as palette_name_face,
    name_height as palette_name_h, pages_of as palette_pages, rows_of as palette_rows,
    Swatch as PaletteSwatch, View as PaletteView, COLS as PALETTE_COLS, FOOT_PX as PALETTE_FOOT_PX,
    PER_PAGE as PALETTE_PER_PAGE, ROWS as PALETTE_ROWS, STOPS as PALETTE_STOPS,
};
pub use plate::{
    arrows_hint_face, arrows_hint_width, cap_width, clock_face, clock_width, dialog_line_face,
    hint_face, hint_quad, hint_row, hint_width, page_face, plate_battery_x, plate_divider_rect,
    plate_fav_x, plate_mark_y,
    shelf_title_face, title_face, word_face, word_width, Hint, UndoFace, ARROW_GAP, CAP,
    CAP_GAP, HINT_EDGE, HINT_GAP, HINT_H, PAGE_H, PAGE_MAX_W, PLATE_MARK, SHELF_TITLE_H,
    SHELF_TITLE_W, STATUS_H, TITLE_H, TITLE_W,
};
pub use polaroids::{photo_face, PhotoFace, Polaroids, DOT, LEGEND, PHOTO_H, PHOTO_W};
pub use power_menu::{
    cheat_row_face, game_row_face, game_title_face, menu_face, menu_face_at, menu_face_white,
    power_bg_face, PowerChoice, GAME_ROW_H, GAME_ROW_PAD, GAME_ROW_PX, GAME_TITLE_H,
    GAME_TITLE_W, MENU_H, MENU_PAD, MENU_PX, POWER_BG_H, POWER_BG_W,
};
pub use refusal::Refusal;
pub use shelf::{Idle, Motion, Shelf, SlotRect, CENTER_SCALE, SIDE_ALPHA};
pub use shell::{
    lookup_order_is_exact_then_family_then_default, shell_for, table_keys, Finish, Shell,
    DEFAULT_SHELL,
};
pub use shortcuts::{
    draw_row as draw_shortcut_row, hint_face as shortcut_hint_face, row_face as shortcut_row_face,
    Row as ShortcutRow, HINT as SHORTCUT_HINT, ROWS as SHORTCUT_ROWS, ROW_W as SHORTCUT_ROW_W,
};
pub use settings::{
    draw_row as draw_settings_row, value_face as settings_value_face, Row as SettingsRow,
    RowFace as SettingsRowFace, HINT_ACT as SETTINGS_HINT_ACT,
    HINT_CHANGE as SETTINGS_HINT_CHANGE, HINT_OPEN as SETTINGS_HINT_OPEN,
    HINT_PICK as SETTINGS_HINT_PICK,
};
pub use silhouette::silhouette;
pub use slot_chrome::{
    coin_angle, coin_cy, coin_fall_s, coin_in_s, coin_total_s, disc_spin_angle, draw_cabinet,
    draw_coin_insert, draw_contact_flash, draw_edge_glow, draw_disc_eject,
    draw_disc_insert, draw_disc_live, draw_empty_slot, draw_slot_inside, draw_slot_logo,
    draw_slot_shell, draw_top_band, ease, edge, fill_logo_highlight, mark_only, housing, opening,
    recess, rim, set_theme, DiscState, CABINET_RISE_S, COIN_DIA, COIN_LIFT_PX, COIN_LIFT_S,
    DISC_DIA_DEFAULT, DISC_SLOT_Y, DISC_DROP_S, DISC_SPIN_S, DISC_SPIN_RATE, DISC_SETTLE_CY,
    DISC_SPIN_START, DISC_TRAY_OUT_S, DISC_DISC_IN_S, DISC_TRAY_IN_S,
    DISC_SPIN_DOWN_S, Seat, SlotChrome, ALERT_PX, LIP_H, MOUTH_H, MOUTH_W, PULL_SHARE,
    PULL_UP, TOP_BAND_H, TOP_WIN_H, TOP_WIN_W, TOP_WIN_X, TOP_WIN_Y,
};
pub use desktop::{desktop_rgba, SLOT_OPEN_H, SLOT_OPEN_W, SLOT_OPEN_X, SLOT_OPEN_Y};
// ★ 2026-10-24（六六）：换机种的卷闸转场。`cover_at` 是时间 → 盖住比例的那一个函数，
// 二进制只负责把秒数递进来；几何与配色都在模块里。
pub use shutter::{at as shutter_at, draw as draw_shutter, glow_face, glow_face_spread, Shutter,
    GLOW_PAD, TOTAL_S as SHUTTER_TOTAL_S, UP_S as SHUTTER_UP_S, SWAP_AT_S as SHUTTER_SWAP_AT_S};
pub use status::{draw_status, Printed};
// ★ 2026-10-29（六六）：牌子上的字与条形码全部去掉，这块面就只剩「解码」与「居中画」。
// 再导出的东西因此只有这四个 —— `head_rows`/`sticker_lines`/`StickerFields`/`CREDITS` 等
// 连同 `barcode` 模块一起删了，它们唯一的用处就是往牌子上印字。
pub use sticker::{draw_sticker, draw_sticker_at, sticker_face, STICKER_H, STICKER_W};
// ★ 2026-10-26（六六）：机种选择面板。几何、按钮底、光晕、绘制都在模块里，二进制只递
// 一个「手里是哪一格、哪几台有卡」进去。
pub use sys_picker::{
    button_face as sys_picker_button, centre as sys_picker_centre, glow_face as sys_picker_glow,
    hot_of as sys_picker_hot, icon_px as sys_picker_icon_px, ink_icon as sys_picker_ink_icon,
    size_of as sys_picker_size, step as sys_picker_step, title_face as sys_picker_title_face,
    draw as draw_sys_picker,
    Cell as SysPickerCell, Faces as SysPickerFaces, Icon as SysPickerIcon, View as SysPickerView,
    BUTTON_HOT as SYS_PICKER_HOT, BUTTON as SYS_PICKER_BUTTON, COLS as SYS_PICKER_COLS,
    COUNT as SYS_PICKER_COUNT, GLOW_PAD as SYS_PICKER_GLOW_PAD, ROWS as SYS_PICKER_ROWS,
};
pub use toast::{toast_box, toast_face, toast_rect, Toast};
