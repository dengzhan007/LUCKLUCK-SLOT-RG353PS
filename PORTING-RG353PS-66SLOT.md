# RG353PS / LUCKLUCK SLOT (66slot) 640×480 port

Status: first device-integration branch; build and hardware validation are separate gates.

- Source repository: `dengzhan007/slot-66`
- Synced upstream branch: `66-mods`
- Port branch: `port/rg353ps-640x480`
- Upstream source base: `ba8f21a651f8721cc12cc9f2bac6f4400bbd413f`
- Device: Anbernic RG353PS, RK3566, 640×480 panel
- OS: `dengzhan007/BaseOS-RG353PS`

## 1. Display: reuse upstream geometry, replace the device surface

The upstream source already handles a physical panel smaller than its logical composition:

- `crates/slot-gfx/src/surface.rs` defines the logical composition as `OUT_W=720`, `OUT_H=480`.
- `blit_rect()` selects `blit_rect_fit()` when the actual window is smaller than 720×480. This preserves aspect ratio and fits the complete composition into the panel.
- `crates/slot-gfx/src/fbdev.rs` obtains the output surface dimensions and queries the EGL window size. The code comments explicitly describe the 640×480-panel fit path.
- Therefore, do not change `OUT_W/OUT_H` to 640×480 merely because the physical panel is 640×480. Doing so would alter the source-pixel scale, LCD mask phase, bezel geometry and other rendering calculations that are deliberately based on the 720×480 logical composition.

The upstream device surface is an EGL framebuffer-native window path. RG353PS BaseOS uses Panfrost and the previously tested platform adapter uses SDL2 KMSDRM + GLES instead. The build copies:

`BaseOS-RG353PS/platforms/rg353ps/frontends/slot/fbdev.rs`

over the upstream device surface for this target. The adapter creates a fullscreen KMSDRM window, requires DRM master, requests an ES 2.0 context, and reads the resulting SDL window size. The physical output target remains 640×480; the upstream compositor performs its existing fit-to-window operation.

This is a backend substitution, not a new resolution implementation. Do not substitute the proprietary Mali fbdev native-window path on Panfrost.

## 2. RG353PS input adaptation

The branch-owned `packaging/rg353ps/apply-rg353ps-input.py` applies narrowly scoped, checked replacements to the current upstream `evdev.rs`:

- RG353PS ADC stick range `15..1023`, centre `519`, vertical polarity, and left-stick menu navigation.
- The RG353PS face-button / shoulder / D-pad / Function key matrix.
- Linux `KEY_POWER=116` mapping and inclusion in the evdev capability filter, so the power-only event node is opened.

The script keeps upstream's newer `AxisMap`, event decoding, device discovery and input threading unchanged. It aborts if its expected source anchors or postconditions do not match; the previous whole-file BaseOS patches are deliberately not applied to this newer upstream revision.

BaseOS adapter ownership is declared by `manifest.conf`:

```ini
adapter_api=1
priority=100
volume_owner=frontend
power_owner=frontend
```

The package carries identical copies of the manifest and launcher under both `.baseos/frontends/slot/` and `baseos/frontends/slot/`. The launcher retains `# baseos: volume-owned` and `# baseos: power-owned` markers.

## 3. Runtime contract

The BaseOS launcher exports:

- `SLOT_ROOT=/mnt/SDCARD` (or the discovered frontend root)
- `BASEOS_TARGET=rg353ps` and `BASEOS_MODEL=rg353ps`
- `BASEOS_SLEEP=/usr/sbin/rg353ps-sleep`
- `SDL_VIDEODRIVER=kmsdrm`
- `SDL_AUDIODRIVER=alsa`
- `SDL_KMSDRM_REQUIRE_DRM_MASTER=1` is set by the SDL surface itself.

The frontend is staged to `/tmp/baseos-slot` before execution so it can be launched from an exFAT data partition without relying on executable-bit support.

## 4. Build

The GitHub Actions workflow `.github/workflows/rg353ps-package.yml`:

1. Checks out this branch and a pinned BaseOS adapter commit.
2. Builds on an AArch64 Alpine/musl container with the Rust 1.96 toolchain declared by upstream.
3. Copies the SDL2/KMSDRM surface and applies the RG353PS input/power patches.
4. Builds the `slot` package with `--no-default-features --features device`.
5. Verifies the output is an AArch64 dynamically linked musl executable.
6. Packages `System/slot`, the BaseOS launcher and two manifest copies, licence/source provenance, a test README and a SHA-256 checksum.

The first package is frontend-only. It does not rebuild or include emulator core shared libraries; existing `System/*_libretro.so` files are not removed or overwritten. If the test card has no compatible core libraries, frontend boot/UI can still be tested but game launch cannot be considered validated.

## 5. First-test acceptance checklist

Record each item independently; do not infer hardware success from a green CI build.

1. ZIP structure, both manifest copies and SHA-256.
2. BaseOS discovers the Slot adapter.
3. Frontend starts and reaches the first screen.
4. Display is reported by the SDL surface as 640×480; no clipping, wrong rotation or unexpected scaling.
5. D-pad, face buttons, shoulders, Start/Select, Function, volume and power key.
6. Audio output through BaseOS ALSA/RK817.
7. Wi-Fi, sleep/wake, shutdown/reboot.
8. Game launch only after confirming the required core shared libraries are present and compatible.

Until these are checked on an RG353PS, label the artifact as a test build, not a release.
