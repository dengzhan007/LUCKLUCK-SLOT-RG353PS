#!/bin/sh
# Build a frontend-only RG353PS test ZIP from this branch and the BaseOS adapter.
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
BASEOS_ROOT="${BASEOS_ROOT:-}"
[ -n "$BASEOS_ROOT" ] || { echo "set BASEOS_ROOT to a BaseOS-RG353PS checkout" >&2; exit 2; }

FRONTEND_PORT="$BASEOS_ROOT/platforms/rg353ps/frontends/slot"
FRONTEND_PACKAGE_MANIFEST="$FRONTEND_PORT/baseos-frontend.conf"
FRONTEND_PROFILE="$FRONTEND_PORT/profiles/640-native.conf"
FRONTEND_LAUNCHER="$FRONTEND_PORT/launch.sh"
for file in "$FRONTEND_PACKAGE_MANIFEST" "$FRONTEND_PROFILE" "$FRONTEND_LAUNCHER" \
            "$FRONTEND_PORT/fbdev.rs"; do
  [ -f "$file" ] || { echo "missing managed frontend package input: $file" >&2; exit 1; }
done

for declaration in package_api=1 volume_owner=frontend power_owner=frontend; do
  grep -Fqx "$declaration" "$FRONTEND_PACKAGE_MANIFEST" || {
    echo "invalid package manifest (missing $declaration)" >&2; exit 1;
  }
done
grep -Fqx "profile_api=1" "$FRONTEND_PROFILE" || {
  echo "invalid frontend profile manifest" >&2; exit 1;
}
PACKAGE_PRIORITY="$(sed -n 's/^priority=//p' "$FRONTEND_PACKAGE_MANIFEST")"
case "$PACKAGE_PRIORITY" in
  ''|*[!0-9]*) echo "invalid package priority" >&2; exit 1 ;;
esac
[ "$PACKAGE_PRIORITY" -le 9999 ] || { echo "package priority must be 0..9999" >&2; exit 1; }
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
INCLUDE_CORE_PACK="${INCLUDE_CORE_PACK:-yes}"
case "$INCLUDE_CORE_PACK" in
  yes|no) ;;
  *) echo "INCLUDE_CORE_PACK must be yes or no (got: $INCLUDE_CORE_PACK)" >&2; exit 2 ;;
esac
DIAGNOSTIC_NO_BOOT_SFX="${DIAGNOSTIC_NO_BOOT_SFX:-no}"
case "$DIAGNOSTIC_NO_BOOT_SFX" in
  yes|no) ;;
  *) echo "DIAGNOSTIC_NO_BOOT_SFX must be yes or no (got: $DIAGNOSTIC_NO_BOOT_SFX)" >&2; exit 2 ;;
esac
WORK="${RG353PS_WORK:-$REPO_ROOT/work/rg353ps}"
OUT="$WORK/frontends"
SRC_HOST="$WORK/slot-src"
ASSET_STAGE="$WORK/upstream-ui-assets"
IMAGE="${SLOT_BUILD_IMAGE:-slot-66-rg353ps-build:rust-1.96}"

mkdir -p "$OUT" "$WORK/cargo-registry" "$WORK/cargo-git" "$WORK/cargo-target"
sh "$HERE/stage-upstream-assets.sh" "$WORK" "$ASSET_STAGE"
if [ "$INCLUDE_CORE_PACK" = yes ]; then
  sh "$HERE/stage-core-pack.sh" "$WORK" "$ASSET_STAGE/CorePack"
else
  rm -rf "$ASSET_STAGE/CorePack"
fi
rm -rf "$SRC_HOST"
mkdir -p "$SRC_HOST"
git -C "$REPO_ROOT" archive --format=tar HEAD | tar -xf - -C "$SRC_HOST"

echo "== building AArch64/musl toolchain image =="
docker build --platform linux/arm64 -f "$HERE/Dockerfile" -t "$IMAGE" "$HERE"

