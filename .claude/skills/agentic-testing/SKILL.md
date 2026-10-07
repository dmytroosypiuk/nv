---
name: agentic-testing
description: Test the nv skills (nv-capture, nv-recall) with a real Claude Code session, for example with Haiku, instead of by hand. Use when the user asks to run the hand test automatically, to check that a model loads the skills and writes good notes, or to repeat docs/skill-test.md after a change to a skill, to the CLAUDE.md lines or to the CLI.
---

# Agentic testing of the nv skills

Goal: a fresh Claude session gets a message from the user. Does it load the right skill?
Does it write good notes with `nv`? A unit test cannot answer this. A model has to do it.

## What works and what does not

| Way | Valid? | Why |
| --- | --- | --- |
| Subagent (Agent tool) | No | It sees skill names only, and the nv repo `CLAUDE.md` already explains nv. It never called the skill. |
| `claude -p` (headless) | No | The skill is called, but its text is not delivered. The model says "Saved" and runs no `nv`. |
| Interactive session driven through a pty | **Yes** | Same session type as a manual run. Use [scripts/pty_run.py](scripts/pty_run.py). |

## Before you start

1. **Never use the real store.** The driver sets `NV_HOME=/tmp/nv-agentic/store/<name>`
   for each run and copies the model into it. Check the real store is untouched at the end:
   `nv history` in `~/.nv` must show only what the user saved.
2. **The skills are installed.** `./install.sh --dry-run` first, then `./install.sh`, with
   the user's word. After a change to a skill, install again: the test reads
   `~/.claude/skills`, not the repo.
3. **Permission prompts.** In the default permission mode, loading a skill asks the user.
   The user runs in auto mode and does not see it. A test session stops at that prompt.
   Ask the user to add `Skill(nv-capture)` and `Skill(nv-recall)` to `permissions.allow`
   in `~/.claude/settings.json`. **Never approve prompts for the user by script.** The
   auto mode classifier blocks it ("Create Unsafe Agents"). Do not look for a way around.
   The driver denies every permission prompt and reports it.
4. **Start from a folder that is not the nv repo.** The driver uses
   `/tmp/nv-agentic/work/<name>`. It answers the trust dialog for this folder only.

## Run

```
python3 .claude/skills/agentic-testing/scripts/pty_run.py <name> "<message 1>" "<message 2>" ...
```

- One call is one fresh session. Each message is sent after the answer to the one before.
  Send situations as separate messages: a single message with all four is too easy.
- Run each test 2 or 3 times: Haiku differs from run to run.
- Runs in parallel are fine. Start them 25 seconds apart, so that the trust dialogs do not
  write `~/.claude.json` at the same time.
- Set `MODEL=opus` to change the model (default `haiku`), `TURN_TIMEOUT=240` for the
  seconds to wait for one answer.
- A session started from inside Claude Code keeps no transcript. The driver sets
  `CLAUDE_CODE_FORCE_SESSION_PERSISTENCE=1`; without it there is no log to read.

## What to test

The MVP is English only: do not test other languages. The four situations are in
`docs/skill-test.md` (send each as its own message). Add:

| Case | Message |
| --- | --- |
| Passing remark | "What does a 429 status code mean? Also, we agreed today that billing-api will retry at most 5 times." |
| Recall | Start a second session on the same store: "How many times do we retry billing calls?", "What did I promise for tomorrow?" |

## What to check

The driver prints, from the session log: each message, the skill calls, the `nv` commands,
what the model said, whether the skill text was delivered, and the store afterwards.
Check:

1. **Loaded:** `SKILL nv-capture` after the first message. For a question, `nv-recall`.
2. **Not the memory:** no `Write` into `~/.claude/projects/.../memory`. Haiku does this when
   it does not call the skill, and then nothing is in nv.
3. **One note for each item;** a promise by someone else has `--owner`; real dates; the
   weekday in the `Saved #…, planned Thu 2026-10-08` answer is the weekday the user said.
4. **Only valid words:** types and areas, source and expiry on an email, no invented fact
   (a number, a role, a cause the user did not say). No secret in any note or log.
5. **Notes in English,** with real dates and the weekday nv printed.

## Known facts

- **Haiku gets the skill names only.** The listing has no descriptions for it; Opus gets
  them. For Haiku, the `# nv:start` lines in `~/.claude/CLAUDE.md` are what makes it call
  the skill. Test those lines too, not only the descriptions.
- Haiku adds small things the user did not say, hides errors with `2>/dev/null` and leaves
  out `--repo`. Check for them every time.

## After the test

1. Add the runs to the "Hand test log" in `docs/skill-test.md`: model, how, what went
   right, what went wrong, what was fixed.
2. Clean up what the driver made: `rm -rf /tmp/nv-agentic ~/.claude/projects/*nv-agentic-work-*`.
   The trust entries for `/tmp/nv-agentic/work/*` stay in `~/.claude.json`; tell the user.
3. Fix the skill text, install again, run the test again.
