#!/bin/sh
# Stage the official 640x480 UI art/font bundle without importing its
# architecture-specific executable or emulator core binaries.
set -eu

WORK="${1:-}"
STAGE="${2:-}"
[ -n "$WORK" ] && [ -n "$STAGE" ] || {
  echo "usage: stage-upstream-assets.sh WORK_DIR STAGE_DIR" >&2
  exit 2
}

UPSTREAM_TAG="v2026.11.09"
ASSET_NAME="66slot-frontend-System-640480-V20261109-fix.zip"
ASSET_URL="https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/download/$UPSTREAM_TAG/$ASSET_NAME"
ASSET_SHA256="5f71f6b8f6fd1c6c4787158b1ccda7740f7b1d68b9b76948d14b71b3ed670c35"

for cmd in curl unzip sha256sum find; do
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "missing required asset-staging tool: $cmd" >&2
    exit 1
  }
done

ARCHIVE="$WORK/$ASSET_NAME"
EXTRACT="$WORK/upstream-640-ui-extract"
TMP="$ARCHIVE.part"

mkdir -p "$WORK"
if [ ! -s "$ARCHIVE" ] || ! printf '%s  %s\n' "$ASSET_SHA256" "$ARCHIVE" | sha256sum -c - >/dev/null 2>&1; then
  rm -f "$ARCHIVE" "$TMP"
  echo "== fetching upstream 640 UI assets: $UPSTREAM_TAG =="
  curl --fail --location --retry 3 --retry-all-errors --silent --show-error \
    "$ASSET_URL" -o "$TMP"
  printf '%s  %s\n' "$ASSET_SHA256" "$TMP" | sha256sum -c -
  mv "$TMP" "$ARCHIVE"
fi
printf '%s  %s\n' "$ASSET_SHA256" "$ARCHIVE" | sha256sum -c -

rm -rf "$EXTRACT" "$STAGE"
mkdir -p "$EXTRACT" "$STAGE"
unzip -q "$ARCHIVE" -d "$EXTRACT"

CARTS_DIR="$(find "$EXTRACT" -type d -ipath '*/System/Carts' -print -quit)"
if [ -z "$CARTS_DIR" ]; then
  echo "upstream archive has no System/Carts directory; relevant archive paths:" >&2
  unzip -Z1 "$ARCHIVE" | grep -iE '(^|/)(system|carts|music|overlay|bootlogos)(/|$)|SLOT_SYS|type_gb' | head -160 >&2 || :
  exit 1
fi
UPSTREAM_ROOT="$(dirname "$(dirname "$CARTS_DIR")")"
SYSTEM_FONTS="$UPSTREAM_ROOT/System/fonts"
[ -d "$SYSTEM_FONTS" ] || {
  echo "upstream archive has no System/fonts directory; interface would fall back to label.ttf without CJK" >&2
  exit 1
}
find "$SYSTEM_FONTS" -maxdepth 1 -type f \( -iname '*.ttf' -o -iname '*.otf' -o -iname '*.ttc' \) -print -quit | grep -q . || {
  echo "upstream System/fonts has no usable TTF/OTF/TTC file" >&2
  exit 1
}
[ -s "$UPSTREAM_ROOT/System/slotnet.sh" ] || {
  echo "upstream archive has no System/slotnet.sh" >&2
  exit 1
}
sh -n "$UPSTREAM_ROOT/System/slotnet.sh"
[ -d "$UPSTREAM_ROOT/Music/Fonts" ] || {
  echo "upstream archive is missing Music/Fonts" >&2
  exit 1
}
[ -d "$UPSTREAM_ROOT/Overlay" ] || {
  echo "upstream archive is missing Overlay" >&2
  exit 1
}
[ -d "$UPSTREAM_ROOT/Bootlogos" ] || {
  echo "upstream archive is missing Bootlogos" >&2
  exit 1
}

# Machine selector badges: absent files are rendered as empty plates.
for machine in GBA GB GBC SFC FC NEOGEO ARCADE MD PS1 PCE; do
  file="$UPSTREAM_ROOT/System/Carts/SLOT_SYS_$machine.png"
  [ -s "$file" ] || {
    echo "upstream UI bundle is missing machine badge: $file" >&2
    exit 1
  }
done

# The lower-left GBA/GB/GBC marks are separate artwork from selector badges.
# Some upstream package revisions omit these legacy files even though this 640
# frontend still probes their names. Record the exact release contents; a
# pinned compatibility supplement is applied below only if they are absent.
MISSING_MACHINE_MARKS=""
LEGACY_TAG="not-needed"
LEGACY_SHA256="not-needed"
for file in type_gb.png type_gbc.png type_gba.png \
            type_gb_light.png type_gbc_light.png type_gba_light.png; do
  if [ ! -s "$UPSTREAM_ROOT/System/Carts/$file" ]; then
    MISSING_MACHINE_MARKS="$MISSING_MACHINE_MARKS $file"
  fi
