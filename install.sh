#!/usr/bin/env bash
# Installs nv for the current user: the binary, the embedding model, the Claude Code
# skill and the permission to run nv. No root needed. Never touches nv.db.
#
#   ./install.sh            install
#   ./install.sh --dry-run  only print what would be done
#
# Where things go (override with the env vars):
#   NV_BIN_DIR         ~/.local/bin        the nv binary
#   NV_HOME            ~/.nv               models/ (and later nv.db)
#   CLAUDE_CONFIG_DIR  ~/.claude           skills/nv/SKILL.md, settings.json
set -euo pipefail

dry_run=false
case "${1:-}" in
  "") ;;
  --dry-run) dry_run=true ;;
  *) echo "usage: ./install.sh [--dry-run]" >&2; exit 2 ;;
esac

repo="$(cd "$(dirname "$0")" && pwd)"
bin_dir="${NV_BIN_DIR:-$HOME/.local/bin}"
nv_home="${NV_HOME:-$HOME/.nv}"
claude_dir="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
model="bge-small-en-v1.5"
permission='Bash(nv:*)'

run() {
  echo "+ $*"
  if ! $dry_run; then "$@"; fi
}

# 1. Another program called nv must not be replaced or shadowed by surprise.
if other="$(command -v nv 2>/dev/null)" && [[ "$other" != "$bin_dir/nv" ]]; then
  echo "error: another nv is on your PATH: $other" >&2
  echo "       remove it or choose a different NV_BIN_DIR, then run again" >&2
  exit 1
fi

# 2. The model is downloaded at setup time only; nv never uses the network when it runs.
if [[ ! -s "$repo/models/$model/model.onnx" ]]; then
  echo "error: model not found in $repo/models/$model" >&2
  echo "       run spikes/embedding/fetch-model.sh first (needs the network once)" >&2
  exit 1
fi

echo "== build"
run cargo build --release --manifest-path "$repo/Cargo.toml"

echo "== binary -> $bin_dir/nv"
run mkdir -p "$bin_dir"
run install -m 755 "$repo/target/release/nv" "$bin_dir/nv"

echo "== model -> $nv_home/models/$model"
run mkdir -p "$nv_home/models/$model"
run cp -u "$repo/models/$model/"* "$nv_home/models/$model/"

echo "== skill -> $claude_dir/skills/nv"
run mkdir -p "$claude_dir/skills/nv"
run cp "$repo/claude/skills/nv/SKILL.md" "$claude_dir/skills/nv/SKILL.md"

echo "== permission $permission -> $claude_dir/settings.json"
settings="$claude_dir/settings.json"
if [[ ! -e "$settings" ]]; then
  run cp "$repo/claude/settings.snippet.json" "$settings"
elif ! command -v jq >/dev/null; then
  echo "jq is not installed: add \"$permission\" to permissions.allow in $settings yourself"
elif jq -e --arg p "$permission" '(.permissions.allow // []) | index($p)' "$settings" >/dev/null; then
  echo "already there"
else
  # Only permissions.allow gets one more entry; everything else stays as it is.
  run cp "$settings" "$settings.before-nv"
  echo "+ add \"$permission\" to permissions.allow"
  if ! $dry_run; then
    jq --arg p "$permission" '.permissions.allow = ((.permissions.allow // []) + [$p])' \
      "$settings.before-nv" > "$settings"
  fi
fi

echo
if $dry_run; then
  echo "Dry run: nothing was changed."
  exit 0
fi
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) echo "note: $bin_dir is not on your PATH; add it so that Claude Code finds nv" ;;
esac
NV_HOME="$nv_home" "$bin_dir/nv" model info
echo
echo "Done. Start a new Claude Code session and try the situations in docs/skill-test.md."
