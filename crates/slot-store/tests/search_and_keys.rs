//! 宿主可跑的回归测试（2026-11-11）。
//!
//! `slot` 这个 crate 的测试二进制在本机 SIGSEGV（环境，见构建笔记），所以 App 里那条
//! 「短按 B 不该残留长按状态」用**源码断言**钉住：手势层确实会发 `GbaUp(Btn::B)`，
//! 缺的从来只是 `Phase::Shelf` 的一个接收臂。文本级断言比什么都不测强，而且它正是
//! 这次 bug 的形状 —— 一行漏写，编译照样过、肉眼也看不出来。

use std::fs;
use std::path::PathBuf;

fn app_source() -> String {
    let p: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "slot", "src", "app.rs"]
        .iter()
        .collect();
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

#[test]
fn shelf_consumes_the_b_release() {
    let src = app_source();
    assert!(
        src.contains("Action::GbaDown(Btn::B) => self.mtp_held = Some(now),"),
        "the B press arm moved; update this test instead of deleting it"
    );
    assert!(
        src.contains("Action::GbaUp(Btn::B) => {\n                    self.mtp_held = None;\n                }"),
        "Phase::Shelf must consume the B release, or a tap pops the direct-link dialog 500ms later"
    );
    assert!(
        src.contains("Action::ColorCycle if self.row == ShelfRow::Games => self.open_search(),"),
        "SELECT+Y on the shelf is the search trigger"
    );
    // The search overlay has to intercept keys *before* the phase match, or the shelf
    // underneath would move while the grid is up.
    let intercept = src
        .find("if self.search.is_some() {\n            return self.search_input(action);")
        .unwrap();
    let phase = src
        .find("        match self.phase {\n            Phase::Shelf => match action {")
        .unwrap();
    assert!(intercept < phase, "search must be ahead of the phase dispatch");
}

#[test]
fn mixed_matching() {
    use slot_store::search::{initials, matches};
    assert_eq!(initials("恶魔城晓月MOD版"), "emcxymodb");
    assert!(matches("emcx", "恶魔城晓月MOD版 [中]"));
    assert!(matches("MOD", "恶魔城晓月MOD版 [中]"));
    assert!(matches("bkm", "宝可梦-红宝石"));
    assert!(!matches("zzz", "宝可梦-红宝石"));
    assert!(matches("", "任何名字"));
}