done
if [ -n "$MISSING_MACHINE_MARKS" ]; then
  # The 2026-11-09 640 release currently omits these six files even though the
  # frontend still reads them for the lower-left machine marks. The upstream
  # 2026-10-02 package explicitly added the complete dark/light pair. Import
  # ONLY these six generic 75x25 RGBA PNGs as a compatibility supplement; do
  # not take its 720-layout badges, card art, fonts, masks or executable.
  LEGACY_TAG="v2026.10.02"
  LEGACY_NAME="slot-frontend-System-2026-10-02.zip"
  LEGACY_SHA256="6c5085907d41f76bb56e5a17a67389409feb647c6fc4567aae27c9c2ae5bdb8a"
  LEGACY_ARCHIVE="$WORK/$LEGACY_NAME"
  LEGACY_TMP="$LEGACY_ARCHIVE.part"
  LEGACY_EXTRACT="$WORK/upstream-legacy-icons-extract"
  if [ ! -s "$LEGACY_ARCHIVE" ] || ! printf '%s  %s\n' "$LEGACY_SHA256" "$LEGACY_ARCHIVE" | sha256sum -c - >/dev/null 2>&1; then
    rm -f "$LEGACY_ARCHIVE" "$LEGACY_TMP"
    echo "== fetching upstream legacy theme marks: $LEGACY_TAG =="
    curl --fail --location --retry 3 --retry-all-errors --silent --show-error \
      "https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/download/$LEGACY_TAG/$LEGACY_NAME" \
      -o "$LEGACY_TMP"
    printf '%s  %s\n' "$LEGACY_SHA256" "$LEGACY_TMP" | sha256sum -c -
    mv "$LEGACY_TMP" "$LEGACY_ARCHIVE"
  fi
  printf '%s  %s\n' "$LEGACY_SHA256" "$LEGACY_ARCHIVE" | sha256sum -c -
  rm -rf "$LEGACY_EXTRACT"
  mkdir -p "$LEGACY_EXTRACT"
  unzip -q "$LEGACY_ARCHIVE" -d "$LEGACY_EXTRACT"
  LEGACY_CARTS_DIR="$(find "$LEGACY_EXTRACT" -type d -ipath '*/System/Carts' -print -quit)"
  [ -n "$LEGACY_CARTS_DIR" ] || {
    echo "legacy upstream package has no System/Carts directory" >&2
    exit 1
  }
  for file in type_gb.png type_gbc.png type_gba.png \
              type_gb_light.png type_gbc_light.png type_gba_light.png; do
    if [ ! -s "$CARTS_DIR/$file" ]; then
      [ -s "$LEGACY_CARTS_DIR/$file" ] || {
        echo "legacy upstream package is missing required machine mark: $file" >&2
        exit 1
      }
      cp -p "$LEGACY_CARTS_DIR/$file" "$CARTS_DIR/$file"
    fi
  done
  echo "Added missing legacy machine marks from $LEGACY_TAG ($LEGACY_SHA256)"
fi

for file in type_gb.png type_gbc.png type_gba.png \
            type_gb_light.png type_gbc_light.png type_gba_light.png; do
  [ -s "$CARTS_DIR/$file" ] || {
    echo "required machine mark is still missing: $file" >&2
    exit 1
  }
done

# These are runtime-loaded by the music deck, not embedded in System/slot.
for file in unifont-16px.otf press-start-2p.ttf; do
  [ -s "$UPSTREAM_ROOT/Music/Fonts/$file" ] || {
    echo "upstream UI bundle is missing music-panel font: $file" >&2
    exit 1
  }
done

# Only copy data assets. Keep the RG353PS-compiled System/slot, any separately
# supplied core libraries and all user files out of this import path.
mkdir -p "$STAGE/System"
cp -a "$CARTS_DIR" "$STAGE/System/Carts"
cp -a "$SYSTEM_FONTS" "$STAGE/System/fonts"
cp -p "$UPSTREAM_ROOT/System/slotnet.sh" "$STAGE/System/slotnet.sh"
chmod 0755 "$STAGE/System/slotnet.sh"
if [ -s "$UPSTREAM_ROOT/System/wifi.conf.example" ]; then
  cp -p "$UPSTREAM_ROOT/System/wifi.conf.example" "$STAGE/System/"
fi
sh -n "$STAGE/System/slotnet.sh"
cp -a "$UPSTREAM_ROOT/Music" "$STAGE/"
cp -a "$UPSTREAM_ROOT/Overlay" "$STAGE/"
cp -a "$UPSTREAM_ROOT/Bootlogos" "$STAGE/"
if [ -s "$UPSTREAM_ROOT/System/boot_opening.slotvid" ]; then
  cp -a "$UPSTREAM_ROOT/System/boot_opening.slotvid" "$STAGE/System/"
fi

cat > "$STAGE/UPSTREAM-ASSET-SOURCE.txt" <<EOF
UI asset source: RetroGame66/LUCKLUCK-SLOT
Release: $UPSTREAM_TAG
Package: $ASSET_NAME
SHA-256: $ASSET_SHA256
Imported paths: System/Carts, System/fonts, upstream System/slotnet.sh, optional System/wifi.conf.example, Music, Overlay, Bootlogos
Missing legacy machine marks:$MISSING_MACHINE_MARKS
Legacy mark supplement: RetroGame66/LUCKLUCK-SLOT $LEGACY_TAG (only six type_*.png files, sha256 $LEGACY_SHA256)
Not imported: System/slot, libretro cores, saves, ROMs or user content
EOF
find "$STAGE/System/Carts" -maxdepth 2 -type f -printf '%P\\n' | sort > "$STAGE/UPSTREAM-CARTS-INDEX.txt"

echo "== upstream UI asset staging verified =="
printf 'System/Carts PNGs: '
find "$STAGE/System/Carts" -type f -iname '*.png' | wc -l
printf 'System font files: '
find "$STAGE/System/fonts" -maxdepth 1 -type f \( -iname '*.ttf' -o -iname '*.otf' -o -iname '*.ttc' \) | wc -l
printf 'Music files: '
find "$STAGE/Music" -type f | wc -l
printf 'Overlay files: '
find "$STAGE/Overlay" -type f | wc -l
printf 'Bootlogo files: '
find "$STAGE/Bootlogos" -type f | wc -l
echo "Asset source: $UPSTREAM_TAG ($ASSET_SHA256)"
