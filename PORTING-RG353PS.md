# RG353PS / LUCKLUCK SLOT native 640×480 port

Status: RG353PS hardware-validated Multi-Frontend integration. The two-profile persistence, selector/default behavior, save-state isolation, shared data, uninstall and reinstall lifecycle have passed on-device. Exhaustive validation of every emulator core and game remains separate.

## Source and branch

- This branch: `port/rg353ps-640-native`
- Logical-layout source: upstream `RetroGame66/LUCKLUCK-SLOT`, branch `66-mods-640`
- Upstream source commit: `4a0b60189fb4e29e57e73de14d997260a2e9d3a1`
- Device: Anbernic RG353PS, Rockchip RK3566, native 640×480 DSI panel
- OS: `dengzhan007/BaseOS-RG353PS`
- Build ABI: AArch64 / musl

## Native 640 layout

This port starts from the full upstream `66-mods-640` tree. It does not derive from the 720 branch and does not merely change two constants.

- `crates/slot-gfx/src/surface.rs`: compile-time canvas is `OUT_W=640`, `OUT_H=480`.
- `crates/slot-gfx/src/system.rs`: per-system drawing rectangles use the 640-specific definitions.
- `slot-ui` layouts, mask geometry, board and selector coordinates are inherited from the 640 branch.
- The 720 branch remains a separate upstream design; do not merge its 720 layout files into this branch.

## RG353PS device backend

The upstream device backend in `crates/slot-gfx/src/fbdev.rs` is replaced at build time with the BaseOS adapter from:

`BaseOS-RG353PS/platforms/rg353ps/frontends/slot/fbdev.rs`

Despite the legacy filename, this adapter uses SDL2 KMSDRM and OpenGL ES 2.0 through Panfrost. It is not the H700 proprietary Mali/EGL fbdev implementation. The adapter keeps the `FbdevSurface` interface consumed by `device_app.rs`, so the upstream 640 compositor is unchanged.

The distribution is a BaseOS managed package, not a legacy `manifest.conf` adapter. It is installed under
`Frontends/slot-rg353ps/` and contains:
- `baseos-frontend.conf` with `package_api=1` (package-wide metadata and ownership).
- `profiles/640-native.conf` and `profiles/640-native-alt.conf`, each with `profile_api=1`.
- One `launch.sh` and one shared `payload/` containing the RG353PS Slot binary and UI resources.

Managed package/profile support requires RG353PS BaseOS **1.2.10 or newer**. The IDs are stable:
`slot-rg353ps` is the package ID and `640-native` / `640-native-alt` are profile IDs.
BaseOS combines them into independent runtime identities such as
`slot-rg353ps--640-native`. Metadata files are plain `key=value` data; never source
or evaluate them as shell scripts.

BaseOS starts the package `launch.sh` from its package directory and exports
`BASEOS_PACKAGE_DIR`, `BASEOS_PACKAGE_ID`, `BASEOS_PROFILE_ID`,
`BASEOS_PROFILE_CONFIG`, `BASEOS_INSTANCE_DIR`, `BASEOS_DATA_DIR`,
`BASEOS_USERDATA_DIR`, `BASEOS_CARD_ROOT` and `BASEOS_GAMES_DIR`.
The launcher must use `BASEOS_PACKAGE_DIR` for the shared program payload,
`BASEOS_CARD_ROOT` for shared card data and `BASEOS_INSTANCE_DIR` for
profile-owned persistent state. BaseOS remains the owner of sleep service,
power/volume ownership contract and USB gadget services.

### Multi-Frontend data contract

The package has one executable/payload shared by both profiles. Do not clone or
rebuild the binary for each profile. The RG353PS Slot adapter enforces these
ownership boundaries:

