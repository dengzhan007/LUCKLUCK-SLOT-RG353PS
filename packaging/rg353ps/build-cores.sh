#!/usr/bin/env bash
# Build the external libretro cores used by LUCKLUCK SLOT for RG353PS.
# Run inside an Alpine AArch64/musl container. Frontend-only builds do not run this.
set -Eeuo pipefail

WORK="$(printenv CORE_WORK || echo /work/cores)"
OUT="$(printenv CORE_OUT || echo /out)"
JOBS="$(printenv MAKE_JOBS || echo 2)"
SYS="$OUT/System"
LIC="$SYS/licenses/cores"
LIB="$SYS/lib"
mkdir -p "$WORK/src" "$SYS" "$LIC" "$LIB"
for command in lddtree zip readelf gcc g++; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "required build command missing: $command" >&2
    exit 127
  }
done
rm -f "$SYS/"*_libretro.so "$LIB/"* "$LIC/"* "$OUT/"slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip*

CORE_CC="$(printenv CORE_CC || echo 'gcc -mcpu=cortex-a55')"
CORE_CXX="$(printenv CORE_CXX || echo 'g++ -mcpu=cortex-a55')"
export MAKEFLAGS="-j$JOBS"
: > "$WORK/source-revisions.tsv"
: > "$WORK/output-map.tsv"

clone_core() {
  local key="$1" url="$2" dir="$3"
  if [ ! -d "$dir/.git" ]; then
    git clone --depth 1 --recurse-submodules --shallow-submodules "$url" "$dir"
  fi
  git -C "$dir" submodule update --init --recursive
  printf '%s\t%s\t%s\n' "$key" "$url" "$(git -C "$dir" rev-parse HEAD)" >> "$WORK/source-revisions.tsv"
}

find_output() {
  local root="$1" file="$2" found
  if [ -f "$root/$file" ]; then printf '%s\n' "$root/$file"; return; fi
  found="$(find "$root" -type f -name "$file" -print -quit)"
  [ -n "$found" ] || { echo "missing build output: $file in $root" >&2; return 1; }
  printf '%s\n' "$found"
}

copy_core() {
  local key="$1" source="$2" output="$3" src_file="$4" license_dir="$5"
  local built license found_license=0
  built="$(find_output "$source" "$src_file")"
  install -m 0644 "$built" "$SYS/$output"
  printf '%s\t%s\t%s\n' "$key" "$src_file" "$output" >> "$WORK/output-map.tsv"
  for license in LICENSE LICENSE.txt COPYING COPYING.txt COPYING.md LICENSE.md; do
    if [ -f "$license_dir/$license" ]; then
      install -m 0644 "$license_dir/$license" "$LIC/$key-$license"
      found_license=1
      break
    fi
  done
  if [ "$found_license" -eq 0 ]; then echo "warning: no top-level licence found for $key" >&2; fi
}

echo "== mGBA =="
MGBA="$WORK/src/mgba"
clone_core mgba https://github.com/libretro/mgba.git "$MGBA"
cmake -S "$MGBA" -B "$MGBA/build-slot" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_C_FLAGS="-O3 -DNDEBUG -mcpu=cortex-a55" \
  -DCMAKE_CXX_FLAGS="-O3 -DNDEBUG -mcpu=cortex-a55" \
  -DLIBMGBA_ONLY=ON -DBUILD_LIBRETRO=ON \
  -DBUILD_QT=OFF -DBUILD_SDL=OFF -DBUILD_PYTHON=OFF \
  -DBUILD_PERF=OFF -DBUILD_TEST=OFF -DBUILD_SUITE=OFF \
  -DBUILD_CINEMA=OFF -DBUILD_HEADLESS=OFF -DBUILD_EXAMPLE=OFF \
  -DBUILD_SHARED=OFF -DBUILD_STATIC=OFF -DSKIP_LIBRARY=ON \
  -DENABLE_SCRIPTING=OFF -DENABLE_DEBUGGERS=OFF -DENABLE_GDB_STUB=OFF \
  -DUSE_FFMPEG=OFF -DUSE_ZLIB=OFF -DUSE_MINIZIP=OFF -DUSE_PNG=OFF \
  -DUSE_LIBZIP=OFF -DUSE_SQLITE3=OFF -DUSE_LUA=OFF -DUSE_JSON_C=OFF \
  -DUSE_FREETYPE=OFF -DUSE_LZMA=OFF -DUSE_DISCORD_RPC=OFF
cmake --build "$MGBA/build-slot" --target mgba_libretro --parallel "$JOBS"
copy_core mgba "$MGBA/build-slot" mgba_libretro.so mgba_libretro.so "$MGBA"

