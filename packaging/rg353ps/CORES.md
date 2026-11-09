# RG353PS libretro cores

The RG353PS port uses a separate reusable core pack. Normal frontend builds
never rebuild or overwrite the emulator core libraries.

## Current reusable pack

- Release: [RG353PS libretro cores — AArch64 / musl v1.0.0](https://github.com/dengzhan007/slot-66/releases/tag/rg353ps-cores-aarch64-musl-v1.0.0)
- Archive: `slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip`
- SHA-256 sidecar: `slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip.sha256`
- Target: RG353PS / RK3566 / Cortex-A55 / AArch64 / musl
- Install destination: merge the archive contents into the root of the same data partition that contains `Games/` and `System/`

This is a prerelease. CI verifies the ELF architecture, rejects a glibc
`libc.so.6` dependency, resolves runtime dependencies, opens every library
with `RTLD_NOW`, and checks `retro_api_version() == 1`. These checks establish
loader compatibility; actual ROM launch, gameplay, performance and audio
still require on-device validation. BIOS files are not included.

## Core filename mapping

| Platform | Core | Installed filename |
|---|---|---|
| GBA | mGBA | `mgba_libretro.so` |
| GBA | gpSP | `gpsp_libretro.so` |
| SFC | Mednafen Supafaust | `supafaust_libretro.so` |
| FC | FCEUmm | `fceumm_libretro.so` |
| Arcade / Neo Geo | FinalBurn Neo | `fbneo_libretro.so` |
| MD | Genesis Plus GX | `genesis_plus_gx_libretro.so` |
| PS1 | PCSX ReARMed | `pcsx_rearmed_libretro.so` |
| PCE | Mednafen PCE Fast | `pce_fast_libretro.so` |

GB and GBC use the frontend built-in core and do not need external libraries.
The two Mednafen builds are renamed to the exact basenames expected by the
frontend. Runtime dependencies that are not part of musl are placed in
`System/lib/`; the BaseOS launcher adds that directory to `LD_LIBRARY_PATH`.

## Build and reuse

The build recipe is maintained on branch
`port/rg353ps-cores-aarch64-musl` in
`packaging/rg353ps/build-cores.sh` and
`packaging/rg353ps/cores.Dockerfile`. The workflow caches source checkouts and
compiled objects. Rebuild only when a core source, compiler configuration or
target ABI changes; publish a new versioned pack instead of replacing v1.0.0.

The frontend test workflow builds Simplified Chinese only. Release tags build
two frontend binaries (Chinese and English); neither job invokes the core
builder. The versioned core pack is installed once and reused across frontend
updates.