| Resource | Storage | Sharing |
|---|---|---|
| ROM library | TF1 `Games/` | Shared across profiles/frontends |
| Battery saves | TF1 `Saves/` | Shared across profiles/frontends |
| Emulator core files | Shared package payload or BaseOS shared-core location | One copy visible to both profiles |
| Favorites, time zone and Slot preferences | `<instance>/userdata/slot/` via runtime `.userdata/` | Per profile |
| Per-game core choices | `.userdata/selected_core.ini` | Per profile |
| Mutable visual/input settings | `<instance>/system/` mounted into payload `System/` | Per profile |
| Save-state and resume snapshots | `<instance>/states/` mounted as `States/` | Per profile |

Slot uses atomic temp-file-plus-rename writes for `.userdata/slot.state`,
`.userdata/library.cache` and `.userdata/selected_core.ini`. Bind-mount the
whole profile `.userdata/` directory; do not single-file bind-mount those
atomic-write targets. Legacy `System/slot.state`, `System/library.cache`
and `System/selected_core.ini` remain read fallbacks for migration.

Uninstalling a profile removes only its profile metadata. Uninstalling the
package removes `Frontends/slot-rg353ps/`. Neither operation deletes
`.baseos/instances/`, shared `Games/`, `Saves/` or shared core data.
Reinstalling the same package ID and profile IDs must reuse the same instance
directories. Do not change IDs casually: changing an ID creates a new profile
identity and therefore a new persistent data location.

The boot selector opens when L1 or R1 is held at power-on (Function-triggered
USB storage has priority). A launches a profile once; START persists it as the
default and launches it; B continues with the saved default; Y removes the
selected profile; X uninstalls the package. The selector displays the current
default at the top and marks the matching row `[DEFAULT]`; `AUTO` means
priority-based discovery. Temporary A launches never modify the default.

## Preserved RG353PS input and BaseOS integration

`packaging/rg353ps/apply-rg353ps-input.py` applies checked, narrowly scoped patches to upstream `evdev.rs`:

