#!/usr/bin/env python3
"""Apply narrow RG353PS compatibility fixes to the pinned upstream slotnet.sh."""
from pathlib import Path
import sys


def replace_once(source: str, old: str, new: str, label: str) -> str:
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"unexpected upstream slotnet.sh: {label} anchor count={count}")
    return source.replace(old, new)


if len(sys.argv) != 2:
    raise SystemExit("usage: patch-slotnet-rg353ps.py PATH")
path = Path(sys.argv[1])
source = path.read_text(encoding="utf-8")

# Trace only requested network operations. AGS_ROOT is set by the BaseOS
# launcher to the active userdata card, whose Logs directory already exists.
source = replace_once(
    source,
    'TRACE="
    'TRACE="$AGS_NET_TRACE"\n[ -n "$TRACE" ] || TRACE="$ROOT/Logs/slotnet.log"',
    "trace path",
)

# This minimal BaseOS rootfs has no standalone rfkill executable. Use the
# kernel's rfkill sysfs ABI as a fallback rather than silently skipping unblock.
old_rfkill = '\trfkill unblock wifi 2>/dev/null || true\n\t$IP link set "$IFACE" up 2>/dev/null || true'
new_rfkill = """\t# BaseOS's minimal rootfs may not include the rfkill executable.
\t# Keep the upstream command when available, then use the kernel ABI directly.
\tif command -v rfkill >/dev/null 2>&1; then
\t\trfkill unblock wifi >/dev/null 2>&1 || true
\tfi
\trfkill_wait=0
\twhile [ "$rfkill_wait" -lt 10 ]; do
\t\trfkill_seen=0
\t\tfor rfkill_dev in /sys/class/rfkill/rfkill*; do
\t\t\t[ -r "$rfkill_dev/type" ] || continue
\t\t\tIFS= read -r rfkill_type < "$rfkill_dev/type" || continue
\t\t\t[ "$rfkill_type" = wlan ] || continue
\t\t\trfkill_seen=1
\t\t\tif [ -w "$rfkill_dev/state" ]; then
\t\t\t\tprintf '1\\n' > "$rfkill_dev/state" 2>/dev/null || true
\t\t\tfi
\t\t\trfkill_state=$(cat "$rfkill_dev/state" 2>/dev/null || echo '?')
\t\t\ttrace "rfkill $rfkill_dev state=$rfkill_state"
\t\tdone
\t\t[ "$rfkill_seen" -eq 1 ] && break
\t\tsleep 0.1 2>/dev/null || sleep 1
\t\trfkill_wait=$((rfkill_wait + 1))
\tdone
\t$IP link set "$IFACE" up 2>/dev/null || true"""
source = replace_once(source, old_rfkill, new_rfkill, "rfkill")

# The old script ignored a rejected scan and showed an empty picker. Report
# whether supplicant readiness or scan request acceptance failed instead.
old_scan = '\t$WPA_CLI -p "$CTRL_DIR" -i "$IFACE" scan >/dev/null 2>&1\n'
new_scan = """\t# -B backgrounds wpa_supplicant; wait for its control socket before
\t# requesting a scan, otherwise the request can be silently lost.
\tif [ ! -S "$CTRL_DIR/$IFACE" ]; then
\t\ttrace "scan: control socket missing after wait"
\t\tscan_cleanup
\t\treturn 1
\tfi
\ttrace "scan: control socket ready; requesting scan"
\tSCAN_REPLY=$($WPA_CLI -p "$CTRL_DIR" -i "$IFACE" scan 2>&1) || {
\t\ttrace "scan: request command failed: $SCAN_REPLY"
\t\tscan_cleanup
\t\treturn 1
\t}
\tcase "$SCAN_REPLY" in
\tOK|OK*) trace "scan: request accepted" ;;
\t*) trace "scan: request rejected: $SCAN_REPLY"; scan_cleanup; return 1 ;;
\tesac
"""
source = replace_once(source, old_scan, new_scan, "scan request")