echo "== gpSP =="
GPSP="$WORK/src/gpsp"
clone_core gpsp https://github.com/libretro/gpsp.git "$GPSP"
make -C "$GPSP" platform=unix CC="$CORE_CC" CXX="$CORE_CXX" -j"$JOBS"
copy_core gpsp "$GPSP" gpsp_libretro.so gpsp_libretro.so "$GPSP"

echo "== Supafaust (SFC) =="
SUPA="$WORK/src/supafaust"
clone_core mednafen_supafaust https://github.com/libretro/supafaust.git "$SUPA"
make -C "$SUPA" platform=unix CC="$CORE_CC" CXX="$CORE_CXX" -j"$JOBS"
# Upstream calls this mednafen_supafaust; SLOT searches for supafaust.
copy_core supafaust "$SUPA" supafaust_libretro.so mednafen_supafaust_libretro.so "$SUPA"

echo "== FCEUmm (FC) =="
FCEU="$WORK/src/fceumm"
clone_core fceumm https://github.com/libretro/libretro-fceumm.git "$FCEU"
make -C "$FCEU" -f Makefile.libretro platform=unix CC="$CORE_CC" CXX="$CORE_CXX" -j"$JOBS"
copy_core fceumm "$FCEU" fceumm_libretro.so fceumm_libretro.so "$FCEU"

echo "== FinalBurn Neo (Arcade / Neo Geo) =="
FBNEO="$WORK/src/fbneo"
clone_core fbneo https://github.com/libretro/FBNeo.git "$FBNEO"
make -C "$FBNEO/src/burner/libretro" platform=unix CC="$CORE_CC" CXX="$CORE_CXX" -j"$JOBS"
copy_core fbneo "$FBNEO/src/burner/libretro" fbneo_libretro.so fbneo_libretro.so "$FBNEO"

echo "== Genesis Plus GX (MD) =="
GENPLUS="$WORK/src/genesis_plus_gx"
clone_core genesis_plus_gx https://github.com/libretro/Genesis-Plus-GX.git "$GENPLUS"
make -C "$GENPLUS" -f Makefile.libretro platform=unix CC="$CORE_CC" CXX="$CORE_CXX" -j"$JOBS"
copy_core genesis_plus_gx "$GENPLUS" genesis_plus_gx_libretro.so genesis_plus_gx_libretro.so "$GENPLUS"

echo "== PCSX ReARMed (PS1) =="
PCSX="$WORK/src/pcsx_rearmed"
clone_core pcsx_rearmed https://github.com/libretro/pcsx_rearmed.git "$PCSX"
make -C "$PCSX" -f Makefile.libretro platform=unix ARCH=aarch64 DYNAREC=ari64 THREAD_RENDERING=0 \
  CC="$CORE_CC" CXX="$CORE_CXX" -j"$JOBS"
copy_core pcsx_rearmed "$PCSX" pcsx_rearmed_libretro.so pcsx_rearmed_libretro.so "$PCSX"

echo "== Beetle PCE Fast (PCE) =="
PCE="$WORK/src/pce_fast"
clone_core mednafen_pce_fast https://github.com/libretro/beetle-pce-fast-libretro.git "$PCE"
make -C "$PCE" platform=unix CC="$CORE_CC" CXX="$CORE_CXX" -j"$JOBS"
# Upstream calls this mednafen_pce_fast; SLOT searches for pce_fast.
copy_core pce_fast "$PCE" pce_fast_libretro.so mednafen_pce_fast_libretro.so "$PCE"

echo "== Verify architecture, dependency closure and libretro ABI =="
cat > "$WORK/core-probe.c" <<'EOF'
#include <dlfcn.h>
#include <stdio.h>
typedef unsigned (*retro_api_version_fn)(void);
int main(int argc, char **argv) {
    if (argc != 2) return 64;
    void *handle = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!handle) {
        fprintf(stderr, "dlopen(%s): %s\n", argv[1], dlerror());
        return 1;
    }
    dlerror();
    retro_api_version_fn version = (retro_api_version_fn)dlsym(handle, "retro_api_version");
    const char *err = dlerror();
    if (err || !version) {
        fprintf(stderr, "missing retro_api_version: %s\n", err ? err : "null symbol");
        return 2;
    }
    unsigned api = version();
    if (api != 1) {
        fprintf(stderr, "unexpected libretro API version: %u\n", api);
        return 3;
    }
    dlclose(handle);
    return 0;
}
EOF
gcc -O2 -Wall -Wextra -o "$WORK/core-probe" "$WORK/core-probe.c" -ldl

