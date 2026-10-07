# Hand test for the nv skills

Four real situations from `docs/design.md`. Say each one to Claude Code in a fresh session
(after `./install.sh`), in any language, then check what was saved.

`./install.sh` puts two skills into `~/.claude/skills`: `nv-capture` (saving) and
`nv-recall` (searching), and the nv lines between `# nv:start` and `# nv:end` into
`~/.claude/CLAUDE.md`. Those lines are what should make a session save without being
asked. For each situation, also note which skill loaded, and whether it loaded at all.

To run this without doing it by hand, use the project skill `agentic-testing`
(`.claude/skills/agentic-testing`): a script drives real Claude sessions and reports what
they did.

Use a scratch store so your real notes stay clean:

```
export NV_HOME=/tmp/nv-skill-test
mkdir -p "$NV_HOME" && cp -r ~/.nv/models "$NV_HOME/"
```

## The situations

| # | Say to Claude | Claude should |
| --- | --- | --- |
| 1 | "Sprint planning just ended. We raise the retry count for billing-api to 5 because of the timeouts in PAY-1234. Anna will review the PR by Friday, and I'll send her last week's retry numbers tomorrow." | Run `nv people search`, add Anna or ask which Anna. Save a **decision** (repo, ticket, person, source meeting), a **commitment owned by Anna** planned for Friday's real date, and **my commitment** planned for tomorrow's real date. |
| 2 | "I'll book the exam slot next week." | Save a **commitment** in area `learning`, with a real date, or ask which day. |
| 3 | Paste an HR email: "December vacation requests must be entered in Workday by 14 November." | Save a **fact** with source `email` and `--expires-on` the real date. |
| 4 | "Found it: the login tokens expired early because token.rs computed expires_at from local time and compared it with UTC. Staging DB password is hunter2 by the way." | Save the **root cause** as a fact with the repo (and the ticket if one was named). **Not** save the password; save how to get access, or nothing. |

Later, in a new session: "How many times do we retry billing calls?", "What did I promise
for tomorrow?", "What is Anna doing for me?" Claude should search nv first and answer from
the notes (`nv-recall`). "What is still open?" should use `--status todo` with a limit.

After each save Claude should say one line with the note ID, and the weekday for any date
("planned for Fri 2026-10-09"). Check the weekday: this is where a wrong date shows.

## What to check

```
nv history                      # one add per note, actor claude
nv search --type commitment --status todo --limit 50   # owner and planned dates are real dates
nv today                        # "Others owe you: Anna Nowak: …"
nv search "why did login tokens expire too early"
nv people list                  # one Anna, with a role if it was said
```

- Bodies make sense alone, in English, with names, numbers and dates.
- No note holds a password. (nv now refuses `password=…` and "password is hunter2", but a
  password without a digit, or said in other words, passes the check: that is the skill's
  job.)
- No duplicate when you repeat situation 1: Claude should find the note and edit it.
- Say "the retry count is 7 now": Claude should replace the decision with only a new
  title and body, and `nv search --ticket PAY-1234` should show the new note first.
- No work note holds customer data, internal hostnames or copied company code (the
  temporary company-data rule in `nv-capture`).

## Result of acting it out by hand (2026-10-07, with the first, single skill)

The commands of all four situations were run exactly as `SKILL.md` describes, with the
real model, in a scratch store installed by `install.sh`:

- 7 notes saved, 7 embedded by the background embedder within 2 seconds.
- `password=…` in the staging how-to was refused with exit code 1; the reworded note
  ("ask DevOps in #infra-help") was saved.
- "why did login tokens expire too early" and "how many times do we retry billing calls"
  each returned the right note first.
- `nv search --planned 2026-10-08` returned my commitment for tomorrow.
- `nv today` showed Anna's commitment under "Others owe you".

This was before the split into two skills and the CLI changes of step 7b (replace copies
fields, `--status`, `--project`, "showing N of M", first body line in results).

Not tested: whether a fresh Claude Code session loads the skill at the right moments and
writes notes of this quality on its own. That is the part to try in a real session.

## Hand test log

Each run used a scratch store and the four situations above. The reasons for each fix
are in `docs/design.md`, "Decided while splitting the skill (step 7b)".

| Run (2026-10-07) | Model | How | Went right | Went wrong | Fixed after the run |
| --- | --- | --- | --- | --- | --- |
| 1 | Opus 5.5 | all four in one message | Five good notes, real dates, weekdays, no password, asked about the exam | First body line in search stopped in the middle of a sentence | Search shows the first paragraph joined, cut at 100 characters |
| 1, recall | Opus 5.5 | three questions in one message | All three answers right, from the notes; used `--planned` and the person ID | nothing | nothing |
| 2 | Haiku 4.5 | all four in one message | Loaded the skill, saved without asking, no password, asked about the exam | "from 3 to 5" copied from the skill's example; role "Code reviewer" invented; told the user "Wed 2026-10-08" (a Thursday); `--type root-cause` and `--type deadline`; HR fact in `personal` without source or expiry | `SKILL.md` lists areas, types, source and expiry itself; "write only what the user said"; other main example; nv prints the weekday |
| 3 | Haiku 4.5 | all four in one message | Valid types, right area and expiry, weekdays copied from nv | Anna's promise inside the decision note with "Friday 2026-10-11" (a Sunday); exam promise dropped; role "QA lead" copied from the people example | First step "one note for each item"; people example with placeholders; no weekday names in bodies |
| 4 | Haiku 4.5 | one message each | Three notes for the first message, Anna's commitment with owner; nv answered `planned Sat 2026-10-10` and Haiku corrected the date itself; HR fact with source and expiry; no password | Body of the corrected note still said "Friday 2026-10-10"; exam promise asked about but never saved; role "Reviewer" invented | A promise with no clear day is saved without a date, then asked about; a corrected date is corrected in the body too; `SKILL.md` no longer shows `--role` |

