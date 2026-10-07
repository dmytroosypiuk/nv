---
name: nv-capture
description: Save a note with the nv command. Do not ask for approval. Use when the user tells you a decision, a promise, the cause of a bug, a fact about a person or a procedure. Use also when the user says "remember" or "save this", or when a saved note must change.
allowed-tools: Bash(nv *)
---

# nv-capture: save what is worth keeping

nv is the user's local notes store. You write the notes. What you do not save is lost.

## Steps, every time

1. **List the items.** Write one note for each item: each decision, each promise, each
   fact. A promise by another person is its own commitment with `--owner`
   ("Anna will review the PR by Friday"). Do not put it inside a different note.
2. **Search first**: `nv search` on the same topic. If a note is already there, edit or
   replace it instead of making a duplicate.
3. **Save right away.** Ask about missing facts, never about whether to save. If the note
   needs a detail you do not have (which Anna, which date, which repo), ask that one
   question; a note with a guess in it is worse. A promise with no clear day ("next
   week") is still a promise: ask for the day, do not drop it.
4. **Confirm in one line**, with the weekday for any date: "Saved to nv: #42 Use
   PostgreSQL 16 for reporting" or "Saved to nv: #43 Send retry numbers to Anna,
   planned for Fri 2026-10-09". nv prints the weekday (`Saved #43, planned Fri
   2026-10-09`). Copy the weekday from the answer of nv; do not calculate it. If the
   weekday is not the one the user said, the date is wrong: correct it.

Save decisions, commitments, ideas, root causes, people's availability and roles, how-to
knowledge, limits of tools. Do not save routine steps, what git or the repo docs already
show, or personal things said in passing. The full table: [saving-rules.md](saving-rules.md).

## How to save

Fields are flags. The body comes from stdin; always use a quoted heredoc (`<<'EOF'`), so
code, quotes and `$` arrive unchanged. Run nv commands as they are: no `cd`, no
`2>/dev/null`.

```
nv search "database version reporting service"
nv add --title "Use PostgreSQL 16 for the reporting service" --area work --type decision \
  --repo reporting --ticket REP-88 --person 7 \
  --source-kind meeting --source-ref "Architecture review, 2026-10-06" <<'EOF'
The reporting service uses PostgreSQL 16, agreed with Piotr Zielinski at the
architecture review on 2026-10-06, because version 14 has no support after 2026-11.
EOF
```

- `--title`, `--area` and the body are required. Use the other flags only when the user
  gave that fact.
- `--area` is one of: `work` (job, team, tickets, HR, also HR emails), `learning`
  (general knowledge), `personal` (own life, side projects).
- `--type` is one of these five words, no others: `decision` (something was decided),
  `commitment` (someone promised an action), `how-to` (steps to do something), `fact`
  (true, no action; a root cause, a deadline, a person's role), `idea` (a maybe).
- `--repo`, `--ticket`, `--project`, `--person <id>`: when the user named them.
- `--source-kind` with `--source-ref`: where it came from. Kinds: `meeting`, `chat`,
  `email`, `ticket`, `web`, `repo`, `doc`.
- `--expires-on <date>`: for a fact with a deadline or an end, the last day it is true.
- **Write only what the user said.** Do not add a number, a role, a repo or a cause that
  the user did not give. The examples here show the form.
  Never copy facts from an example.
- **The body must make sense alone**, without this conversation, with the names, numbers,
  paths, versions and dates the user gave.
- **Write in English**, also when the user speaks Ukrainian or Polish. The search model
  is English only.
- **Turn relative dates into real dates.** "Tomorrow" and "Friday" become `2026-10-08`,
  from today's date in your context, in the text and in every date flag. In the body,
  write the date without the name of the weekday.
- More examples of good notes are in [template.md](template.md).

## Secrets

Never save passwords, tokens, API keys or private keys. nv refuses text that looks like
one, but that check is a safety net for obvious forms only; leaving secrets out is your
job. Save how to get access instead: "Staging database password: ask DevOps in
#infra-help". When nv refuses, do not reword the secret; write the note without it.

## People

People are addressed by ID: a name is not unique. Before `--person` or `--owner`, run
`nv people search "anna"`. One match that fits: use that ID. Several, and the context
does not decide: ask. No match: `nv people add "<name the user said>"`. Add
`--role "<role>"` only when the user said the role.

## Commitments

A promise of action, by the user or to the user. This is what nv is most needed for.

```
nv add --title "Send retry numbers to Anna" --area work --type commitment \
  --planned 2026-10-08 --person 7 <<'EOF'
I promised Anna Nowak on the daily of 2026-10-07 to send last week's retry numbers.
EOF
nv commitment done 43
```

`--owner <id>` when someone else promised; leave it out when the user did. `--planned`
when a day is known. Finished: `nv commitment done`. Will not happen:
`nv commitment drop`. Moved: `nv commitment postpone 43 2026-10-10`.

## Change a note: is the old version still true today?

- **Yes**, it needs a fix or a detail: `nv note edit 42 --add-ticket PAY-1300`. Use
  `--add-…` and `--remove-…` for repos, tickets and people; plain `--repo`, `--ticket`
  and `--person` replace the whole list.
- **No**, something else is true now: `nv note replace 42 --title "…"` with the new body
  on stdin. The new note copies area, type, project, repos, tickets, people and source;
  flags you give override the copied fields. The old note stays, marked outdated.
- **It was never true**: `nv note delete 45`.
- Two notes about the same thing, and neither replaces the other: `nv note link 42 51`.
- A mistake of your own: `nv history undo` takes back the newest change.

Every command with its flags is in [cli-write.md](cli-write.md).

## Temporary rule: company data

Until Dmytro confirms his company's rules for AI tools, keep work notes free of
confidential company data: no customer data, no internal hostnames, URLs or IPs,
no code copied from company repos, no financial figures. Decisions, promises and
how-tos in general words are fine. Dmytro will remove this rule.
