#!/usr/bin/env python3
"""Apply only the measured RG353PS input mapping to the current upstream evdev code."""
from pathlib import Path
import sys

if len(sys.argv) != 2:
    raise SystemExit("usage: apply-rg353ps-input.py path/to/evdev.rs")

path = Path(sys.argv[1])
source = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global source
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one upstream anchor, found {count}")
    source = source.replace(old, new, 1)


# The RG353PS ADC stick reports 15..1023, resting at 519.
comment_start = source.index("/// Full-scale of an analog stick axis")
constant_start = source.index("const STICK_RANGE:", comment_start)
source = (
    source[:comment_start]
    + "/// RG353PS ADC stick: measured range 15..1023, centre at 519.\n"
    + source[constant_start:]
)
replace_once("const STICK_RANGE: i32 = 4096;", "const STICK_RANGE: i32 = 504;", "stick range")
replace_once("const STICK_CENTER: i32 = 0;", "const STICK_CENTER: i32 = 519;", "stick centre")

old_stick = """        let axis = match ev.code {
            ABS_X | ABS_RX => 0,
            ABS_Y | ABS_RY => 1,
            _ => return Vec::new(),
        };
        let raw = ev.value - STICK_CENTER;"""
new_stick = """        // The RG353PS left ADC stick is used for menu navigation. Keep the
        // right axes independent rather than treating them as a second menu stick.
        let axis = match ev.code {
            ABS_X => 0,
            ABS_Y => 1,
            ABS_RX | ABS_RY => return Vec::new(),
            _ => return Vec::new(),
        };
        // The panel's vertical ADC polarity is opposite to the menu's Y direction.
        let polarity = if axis == 1 { -1 } else { 1 };
        let raw = (ev.value.clamp(15, 1023) - STICK_CENTER) * polarity;"""
replace_once(old_stick, new_stick, "RG353PS stick feed")

# Replace only the device-specific key table, retaining upstream's event decoder,
# AxisMap, node probing and tests around it.
fn_start = source.index("pub fn code_to_btn(code: u16) -> Option<Btn> {")
fn_marker = "\n/// Every key code the frontend has a use for."
fn_end = source.index(fn_marker, fn_start)
new_function = """pub fn code_to_btn(code: u16) -> Option<Btn> {
    Some(match code {
        // RG353PS key matrix: South=B, East=A, North=X, West=Y.
        0x130 => Btn::B,
        0x131 => Btn::A,
        0x133 => Btn::X,
        0x134 => Btn::Y,
        0x136 => Btn::L1,
        0x137 => Btn::R1,
        0x138 => Btn::L2,
        0x139 => Btn::R2,
        0x13a => Btn::Select,
        0x13b => Btn::Start,
        0x13c => Btn::Menu, // Function
        // The D-pad is reported as gpio-keys; retain keyboard-arrow fallbacks.
        0x220 => Btn::Up,
        0x221 => Btn::Down,
        0x222 => Btn::Left,
        0x223 => Btn::Right,
        103 => Btn::Up,
        108 => Btn::Down,
        105 => Btn::Left,
        106 => Btn::Right,
        115 => Btn::VolUp,
        114 => Btn::VolDown,
        116 => Btn::Power,
        // 0x162 accompanies Function on this kernel; selecting the node is useful,
        // but mapping this second event would turn one press into a double press.
        _ => return None,
    })
}"""
source = source[:fn_start] + new_function + source[fn_end:]

wanted_start = source.index("const WANTED:")
wanted_end = source.index("];", wanted_start) + 2
new_wanted = """const WANTED: [u16; 23] = [
    0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x138, 0x139, 0x13a, 0x13b, 0x13c,
    0x220, 0x221, 0x222, 0x223, 103, 108, 105, 106, 115, 114, 0x162, 116,
];"""
source = source[:wanted_start] + new_wanted + source[wanted_end:]

for required in (
    "const STICK_RANGE: i32 = 504;",
    "const STICK_CENTER: i32 = 519;",
    "0x130 => Btn::B,",
    "0x13c => Btn::Menu, // Function",
    "116 => Btn::Power,",
    "const WANTED: [u16; 23] = [",
):
    if required not in source:
        raise SystemExit(f"postcondition failed: {required}")

path.write_text(source, encoding="utf-8")
print("RG353PS input mapping applied: ADC centre=519, key matrix + power node.")
