# LUCKLUCK SLOT (66slot)

**English** · [简体中文](README.zh-CN.md)

<p align="center">
  <img src="assets/66slot-title.webp" width="820"
       alt="LUCKLUCK SLOT — a ten-system retro game frontend for Anbernic H700 handhelds on BaseOS">
</p>

> **RG353PS / BaseOS port.** Branch `port/rg353ps-640-native` is based on the real upstream
> `66-mods-640` source tree (upstream commit `4a0b60189fb4e29e57e73de14d997260a2e9d3a1`),
> with the native 640×480 compositor/layout retained. For RG353PS, the H700-specific EGL
> framebuffer surface is replaced at package time with BaseOS SDL2/KMSDRM + GLES2 on Panfrost.
> The BaseOS launcher/manifest and USB ownership remain with BaseOS; the existing card-side
> MTP helper hook is retained. Test CI builds Simplified Chinese only; tagged releases build separate Chinese and English
> RG353PS packages. Each full release package includes the pinned AArch64/musl emulator cores
> from [core pack v1.0.0](https://github.com/dengzhan007/LUCKLUCK-SLOT-RG353PS/releases/tag/rg353ps-cores-aarch64-musl-v1.0.0),
> their runtime libraries, and license/source materials.
>
> **Branch `66-mods-640` — the 640×480 edition.** The `66-mods` branch is the 720×480 tree
> (RG SP, RG34XX, RG34XXSP and the rest of the 720 group); this branch is the same frontend laid
> out for the **640×480** panel (RG35XXSP and the 640 group). The two are not a build flag apart:
> the row geometry, the plate, the shelf, the bezel aperture, the desktop art and the power menu
> are laid out per panel, and this tree carries no MTP responder. They are kept as two branches
> on purpose — do not merge them back into one.

> **This is a modified fork.** It is based on
> [slot-cfw/slot](https://github.com/slot-cfw/slot) by **Brandon T. Kowalski** (MIT; upstream was
> `BrandonKowalski/slot` until November 2026 and the old URL still redirects), with changes by
> [@RetroGame66](https://github.com/RetroGame66) on top.
> It is **not** the upstream project, and is not endorsed by or affiliated with its author.
> What changed is in [`CHANGES.md`](CHANGES.md); origin and licensing in [`NOTICE.md`](NOTICE.md).
>
> **本仓库是上游 slot 的修改版（fork），非官方版本。** 原作 © Brandon T. Kowalski，MIT 许可；
> 修改由 [@RetroGame66](https://github.com/RetroGame66)（你的六六）完成。上游仓库已迁至
> [slot-cfw/slot](https://github.com/slot-cfw/slot)，旧地址 `BrandonKowalski/slot` 会重定向。

LUCKLUCK SLOT — abbreviated **66slot** — is a **multi-system game frontend** for Anbernic H700
handhelds running **BaseOS** (RG SP and other BaseOS devices). It began as a GBA-only frontend for
the RG SP and now covers **ten** systems. The user-facing interface is Chinese (a separate English
build exists); this file describes the fork, not upstream.

Upstream's own README is reproduced **verbatim and unmodified** at
[`README.upstream.md`](README.upstream.md) — worth reading, but it documents *upstream*, which is
GBA-only and uses a different base OS.

---

## History & renaming（项目更名来龙去脉）

The project grew out of upstream `slot` and went through a base-OS swap, a feature expansion, and a
naming step:

1. **Upstream origin.** `slot` ("slot.") is Brandon T. Kowalski's *bespoke, GBA-only* frontend for
   the Anbernic RG SP, MIT-licensed. It only ever ran Game Boy Advance. In November 2026 upstream
   moved from `BrandonKowalski/slot` to the **`slot-cfw`** organisation and broadened into a
   Game-Boy-centric CFW; this fork is still based on the GBA-era commit below.
2. **The fork.** This repository is now **`RetroGame66/LUCKLUCK-SLOT`**, branch **`66-mods`**, based on
   upstream `4345cb8b` (2026-09-11), maintained by *你的六六*. It was `RetroGame66/slot` until
   2026-11-09.
3. **Base-OS swap.** The fork was first deployed on **AGS-102** — Brandon Kowalski's RG-SP-specific
   fork of BaseOS. As of **2026-10-24** AGS-102 is no longer used: its features had merged into
   upstream **BaseOS** and the AGS-102 branch had gone stale, so the fork now runs directly on
   BaseOS. Link play and networking were moved to on-card scripts so the code is base-OS-agnostic.
4. **Naming.** On **2026-10-26** the project's *display* name was set to **LUCKLUCK SLOT**,
   abbreviated **66slot**; on **2026-11-09** the repository followed (`RetroGame66/slot` →
   `RetroGame66/LUCKLUCK-SLOT`). The repository carries the full name; the abbreviation stays
   where it is a prefix rather than a title — the release zips (`66slot-frontend-…`) and the
   interface. What did **not** change is the code inside: the crates are still `slot`,
   `slot-ui`, `slot-store`…, the binary on the card is still `System/slot`, and the state file is
   still `System/slot.state`. Those are internals — renaming them would break every card already in
   the field and buy a reader nothing. **"slot" in a path is the code name, not the old name.**
5. **Scope.** From GBA-only to **ten systems** (below).

> In one line: it used to be "a GBA frontend on the RG SP"; it is now a BaseOS-based, ten-system
> handheld frontend called **LUCKLUCK SLOT (66slot)**.

> **中文**：本项目派生自上游 slot（原作仅为 RG SP 的 GBA 前端，MIT；上游已于 2026-11 从
> `BrandonKowalski/slot` 迁至 [`slot-cfw/slot`](https://github.com/slot-cfw/slot)）。fork 仓库为
> `RetroGame66/LUCKLUCK-SLOT`（原 `RetroGame66/slot`）的 `66-mods` 分支，由「你的六六」维护。早期底座用
> AGS-102（BaseOS 的 RG SP 分支），2026-10-24 起改以 **BaseOS** 为底座（AGS-102 功能并入上游且停更）。
> 2026-10-26 项目展示名定为 **LUCKLUCK SLOT（缩写 66slot）**，2026-11-09 仓库随之改名。**代码里
> 没有跟着改**：crate 仍叫 `slot` / `slot-ui` / `slot-store`，卡上二进制仍是 `System/slot`，
> 状态文件仍是 `slot.state` —— 这些是内部名，改了只会让已经在用的卡失效。机种由 GBA-only
> 扩展到 **10 个**。

## About this fork

This port uses the **BaseOS managed frontend package/profile API 1** (RG353PS BaseOS 1.2.10+).
Install it as `Frontends/LUCKLUCK-SLOT/`. The package exposes `640-native` profile by default; users who need more configurations can create additional
profile files themselves. All profiles share one executable/resource payload but keep
independent settings, favorites, core selections and save-state snapshots. BaseOS owns
discovery, the boot selector, default startup and uninstall; Slot owns its UI and game runtime.


LUCKLUCK SLOT turns a BaseOS handheld into a shelf of game carts. Beyond upstream it gives you:

- **Ten machines on one shelf** — GBA, GB, GBC, Famicom, Super Famicom, Mega Drive, PlayStation,
  PC Engine, Arcade and Neo Geo, each read out of its own folder under `Games/` and each with its
  own core, frame size and key seating.
- **A Chinese interface** that takes its typeface off the card.
- **A shelf indexed across the top of the case** — a hanzi library has no A–Z order, so the index
  runs left-to-right up top. Cheats are per cart (`System/Cheats/<stem>.cht`); the button map
  (`System/remap.txt`) and each machine's screen look (`System/display.txt`) belong to the card.
- **Light and dark case themes**, switchable on the shelf with a key chord, and seven interface
  palettes under that.
- **Per-machine screen tools upstream never had** — selectable panel masks / aperture presets, a
  colour-correction stage, scanlines for the consoles, and per-machine screen looks — all
  adjustable on the device, no rebuild.
- **A music player** (像素随身听) that grew into a deck: two faces on a four-shade LCD panel, 40
  colour ramps, scrolling lyrics and a ten-band equaliser, reached from the menu and returning to
  its own disc shelf.
- **Search by initials** — press `SELECT`+`Y` on the game shelf to open a full keyboard. Type pinyin initials (`cjjq` for 超级机器人大战) or a title substring; `X` restores the pre-search shelf, `START` applies, and `MENU` cancels.
- **Saves in numbered slots**, plus a save-state browser that reaches every cart, and cheats that
  live with the cart rather than in one global file.
- **A boot opening** between your own boot picture and the shelf's first frame.
- **MTP and Wi-Fi direct connect** — the card appears in Explorer plugged in, or is reachable over
  sftp on the same network without pulling it.
- **A random-wallpaper carousel**, **cartridge art the player owns** (whole-cart art and the paper
  in the recess), and **boot-picture switching**.
- **One source tree, two builds**: Chinese (default) and English (`--features device,lang-en`).
  The interface strings live in `slot-ui/src/lang.rs` in two tables, so a change updates both.

## Supported systems (10)

The machine is decided by the **folder** under `Games/`. Drop each ROM into `Games/<machine>/`;
inside a machine folder, almost any extension that is not a known non-ROM (`.txt`, `.png`,
`.sav`, …) is treated as that machine's game. A few extensions collide with non-game meanings —
`.md` is Markdown and `.ps1` is a PowerShell script — so keep those **inside the correct folder**,
never loose at the top of `Games/`. Neo Geo and Arcade are `.zip` romsets identified by their
folder, not by extension.

`selected_core.ini` only lists the *switchable* cores (the ones below marked "default / optional");
GB and GBC run on the frontend's built-in Game Boy core, which is not user-switchable.

| Machine | Console | ROM folder | Accepted ROM formats | Core |
|---|---|---|---|---|
| GBA | Game Boy Advance | `Games/GBA/` | `.gba` | mGBA (default) / gpSP |
| GB | Game Boy | `Games/GB/` | `.gb` | built-in Game Boy core |
| GBC | Game Boy Color | `Games/GBC/` | `.gbc` | built-in Game Boy Color core |
| SFC | Super Famicom / SNES | `Games/SFC/` (also `SNES`) | `.smc`, `.sfc`, `.fig` | supafaust |
| FC | Famicom / NES | `Games/FC/` (also `NES`) | `.nes`, `.fds` | fceumm |
| NEOGEO | Neo Geo | `Games/NEOGEO/` | `.zip` (folder decides) | fbneo |
| ARCADE | Arcade — CPS1 / CPS2 / CPS3, FBNeo | `Games/ARCADE/` | `.zip` (folder decides) | fbneo |
| MD | Mega Drive / Genesis | `Games/MD/` (also `GENESIS`, `MEGADRIVE`) | `.md`, `.gen`, `.smd` (and `.bin` inside the folder) | genesis_plus_gx |
| PS1 | PlayStation | `Games/PS1/` (also `PSX`) | `.cue`, `.chd`, `.pbp`, `.iso` (and `.bin` inside the folder) | pcsx_rearmed |
| PCE | PC Engine / TurboGrafx-16 | `Games/PCE/` (also `TG16`, `TURBOGRAFX`, `PCENGINE`) | `.pce` (and `.bin` / `.cue` inside the folder) | pce_fast |

**NDS is not supported.**

## Downloads

Packages are published on the [Releases page](https://github.com/dengzhan007/LUCKLUCK-SLOT-RG353PS/releases). This repository targets the RG353PS native **640×480** panel and publishes two complete packages:

| Package | Language |
|---|---|
| `LUCKLUCK-SLOT-RG353PS-640x480-<tag>-zh.zip` | Simplified Chinese |
| `LUCKLUCK-SLOT-RG353PS-640x480-<tag>-en.zip` | English |

Each package contains the frontend, BaseOS adapter files, UI assets, all eight external AArch64/musl libretro cores, required runtime libraries, and the associated licence/source materials. GB and GBC use the frontend's built-in cores.

`SHA256SUMS.txt` contains checksums for both complete packages. Back up saves and any customised core or asset files before merging a release onto an existing card. The release notes state the exact validation scope; CI/loader checks do not imply that every core has completed gameplay testing on hardware.

## Controls

A `+` means both together. The full, always-current list is on the device: **MENU → 全键位一览**
(the shortcut card), which is generated from the same table the program itself reads.

### Anywhere

| Input | Action |
|---|---|
| `SELECT` + `Up` / `Down` | Brightness |
| `SELECT` + `Left` / `Right` | Blue-light filter |
| `L3` / `R3` | Brightness / blue light, one step (machines with a clickable stick) |
| Stick | The D-pad, on machines that have one |
| `VOL+` / `VOL-` | Volume; both together mute |
| Tap `POWER` | Sleep / wake (state made durable on the way down) |
| Hold `POWER` | Power menu: restart / power off |

### On the shelf

| Input | Action |
|---|---|
| `Left` / `Right` | Browse the cart row |
| `L` / `R` | Previous / next letter |
| Tap `A` | Resume the last save state |
| Hold `A` | Start the game fresh |
| Tap `X` | Star / unstar this cart |
| Hold `X` | Favourites ↔ all |
| Tap `Y` | Switch subfolder — and, on the music side, between tapes and games |
| Hold `Y` | Pick the machine's shelf (the blind comes down, the picker floats on it) |
| Hold `B` | Direct connect: the card's Wi-Fi comes up, the screen shows an address, a PC reaches it over sftp — no need to pull the card |
| `SELECT` + `Y` | Search game titles by pinyin initials or literal text (game shelf only) |
| `START` | Pick the core for this cart |
| `MENU` | Main menu (and the shortcut card under it) |
| `SELECT` + `START` | Day / night theme |
| `SELECT` + `R1` | Interface palette (7) |
| `SELECT` + `VOL+` / `VOL-` | Audio-latency profile |

Search uses four keyboard layers (`abc / ABC / 123 / #+=`): `L1`/`R1` switch layers, the D-pad moves, `A` types and `B` erases. `X` restores the pre-search shelf, `START` applies the result, and `MENU` cancels. Matching uses pinyin initials or a literal substring; polyphonic Chinese characters use the reading in the built-in table.

### In game — common to every machine

| Input | Action |
|---|---|
| Tap `MENU` | Game menu |
| `Left` / `Right` in the slot table | Change save slot |
| Tap `X` in the slot table | Clear this slot |
| 菜单 → 重开本局 | Reseat the cart without loading a save |
| `SELECT` + `MENU` | The same game menu |
| Hold `MENU` | Save, eject, back to the shelf |
| Double tap `MENU` | Save-state browser (every cart, every slot) |
| `SELECT` + `L1` / `R1` | Load / save latest |
| `SELECT` + `A` | Cheat table (press again: toggle all) |
| Hold `L2` / `R2` | Rewind / fast-forward — every machine **except PS1** |
| Double tap `R2` | Lock fast-forward |
| Double tap `L2` | Half-speed slow-mo |
| `VOL+` / `VOL-` | Volume; both mute |

On **PS1** the shoulders are real game buttons, so its transport is `SELECT` + `L2` / `R2`.

### In game — per-machine screen look

These work in game only and write the chosen look to `System/display.txt` immediately, so it
survives a reboot. `SELECT` is deferred (waits up to 600 ms for a second button), so a chord never
reaches the game, and a lone `SELECT` still reaches the game after the window.

| Machine | `SELECT` + `X` | `SELECT` + `Y` |
|---|---|---|
| GBA | Aperture preset (7: off, LCD3x 25 / 50 / 75 / 100%, scanline 50 / 100%) | 18 colour grades; **hold** to open the browser |
| GB | Overlay art (the ring) | 343 palettes; **hold** for the browser |
| GBC | Overlay art, grid riding it | 18 colour grades; **hold** for the browser |
| FC / SFC / MD / PS1 / PCE / NEOGEO / ARCADE | Scanline / overlay preset | 18 colour grades; **hold** for the browser |

The aperture table is **GBA-only**: the other machines cannot use a dot mask at all, which is why
their `X` addresses the scanline ring instead. `Y` is the colour grade everywhere.

### In the pixel player（像素随身听）

| Input | Action |
|---|---|
| Tap `A` | Play / pause |
| Hold `A` | Stop (back to 0:00) |
| `Left` / `Right` | Rewind / fast-forward, repeating while held |
| `Up` / `Down` | Previous / next track |
| `L1` / `R1` | Previous / next disc |
| `START` | Next EQ preset |
| Tap `X` / `Y` | Switch panel / switch palette |
| Tap `MENU` | Disc info |
| Hold `MENU` | Back to the menu |
| Tap `B` | Back to the disc shelf |

Closing the lid writes the resume snapshot (it is always saved) and dims the screen; whether the
device then powers off, and how long it waits, is set in **Settings → 息屏后关机** (one tier keeps
the screen off without powering down). **While music is playing, the idle power-off is suppressed**
and waking returns to the player rather than the shelf.

## Settings（参数调整页）— 10 items

Main menu → 参数调整页. Ten rows, two per line; direction keys move the caret, `A` changes value /
opens.

1. **日期与时间** — open the clock picker
2. **刷新游戏库** — rescan the card for ROMs (no reboot needed)
3. **加速时声音** — play audio during fast-forward / rewind (on / off)
4. **息屏后关机** — minutes before power-off after the screen goes dark (one tier = screen off only)
5. **快进倍率** — fast-forward gear
6. **回溯倍率** — rewind gear
7. **插卡续玩** — auto-resume on insert / boot when a cart is present
8. **拔卡续存** — auto-save a resume snapshot on eject
9. **开机画面** — choose from `Bootlogos/` on the card
10. **无线网络** — open the Wi-Fi config

## The music player（像素随身听）

Main menu → 像素随身听. A disc plays on a deck: the disc behind its window, the name of what is
playing in the shelf's top band, and the transport keys on the slot band, which is where a deck's
transport belongs. The deck draws itself in a four-shade retro LCD panel, blown up by a whole
number so the grid stays square.

**Two faces, and the second one needs the first.** The resting face is the track's name with a
spectrum; the other is the same track's scrolling lyrics. A card whose music carries no `.lrc` has
no second face to show — the key still works, it just shows the first one again. A short `MENU`
opens the disc-info page (tags, track count, length). Keys are in the table above.

What it reads: `Music/` holds albums as folders, `Music/Covers/` the sleeves, `Music/Lyrics/` the
`.lrc` sidecars, `Music/Fonts/` the panel's CJK typeface. The decoder is chosen by container rather
than by extension, because a card's music is whatever a phone or a store left there.

## Wallpapers

Drop PNGs into `Wallpapers/`; one is picked at random each boot and drawn behind the shelf (reboot
= new pick). With no `Wallpapers/` folder the frontend falls back to a built-in default wallpaper
(a neon tunnel with a theme-following breathing light).

## What goes on the card

```
BIOS/              optional gba_bios.bin (or the core's own high-level BIOS).
Games/<machine>/   ROMs, one subfolder per machine (GBA/GB/GBC/SFC/FC/NEOGEO/ARCADE/MD/PS1/PCE).
Labels/<machine>/  <stem>.png — the picture pasted into that cart's recess.
Saves/<machine>/   <stem>.sav / .srm battery saves. A flat Saves/<stem>.sav is still read.
States/<core>/     <stem>/, save-state slots per cart, filed under the core that made them —
                   mgba / gpsp / fceumm / supafaust / genesis_plus_gx / pcsx_rearmed / pce_fast / fbneo.
System/            the binary, cores, and the optional settings below.
Wallpapers/        .png, one at random each boot, behind the shelf.
Bootlogos/         boot pictures; switch in Settings → 开机画面.
Cartridge/<machine>/  whole-cart art <stem>.png; missing falls back to System/Carts/66SLOT_CART_<machine>.png.
Music/Fonts/       the interface typeface — and the deck panel's own face.
Music/Covers/      album art.
Music/Lyrics/      lyrics (.lrc).
```

Everything in `System/` beyond the binary and cores is optional:

```
theme.txt          housing / recess / opening / edge — the shell colours.
selected_core.ini  <stem> = <core>; defaults to mGBA; both GBA cores ship in System/.
labels.txt         strip_tags on/off — keep or drop [中]/[日]/[英] edition tags.
display.txt        "mask cc_gba cc_gb cc_gbc" — the per-machine screen look.
audio.txt          stable / balanced / strict — the audio-latency profile.
mask.txt / cc.txt  card-supplied panel mask / colour-correction matrix.
remap.txt          physical button remapping, one "<physical> = <game>" per line.
Cheats/<stem>.cht  the cheat file, in the RetroArch `.cht` format (cheatN_enable decides whether
                   an entry starts on; `#` starts a comment).
wifi.conf          the card's own Wi-Fi credentials, written by Settings → 无线网络.
slot.state         light/dark, plus other booleans — written by the frontend.
overlay.txt / shelf.txt  last overlay per Game Boy / last machine's shelf.
bootlogo.txt       which picture in Bootlogos/ is the current one.
bootlogo-original.bmp  the boot picture saved before the first partition write, so a bad flash
                   is recoverable from the card.
library.cache      the rom scan's cache, keyed on folder mtimes. Deleting it forces a full rescan.
Carts/carts.tsv    per-machine shell colours and cart flags; the built-in table is the fallback.
Carts/66SLOT_CART_<machine>.png  the built-in cart art for a machine.
boot_opening.slotvid  the animation between the boot picture and the shelf.
mtp-responder + mtp-auto.sh  what makes the card appear as a device in a file manager.
```

**There is no `System/fonts/`.** The interface typeface is read from the card root —
`Music/Fonts/` — because that is where the release package puts it; see `Music/Fonts/` above.

> **Two places take cart art, and they are not alternatives.** `Cartridge/<sys>/<stem>.png` is the
> **whole cart**; `Labels/<sys>/<stem>.png` is the **paper in its recess** — pasted cover-fit (not
> squashed), and left a flat colour when the file is absent. `Labels/` was removed on 2026-10-28 and
> **put back on 2026-11-03**: the players who used it said dropping it made editing inconvenient.
> (`labels.txt` above is the `strip_tags` switch and is unrelated — still in use.)

**A PS1 disc may bring its own BIOS.** A translated or patched PS1 game often needs a specific
BIOS, and the convention is to put it next to the rom **under the same name** — `Ridge.cue`
beside `Ridge.bios`. It is read for that cart only and never shows up as a cart of its own.

`System/theme.txt` example:

```
housing #24242a
recess  #1a1a1e
opening #050508
edge    #4d4d57
```

`System/selected_core.ini` example (gpSP carries the serial-link hardware mGBA's libretro build does
not):

```
Emerald = gpsp
```

## Installing

This RG353PS package requires **BaseOS 1.2.10 or newer**, which provides managed frontend
package/profile API 1. Older BaseOS builds discover legacy adapters only and cannot launch
this managed package.

1. Flash the RG353PS BaseOS image to an SD card.
2. Insert it, power on once, and let the first boot finish so BaseOS can expand the data partition.
   **Do not copy files to the card before this first boot**; partition expansion may recreate it.
3. Power off, connect the card to a computer, and open the user-data volume (usually labelled
   `BASEOS`).
4. Create `Frontends/` at the data-volume root if it does not exist. Extract the release ZIP into
   `Frontends/` while preserving its top-level `LUCKLUCK-SLOT/` directory. The final path must be
   `Frontends/LUCKLUCK-SLOT/baseos-frontend.conf`; do not extract `payload/` or `profiles/`
   directly into the data root.
5. Keep ROMs in `Games/<machine>/` and battery saves in `Saves/` at the data-volume root.
   Do not put games or saves inside the frontend package.
6. Safely eject the card, reinsert it and boot. BaseOS discovers the single default `640-native` profile automatically.

When replacing the earlier package, remove the old `Frontends/slot-rg353ps/` program folder after copying `Frontends/LUCKLUCK-SLOT/`. Do **not** delete `.baseos/instances/`; BaseOS and the Slot launcher migrate the old default ID and per-profile data automatically, retaining the old instance directory as a rollback copy.

BaseOS supports frontend discovery on TF1 and TF2, with TF2 taking priority. The RG353PS Slot
launcher binds shared games and battery saves from the BaseOS TF1 data root
(`/mnt/SDCARD/Games` and `/mnt/SDCARD/Saves`). For the validated setup, keep the Slot package
and game data on TF1.

### Profiles, default startup and data ownership

The package includes one default profile, `640-native`. It uses the shared Slot executable,
UI payload and core files. Hold **L1 or R1 while
powering on** to open the BaseOS selector (a Function-triggered USB storage request has priority).

- **A** launches the highlighted profile once without changing the saved default.
- **START** saves the highlighted profile as the default and launches it.
- **B** cancels and continues with the saved default.
- **Y** confirms removal of only the highlighted profile.
- **X** confirms removal of the complete Slot package.

The selector shows `DEFAULT STARTUP` at the top and marks the matching row with `[DEFAULT]`.
In automatic mode it displays `AUTO`. An A-key temporary launch never changes the default.

| Data | Ownership |
|---|---|
| ROMs in `Games/` | Shared by all profiles/frontends |
| Battery saves in `Saves/` (`.sav` / `.srm`) | Shared |
| Emulator core files | One shared package/core copy |
| Theme, time zone, favorites and Slot preferences | Per profile |
| `selected_core.ini` core choices | Per profile |
| `States/` save-state/resume snapshots | Per profile |

Profile data lives under
`/mnt/SDCARD/.baseos/instances/LUCKLUCK-SLOT--<profile-id>/`, separately from the package
directory. Removing a profile or the entire package does not delete these instance directories,
`Games/`, `Saves/` or shared cores. Reinstalling the same package ID and profile IDs reconnects
the previous settings and save states. Do not manually delete `.baseos/instances/` unless you
intend to erase that profile's data.

When upgrading from the earlier package ID `slot-rg353ps`, BaseOS automatically maps
the saved default to `LUCKLUCK-SLOT--<profile-id>`. On the first launch of each matching
profile, the launcher copies its old instance data into the new
`.baseos/instances/LUCKLUCK-SLOT--<profile-id>/` directory and leaves the old directory
untouched as a rollback copy. Keep the old instance directories until you have verified
the new package's settings and save states.

#### Creating additional profiles (optional)

The default package contains only `640-native`. To add another independent configuration,
copy `Frontends/LUCKLUCK-SLOT/profiles/640-native.conf` to a new file such as
`my-config.conf`, change its `display_name`, and keep `profile_api=1`. The filename without
`.conf` becomes the new Profile ID. Safely eject the card and reboot; BaseOS discovers the new
profile automatically. Do not duplicate `payload/`, `launch.sh` or the frontend package: every
profile uses the same executable, UI resources and cores. Each new ID receives its own settings
and save-state storage. Keep existing profile filenames unchanged to reconnect their data.

**Restoring a previous 640-native-alt profile:** the default package no longer ships this second profile, but its old instance data is preserved. To reconnect it, copy 640-native.conf to 640-native-alt.conf, change display_name, and reboot. Keeping the exact old filename preserves the old Profile ID, so the launcher can migrate its previous settings and save states. Using a different filename creates a new, empty profile instead.

The current RG353PS integration build includes the pinned AArch64/musl core pack v1.0.0:
eight libretro cores, runtime libraries, license texts and source metadata. The core binaries
are reused byte-for-byte and are not rebuilt by the frontend workflow.

Once running, **hold `B` on the shelf** to expose SFTP access to a PC on the same Wi-Fi. USB
connection also exposes the card through BaseOS MTP.
## Boot picture and boot opening

The boot picture and the boot opening are separate assets. The picture is selected from `Bootlogos/` in Settings; the optional opening lives at `System/boot_opening.slotvid` and plays between the picture and the shelf's first frame.

The `.slotvid` v1 file contains a 64-byte header, a 16-byte index per frame, then the JPEG frame and its PCM audio slice. The RG353PS build uses the native **640×480** panel size. The opening is optional; remove the file to skip the animation.

## SLOTVID STUDIO

[Download `slotvid-studio.zip` from the upstream release](https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/download/v2026.11.09/slotvid-studio.zip) (Windows, self-contained). SLOTVID STUDIO converts a video into `boot_opening.slotvid`; it is a separate utility, not part of the RG353PS frontend ZIP. No Python, separate ffmpeg installation, or network access is required.

Choose **640×480** for RG353PS, select cover / contain / stretch, 24 or 30 fps, audio, and clip length. Around **5 seconds** is recommended because the opening plays while the frontend finishes startup. Copy the generated `boot_opening.slotvid` into `System/` on the card.

## Building

The device build is a cargo invocation against an aarch64 Linux target:

```
cargo build --release --target aarch64-unknown-linux-gnu \
  -p slot --no-default-features --features device
```

`--no-default-features --features device` is required: default features pull in host backends that
do not cross-compile. This fork links with **zig** (`zig cc`) through a small forwarder, because
rustc cannot invoke a `cc`-style driver named `linker`. The helper `_tools/build_slot.py` wraps this
and emits the Chinese (default) or English (`SLOT_FEATURES=device,lang-en`) binary to
`target-device/.../release/slot`.

Two other binaries are built the same way and are not part of the frontend: `-p mtp-responder`
(the card's MTP answerer, Linux-only by nature) and `-p render_palettes` (a development tool that
draws the deck's colour ramps; it is in the workspace's `exclude` list because it has drifted behind
the `slot-ui` API it calls).

Useful scripts after a build: `check_glibc.py` (the highest `GLIBC_*` the binary needs — BaseOS
ships a glibc userspace, so build against a 2.35-class toolchain to stay portable) and
`verify_slot.py` (confirms an aarch64 ELF and that it carries the Chinese strings, not the old
English ones).

## Credits and licensing

This fork is **MIT**. Upstream's copyright notice atop [`LICENSE`](LICENSE) is untouched; the
modifications are the line beneath it. Full origin / baseline / attribution in
[`NOTICE.md`](NOTICE.md).

Emulation is [mGBA](https://mgba.io) (endrift), [gpSP](https://github.com/libretro/gpsp)
(Exophase), fceumm, supafaust, genesis_plus_gx, pcsx_rearmed, pce_fast and FBNeo — all through
libretro. The frontend loads a core with `dlopen` and never links or modifies one; each core keeps
its own licence, and the release's `System/licenses/` carries the texts plus the corresponding
sources that those licences ask for. The device boots **BaseOS**
([pvaibhav/BaseOS](https://github.com/pvaibhav/BaseOS)). The interface typeface is **Plix Bold**
(普力士) by cathree3, SIL OFL, built on IBM Plex Sans; the deck's panel face is **Ark Pixel**
(SIL OFL). The panel mask is derived from LCD3x (public-domain, Gigaherz, libretro). Type is Open
Sans (SIL OFL) and Nerd Fonts (MIT).

## AI disclosure

Upstream's frontend was written with Claude and reviewed by its author. The changes in this fork
were drafted with an AI assistant and then reviewed and tested on-device by
[@RetroGame66](https://github.com/RetroGame66), including the parts that only show up on real
hardware. **Much of the prose here is machine-written** — a departure from upstream worth stating
plainly.

This is a personal fork. Issues may go unanswered; for `slot` itself, upstream is the place to go.
Upstream's original README is at [`README.upstream.md`](README.upstream.md).

## Upstream

Everything not listed above is **Brandon T. Kowalski's** work; this fork would not exist without it.
His README (including his release pipeline, hardware notes, and credits) is reproduced unmodified at
[`README.upstream.md`](README.upstream.md). For `slot` itself, see
<https://github.com/slot-cfw/slot> — upstream moved there from `BrandonKowalski/slot` in November
2026, and the old address redirects.
