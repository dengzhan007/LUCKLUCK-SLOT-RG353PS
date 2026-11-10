#!/usr/bin/env python3
"""Apply measured RG353PS ADC calibration to upstream Slot's evdev input path."""
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


# Keep upstream Stick::feed and its edge/dead-zone hysteresis. Calibrate each
# RG353PS ADC axis independently, then normalize it to the scale that Stick expects.
comment_start = source.index("/// Full-scale of an analog stick axis")
constant_start = source.index("const STICK_RANGE:", comment_start)
source = (
    source[:comment_start]
    + "/// RG353PS ADC axes: raw range 15..1023; each axis is independently centred and normalized.\n"
    + source[constant_start:]
)
replace_once("const STICK_RANGE: i32 = 4096;", "const STICK_RANGE: i32 = 504;", "stick range")
replace_once(
    "const STICK_CENTER: i32 = 0;",
    """/// Normalize one measured ADC axis to the range expected by upstream Stick::feed.
/// Centres come from the RG353PS probe: Y averages its resting readings 498/511;
/// RX averages its observed 511..523 drift.
fn calibrated_axis_value(code: u16, value: i32) -> i32 {
    let (min, max, center) = match code {
        ABS_X => (15, 1023, 527),
        ABS_Y => (15, 1023, 505),
        ABS_RX => (15, 1023, 517),
        ABS_RY => (15, 1023, 542),
        _ => return 0,
    };
    let value = value.clamp(min, max);
    if value < center {
        (value - center) * STICK_RANGE / (center - min)
    } else {
        (value - center) * STICK_RANGE / (max - center)
    }
}""",
    "per-axis ADC centres",
)

old_stick = """        let axis = match ev.code {
            ABS_X | ABS_RX => 0,
            ABS_Y | ABS_RY => 1,
            _ => return Vec::new(),
        };
        let raw = ev.value - STICK_CENTER;"""
new_stick = """        let axis = match ev.code {
            ABS_X | ABS_RX => 0,
            ABS_Y | ABS_RY => 1,
            _ => return Vec::new(),
        };
        let raw = calibrated_axis_value(ev.code, ev.value);"""
replace_once(old_stick, new_stick, "four-axis RG353PS feed")

# Keep the tested RG353PS physical key table.
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
        // 0x162 accompanies Function on this kernel; mapping it too doubles the press.
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

test_marker = "    #[test]\n    fn axis_map_identity_passes_standard_codes_through()"
new_tests = """    #[test]
    fn rg353ps_adc_calibration_uses_each_measured_centre_and_full_range() {
        for (code, center) in [
            (ABS_X, 527),
            (ABS_Y, 505),
            (ABS_RX, 517),
            (ABS_RY, 542),
        ] {
            assert_eq!(calibrated_axis_value(code, center), 0, "axis {code}");
            assert_eq!(calibrated_axis_value(code, 15), -STICK_RANGE, "axis {code} min");
            assert_eq!(calibrated_axis_value(code, 1023), STICK_RANGE, "axis {code} max");
        }
    }

    #[test]
    fn all_four_rg353ps_axes_use_the_upstream_slot_stick_directions() {
        for (code, center, edge, expected) in [
            (ABS_X, 527, 1023, RawEvent::Down(Btn::Right)),
            (ABS_X, 527, 15, RawEvent::Down(Btn::Left)),
            (ABS_Y, 505, 1023, RawEvent::Down(Btn::Down)),
            (ABS_Y, 505, 15, RawEvent::Down(Btn::Up)),
            (ABS_RX, 517, 1023, RawEvent::Down(Btn::Right)),
            (ABS_RX, 517, 15, RawEvent::Down(Btn::Left)),
            (ABS_RY, 542, 1023, RawEvent::Down(Btn::Down)),
            (ABS_RY, 542, 15, RawEvent::Down(Btn::Up)),
        ] {
            let mut stick = Stick::default();
            let events = feed(&mut stick, code, &[center, edge]);
            assert!(events.contains(&expected), "axis {code}: {events:?}");
        }
    }

"""
if source.count(test_marker) != 1:
    raise SystemExit("stick regression tests: upstream test anchor missing or duplicated")
source = source.replace(test_marker, new_tests + test_marker, 1)

for required in (
    "const STICK_RANGE: i32 = 504;",
    "fn calibrated_axis_value(code: u16, value: i32) -> i32",
    "ABS_X | ABS_RX => 0,",
    "ABS_Y | ABS_RY => 1,",
    "all_four_rg353ps_axes_use_the_upstream_slot_stick_directions",
    "0x13c => Btn::Menu, // Function",
    "116 => Btn::Power,",
    "const WANTED: [u16; 23] = [",
):
    if required not in source:
        raise SystemExit(f"postcondition failed: {required}")

path.write_text(source, encoding="utf-8")
print("RG353PS mapping applied: calibrated ABS_X/Y/RX/RY with upstream Slot Stick logic.")
