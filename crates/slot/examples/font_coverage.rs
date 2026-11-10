//! 一张字体**到底覆盖了多少汉字** —— 逐码位点名，不是看它宣传什么。
//!
//! 为什么要有这个：`font_size_audit` 只报"这一行样本里缺不缺字"，看不出总量。
//! 而"点阵字体中文字库不全"是这类字体的通病，**全到什么程度**必须点名数：
//! 常用字（一级 3500）够不够？GB2312（6763）够不够？还是只做了最常用的那两千个？
//!
//! 数法：把码位逐段点名，用 `lookup_glyph_index(ch) != 0` 判有无 ——
//! 与项目自己选面的判据同一个（见 `slot_ui::text` 里 `carries` 的注释）。
//!
//! ⚠️ **量的是"装进 `set_font` 的那一张"**，所以一次只量一张；报的也是它自己的数，
//! 不含内嵌兜底面（`missing_in` 会把两者合起来看，不能用来量覆盖率）。
//!
//! ⚠️ **一次只喂一张字体。** `slot_ui::text::set_font` 是 `OnceLock::set` —— **只有第一次
//! 生效**，第二次起静默失败，于是"量三张字体"会得到三组**完全一样**的数（我第一次就
//! 这么被骗了：六张字体报出同一个 18299）。要量多张，**分开跑多次**。
//!
//! 跑法（**从工作区根**）：
//!
//! ```text
//! cargo run --release --manifest-path Cargo.toml --target-dir target-host \
//!   -p slot --example font_coverage -- <字体.ttf>
//! ```

/// 要点名的区段：`(名字, 起, 止)`。区间是**闭区间**。
const BLOCKS: [(&str, u32, u32); 5] = [
    ("CJK 基本区 U+4E00..9FFF", 0x4E00, 0x9FFF),
    ("CJK 扩展A U+3400..4DBF", 0x3400, 0x4DBF),
    ("CJK 兼容 U+F900..FAFF", 0xF900, 0xFAFF),
    ("CJK 标点 U+3000..303F", 0x3000, 0x303F),
    ("全角/半角 U+FF00..FFEF", 0xFF00, 0xFFEF),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(font_path) = args.first() else {
        eprintln!("usage: font_coverage <font.ttf> [corpus.txt]");
        std::process::exit(2);
    };
    // ★ **可选第二参：一份"真实用字表"。** 覆盖率的百分比是"占 Unicode 那个区段的多少"，
    //   而真正要回答的是"**能不能排我要读的那本书**"——那要用真书的用字去点名。
    //   语料按 UTF-8 读，去重，只留汉字。
    let corpus: Option<Vec<char>> = args.get(1).map(|p| {
        let text = std::fs::read_to_string(p).unwrap_or_default();
        let mut v: Vec<char> = text
            .chars()
            .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    });

    println!("########## {font_path}");
    let Ok(bytes) = std::fs::read(font_path) else {
        println!("  读不了");
        return;
    };
    let n = bytes.len();
    if !slot_ui::text::set_font(bytes) {
        println!("  字体解析失败：本次覆盖率测试终止（不会统计内嵌 fallback）");
        return;
    }
    let Some(font) = slot_ui::text::label_font() else {
        println!("  没有字体");
        return;
    };
    println!("  文件 {n} 字节（已由 fontdue 成功解析）");
    for (name, lo, hi) in BLOCKS {
        let total = hi - lo + 1;
        let mut have = 0usize;
        for cp in lo..=hi {
            if let Some(ch) = char::from_u32(cp) {
                if font.lookup_glyph_index(ch) != 0 {
                    have += 1;
                }
            }
        }
        println!(
            "  {name:<26} {have:>6} / {total:>6}  {:>5.1}%",
            have as f32 / total as f32 * 100.0
        );
    }
    if let Some(chars) = &corpus {
        let missing: String = chars
            .iter()
            .copied()
            .filter(|ch| font.lookup_glyph_index(*ch) == 0)
            .collect();
        let have = chars.len() - missing.chars().count();
        println!(
            "  ★ 真实用字表 {:<14} {have:>6} / {:>6}  {:>5.1}%   缺 {} 个",
            format!("({} 字)", chars.len()),
            chars.len(),
            have as f32 / chars.len() as f32 * 100.0,
            missing.chars().count()
        );
        if !missing.is_empty() && missing.chars().count() <= 80 {
            println!("    缺的是：{missing}");
        }
    }
    println!();
}