old_results = '\t/bin/rm -f "$scan_file"\n\tscan_cleanup\n\ttrace "scan: done"'
new_results = '\ttrace "scan: result rows=$(($(wc -l < "$scan_file" 2>/dev/null || echo 0) - 1))"\n' + old_results
source = replace_once(source, old_results, new_results, "scan result")

path.write_text(source, encoding="utf-8")
print("Applied RG353PS rfkill sysfs fallback and scan-request diagnostics")
 + '{AGS_NET_TRACE:-}"',
    'TRACE="
    'TRACE="$AGS_NET_TRACE"\n[ -n "$TRACE" ] || TRACE="$ROOT/Logs/slotnet.log"',
    "trace path",
)

# This minimal BaseOS rootfs has no standalone rfkill executable. Use the
# kernel's rfkill sysfs ABI as a fallback rather than silently skipping unblock.
old_rfkill = '\trfkill unblock wifi 2>/dev/null || true\n\t$IP link set "$IFACE" up 2>/dev/null || true'
new_rfkill = """\t# BaseOS's minimal rootfs may not include the rfkill executable.
\t# Keep the upstream command when available, then use the kernel ABI directly.
\tif command -v rfkill >/dev/null 2>&1; then
\t\trfkill unblock wifi >/dev/null 2>&1 || true
\tfi
\trfkill_wait=0
\twhile [ "$rfkill_wait" -lt 10 ]; do
\t\trfkill_seen=0
\t\tfor rfkill_dev in /sys/class/rfkill/rfkill*; do
\t\t\t[ -r "$rfkill_dev/type" ] || continue
\t\t\tIFS= read -r rfkill_type < "$rfkill_dev/type" || continue
\t\t\t[ "$rfkill_type" = wlan ] || continue
\t\t\trfkill_seen=1
\t\t\tif [ -w "$rfkill_dev/state" ]; then
\t\t\t\tprintf '1\\n' > "$rfkill_dev/state" 2>/dev/null || true
\t\t\tfi
\t\t\trfkill_state=$(cat "$rfkill_dev/state" 2>/dev/null || echo '?')
\t\t\ttrace "rfkill $rfkill_dev state=$rfkill_state"
\t\tdone
\t\t[ "$rfkill_seen" -eq 1 ] && break
\t\tsleep 0.1 2>/dev/null || sleep 1
\t\trfkill_wait=$((rfkill_wait + 1))
\tdone
\t$IP link set "$IFACE" up 2>/dev/null || true"""
source = replace_once(source, old_rfkill, new_rfkill, "rfkill")

# The old script ignored a rejected scan and showed an empty picker. Report
# whether supplicant readiness or scan request acceptance failed instead.
old_scan = '\t$WPA_CLI -p "$CTRL_DIR" -i "$IFACE" scan >/dev/null 2>&1\n'
new_scan = """\t# -B backgrounds wpa_supplicant; wait for its control socket before
\t# requesting a scan, otherwise the request can be silently lost.
\tif [ ! -S "$CTRL_DIR/$IFACE" ]; then
\t\ttrace "scan: control socket missing after wait"
\t\tscan_cleanup
\t\treturn 1
\tfi
\ttrace "scan: control socket ready; requesting scan"
\tSCAN_REPLY=$($WPA_CLI -p "$CTRL_DIR" -i "$IFACE" scan 2>&1) || {
\t\ttrace "scan: request command failed: $SCAN_REPLY"
\t\tscan_cleanup
\t\treturn 1
\t}
\tcase "$SCAN_REPLY" in
\tOK|OK*) trace "scan: request accepted" ;;
\t*) trace "scan: request rejected: $SCAN_REPLY"; scan_cleanup; return 1 ;;
\tesac
"""
source = replace_once(source, old_scan, new_scan, "scan request")

old_results = '\t/bin/rm -f "$scan_file"\n\tscan_cleanup\n\ttrace "scan: done"'
new_results = '\ttrace "scan: result rows=$(($(wc -l < "$scan_file" 2>/dev/null || echo 0) - 1))"\n' + old_results
source = replace_once(source, old_results, new_results, "scan result")

