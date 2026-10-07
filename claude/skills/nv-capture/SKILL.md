---
name: nv-capture
description: Save a note with the nv command. Do not ask for approval. Use when the user tells you a decision, a promise, the cause of a bug, a fact about a person or a procedure. Use also when the user says "remember" or "save this", or when a saved note must change.
allowed-tools: Bash(nv *)
---

# nv-capture: save what is worth keeping

nv is the user's local notes store. You write the notes; the user rarely runs nv by hand.
If you do not save a decision or a passing "I'll do it", it is lost.

## Steps, every time

1. **Search first**: `nv search` on the same topic. If a note is already there, edit or
   replace it instead of making a duplicate.
2. **Save right away.** Ask about missing facts, never about whether to save. If the note
   needs a detail you do not have (which Anna, which date, which repo), ask that one
   question; a note with a guess in it is worse.
3. **Confirm in one line**, with the weekday for any date: "Saved to nv: #42 Retry count
   for billing-api is 5" or "Saved to nv: #43 Send retry numbers to Anna,
   planned for Fri 2026-10-09". The user catches a wrong date there.

Save when the user will be glad to find it in a month and it is hard to find anywhere
else: decisions, commitments, ideas, root causes, people's availability and roles, setup
and how-to knowledge, limits and traps of tools. Do not save routine steps, what git or
the repo docs already show, or personal things said in passing. The full table is in
[saving-rules.md](saving-rules.md).

## How to save

Fields are flags. The body comes from stdin; always use a quoted heredoc (`<<'EOF'`), so
code, quotes and `$` arrive unchanged.

```
nv search "retry count billing-api"
nv add --title "Retry count for billing-api is 5" --area work --type decision \
  --repo billing-api --ticket PAY-1234 --person 7 \
  --source-kind meeting --source-ref "Sprint planning, 2026-10-06" <<'EOF'
Retry count for billing-api calls is 5 (was 3), agreed with Anna Nowak because of
timeouts in PAY-1234. Code: `retry(max = 5)` in src/client.rs.
EOF
```

- **The body must make sense alone**, without this conversation, with the names, numbers,
  paths, versions and dates.
- **Write in English**, also when the user speaks Ukrainian or Polish. The search model
  is English only.
- **Turn relative dates into real dates.** "Tomorrow" and "Friday" become `2026-10-08`,
  from today's date in your context, in the text and in every date flag.
- `--title`, `--area` (`work`, `learning`, `personal`) and the body are required. All
  fields, the types and good examples are in [template.md](template.md).

## Secrets

Never save passwords, tokens, API keys or private keys. nv refuses text that looks like
one, but that check is a safety net for obvious forms only; leaving secrets out is your
job. Save how to get access instead: "Staging database password: ask DevOps in
#infra-help". When nv refuses, do not reword the secret; write the note without it.

## People

People are addressed by ID: a name is not unique. Before `--person` or `--owner`, run
`nv people search "anna"`. One match that fits: use that ID. Several, and the context
does not decide: ask. No match: `nv people add "Anna Nowak" --role "QA lead"`.

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
