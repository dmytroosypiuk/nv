#!/bin/sh
# Makes sure that the nv binary (and, without --binary, the embedding model) is on this
# machine, and prints the path of the binary on stdout. Everything else goes to stderr.
#
# This is the only place that uses the network, and only at setup time, once per version:
# it downloads release files of this project from GitHub and checks their SHA-256 against
# the release's SHA256SUMS. The nv binary itself never uses the network.
#
#   ensure-nv.sh            the binary and the model
#   ensure-nv.sh --binary   the binary only (what the nv launcher needs)
#
# Environment:
#   NV_HOME          ~/.nv           the model goes to $NV_HOME/models/bge-small-en-v1.5
#   NV_BIN_DIR       ~/.nv/bin       the binary goes to $NV_BIN_DIR/nv-<version>
#   NV_RELEASE_BASE  GitHub release  where the files are (any URL curl reads, like file://)
#   NV_NO_DOWNLOAD   unset           1: never download, only report what is missing
#
# Exit codes: 0 ready, 1 not ready (the reason is on stderr).
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
VERSION=$(cat "$ROOT/VERSION")
MODEL_NAME=bge-small-en-v1.5
NV_HOME_DIR=${NV_HOME:-$HOME/.nv}
BIN_DIR=${NV_BIN_DIR:-$HOME/.nv/bin}
BINARY=$BIN_DIR/nv-$VERSION
MODEL_DIR=$NV_HOME_DIR/models/$MODEL_NAME
BASE=${NV_RELEASE_BASE:-https://github.com/dmytroosypiuk/nv/releases/download/v$VERSION}

want_model=true
[ "${1:-}" = "--binary" ] && want_model=false

# printf, not echo: on macOS echo turns a backslash and c in a path into "stop printing".
say() { printf "nv: %s\n" "$*" >&2; }
fail() { say "$*"; exit 1; }

# ---- which release file fits this machine
PLATFORM=${NV_TEST_UNAME:-$(uname -sm)} # NV_TEST_UNAME is for the tests
case "$PLATFORM" in
  "Linux x86_64") TARGET=x86_64-unknown-linux-gnu ;;
  "Darwin arm64") TARGET=aarch64-apple-darwin ;;
  *) fail "no prebuilt binary for $PLATFORM. Supported: Linux x86_64 and macOS Apple silicon. Build from source: see the README." ;;
esac
ASSET=nv-$VERSION-$TARGET.tar.gz
MODEL_ASSET=model-$MODEL_NAME.tar.gz

ready() {
  [ -x "$BINARY" ] || return 1
  $want_model || return 0
  [ -s "$MODEL_DIR/model.onnx" ]
}

if ready; then
  printf "%s\n" "$BINARY"
  exit 0
fi

if [ "${NV_NO_DOWNLOAD:-}" = "1" ]; then
  missing=""
  [ -x "$BINARY" ] || missing="the nv binary ($BINARY)"
  if $want_model && [ ! -s "$MODEL_DIR/model.onnx" ]; then
    missing="${missing:+$missing and }the model ($MODEL_DIR)"
  fi
  fail "not installed: $missing is missing and NV_NO_DOWNLOAD=1 stops the download."
fi

# ---- tools
if command -v curl >/dev/null 2>&1; then
  fetch() { curl --fail --location --silent --show-error --connect-timeout 20 --max-time 900 --retry 2 --output "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget --quiet --tries=3 --timeout=60 --output-document="$2" "$1"; }
else
  fail "neither curl nor wget is installed: cannot download nv."
fi
if command -v sha256sum >/dev/null 2>&1; then
  sum_of() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  sum_of() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  fail "neither sha256sum nor shasum is installed: cannot check the download."
fi

# ---- one installer at a time: two sessions, or the hook and the launcher
mkdir -p "$BIN_DIR"
LOCK=$BIN_DIR/.install.lock
waited=0
until mkdir "$LOCK" 2>/dev/null; do
  holder=$(cat "$LOCK/pid" 2>/dev/null || true)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    # The installer died. Remove the lock only if it is still that installer's: another
    # waiter may have taken it over a moment ago.
    [ "$(cat "$LOCK/pid" 2>/dev/null || true)" = "$holder" ] && rm -rf "$LOCK"
    continue
  fi
  waited=$((waited + 1))
  [ "$waited" -le 900 ] || fail "another install did not finish in 15 minutes: remove $LOCK and try again."
  sleep 1
  if ready; then
    printf "%s\n" "$BINARY"
    exit 0
  fi
done
echo $$ > "$LOCK/pid"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/nv-install.XXXXXX")
trap 'rm -rf "$TMP" "$LOCK"' EXIT

if ready; then # the other installer finished first
  printf "%s\n" "$BINARY"
  exit 0
fi

say "setting up nv $VERSION (once): downloading from $BASE"
fetch "$BASE/SHA256SUMS" "$TMP/SHA256SUMS" || fail "cannot download $BASE/SHA256SUMS"

# Download one release file and check it against SHA256SUMS.
get() {
  fetch "$BASE/$1" "$TMP/$1" || fail "cannot download $BASE/$1"
  expected=$(awk -v name="$1" '$2 == name || $2 == "*" name { print $1 }' "$TMP/SHA256SUMS")
  [ -n "$expected" ] || fail "$1 is not in SHA256SUMS: nothing was installed."
  actual=$(sum_of "$TMP/$1")
  [ "$expected" = "$actual" ] || fail "checksum of $1 does not match (expected $expected, got $actual): nothing was installed."
}

if [ ! -x "$BINARY" ]; then
  say "downloading $ASSET"
  get "$ASSET"
  mkdir "$TMP/bin"
  tar -xzf "$TMP/$ASSET" -C "$TMP/bin"
  [ -f "$TMP/bin/nv" ] || fail "$ASSET has no nv inside."
  # Copy next to the target, then rename: a half-written binary is never in place.
  cp "$TMP/bin/nv" "$BINARY.part.$$"
  chmod 755 "$BINARY.part.$$"
  mv "$BINARY.part.$$" "$BINARY"
fi

if $want_model && [ ! -s "$MODEL_DIR/model.onnx" ]; then
  say "downloading $MODEL_ASSET (about 128 MB)"
  get "$MODEL_ASSET"
  mkdir "$TMP/model"
  tar -xzf "$TMP/$MODEL_ASSET" -C "$TMP/model"
  [ -s "$TMP/model/$MODEL_NAME/model.onnx" ] || fail "$MODEL_ASSET has no model inside."
  mkdir -p "$NV_HOME_DIR/models"
  rm -rf "$MODEL_DIR.part.$$"
  cp -R "$TMP/model/$MODEL_NAME" "$MODEL_DIR.part.$$"
  rm -rf "$MODEL_DIR"
  mv "$MODEL_DIR.part.$$" "$MODEL_DIR"
fi

# An older version is not needed any more.
for old in "$BIN_DIR"/nv-*; do
  [ -e "$old" ] || continue
  case "$old" in "$BINARY" | *.part.*) ;; *) rm -f "$old" ;; esac
done

say "nv $VERSION is ready"
printf "%s\n" "$BINARY"
