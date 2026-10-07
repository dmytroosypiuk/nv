#!/usr/bin/env bash
# Developer install of nv for the current user, from this checkout: the binary, the
# embedding model, the Claude Code plugin (the two skills) from this folder as a local
# marketplace and the permissions; it removes the old nv lines from CLAUDE.md. No root
# needed. Never touches nv.db. Running it again replaces what it installed before.
# Other people install the plugin from GitHub: see README.md.
#
#   ./install.sh            install
#   ./install.sh --dry-run  only print what would be done
#
# Where things go (override with the env vars):
#   NV_BIN_DIR         ~/.nv/bin           the nv binary, as nv-<version>
#   NV_LINK_DIR        ~/.local/bin        nv: a link to it, for a terminal
#   NV_HOME            ~/.nv               models/ (and later nv.db)
#   CLAUDE_CONFIG_DIR  ~/.claude           settings.json, CLAUDE.md (the plugin is
#                                          installed by the claude CLI)
set -euo pipefail

dry_run=false
case "${1:-}" in
  "") ;;
  --dry-run) dry_run=true ;;
  *) echo "usage: ./install.sh [--dry-run]" >&2; exit 2 ;;
esac

repo="$(cd "$(dirname "$0")" && pwd)"
bin_dir="${NV_BIN_DIR:-$HOME/.nv/bin}"
link_dir="${NV_LINK_DIR:-$HOME/.local/bin}"
version="$(cat "$repo/plugin/VERSION")"
binary="$bin_dir/nv-$version"
nv_home="${NV_HOME:-$HOME/.nv}"
claude_dir="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
model="bge-small-en-v1.5"
marketplace='nv-marketplace'
plugin="nv@$marketplace"

run() {
  echo "+ $*"
  if ! $dry_run; then "$@"; fi
}

# 0. The plugin is installed through the claude CLI.
if ! command -v claude >/dev/null; then
  echo "error: the claude CLI is not on your PATH" >&2
  exit 1
fi

# 1. Another program called nv must not be replaced or shadowed by surprise.
if other="$(command -v nv 2>/dev/null)" && [[ "$other" != "$link_dir/nv" ]]; then
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

# Where the plugin's launcher (plugin/bin/nv) looks for it: it then downloads nothing.
echo "== binary -> $binary"
run mkdir -p "$bin_dir"
run install -m 755 "$repo/target/release/nv" "$binary"
for old in "$bin_dir"/nv-*; do
  if [[ -e "$old" && "$old" != "$binary" ]]; then run rm -f "$old"; fi
done

# `nv` in a terminal: a link to that binary (an earlier install left a copy here).
echo "== link -> $link_dir/nv"
run mkdir -p "$link_dir"
run ln -sfn "$binary" "$link_dir/nv"

echo "== model -> $nv_home/models/$model"
run mkdir -p "$nv_home/models/$model"
run cp -u "$repo/models/$model/"* "$nv_home/models/$model/"

# The skills are in the plugin now. Earlier versions copied them into the skills folder,
# and Claude would see both: remove those copies.
for old in nv nv-recall nv-capture; do
  if [[ -d "$claude_dir/skills/$old" ]]; then
    echo "== old standalone skill, removed: $claude_dir/skills/$old"
    run rm -r "$claude_dir/skills/$old"
  fi
done

echo "== plugin $plugin from $repo"
if claude plugin marketplace list 2>/dev/null | grep -q "$marketplace"; then
  run claude plugin marketplace update "$marketplace"
else
  run claude plugin marketplace add "$repo"
fi
if claude plugin list 2>/dev/null | grep -q "$plugin"; then
  run claude plugin update "$plugin"
else
  run claude plugin install "$plugin"
fi

# A plugin cannot add permission rules: the user's settings must hold them.
snippet_settings="$repo/claude/settings.snippet.json"
settings="$claude_dir/settings.json"
echo "== permissions $(jq -r '.permissions.allow | join(", ")' "$snippet_settings" 2>/dev/null || echo "(see claude/settings.snippet.json)") -> $settings"
if [[ ! -e "$settings" ]]; then
  run cp "$snippet_settings" "$settings"
elif ! command -v jq >/dev/null; then
  echo "jq is not installed: add the rules of $snippet_settings to permissions.allow in $settings yourself"
elif jq -e --argjson want "$(jq -c '.permissions.allow' "$snippet_settings")" \
    '(.permissions.allow // []) as $have | $want | all(. as $p | $have | index($p))' "$settings" >/dev/null; then
  echo "already there"
else
  # Only permissions.allow gets the missing entries; everything else stays as it is.
  run cp "$settings" "$settings.before-nv"
  echo "+ add the missing rules to permissions.allow"
  if ! $dry_run; then
    jq --argjson want "$(jq -c '.permissions.allow' "$snippet_settings")" \
      '.permissions.allow = ((.permissions.allow // []) as $have | $have + ($want - $have))' \
      "$settings.before-nv" > "$settings"
  fi
fi

# Earlier versions put lines about nv into CLAUDE.md, between two markers. The plugin's
# SessionStart hook tells Claude about nv now, so those lines would only say it twice.
claude_md="$claude_dir/CLAUDE.md"
block_start='# nv:start'
block_end='# nv:end'
echo "== old nv lines ($block_start ... $block_end) in $claude_md"
if [[ -e "$claude_md" ]] && grep -qxF "$block_start" "$claude_md"; then
  if ! grep -qxF "$block_end" "$claude_md"; then
    echo "error: $claude_md has '$block_start' but no '$block_end'" >&2
    echo "       remove the broken nv lines yourself, then run again" >&2
    exit 1
  fi
  run cp "$claude_md" "$claude_md.before-nv"
  echo "+ remove the nv lines"
  if ! $dry_run; then
    # The block, and the one empty line after it, are dropped; everything else stays.
    awk -v from="$block_start" -v to="$block_end" '
      $0 == from { inside = 1; next }
      inside { if ($0 == to) { inside = 0; skipblank = 1 } next }
      skipblank { skipblank = 0; if ($0 == "") next }
      { print }
    ' "$claude_md.before-nv" > "$claude_md"
    if ! grep -q '[^[:space:]]' "$claude_md"; then
      rm "$claude_md"
      echo "+ nothing else was in the file: removed it"
    fi
  fi
else
  echo "none found"
fi

echo
if $dry_run; then
  echo "Dry run: nothing was changed."
  exit 0
fi
case ":$PATH:" in
  *":$link_dir:"*) ;;
  *) echo "note: $link_dir is not on your PATH; add it to use nv in a terminal (Claude Code finds nv through the plugin)" ;;
esac
NV_HOME="$nv_home" "$binary" model info
echo
echo "Done. Start a new Claude Code session and try the situations in docs/skill-test.md."
