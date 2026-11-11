//! ★ 2026-11-11（六六）：货架的**首字母搜索**匹配器。
//!
//! 放在 `slot-store` 而不是 UI，是因为它只认字符串，不认像素、纹理和按键 —— 5120 张卡
//! 逐键重扫也在微秒级，宿主还能直接跑真测试（本 crate 零依赖）。键盘本身是 Wi-Fi 配置
//! 浮层那张四层全键盘（`app::WIFI_LAYERS`），这里只回答「这串字符命中这张卡吗」。
//!
//! **两条路，任一命中即可**：
//!   ① 原文子串 —— 英文 ROM 名、数字、假名，`1080`、`mario` 都靠它；
//!   ② 拼音首字母串 —— 中文名 `超级机器人大战A` → `cjjqrdzza`，打 `cjjq` 就能找到。
//!
//! 名字一律先过 `clean_label_stripped`：字母轮、卡面标题、这里读的是同一份串，
//! 否则 `[中]` 之类看得见却搜不到。

use crate::{name, pinyin};

/// 把一个显示名压成可匹配的首字母串（全小写 ASCII）。
///
/// - ASCII 字母/数字原样留下（`Advance Wars 2` → `advancewars2`）；
/// - 汉字取拼音首字母（`超` → `c`）；
/// - 空格、标点、表里没有的字：跳过 —— 它们仍能被①原文子串那一路吃到最后。
///
/// ⚠️ 表是**一字一音**，`重 / 长 / 乐 / 行` 只按常用音收录，另一种读音搜不到；
/// 要彻底修得新增多音字表（按素材铁律先落工作区），不属于这一轮。
pub fn initials(display: &str) -> String {
    let mut out = String::with_capacity(display.len());
    for c in display.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii() {
            // punctuation and spacing: deliberately skipped
        } else {
            let l = pinyin::letter_of(c);
            if l != '#' {
                out.push(l.to_ascii_lowercase());
            }
        }
    }
    out
}

/// 网格打出来的 `query` 是否命中这张卡。`query` 里的空白不算字符（键盘上的空格
/// 只是分隔手感），大小写不分 —— 网格有 a–z / A–Z 两层，搜游戏不需要区分。
pub fn matches(query: &str, display: &str) -> bool {
    let q: String = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if q.is_empty() {
        return true;
    }
    let bare = name::clean_label_stripped(display);
    let lower: String = bare.chars().flat_map(char::to_lowercase).collect();
    lower.contains(&q) || initials(&bare).contains(&q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinyin_initials_walk_every_character() {
        assert_eq!(initials("超级机器人大战A"), "cjjqrdza");
        assert_eq!(initials("宝可梦 红宝石"), "bkmhbs");
        assert_eq!(initials("Advance Wars 2"), "advancewars2");
        assert_eq!(initials("1080 Snowboarding"), "1080snowboarding");
    }

    #[test]
    fn both_roads_match() {
        assert!(matches("cjjq", "超级机器人大战A"), "pinyin initials");
        assert!(matches("CJJQ", "超级机器人大战A"), "case insensitive");
        assert!(matches("1080", "1080 Snowboarding"), "literal digits");
        assert!(matches("mario", "Super Mario 64 (USA)"), "literal substring");
        assert!(matches("wars", "Advance Wars (USA) (Rev 1)"), "tags stripped");
        assert!(matches("", "anything"), "an empty query keeps the whole view");
        assert!(!matches("zzzz", "超级机器人大战A"));
        assert!(!matches("xyz", "1080 Snowboarding"));
    }
}