: > "$OUT/CORE-RUNTIME-DEPENDENCIES.txt"
for core in "$SYS/"*_libretro.so; do
  [ -f "$core" ] || { echo "no core libraries produced" >&2; exit 1; }
  readelf -h "$core" | grep -q 'AArch64' || { echo "not AArch64: $core" >&2; exit 1; }
  if readelf -d "$core" | grep -q 'libc\.so\.6'; then
    echo "glibc-linked core is not accepted: $core" >&2
    exit 1
  fi
  while IFS= read -r dep; do
    [ -f "$dep" ] || continue
    [ "$(readlink -f "$dep")" = "$(readlink -f "$core")" ] && continue
    case "$dep" in */ld-musl-*.so.1|*/libc.musl-*.so.1) continue ;; esac
    soname="$(readelf -d "$dep" 2>/dev/null | sed -n 's/.*Library soname: \[\(.*\)\].*/\1/p' | head -n 1)"
    [ -n "$soname" ] || soname="$(basename "$dep")"
    if [ ! -f "$LIB/$soname" ]; then
      cp -L "$dep" "$LIB/$soname"
      printf '%s <- %s\n' "$soname" "$dep" >> "$OUT/CORE-RUNTIME-DEPENDENCIES.txt"
    fi
  done < <(lddtree -l "$core")
  LD_LIBRARY_PATH="$LIB:/usr/lib:/lib" "$WORK/core-probe" "$core"
  echo "PASS: $(basename "$core")"
done

cat > "$OUT/CORE-BUILD-INFO.txt" <<EOF
Project: LUCKLUCK SLOT (66slot)
Target device: Anbernic RG353PS / Rockchip RK3566 / Cortex-A55
ELF architecture: AArch64
C library ABI: musl (built in Alpine Linux arm64)
Compiler: $(gcc -dumpmachine) / GCC $(gcc -dumpfullversion)
C++ compiler: $(g++ -dumpmachine) / G++ $(g++ -dumpfullversion)
Build date (UTC): $(date -u +%Y-%m-%dT%H:%M:%SZ)
Parallel jobs: $JOBS

The eight libraries were checked for AArch64 ELF, absence of glibc libc.so.6,
successful RTLD_NOW dlopen, and libretro API version 1.
The ROM BIOS files are not included.

Source repositories and exact checked-out commits:
EOF
cat "$WORK/source-revisions.tsv" >> "$OUT/CORE-BUILD-INFO.txt"
{
  echo
  echo "Upstream output names mapped to SLOT filenames:"
  cat "$WORK/output-map.tsv"
} >> "$OUT/CORE-BUILD-INFO.txt"

cat > "$OUT/README-RG353PS-CORES.txt" <<'EOF'
LUCKLUCK SLOT (66slot) — RG353PS emulator core pack
====================================================

Target: Anbernic RG353PS / RK3566 / AArch64 / BaseOS
ABI: AArch64 + musl. Do not use these libraries on glibc-based systems.

Install
-------
Extract this ZIP into the root of the same user-data partition that contains
Games/ and System/. Merge the System directory; do not delete existing files.
This archive supplies only the eight libretro cores listed below and any
non-musl runtime libraries they require under System/lib/.

Core files
----------
System/mgba_libretro.so
System/gpsp_libretro.so
System/supafaust_libretro.so
System/fceumm_libretro.so
System/fbneo_libretro.so
System/genesis_plus_gx_libretro.so
System/pcsx_rearmed_libretro.so
System/pce_fast_libretro.so

GB and GBC use SLOT's built-in core; no separate library is required.
The two Mednafen upstream products are renamed to the exact basenames SLOT
looks for. The archive does not include ROMs or copyrighted BIOS files.

Validation
----------
Every core was checked as AArch64 ELF, loaded with RTLD_NOW in the same
Alpine/musl build environment, and queried for libretro API version 1.
This validates binary/loader compatibility, not full gameplay or performance
on physical RG353PS hardware. Test each machine and its game content on-device.

Build provenance and third-party licences are in CORE-BUILD-INFO.txt and
System/licenses/cores/.
EOF

(
  cd "$OUT"
  : > CORE-SHA256SUMS.txt
  for file in System/*_libretro.so System/lib/*; do
    [ -f "$file" ] && sha256sum "$file" >> CORE-SHA256SUMS.txt
  done
  zip -qr slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip \
    System README-RG353PS-CORES.txt CORE-BUILD-INFO.txt \
    CORE-RUNTIME-DEPENDENCIES.txt CORE-SHA256SUMS.txt
  sha256sum slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip \
    > slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip.sha256
)
ls -lh "$OUT/slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip" \
  "$OUT/slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip.sha256"
