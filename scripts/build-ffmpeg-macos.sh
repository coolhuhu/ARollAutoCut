#!/bin/sh

set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
FFMPEG_VERSION=8.1.1
FFMPEG_ARCHIVE="$ROOT_DIR/.cache/ffmpeg/ffmpeg-$FFMPEG_VERSION.tar.xz"
FFMPEG_SHA256=b6863adde98898f42602017462871b5f6333e65aec803fdd7a6308639c52edf3
FFMPEG_SOURCE="$ROOT_DIR/.cache/ffmpeg/ffmpeg-$FFMPEG_VERSION"
FFMPEG_PREFIX="$ROOT_DIR/.cache/ffmpeg/install-$FFMPEG_VERSION"
LAME_PREFIX=${LAME_PREFIX:-/opt/homebrew/opt/lame}
LAME_STAGING="$ROOT_DIR/.cache/ffmpeg/lame-static"
SIDECAR_DIR="$ROOT_DIR/src-tauri/binaries"

if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
  echo "This script only builds the macOS Apple Silicon sidecars." >&2
  exit 1
fi

if [ ! -f "$FFMPEG_ARCHIVE" ]; then
  echo "Missing FFmpeg source archive: $FFMPEG_ARCHIVE" >&2
  exit 1
fi

actual_sha256=$(shasum -a 256 "$FFMPEG_ARCHIVE" | awk '{print $1}')
if [ "$actual_sha256" != "$FFMPEG_SHA256" ]; then
  echo "FFmpeg archive SHA-256 mismatch." >&2
  exit 1
fi

if [ ! -f "$LAME_PREFIX/lib/libmp3lame.a" ] || [ ! -d "$LAME_PREFIX/include/lame" ]; then
  echo "Missing static LAME installation at $LAME_PREFIX." >&2
  echo "Install it with Homebrew or set LAME_PREFIX." >&2
  exit 1
fi

if [ ! -d "$FFMPEG_SOURCE" ]; then
  tar -xJf "$FFMPEG_ARCHIVE" -C "$ROOT_DIR/.cache/ffmpeg"
fi

mkdir -p "$LAME_STAGING/include" "$LAME_STAGING/lib" "$SIDECAR_DIR"
cp -R "$LAME_PREFIX/include/lame" "$LAME_STAGING/include/"
cp "$LAME_PREFIX/lib/libmp3lame.a" "$LAME_STAGING/lib/"

cd "$FFMPEG_SOURCE"
./configure \
  --prefix="$FFMPEG_PREFIX" \
  --arch=arm64 \
  --target-os=darwin \
  --cc=clang \
  --disable-doc \
  --disable-debug \
  --disable-ffplay \
  --enable-audiotoolbox \
  --enable-videotoolbox \
  --enable-libmp3lame \
  --extra-cflags="-I$LAME_STAGING/include" \
  --extra-ldflags="-L$LAME_STAGING/lib"

make -j"$(sysctl -n hw.logicalcpu)"
make install

cp "$FFMPEG_PREFIX/bin/ffmpeg" "$SIDECAR_DIR/ffmpeg-aarch64-apple-darwin"
cp "$FFMPEG_PREFIX/bin/ffprobe" "$SIDECAR_DIR/ffprobe-aarch64-apple-darwin"
chmod +x \
  "$SIDECAR_DIR/ffmpeg-aarch64-apple-darwin" \
  "$SIDECAR_DIR/ffprobe-aarch64-apple-darwin"

echo "Built FFmpeg sidecars in $SIDECAR_DIR"