path.write_text(source, encoding="utf-8")
print("Applied RG353PS rfkill sysfs fallback and scan-request diagnostics")
 + '{AGS_NET_TRACE:-$ROOT/Logs/slotnet.log}"',
    'TRACE="$AGS_NET_TRACE"\n[ -n "$TRACE" ] || TRACE="$ROOT/Logs/slotnet.log"',
    "trace path",
)

# This minimal BaseOS rootfs has no standalone rfkill executable. Use the
# kernel's rfkill sysfs ABI as a fallback rather than silently skipping unblock.
old_rfkill = '\trfkill unblock wifi 2>/dev/null || true\n\t$IP link set "$IFACE" up 2>/dev/null || true'
new_rfkill = """\t# BaseOS's minimal rootfs may not include the rfkill executable.
\t# Keep the upstream command when available, then use the kernel ABI directly.
\tif command -v rfkill >/dev/null 2>&1; then
\t\trfkill unblock wifi >/dev/null 2>&1 || true
\tfi
\trfkill_wait=0
\twhile [ "$rfkill_wait" -lt 10 ]; do
\t\trfkill_seen=0
\t\tfor rfkill_dev in /sys/class/rfkill/rfkill*; do
\t\t\t[ -r "$rfkill_dev/type" ] || continue
\t\t\tIFS= read -r rfkill_type < "$rfkill_dev/type" || continue
\t\t\t[ "$rfkill_type" = wlan ] || continue
\t\t\trfkill_seen=1
\t\t\tif [ -w "$rfkill_dev/state" ]; then
\t\t\t\tprintf '1\\n' > "$rfkill_dev/state" 2>/dev/null || true
\t\t\tfi
\t\t\trfkill_state=$(cat "$rfkill_dev/state" 2>/dev/null || echo '?')
\t\t\ttrace "rfkill $rfkill_dev state=$rfkill_state"
\t\tdone
\t\t[ "$rfkill_seen" -eq 1 ] && break
\t\tsleep 0.1 2>/dev/null || sleep 1
\t\trfkill_wait=$((rfkill_wait + 1))
\tdone
\t$IP link set "$IFACE" up 2>/dev/null || true"""
source = replace_once(source, old_rfkill, new_rfkill, "rfkill")

# The old script ignored a rejected scan and showed an empty picker. Report
# whether supplicant readiness or scan request acceptance failed instead.
old_scan = '\t$WPA_CLI -p "$CTRL_DIR" -i "$IFACE" scan >/dev/null 2>&1\n'
new_scan = """\t# -B backgrounds wpa_supplicant; wait for its control socket before
\t# requesting a scan, otherwise the request can be silently lost.
\tif [ ! -S "$CTRL_DIR/$IFACE" ]; then
\t\ttrace "scan: control socket missing after wait"
\t\tscan_cleanup
\t\treturn 1
\tfi
\ttrace "scan: control socket ready; requesting scan"
\tSCAN_REPLY=$($WPA_CLI -p "$CTRL_DIR" -i "$IFACE" scan 2>&1) || {
\t\ttrace "scan: request command failed: $SCAN_REPLY"
\t\tscan_cleanup
\t\treturn 1
\t}
\tcase "$SCAN_REPLY" in
\tOK|OK*) trace "scan: request accepted" ;;
\t*) trace "scan: request rejected: $SCAN_REPLY"; scan_cleanup; return 1 ;;
\tesac
"""
source = replace_once(source, old_scan, new_scan, "scan request")

old_results = '\t/bin/rm -f "$scan_file"\n\tscan_cleanup\n\ttrace "scan: done"'
new_results = '\ttrace "scan: result rows=$(($(wc -l < "$scan_file" 2>/dev/null || echo 0) - 1))"\n' + old_results
source = replace_once(source, old_results, new_results, "scan result")

path.write_text(source, encoding="utf-8")
print("Applied RG353PS rfkill sysfs fallback and scan-request diagnostics")
