# Hand test for the nv skills

Four real situations from `docs/design.md`. Say each one to Claude Code in a fresh session
(after `./install.sh`), in any language, then check what was saved.

`./install.sh` puts two skills into `~/.claude/skills`: `nv-capture` (saving) and
`nv-recall` (searching), and the nv lines between `# nv:start` and `# nv:end` into
`~/.claude/CLAUDE.md`. Those lines are what should make a session save without being
asked. For each situation, also note which skill loaded, and whether it loaded at all.

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
