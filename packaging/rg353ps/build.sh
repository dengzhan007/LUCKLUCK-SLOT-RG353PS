#!/bin/sh
# Build a frontend-only RG353PS test ZIP from this branch and the BaseOS adapter.
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
BASEOS_ROOT="${BASEOS_ROOT:-}"
[ -n "$BASEOS_ROOT" ] || { echo "set BASEOS_ROOT to a BaseOS-RG353PS checkout" >&2; exit 2; }

FRONTEND_PORT="$BASEOS_ROOT/platforms/rg353ps/frontends/slot"
FRONTEND_MANIFEST="$FRONTEND_PORT/manifest.conf"
FRONTEND_LAUNCHER="$FRONTEND_PORT/launch.sh"
for file in "$FRONTEND_MANIFEST" "$FRONTEND_LAUNCHER" \
            "$FRONTEND_PORT/fbdev.rs"; do
  [ -f "$file" ] || { echo "missing RG353PS adapter input: $file" >&2; exit 1; }
done

for declaration in adapter_api=1 volume_owner=frontend power_owner=frontend; do
  grep -Fqx "$declaration" "$FRONTEND_MANIFEST" || {
    echo "invalid adapter manifest (missing $declaration)" >&2; exit 1;
  }
done
PRIORITY="$(sed -n 's/^priority=//p' "$FRONTEND_MANIFEST")"
case "$PRIORITY" in
  ''|*[!0-9]*) echo "invalid adapter priority" >&2; exit 1 ;;
esac
[ "$PRIORITY" -le 9999 ] || { echo "adapter priority must be 0..9999" >&2; exit 1; }
grep -Fqx '# baseos: volume-owned' "$FRONTEND_LAUNCHER"
grep -Fqx '# baseos: power-owned' "$FRONTEND_LAUNCHER"

SLOT_REV="${SLOT_REV:-$(git -C "$REPO_ROOT" rev-parse HEAD)}"
BASEOS_REF="${BASEOS_REF:-main}"
SLOT_BRANCH="${SLOT_BRANCH:-port/rg353ps-640-native}"
LANGUAGE="${LANGUAGE:-zh}"
PACKAGE_TAG="${PACKAGE_TAG:-test}"
SLOT_REPO="${SLOT_REPO:-${GITHUB_REPOSITORY:-dengzhan007/LUCKLUCK-SLOT-RG353PS}}"
case "$LANGUAGE" in
  zh|en) ;;
  *) echo "LANGUAGE must be zh or en (got: $LANGUAGE)" >&2; exit 2 ;;
esac
WORK="${RG353PS_WORK:-$REPO_ROOT/work/rg353ps}"
OUT="$WORK/frontends"
SRC_HOST="$WORK/slot-src"
ASSET_STAGE="$WORK/upstream-ui-assets"
IMAGE="${SLOT_BUILD_IMAGE:-slot-66-rg353ps-build:rust-1.96}"

mkdir -p "$OUT" "$WORK/cargo-registry" "$WORK/cargo-git" "$WORK/cargo-target"
sh "$HERE/stage-upstream-assets.sh" "$WORK" "$ASSET_STAGE"
sh "$HERE/stage-core-pack.sh" "$WORK" "$ASSET_STAGE/CorePack"
rm -rf "$SRC_HOST"
mkdir -p "$SRC_HOST"
git -C "$REPO_ROOT" archive --format=tar HEAD | tar -xf - -C "$SRC_HOST"

echo "== building AArch64/musl toolchain image =="
docker build --platform linux/arm64 -f "$HERE/Dockerfile" -t "$IMAGE" "$HERE"

