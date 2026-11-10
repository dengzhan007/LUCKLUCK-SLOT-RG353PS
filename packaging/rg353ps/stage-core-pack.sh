#!/bin/sh
# Fetch and stage the pinned RG353PS AArch64/musl libretro core pack.
set -eu

WORK="${1:-}"
STAGE="${2:-}"
[ -n "$WORK" ] && [ -n "$STAGE" ] || {
  echo "usage: stage-core-pack.sh WORK_DIR STAGE_DIR" >&2
  exit 2
}

CORE_TAG="rg353ps-cores-aarch64-musl-v1.0.0"
ARCHIVE_NAME="slot-66-rg353ps-cores-aarch64-musl-v1.0.0.zip"
ARCHIVE_SHA256="26907ff4b2013c654c6cd58720ccd0f167609a203744ec01a9acae563b793ec8"
URL="https://github.com/dengzhan007/LUCKLUCK-SLOT-RG353PS/releases/download/$CORE_TAG/$ARCHIVE_NAME"

for cmd in curl unzip sha256sum find file; do
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "missing required core-pack tool: $cmd" >&2
    exit 1
  }
done

ARCHIVE="$WORK/$ARCHIVE_NAME"
TMP="$ARCHIVE.part"
EXTRACT="$WORK/core-pack-extract"
rm -rf "$STAGE" "$EXTRACT"
mkdir -p "$WORK" "$STAGE"

if [ ! -s "$ARCHIVE" ] || ! printf '%s  %s\n' "$ARCHIVE_SHA256" "$ARCHIVE" | sha256sum -c - >/dev/null 2>&1; then
  rm -f "$ARCHIVE" "$TMP"
  echo "== fetching pinned RG353PS core pack: $CORE_TAG =="
  curl --fail --location --retry 3 --retry-all-errors --silent --show-error "$URL" -o "$TMP"
  printf '%s  %s\n' "$ARCHIVE_SHA256" "$TMP" | sha256sum -c -
  mv "$TMP" "$ARCHIVE"
fi
printf '%s  %s\n' "$ARCHIVE_SHA256" "$ARCHIVE" | sha256sum -c -

mkdir -p "$EXTRACT"
unzip -q "$ARCHIVE" -d "$EXTRACT"
SYSTEM_DIR="$(find "$EXTRACT" -type d -ipath '*/System' -print -quit)"
[ -n "$SYSTEM_DIR" ] || {
  echo "core pack has no System directory" >&2
  unzip -Z1 "$ARCHIVE" | head -100 >&2 || :
  exit 1
}

mkdir -p "$STAGE/System"
cp -a "$SYSTEM_DIR/." "$STAGE/System/"

CORES="mgba_libretro.so gpsp_libretro.so fceumm_libretro.so supafaust_libretro.so fbneo_libretro.so genesis_plus_gx_libretro.so pcsx_rearmed_libretro.so pce_fast_libretro.so"
for core in $CORES; do
  path="$STAGE/System/$core"
  [ -s "$path" ] || { echo "missing core from pack: $path" >&2; exit 1; }
  file "$path" | grep -q 'AArch64\|aarch64' || {
    echo "wrong architecture for $path: $(file "$path")" >&2
    exit 1
  }
done

[ -d "$STAGE/System/licenses" ] || {
  echo "core pack is missing System/licenses" >&2
  exit 1
}
find "$STAGE/System/licenses" -type f | grep -q . || {
  echo "core pack System/licenses is empty" >&2
  exit 1
}
[ -d "$STAGE/System/lib" ] || {
  echo "core pack is missing System/lib runtime dependencies" >&2
  exit 1
}

cat > "$STAGE/CORE-PACK-SOURCE.txt" <<EOF
Core pack: $CORE_TAG
Archive: $ARCHIVE_NAME
SHA-256: $ARCHIVE_SHA256
Target: RG353PS / RK3566 / AArch64 / musl
Included: $CORES
Source: dengzhan007/LUCKLUCK-SLOT-RG353PS release $CORE_TAG
EOF

echo "== RG353PS emulator core pack verified =="
printf 'Core libraries: '
find "$STAGE/System" -maxdepth 1 -type f -name '*_libretro.so' | wc -l
printf 'Runtime libraries: '
find "$STAGE/System/lib" -type f | wc -l
printf 'License/source files: '
find "$STAGE/System/licenses" -type f | wc -l
echo "Core pack source: $CORE_TAG ($ARCHIVE_SHA256)"