- All four measured ADC axes are enabled: ABS_X/ABS_RX share the horizontal direction and ABS_Y/ABS_RY share the vertical direction, matching upstream Slot Stick behavior.
- Raw range is clamped to 15–1023 and independently normalized per axis using measured centres: X=527, Y=505 (midpoint of resting readings 498/511), RX=517 (midpoint of observed 511–523), RY=542. Upstream edge/dead-zone hysteresis is retained. Both vertical ADC axes are inverted to preserve the RG353PS physical direction convention (positive raw Y means Up, while upstream Slot's positive menu Y means Down).
- Physical face/shoulder/Function/D-pad mapping is retained from the tested RG353PS adapter.
- Linux KEY_POWER (116) remains included in the event-node capability filter.

The upstream 640 branch removed the card-side MTP launch hook. This port restores `slot::mtp::start(&root)` and `slot/src/mtp.rs`. It only starts the existing `System/mtp-auto.sh` helper in the background; BaseOS continues to own USB gadget setup and mass-storage/MTP services. No MTP responder or USB debug service is newly enabled by the frontend package. The installer never deletes existing `System/mtp-*` scripts, emulator cores or user data.

The GBA picker has two physical socket positions. The existing eight-core `Core::ALL` remains for persistence/artwork; the picker uses a dedicated `Core::GBA_PICKER=[Mgba,Gpsp]` list and regression test so the two UI sockets cannot index eight choices.


## RG353PS power and Wi-Fi integration

The BaseOS launcher exports BASEOS_SLEEP=/usr/sbin/rg353ps-sleep. Slot flushes resume state on a user power tap. When the music decoder is actively playing, a short power tap is **display-off playback**, not system suspend: Slot uses Power::on_close() to blank the LCD, leaves the audio device and decoder running, and does not start the BaseOS sleep helper. The next short power tap restores the LCD and returns to the music player. For all other doze states, Slot starts the native helper. In that native-suspend path, **BaseOS alone owns physical backlight blanking and restoration**: the helper captures the current sysfs brightness before setting it to zero and restores that value after resume. Slot must not call Power::on_close() before spawning the helper (that would make the captured restore value zero), and must not call Power::on_open() after native resume (the frontend's remembered level can still be its default zero). If the helper cannot be spawned, Slot falls back to its generic LidPolicy backlight handling and timeout instead of remaining dark indefinitely. Long-press shutdown remains a separate poweroff path.

Wi-Fi follows the upstream BaseOS ownership model:

- BaseOS initializes the RG353PS radio asynchronously during boot, waits for wlan0, unblocks rfkill and raises the link. It does not connect to an access point.
- Slot uses the upstream card-side System/slotnet.sh and System/wifi.conf format. The frontend script owns scan, saved credentials, wpa_supplicant, DHCP and the /tmp/slot-ip result.
- The package imports System/slotnet.sh from the pinned upstream release rather than translating Slot calls into a BaseOS-specific connection manager.
- The deep-suspend helper asks the card-side script to take the network down before unbinding the SDIO device and brings it back after rebind when it was active.

This matches upstream responsibility boundaries: the OS makes the radio available; the frontend owns the connection. Hardware association, DHCP, route reachability and suspend/wake still require on-device acceptance.

BASEOS_SLEEP is documented in BaseOS-RG353PS/platforms/rg353ps/POWER.md and FRONTENDS.md. The RG353PS packaging workflow validates the shell scripts and calibrated four-axis input patch; CI does not replace hardware validation.

## Chinese / English build policy

The source supports two distinct compile-time UI variants:

- Chinese test build (default): `--no-default-features --features device`
- English release build: `--no-default-features --features device,lang-en`

The push / manual test workflow builds **Chinese only**. The release workflow builds **both zh and en** from the same tagged source. Language is compiled into the binary; it is not a run-time menu switch. Core shared libraries are separate versioned assets and are not rebuilt by frontend-only builds.

## CI package and UI assets

The Chinese test artifact contains:

- The RG353PS-native System/slot binary and BaseOS launcher/manifest.conf.
- The upstream System/slotnet.sh and System/fonts/ interface typeface directory. The latter is required for CJK interface text; an empty directory silently falls back to embedded label.ttf, which has no Chinese glyphs.
- System/Carts/ from the upstream 640×480 UI bundle: ten selector badges, the machine-select title, default cartridge/disc art, loading/slot artwork and carts.tsv.
- Six legacy GB/GBC/GBA dark/light theme marks (type_*.png) still read by the 640 frontend. The current 640 package omits these six, so CI supplements only these six PNGs from the older upstream package; no 720-layout art is taken.
- Music/, including the two runtime-loaded music-panel fonts and their license texts.
- Overlay/ screen-mask artwork and Bootlogos/ boot-picture directory.
- UPSTREAM-ASSET-SOURCE.txt with both pinned archive digests.

The primary art, interface font and Slot network script are staged from the pinned RetroGame66/LUCKLUCK-SLOT release v2026.11.09, asset 66slot-frontend-System-640480-V20261109-fix.zip (SHA-256 5f71f6b8…d670c35). The six theme marks are supplemented from v2026.10.02, asset slot-frontend-System-2026-10-02.zip (SHA-256 6c508590…b5db8a). CI verifies both archive digests, the presence of System/fonts/ and System/slotnet.sh, and the required asset filenames before packaging. Only data assets are imported: the upstream System/slot executable and upstream libretro cores are never copied. The RG353PS musl binary remains the one built by this repository; independently provisioned *_libretro.so files are not deleted or replaced.

The archive name contains the source run/tag and language. SHA-256 sidecar is generated for every artifact. When updating an existing card, back up customised System/Carts/, System/fonts/, Overlay/, Bootlogos/ and Music/Fonts/ first because matching bundled defaults can be overwritten during extraction.

Acceptance on the actual console remains separate: cold boot, display geometry/orientation, D-pad and buttons, analog stick, audio/RK817, Wi-Fi, Bluetooth, MTP/mass-storage, sleep/wake, shutdown/reboot, appearance of all ten machine badges and cartridge art, music-panel typography, and actual game launch with the matching AArch64/musl core pack.
