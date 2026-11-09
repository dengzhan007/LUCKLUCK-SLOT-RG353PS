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
SLOT_BRANCH="${SLOT_BRANCH:-port/rg353ps-640x480}"
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

echo "== building RG353PS 640x480 frontend test package =="
echo "   slot: dengzhan007/slot-66 @ $SLOT_REV ($SLOT_BRANCH)"
echo "   BaseOS adapter: dengzhan007/BaseOS-RG353PS @ $BASEOS_REF"
docker run --rm --network host --platform linux/arm64 \
  -e "SLOT_REV=$SLOT_REV" \
  -e "SLOT_BRANCH=$SLOT_BRANCH" \
  -e "BASEOS_REF=$BASEOS_REF" \
  -e "CARGO_HOME=/usr/local/cargo" \
  -e "CARGO_BUILD_JOBS=2" \
  -v "$SRC_HOST:/slot-up:ro" \
  -v "$FRONTEND_PORT:/slot-port:ro" \
  -v "$HERE:/slot-build:ro" \
  -v "$OUT:/out" \
  -v "$WORK/cargo-registry:/usr/local/cargo/registry" \
  -v "$WORK/cargo-git:/usr/local/cargo/git" \
  -v "$WORK/cargo-target:/cargo-target" \
  "$IMAGE" sh -euc '
    export RUSTFLAGS="-C target-feature=-crt-static"
    SRC=/tmp/slot-src
    TARGET=/cargo-target
    PKG=/tmp/package

    rm -rf "$SRC" "$PKG"
    mkdir -p "$SRC" "$PKG"
    cp -R /slot-up/. "$SRC/"
    cp /slot-port/fbdev.rs "$SRC/crates/slot-gfx/src/fbdev.rs"

    # Keep the new upstream AxisMap / device probing code. Apply only the
    # RG353PS-specific input constants and key table with checked anchors.
    python3 /slot-build/apply-rg353ps-input.py "$SRC/crates/slot/src/input/evdev.rs"

    cd "$SRC"
    echo "=== cargo build (device / release / AArch64-musl) ==="
    cargo build --release --target-dir "$TARGET" -p slot --no-default-features --features device
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

    cat > "$PKG/README-RG353PS-TEST.txt" <<EOF
LUCKLUCK SLOT / 66slot - RG353PS first integration test
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
The panel output is 640x480. Upstream retains its 720x480 logical composition
and uses its existing fit-to-window path for smaller 4:3 panels; this test
does not rewrite OUT_W/OUT_H or resample each emulator core separately.

This is a first integration build, not a hardware-validated release. Verify
boot-to-frontend, screen geometry, button mapping, audio, sleep/wake and game
launch separately. A successful compile does not prove hardware operation.
EOF

    cat > "$PKG/BUILD-INFO.txt" <<EOF
Slot repository: dengzhan007/slot-66
Slot branch: $SLOT_BRANCH
Slot source commit: $SLOT_REV
BaseOS repository: dengzhan007/BaseOS-RG353PS
BaseOS adapter ref: $BASEOS_REF
Target: aarch64-unknown-linux-musl
Panel: 640x480
Build mode: release / device feature
Emulator cores: not built or included
EOF

    ARCHIVE=/out/slot-66-rg353ps-640x480-test.zip
    rm -f "$ARCHIVE" "$ARCHIVE.sha256"
    cd "$PKG"
    zip -qr "$ARCHIVE" .
    cd /out
    sha256sum "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256"
    ls -lh "$ARCHIVE" "$ARCHIVE.sha256"
  '

echo "Package output: $OUT/slot-66-rg353ps-640x480-test.zip"
