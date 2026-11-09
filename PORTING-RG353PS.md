# RG353PS / LUCKLUCK SLOT native 640×480 port

Status: CI test branch. A successful build is not a substitute for RG353PS hardware validation.

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

The package also installs the BaseOS `launch.sh` and `manifest.conf` to both adapter discovery paths. BaseOS remains the owner of its launcher, sleep service, power/volume contract and USB gadget services.

## Preserved RG353PS input and BaseOS integration

`packaging/rg353ps/apply-rg353ps-input.py` applies checked, narrowly scoped patches to upstream `evdev.rs`:

- Stick range 15–1023, centre 519; left stick drives menu navigation with RG353PS vertical polarity.
- Physical face/shoulder/Function/D-pad mapping is retained from the tested RG353PS adapter.
- Linux KEY_POWER (116) remains included in the event-node capability filter.

The upstream 640 branch removed the card-side MTP launch hook. This port restores `slot::mtp::start(&root)` and `slot/src/mtp.rs`. It only starts the existing `System/mtp-auto.sh` helper in the background; BaseOS continues to own USB gadget setup and mass-storage/MTP services. No MTP responder or USB debug service is newly enabled by the frontend package. The installer never deletes existing `System/mtp-*` scripts, emulator cores or user data.

The GBA picker has two physical socket positions. The existing eight-core `Core::ALL` remains for persistence/artwork; the picker uses a dedicated `Core::GBA_PICKER=[Mgba,Gpsp]` list and regression test so the two UI sockets cannot index eight choices.


## RG353PS power and Wi-Fi service adapters

The BaseOS launcher exports two device-service contracts:

- `BASEOS_SLEEP=/usr/sbin/rg353ps-sleep`: Slot uses this on a user power tap after flushing resume state and blanking the panel. On RG353PS the settings row reports **System Deep Suspend**; legacy `doze_min` values on existing cards no longer cause an idle poweroff. If the helper cannot be spawned, Slot falls back to the generic frontend timeout instead of remaining dark indefinitely. Long-press shutdown remains a separate poweroff path.
- `BASEOS_WIFI=/usr/sbin/rg353ps-wifi-connect`: the BaseOS package's `System/slotnet.sh` compatibility adapter translates Slot's card-side `up/down/scan` contract to the BaseOS Wi-Fi control plane. Scan output is normalized to `SSID<TAB>signal<TAB>secured`, the address is written to `/tmp/slot-ip` for the direct-connect dialog, and credentials saved by Slot in `System/wifi.conf` are used by the BaseOS supplicant helper. Slot must not start a second Wi-Fi driver or supplicant itself.

The BaseOS integration is documented in `BaseOS-RG353PS/platforms/rg353ps/POWER.md` and `FRONTENDS.md`. The RG353PS packaging workflow validates the helper shell syntax and tests the scan-field conversion using a stub helper. This CI test does not replace on-device association, DHCP, internet reachability, suspend-current and wake/resume validation.

## Chinese / English build policy

The source supports two distinct compile-time UI variants:

- Chinese test build (default): `--no-default-features --features device`
- English release build: `--no-default-features --features device,lang-en`

The push / manual test workflow builds **Chinese only**. The release workflow builds **both zh and en** from the same tagged source. Language is compiled into the binary; it is not a run-time menu switch. Core shared libraries are separate versioned assets and are not rebuilt by frontend-only builds.

## CI package and UI assets

The Chinese test artifact contains:

- The RG353PS-native `System/slot` binary and BaseOS launcher/`manifest.conf`.
- `System/Carts/` from the upstream 640×480 UI bundle: ten selector badges, the machine-select title, default cartridge/disc art, loading/slot artwork and `carts.tsv`.
- Six legacy GB/GBC/GBA dark/light theme marks (`type_*.png`) still read by the 640 frontend. The current 640 package omits these six, so CI supplements only these six PNGs from the older upstream package; no 720-layout art is taken.
- `Music/`, including the two runtime-loaded music-panel fonts and their license texts.
- `Overlay/` screen-mask artwork and `Bootlogos/` boot-picture directory.
- `UPSTREAM-ASSET-SOURCE.txt` with both pinned archive digests.

The primary art/font bundle is pinned to `RetroGame66/LUCKLUCK-SLOT` release `v2026.11.09`, asset `66slot-frontend-System-640480-V20261109-fix.zip` (SHA-256 `5f71f6b8…d670c35`). The six theme marks are supplemented from `v2026.10.02`, asset `slot-frontend-System-2026-10-02.zip` (SHA-256 `6c508590…b5db8a`). CI verifies both archive digests and all required asset filenames before packaging. The verified package contains 48 PNGs under `System/Carts/` and ten screen overlays. Only data assets are imported: the upstream `System/slot` executable and upstream libretro cores are never copied. The RG353PS musl binary remains the one built by this repository; independently provisioned `*_libretro.so` files are not deleted or replaced.

The archive name contains the source run/tag and language. SHA-256 sidecar is generated for every artifact. When updating an existing card, back up customised `System/Carts/`, `Overlay/`, `Bootlogos/` and `Music/Fonts/` first because matching bundled defaults can be overwritten during extraction.

Acceptance on the actual console remains separate: cold boot, display geometry/orientation, D-pad and buttons, analog stick, audio/RK817, Wi-Fi, Bluetooth, MTP/mass-storage, sleep/wake, shutdown/reboot, appearance of all ten machine badges and cartridge art, music-panel typography, and actual game launch with the matching AArch64/musl core pack.
