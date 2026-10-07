#!/bin/sh
# SessionStart hook of the nv plugin. Claude Code runs it when a session starts or resumes,
# and after /clear and after a compaction.
#
# 1. It makes sure that nv is installed (scripts/ensure-nv.sh: a no-op when it is).
# 2. It answers with the text of scripts/context.txt as the context of the session: a
#    plugin cannot ship CLAUDE.md lines, and a small model calls the skills only because of
#    these lines.
#
# It never runs nv, never fails the session, and prints one JSON document on stdout.
# If nv cannot be installed, the context says so, so that Claude tells the user.
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

# Text -> the inside of a JSON string: backslash, quote, newline; tab and CR become a space.
json_escape() {
  printf '%s' "$1" |
    tr '\t\r' '  ' |
    sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' |
    awk 'NR > 1 { printf "\\n" } { printf "%s", $0 }'
}

emit() {
  printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' \
    "$(json_escape "$1")"
}

ERR=$(mktemp "${TMPDIR:-/tmp}/nv-hook.XXXXXX")
trap 'rm -f "$ERR"' EXIT

if sh "$ROOT/scripts/ensure-nv.sh" >/dev/null 2>"$ERR"; then
  emit "$(cat "$ROOT/scripts/context.txt")"
else
  emit "nv is not installed on this machine, so the nv:capture and nv:recall skills cannot work yet. Tell the user once, in one short line, and carry on without nv. Reason: $(cat "$ERR")"
fi
exit 0