echo "== building RG353PS native-640x480 frontend ($LANGUAGE) =="
echo "   slot: $SLOT_REPO @ $SLOT_REV ($SLOT_BRANCH)"
echo "   BaseOS package adapter: dengzhan007/BaseOS-RG353PS @ $BASEOS_REF"
docker run --rm --network host --platform linux/arm64 \
  -e "SLOT_REV=$SLOT_REV" \
  -e "SLOT_BRANCH=$SLOT_BRANCH" \
  -e "BASEOS_REF=$BASEOS_REF" \
  -e "LANGUAGE=$LANGUAGE" \
  -e "PACKAGE_TAG=$PACKAGE_TAG" \
  -e "SLOT_REPO=$SLOT_REPO" \
  -e "INCLUDE_CORE_PACK=$INCLUDE_CORE_PACK" \
  -e "DIAGNOSTIC_NO_BOOT_SFX=$DIAGNOSTIC_NO_BOOT_SFX" \
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
    FRONTEND_PACKAGE="$PKG/LUCKLUCK-SLOT"
    PAYLOAD="$FRONTEND_PACKAGE/payload"
    SYSTEM="$PAYLOAD/System"
    LANGUAGE="${LANGUAGE:-zh}"
    PACKAGE_TAG="${PACKAGE_TAG:-test}"
    SLOT_REPO="${SLOT_REPO:-dengzhan007/LUCKLUCK-SLOT-RG353PS}"
    INCLUDE_CORE_PACK="${INCLUDE_CORE_PACK:-yes}"
    case "$INCLUDE_CORE_PACK" in
      yes|no) ;;
      *) echo "INCLUDE_CORE_PACK must be yes or no" >&2; exit 2 ;;
    esac
    DIAGNOSTIC_NO_BOOT_SFX="${DIAGNOSTIC_NO_BOOT_SFX:-no}"
    case "$DIAGNOSTIC_NO_BOOT_SFX" in
      yes|no) ;;
      *) echo "DIAGNOSTIC_NO_BOOT_SFX must be yes or no" >&2; exit 2 ;;
    esac
    case "$LANGUAGE" in
      zh) FEATURES=device; LANGUAGE_NAME="Simplified Chinese" ;;
      en) FEATURES=device,lang-en; LANGUAGE_NAME="English" ;;
      *) echo "LANGUAGE must be zh or en" >&2; exit 2 ;;
    esac
    if [ "$DIAGNOSTIC_NO_BOOT_SFX" = yes ]; then
      FEATURES="$FEATURES,diag-no-boot-sfx"
      AUDIO_DIAGNOSTIC_NOTE="Sfx::Boot is suppressed at compile time; ALSA device open and all other audio paths are unchanged."
    else
      AUDIO_DIAGNOSTIC_NOTE="No audio diagnostic override; normal boot sound and ALSA behavior."
    fi

    rm -rf "$SRC" "$PKG"
    mkdir -p "$SRC" "$SYSTEM/licenses" "$FRONTEND_PACKAGE/profiles"
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
    echo "=== cargo test -p slot-store (profile state migration and private cache) ==="
    cargo test --target-dir "$TARGET" -p slot-store
    echo "=== cargo build (device / release / AArch64-musl; language=$LANGUAGE; features=$FEATURES) ==="
    cargo build --release --target-dir "$TARGET" -p slot --no-default-features --features "$FEATURES"
    BIN="$TARGET/release/slot"
    test -x "$BIN"
    readelf -h "$BIN" | grep -q "AArch64"
    readelf -l "$BIN" | grep -q "ld-musl-aarch64.so.1"

    cp "$BIN" "$SYSTEM/slot"
    chmod 0755 "$SYSTEM/slot"
    # Use the upstream card-side script. BaseOS initializes wlan0; Slot owns
    # association, saved credentials and DHCP as it does on the upstream release.
    cp /upstream-assets/System/slotnet.sh "$SYSTEM/slotnet.sh"
    chmod 0755 "$SYSTEM/slotnet.sh"
    sh -n "$SYSTEM/slotnet.sh"

    cp /slot-port/launch.sh "$FRONTEND_PACKAGE/launch.sh"
    cp /slot-port/baseos-frontend.conf "$FRONTEND_PACKAGE/baseos-frontend.conf"
    for profile_file in /slot-port/profiles/*.conf; do
      [ -f "$profile_file" ] || continue
      cp "$profile_file" "$FRONTEND_PACKAGE/profiles/"
      grep -Fqx "profile_api=1" "$FRONTEND_PACKAGE/profiles/${profile_file##*/}"
    done
    chmod 0644 "$FRONTEND_PACKAGE/baseos-frontend.conf" "$FRONTEND_PACKAGE"/profiles/*.conf
    sh -n "$FRONTEND_PACKAGE/launch.sh"
    grep -Fqx "package_api=1" "$FRONTEND_PACKAGE/baseos-frontend.conf"
    test -s "$FRONTEND_PACKAGE/profiles/640-native.conf"
    PROFILE_COUNT="$(find "$FRONTEND_PACKAGE/profiles" -maxdepth 1 -type f -name "*.conf" | wc -l | tr -d " ")"
    [ "$PROFILE_COUNT" = 1 ] || {
      echo "expected one default Slot profile, found $PROFILE_COUNT" >&2
      exit 1
    }
    grep -Fq "# baseos: volume-owned" "$FRONTEND_PACKAGE/launch.sh"
    grep -Fq "# baseos: power-owned" "$FRONTEND_PACKAGE/launch.sh"

    cp LICENSE "$SYSTEM/licenses/slot-LICENSE.txt"
    cp NOTICE.md "$SYSTEM/licenses/slot-NOTICE.md"
    for license in crates/slot-ui/assets/*.LICENSE.txt crates/slot-ui/assets/*.OFL.txt; do
      [ -f "$license" ] && cp "$license" "$SYSTEM/licenses/"
    done
    tar --exclude=.git -czf "$SYSTEM/licenses/LUCKLUCK-SLOT-$SLOT_REV-source.tar.gz" -C "$SRC" .
    "$SYSTEM/slot" --init-root "$PAYLOAD"

    # The upstream 640 release carries the UI resources as external card files.
    # Import only those data assets; retain the native RG353PS binary and never
    # replace separately provisioned libretro cores.
    cp -a /upstream-assets/System/Carts "$SYSTEM/"
    cp -a /upstream-assets/System/fonts "$SYSTEM/"
    test -n "$(find "$SYSTEM/fonts" -maxdepth 1 -type f \( -iname "*.ttf" -o -iname "*.otf" -o -iname "*.ttc" \) -print -quit)"
    test -s "$SYSTEM/slotnet.sh"
    for asset_dir in Music Overlay Bootlogos; do
      test -d "/upstream-assets/$asset_dir"
      cp -a "/upstream-assets/$asset_dir" "$PAYLOAD/"
    done
    if [ -s /upstream-assets/System/boot_opening.slotvid ]; then
      cp -a /upstream-assets/System/boot_opening.slotvid "$SYSTEM/"
    fi
    cp /upstream-assets/UPSTREAM-ASSET-SOURCE.txt "$FRONTEND_PACKAGE/"
    # Full releases reuse the separately validated core pack. The frontend test
    # package skips staging and merging it entirely, so no emulator libraries ship.
    if [ "$INCLUDE_CORE_PACK" = yes ]; then
      cp -a /upstream-assets/CorePack/System/. "$SYSTEM/"
      cp /upstream-assets/CorePack/CORE-PACK-SOURCE.txt "$FRONTEND_PACKAGE/"
      for core in mgba gpsp fceumm supafaust fbneo genesis_plus_gx pcsx_rearmed pce_fast; do
        test -s "$SYSTEM/${core}_libretro.so"
      done
      CORE_PACKAGE_NOTE="The pinned RG353PS AArch64/musl core pack is included; it is reused byte-for-byte and is not rebuilt."
      CORE_INSTALL_NOTE="The pinned core-pack files update matching System/*_libretro.so and System/lib/ entries; unmatched existing files remain."
    else
      CORE_PACKAGE_NOTE="Emulator core libraries and their runtime dependencies are intentionally excluded from this frontend-only package."
      CORE_INSTALL_NOTE="No emulator cores or System/lib runtime dependencies are included. Merging this package leaves existing core files untouched."
      if find "$PKG" -type f -name "*_libretro.so" -print -quit | grep -q .; then
        echo "frontend-only package unexpectedly contains a libretro core" >&2
        exit 1
      fi
      test ! -e "$FRONTEND_PACKAGE/CORE-PACK-SOURCE.txt"
      test ! -d "$SYSTEM/lib"
    fi

    # Catch an incomplete distribution package before it reaches the SD card.
    for machine in GBA GB GBC SFC FC NEOGEO ARCADE MD PS1 PCE; do
      test -s "$SYSTEM/Carts/SLOT_SYS_$machine.png"
    done
    # The PS1 insert animation uses this branded disc when a game has no custom art.
    test -s "$SYSTEM/Carts/66SLOT_DISC_PS1.png"
    for icon in type_gb.png type_gbc.png type_gba.png \
                type_gb_light.png type_gbc_light.png type_gba_light.png; do
      test -s "$SYSTEM/Carts/$icon"
    done
    test -s "$PAYLOAD/Music/Fonts/unifont-16px.otf"
    test -s "$PAYLOAD/Music/Fonts/press-start-2p.ttf"
    # Check that asset staging and package copying preserve exact font bytes.
    cmp -s /upstream-assets/System/fonts/Plix-Bold.otf "$SYSTEM/fonts/Plix-Bold.otf"
    cmp -s /upstream-assets/Music/Fonts/unifont-16px.otf "$PAYLOAD/Music/Fonts/unifont-16px.otf"
    cmp -s /upstream-assets/Music/Fonts/press-start-2p.ttf "$PAYLOAD/Music/Fonts/press-start-2p.ttf"
    test -d "$PAYLOAD/Overlay"
    test -d "$PAYLOAD/Bootlogos"

    cat > "$FRONTEND_PACKAGE/README-RG353PS.txt" <<EOF
LUCKLUCK SLOT / 66slot - RG353PS 640x480 ${LANGUAGE} build
========================================================

Target: Anbernic RG353PS / RK3566 / 640x480 panel / BaseOS
Language: $LANGUAGE ($LANGUAGE_NAME)
Display: SDL2 KMSDRM fullscreen + OpenGL ES; not the upstream Mali fbdev EGL path.
This archive has one top-level LUCKLUCK-SLOT/ folder. Extract it directly
into DATA/Frontends/; BaseOS discovers it as one managed frontend package.
It contains the RG353PS executable and the UI art/font bundle inside payload/:
System/Carts/, System/fonts/, Music/, Overlay/ and Bootlogos/. It also uses the upstream System/slotnet.sh, so Slot owns Wi-Fi
credentials, association and DHCP while BaseOS only initializes the radio. The main artwork is pinned to the upstream 640x480 v2026.11.09
release. That package omits six legacy GB/GBC/GBA theme marks still read by
this frontend; only those six type_*.png files are supplemented from the
checksum-pinned upstream v2026.10.02 package. No 720-layout art is imported.
Core contents
-------------
$CORE_PACKAGE_NOTE

Audio diagnostic
----------------
$AUDIO_DIAGNOSTIC_NOTE
The PCM device is still opened normally before frontend initialization. This test
does not change ALSA configuration, mixer controls, volume, amplifier timing, or
the boot-opening video audio.

Install
-------
Requires RG353PS BaseOS 1.2.10 or newer (managed package/profile API 1).
Extract the ZIP so the one top-level LUCKLUCK-SLOT/ directory is directly
inside DATA/Frontends/. Do not merge its contents into the DATA root.
BaseOS discovers one default profile, 640-native, automatically; the selector displays
LUCKLUCK-SLOT as the default frontend name. Users who
need another configuration can copy profiles/640-native.conf to a new
<profile-id>.conf in Frontends/LUCKLUCK-SLOT/profiles/ and change display_name.
For example, copy it to profiles/my-config.conf and set display_name=My Config.
The new filename (without .conf) becomes the independent profile ID. The
executable, payload and core files remain shared; do not duplicate them.

Hold L1 or R1 while powering on to open the frontend selector (Function USB
storage keeps priority). The top line names the current default startup and
the matching profile row is marked [DEFAULT]; AUTO means BaseOS will choose
by discovery priority. A launches the highlighted profile once without
changing the default. START saves it as the default and launches it. B
cancels and continues with the saved default.

Y removes only the selected profile. X uninstalls the whole Slot package.
Both actions require confirmation. Removing the last profile or the whole
package deletes Frontends/LUCKLUCK-SLOT/, but does not delete per-profile data
under .baseos/instances/, Games/, Saves/ or shared core data. Reinstalling
the same package/profile IDs reuses the saved profile settings and states.
Keep the package ID and profile file names unchanged when you want to retain
the existing profile data.

Upgrade from the former package ID slot-rg353ps is supported. BaseOS migrates the
saved default ID automatically; when each matching profile first launches, the
launcher copies the old instance data into the new LUCKLUCK-SLOT instance directory.
The old instance directory is kept as a rollback copy and is not deleted.

Data ownership
--------------
Shared by all profiles: Games/ ROMs, battery saves under Saves/, and the
emulator core files exposed from the shared package/core location.
Private to each profile: .userdata/ (including slot.state and
selected_core.ini), mutable System preferences, favorites, time zone,
per-game core selections and States/ save-state snapshots. All profiles
use the same executable and core files but do not share preferences, core
choices or save-state snapshots.

Core handling: $CORE_INSTALL_NOTE
This build includes the pinned RG353PS AArch64/musl emulator core pack,
runtime libraries, license texts and core-pack source metadata. The launcher
exposes these shared core files to the selected profile.

Back up customised UI resources before replacing this package. Do not delete
/mnt/SDCARD/.baseos/instances/ when removing or reinstalling the frontend;
those directories are the persistent profile data.

Resolution
----------
This is the upstream 640x480 source tree, not a 720x480 canvas scaled down.
Its compile-time logical canvas is OUT_W=640 and OUT_H=480; system-specific
frame geometry and UI layout come from upstream branch 66-mods-640. The RG353PS
BaseOS build replaces only the device surface with the SDL2/KMSDRM + GLES2
adapter; it does not use the upstream H700 Mali/EGL framebuffer backend.

The RG353PS BaseOS package/profile lifecycle is hardware-validated for
selector entry/exit, independent profile settings and save states, shared
Games/Saves/core resources, independent core choices, default versus
temporary launch, profile/package uninstall, and reinstall with retained
profile data. This does not claim exhaustive compatibility for every
emulator core or every game title.
EOF

    cat > "$FRONTEND_PACKAGE/BUILD-INFO.txt" <<EOF
Slot repository: $SLOT_REPO
Slot branch: $SLOT_BRANCH
Slot source commit: $SLOT_REV
BaseOS repository: dengzhan007/BaseOS-RG353PS
BaseOS adapter ref: $BASEOS_REF
Target: aarch64-unknown-linux-musl
Panel: 640x480
Build mode: release / features $FEATURES
Language: $LANGUAGE ($LANGUAGE_NAME)
Emulator cores: $CORE_PACKAGE_NOTE
Audio diagnostic: $AUDIO_DIAGNOSTIC_NOTE
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
    python3 - "$ARCHIVE" "$PKG" <<PYFONT
import hashlib
import pathlib
import sys
import zipfile

archive = pathlib.Path(sys.argv[1])
package = pathlib.Path(sys.argv[2])
members = (
    "LUCKLUCK-SLOT/payload/System/fonts/Plix-Bold.otf",
    "LUCKLUCK-SLOT/payload/Music/Fonts/unifont-16px.otf",
    "LUCKLUCK-SLOT/payload/Music/Fonts/press-start-2p.ttf",
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