echo "== building RG353PS native-640x480 frontend ($LANGUAGE) =="
echo "   slot: $SLOT_REPO @ $SLOT_REV ($SLOT_BRANCH)"
echo "   BaseOS adapter: dengzhan007/BaseOS-RG353PS @ $BASEOS_REF"
docker run --rm --network host --platform linux/arm64 \
  -e "SLOT_REV=$SLOT_REV" \
  -e "SLOT_BRANCH=$SLOT_BRANCH" \
  -e "BASEOS_REF=$BASEOS_REF" \
  -e "LANGUAGE=$LANGUAGE" \
  -e "PACKAGE_TAG=$PACKAGE_TAG" \
  -e "SLOT_REPO=$SLOT_REPO" \
  -e "CARGO_HOME=/usr/local/cargo" \
  -e "CARGO_BUILD_JOBS=2" \
  -v "$SRC_HOST:/slot-up:ro" \
  -v "$FRONTEND_PORT:/slot-port:ro" \
  -v "$HERE:/slot-build:ro" \
  -v "$ASSET_STAGE:/upstream-assets:ro" \
  -v "$OUT:/out" \
  -v "$WORK/cargo-registry:/usr/local/cargo/registry" \
  -v "$WORK/cargo-git:/usr/local/cargo/git" \
  -v "$WORK/cargo-target:/cargo-target" \
  "$IMAGE" sh -euxc '
    echo "[rg353ps] container entered"
    export RUSTFLAGS="-C target-feature=-crt-static"
    SRC=/tmp/slot-src
    TARGET=/cargo-target
    PKG=/tmp/package
    LANGUAGE="${LANGUAGE:-zh}"
    PACKAGE_TAG="${PACKAGE_TAG:-test}"
    SLOT_REPO="${SLOT_REPO:-dengzhan007/LUCKLUCK-SLOT-RG353PS}"
    case "$LANGUAGE" in
      zh) FEATURES=device; LANGUAGE_NAME="Simplified Chinese" ;;
      en) FEATURES=device,lang-en; LANGUAGE_NAME="English" ;;
      *) echo "LANGUAGE must be zh or en" >&2; exit 2 ;;
    esac

    rm -rf "$SRC" "$PKG"
    mkdir -p "$SRC" "$PKG"
    echo "[rg353ps] staging source"
    cp -R /slot-up/. "$SRC/"
    echo "[rg353ps] source staged"
    cp /slot-port/fbdev.rs "$SRC/crates/slot-gfx/src/fbdev.rs"
    echo "[rg353ps] BaseOS SDL2/KMSDRM surface installed"
    grep -Fqx "pub const OUT_W: u32 = 640;" "$SRC/crates/slot-gfx/src/surface.rs"
    grep -Fqx "pub const OUT_H: u32 = 480;" "$SRC/crates/slot-gfx/src/surface.rs"
    echo "[rg353ps] native 640x480 canvas confirmed"

    # Keep the new upstream AxisMap / device probing code. Apply only the
    # RG353PS-specific input constants and key table with checked anchors.
    echo "[rg353ps] applying measured input mapping"
    python3 /slot-build/apply-rg353ps-input.py "$SRC/crates/slot/src/input/evdev.rs"
    echo "[rg353ps] input mapping applied"

    cd "$SRC"
    echo "=== cargo build (device / release / AArch64-musl; language=$LANGUAGE; features=$FEATURES) ==="
    cargo build --release --target-dir "$TARGET" -p slot --no-default-features --features "$FEATURES"
    BIN="$TARGET/release/slot"
    test -x "$BIN"
    readelf -h "$BIN" | grep -q "AArch64"
    readelf -l "$BIN" | grep -q "ld-musl-aarch64.so.1"

    mkdir -p "$PKG/System/licenses" \
      "$PKG/.baseos/frontends/slot" "$PKG/baseos/frontends/slot"
    cp "$BIN" "$PKG/System/slot"
    chmod 0755 "$PKG/System/slot"
    # Use the upstream card-side script. BaseOS initializes wlan0; Slot owns
    # association, saved credentials and DHCP as it does on the upstream release.
    cp /upstream-assets/System/slotnet.sh "$PKG/System/slotnet.sh"
    chmod 0755 "$PKG/System/slotnet.sh"
    sh -n "$PKG/System/slotnet.sh"

    cp /slot-port/launch.sh "$PKG/.baseos/frontends/slot/launch.sh"
    cp /slot-port/launch.sh "$PKG/baseos/frontends/slot/launch.sh"
    chmod 0755 "$PKG/.baseos/frontends/slot/launch.sh" "$PKG/baseos/frontends/slot/launch.sh"
    cp /slot-port/manifest.conf "$PKG/.baseos/frontends/slot/manifest.conf"
    cp /slot-port/manifest.conf "$PKG/baseos/frontends/slot/manifest.conf"
    chmod 0644 "$PKG/.baseos/frontends/slot/manifest.conf" "$PKG/baseos/frontends/slot/manifest.conf"
    cmp -s "$PKG/.baseos/frontends/slot/manifest.conf" "$PKG/baseos/frontends/slot/manifest.conf"

    for declaration in adapter_api=1 volume_owner=frontend power_owner=frontend; do
      grep -Fqx "$declaration" "$PKG/.baseos/frontends/slot/manifest.conf"
    done
    grep -Fqx "# baseos: volume-owned" "$PKG/.baseos/frontends/slot/launch.sh"
    grep -Fqx "# baseos: power-owned" "$PKG/.baseos/frontends/slot/launch.sh"
    grep -Fqx "# baseos: volume-owned" "$PKG/baseos/frontends/slot/launch.sh"
    grep -Fqx "# baseos: power-owned" "$PKG/baseos/frontends/slot/launch.sh"

    cp LICENSE "$PKG/System/licenses/slot-LICENSE.txt"
    cp NOTICE.md "$PKG/System/licenses/slot-NOTICE.md"
    for license in crates/slot-ui/assets/*.LICENSE.txt crates/slot-ui/assets/*.OFL.txt; do
      [ -f "$license" ] && cp "$license" "$PKG/System/licenses/"
    done
    tar --exclude=.git -czf "$PKG/System/licenses/slot-rg353ps-$SLOT_REV-source.tar.gz" -C "$SRC" .
    "$PKG/System/slot" --init-root "$PKG"

    # The upstream 640 release carries the UI resources as external card files.
    # Import only those data assets; retain the native RG353PS binary and never
    # replace separately provisioned libretro cores.
    cp -a /upstream-assets/System/Carts "$PKG/System/"
    cp -a /upstream-assets/System/fonts "$PKG/System/"
    test -n "$(find "$PKG/System/fonts" -maxdepth 1 -type f \( -iname '*.ttf' -o -iname '*.otf' -o -iname '*.ttc' \) -print -quit)"
    test -s "$PKG/System/slotnet.sh"
    for asset_dir in Music Overlay Bootlogos; do
      test -d "/upstream-assets/$asset_dir"
      cp -a "/upstream-assets/$asset_dir" "$PKG/"
    done
    if [ -s /upstream-assets/System/boot_opening.slotvid ]; then
      cp -a /upstream-assets/System/boot_opening.slotvid "$PKG/System/"
    fi
    cp /upstream-assets/UPSTREAM-ASSET-SOURCE.txt "$PKG/"
    # Add the pinned AArch64/musl core pack to the same release tree. This is a
    # byte-for-byte reuse of the separately validated pack, not a core rebuild.
    cp -a /upstream-assets/CorePack/System/. "$PKG/System/"
    cp /upstream-assets/CorePack/CORE-PACK-SOURCE.txt "$PKG/"
    for core in mgba gpsp fceumm supafaust fbneo genesis_plus_gx pcsx_rearmed pce_fast; do
      test -s "$PKG/System/${core}_libretro.so"
    done

    # Catch an incomplete distribution package before it reaches the SD card.
    for machine in GBA GB GBC SFC FC NEOGEO ARCADE MD PS1 PCE; do
      test -s "$PKG/System/Carts/SLOT_SYS_$machine.png"
    done
    # The PS1 insert animation uses this branded disc when a game has no custom art.
    test -s "$PKG/System/Carts/66SLOT_DISC_PS1.png"
    for icon in type_gb.png type_gbc.png type_gba.png \
                type_gb_light.png type_gbc_light.png type_gba_light.png; do
      test -s "$PKG/System/Carts/$icon"
    done
    test -s "$PKG/Music/Fonts/unifont-16px.otf"
    test -s "$PKG/Music/Fonts/press-start-2p.ttf"
    # Check that asset staging and package copying preserve exact font bytes.
    cmp -s /upstream-assets/System/fonts/Plix-Bold.otf "$PKG/System/fonts/Plix-Bold.otf"
    cmp -s /upstream-assets/Music/Fonts/unifont-16px.otf "$PKG/Music/Fonts/unifont-16px.otf"
    cmp -s /upstream-assets/Music/Fonts/press-start-2p.ttf "$PKG/Music/Fonts/press-start-2p.ttf"
    test -d "$PKG/Overlay"
    test -d "$PKG/Bootlogos"

    cat > "$PKG/README-RG353PS.txt" <<EOF
LUCKLUCK SLOT / 66slot - RG353PS 640x480 ${LANGUAGE} build
========================================================

Target: Anbernic RG353PS / RK3566 / 640x480 panel / BaseOS
Language: $LANGUAGE ($LANGUAGE_NAME)
Display: SDL2 KMSDRM fullscreen + OpenGL ES; not the upstream Mali fbdev EGL path.
This package contains the RG353PS frontend executable, BaseOS adapter,
and the upstream UI art/font bundle: System/Carts/, System/fonts/, Music/, Overlay/ and
Bootlogos/. It also uses the upstream System/slotnet.sh, so Slot owns Wi-Fi
credentials, association and DHCP while BaseOS only initializes the radio. The main artwork is pinned to the upstream 640x480 v2026.11.09
release. That package omits six legacy GB/GBC/GBA theme marks still read by
this frontend; only those six type_*.png files are supplemented from the
checksum-pinned upstream v2026.10.02 package. No 720-layout art is imported.
It does NOT contain or rebuild libretro emulator core libraries.

Install
-------
Extract this ZIP into the root of the same user-data partition that contains
your Games and System directories. Merge the directories. Back up saves first.
Existing System/*_libretro.so files are not deleted or replaced. The package
replaces System/slot and installs Slot adapter and default UI asset files.
Before updating an existing customised card, back up System/Carts/, System/fonts/, Overlay/,
Bootlogos/ and Music/Fonts/; matching paths in the package may overwrite them.

Resolution
----------
This is the upstream 640x480 source tree, not a 720x480 canvas scaled down.
Its compile-time logical canvas is OUT_W=640 and OUT_H=480; system-specific
frame geometry and UI layout come from upstream branch 66-mods-640. The RG353PS
BaseOS build replaces only the device surface with the SDL2/KMSDRM + GLES2
adapter; it does not use the upstream H700 Mali/EGL framebuffer backend.

This is a first integration build, not a hardware-validated release. Verify
boot-to-frontend, screen geometry, button mapping, audio, sleep/wake and game
launch separately. A successful compile does not prove hardware operation.
EOF

    cat > "$PKG/BUILD-INFO.txt" <<EOF
Slot repository: $SLOT_REPO
Slot branch: $SLOT_BRANCH
Slot source commit: $SLOT_REV
BaseOS repository: dengzhan007/BaseOS-RG353PS
BaseOS adapter ref: $BASEOS_REF
Target: aarch64-unknown-linux-musl
Panel: 640x480
Build mode: release / features $FEATURES
Language: $LANGUAGE ($LANGUAGE_NAME)
Emulator cores: not built or included
Primary UI artwork: RetroGame66/LUCKLUCK-SLOT v2026.11.09 640x480
Primary UI artwork SHA-256: 5f71f6b8f6fd1c6c4787158b1ccda7740f7b1d68b9b76948d14b71b3ed670c35
Legacy theme marks: RetroGame66/LUCKLUCK-SLOT v2026.10.02 (six type_*.png only)
Legacy marks archive SHA-256: 6c5085907d41f76bb56e5a17a67389409feb647c6fc4567aae27c9c2ae5bdb8a
EOF

    ARCHIVE=/out/LUCKLUCK-SLOT-RG353PS-640x480-${PACKAGE_TAG}-${LANGUAGE}.zip
    rm -f "$ARCHIVE" "$ARCHIVE.sha256"
    cd "$PKG"
    zip -qr "$ARCHIVE" .
    # Check the ZIP CRCs and verify its font members byte-for-byte against PKG.
    python3 - "$ARCHIVE" "$PKG" <<'PYFONT'
import hashlib
import pathlib
import sys
import zipfile

archive = pathlib.Path(sys.argv[1])
package = pathlib.Path(sys.argv[2])
members = (
    "System/fonts/Plix-Bold.otf",
    "Music/Fonts/unifont-16px.otf",
    "Music/Fonts/press-start-2p.ttf",
)
with zipfile.ZipFile(archive) as zf:
    bad = zf.testzip()
    if bad:
        raise SystemExit(f"ZIP CRC failure: {bad}")
    for name in members:
        packed = zf.read(name)
        source = (package / name).read_bytes()
        if packed != source:
            raise SystemExit(f"font payload mismatch: {name}")
        print(f"font payload OK: {name} sha256={hashlib.sha256(packed).hexdigest()}")
PYFONT
    cd /out
    sha256sum "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256"
    ls -lh "$ARCHIVE" "$ARCHIVE.sha256"
  '

echo "Package output: $OUT/LUCKLUCK-SLOT-RG353PS-640x480-${PACKAGE_TAG}-${LANGUAGE}.zip"
