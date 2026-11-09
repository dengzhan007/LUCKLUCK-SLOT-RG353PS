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
IMAGE="${SLOT_BUILD_IMAGE:-slot-66-rg353ps-build:rust-1.96}"

mkdir -p "$OUT" "$WORK/cargo-registry" "$WORK/cargo-git" "$WORK/cargo-target"
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

    cat > "$PKG/README-RG353PS.txt" <<EOF
LUCKLUCK SLOT / 66slot - RG353PS 640x480 ${LANGUAGE} build
========================================================

Target: Anbernic RG353PS / RK3566 / 640x480 panel / BaseOS
Display: SDL2 KMSDRM fullscreen + OpenGL ES; not the upstream Mali fbdev EGL path.
This package contains the frontend executable and BaseOS adapter only.
It does NOT contain or rebuild libretro emulator core libraries.

Install
-------
Extract this ZIP into the root of the same user-data partition that contains
your Games and System directories. Merge/overwrite when prompted. Back up
saves first. Existing System/*_libretro.so files are not deleted or replaced.
The package replaces System/slot and installs the Slot frontend adapter files.

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
Build mode: release / device feature
Emulator cores: not built or included
EOF

    ARCHIVE=/out/LUCKLUCK-SLOT-RG353PS-640x480-${PACKAGE_TAG}-${LANGUAGE}.zip
    rm -f "$ARCHIVE" "$ARCHIVE.sha256"
    cd "$PKG"
    zip -qr "$ARCHIVE" .
    cd /out
    sha256sum "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256"
    ls -lh "$ARCHIVE" "$ARCHIVE.sha256"
  '

echo "Package output: $OUT/LUCKLUCK-SLOT-RG353PS-640x480-${PACKAGE_TAG}-${LANGUAGE}.zip"
