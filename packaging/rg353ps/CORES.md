# RG353PS libretro cores

This is a separate build product from the Slot frontend. The frontend package
does not rebuild or replace emulator cores. Build the core pack only when a
core source or the target ABI changes; install the resulting versioned release
asset onto the same data partition as Games/ and System/.

## Target and ABI

- Device: Anbernic RG353PS, RK3566 / Cortex-A55.
- ELF: AArch64.
- C library: musl, built in an Alpine Linux arm64 container.
- Build output path: System/.
- Runtime dependency path: System/lib/ (the BaseOS Slot launcher already adds it to LD_LIBRARY_PATH).

Do not substitute generic linux/aarch64 Buildbot binaries without checking
their C library ABI. An AArch64 glibc core is not interchangeable with a musl core.

## Build

The workflow is RG353PS reusable libretro cores. It builds eight external
cores and creates a prerelease containing:

- slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip
- the adjacent .sha256 checksum.

The build runner also caches the source checkouts and native object files under
`.core-work`; a repeat run with the same builder definition resumes incremental
compilation instead of starting every core from a clean tree. The first corrected
run still needs to create that cache once.

The versioned release asset is the reusable result. Normal frontend-only builds
do not invoke the core builder, so a UI/input change does not cause another
emulator-core build. When deliberately rebuilding, bump the core-pack version/tag
rather than overwriting a previously published package.

## Core-name mapping

The frontend Core::as_str() names are the install names. Two upstream
Mednafen targets have a longer upstream basename; the build script renames
them to the exact filenames the frontend searches for.

| Platform | Upstream project | Installed filename |
|---|---|---|
| GBA | mGBA | mgba_libretro.so |
| GBA | gpSP | gpsp_libretro.so |
| SFC | Mednafen Supafaust | supafaust_libretro.so |
| FC | FCEUmm | fceumm_libretro.so |
| Arcade / Neo Geo | FinalBurn Neo | fbneo_libretro.so |
| MD | Genesis Plus GX | genesis_plus_gx_libretro.so |
| PS1 | PCSX ReARMed | pcsx_rearmed_libretro.so |
| PCE | Mednafen PCE Fast | pce_fast_libretro.so |

GB and GBC use the frontend's built-in core and do not require external .so files.

## Verification boundary

CI verifies each output is an AArch64 ELF, rejects a glibc libc.so.6
dependency, resolves required non-musl runtime libraries into System/lib/,
opens the library with RTLD_NOW, and checks retro_api_version() == 1.
This is a binary/loader check, not a substitute for testing actual ROMs on
the RG353PS. BIOS files are not included; users must provide legally obtained
firmware where required.

Third-party licence files and exact source commit hashes are recorded in the ZIP.