### Open issues

- **The fixes after run 4 are not tested** with Haiku yet.
- **Loading on a passing remark is not shown.** In every run the skill loaded on the
  first, sprint-planning message. A decision said in the middle of a coding task, in a
  fresh session, was never tried. Recall was tried with Opus only.
- **Ukrainian and Polish input was not tried.** The descriptions and the CLAUDE.md lines
  are in English.
- **Haiku adds small things the user did not say** ("Need to standardize on UTC", a
  source reference "token.rs investigation") in spite of the rule.
- **Haiku hides errors** with `2>/dev/null` on searches in spite of the rule.
- **Haiku leaves out fields the user gave**: `--repo billing-api` in every run.
- **A wrong weekday in a body is not caught.** The weekday check and the `nv date` helper
  are under "Later, not MVP" in `docs/design.md`.
- **A password without a digit**, or said in other words, still passes the secret check.
- **`nv note edit` and `nv note replace` cannot clear a field**; there is no unlink.
- **The company-data rule in `nv-capture` is temporary**: Dmytro removes it when the
  company rules for AI tools are checked.
- **Work laptop:** build, both test runs, the background embedder and offline search are
  only tested on Linux x86_64.

### Trying to automate the hand test (2026-10-07)

Three ways were tried so that the user does not have to run the sessions by hand:

| Way | Result |
| --- | --- |
| Haiku subagent (Agent tool) | **Not a valid test.** The subagent never called the skill (it saw only the skill names) and worked out `nv` from the project `CLAUDE.md` and `--help`, because it ran in the nv repo. It saved three notes, with "Friday" as 2026-10-10 (a Saturday) and no correction. |
| Headless `claude -p --model haiku` | **Not a valid test.** The skill was called, but its result was only "Execute skill: nv-capture" and the skill text is nowhere in the session log. Haiku answered "Saved" four times out of four without running `nv`; once it wrote to Claude's auto-memory instead. |
| Interactive session driven through a pty (`pty_run.py`, not in the repo) | Valid: the same session type as the manual runs. Six sessions (two four-message runs, two passing remarks in a question, one Ukrainian, one Polish) **all called the right skill on the first message**, including `nv-recall` for the question with a passing decision. Then a **permission prompt for loading the skill** appeared, and the driver denied it, so no note was saved. The runs have to be repeated with that prompt approved. |

Findings:

- **For Haiku the skill listing has names only.** In all three manual Haiku sessions and
  in the pty sessions, `nv-capture` and `nv-recall` appear without description (other
  skills keep theirs); Opus sessions get the full descriptions. So for Haiku the lines in
  `~/.claude/CLAUDE.md` are what makes it call the skill, and the descriptions do not
  matter. The pty probe where the skill was not called is the proof of the risk: Haiku
  wrote "Friday 2026-10-11" into Claude's own auto-memory and nothing into nv.
- **In the default permission mode, loading a skill asks for permission**, once per skill.
  The install allows only `Bash(nv:*)`. The user's sessions run in auto mode, which
  approves it, so this was not seen in the manual runs. Adding `Skill(nv-capture)` and
  `Skill(nv-recall)` to the allowed list is an open decision.
- **A session started from inside Claude Code keeps no transcript** unless
  `CLAUDE_CODE_FORCE_SESSION_PERSISTENCE=1` is set.

### First automated run (2026-10-07, Haiku 4.5, pty driver, `Skill(nv-*)` allowed)

Seven fresh sessions, each with its own scratch store.

| Case | Skill loaded | Notes saved | Went wrong |
| --- | --- | --- | --- |
| Four messages, run 1 | yes, on message 1 | none | Stopped at a prompt: Haiku ran `cd ~/.claude/skills/nv-capture && nv search …`, which asks to read that folder in default mode. The driver denies prompts. The skill says "no `cd`". |
| Four messages, run 2 | yes, on message 1 | 6 (one outdated) | "Friday" written as 2026-10-10, found wrong by nv's `Sat`; corrected with `nv note replace`, not `edit`, so one extra outdated note. The exam promise was asked about and **not saved**. Invented source reference "code investigation, 2026-10-07". No `--repo` anywhere. Anna added without a role (the fix works). |
| Passing remark, question 1 | yes, after it answered | 1 | Source "Team discussion" is a guess. |
| Passing remark, question 2 | yes, after it answered | 1 | nothing |
| Ukrainian | yes | 2, in English | "Friday" as 2026-10-10 again, fixed with `edit`, but the body still says "Friday 2026-10-10"; "from default to 5" is invented; `2>/dev/null` used; "Аня" became "Anya". |
| Polish | yes | 2, in English | "Friday" as 2026-10-10; saw it was a Saturday but only **asked** the user, so the wrong date stays saved. |
| (real store `~/.nv`) | | untouched | |

- **Loading is not the problem:** the right skill loaded in 6 of 6 sessions that got past
  start-up, also for a passing remark and in both languages. Haiku had names only.
- **"Friday" is wrong every time:** 4 of 4 sessions wrote 2026-10-10. The weekday in nv's
  answer caught it every time, but the fix differs (replace, edit, or only a question).
  This is the strongest case for the `nv date` helper and the weekday check ("Later, not
  MVP" in `docs/design.md`).
- **Still not followed by Haiku:** save a promise without a date and then ask; no
  invented source; correct a date in the body too; no `cd`, no `2>/dev/null`; `--repo`.
