---
name: nv-recall
description: Search the user's notes with the nv command before you answer. Use when the user asks about a past decision, a promise, a person, a setup, a cause or a plan.
allowed-tools: Bash(nv *)
---

# nv-recall: search the user's notes before you answer

nv is the user's local notes store: decisions, commitments, how-tos, facts and ideas from
work, learning and personal life. The user told these things to Claude once. If you
answer without searching, the answer ignores what they already said.

Search first, without asking for permission, when the question is about something nv may
know: what was decided, why something happened, who someone is, how something is set up,
what was promised, what is planned or open.

## How to search

```
nv search "how many times do we retry billing calls"
nv search "staging postgres access" --area work --limit 3
nv search --ticket PAY-1234 --limit 50
nv search --person 7 --type commitment --status todo --limit 50
nv search --planned 2026-10-08 --limit 50
nv today
```

- **Write the query in English**, as a question or as keywords, also when the user speaks
  Ukrainian or Polish. The search model is English only.
- **Exact questions are filter questions.** "What did I promise for tomorrow" is
  `--planned <date>`, not a text query. A ticket is `--ticket`, a project is `--project`,
  a person is `--person <id>`, open promises are `--type commitment --status todo`.
  A search with only filters is fine.
- **Turn relative dates into real dates** before you call nv. "Tomorrow", "Friday" and
  "last week" become `2026-10-08`, from today's date in your context. nv refuses words.
- **On a search with only filters, pass `--limit 50`.** The default is 5. When there are
  more, the last line says `showing 5 of 7, use --limit`: run it again with a bigger limit
  before you answer "that is all".
- **People are addressed by ID**, because a name is not unique. Run
  `nv people search "anna"` first; it shows every match with role and aliases. One match
  that fits: use that ID. Several, and the context does not decide: ask the user.
- `nv today` shows what the user planned for today (overdue included) and what others owe
  them. Run it when the user asks what is planned, what is open, or what they wait for.

All filters and the other read commands are in [cli-read.md](cli-read.md).

## How to read the results

```
#42  decision · work · 2026-10-06 · active
     Retry count for billing-api is 5
     Retry count for billing-api calls is 5 (was 3), agreed with Anna Nowak because of…
     repos: billing-api · tickets: PAY-1234 · people: Anna Nowak

#17  decision · work · 2026-09-12 · outdated → #42
     Retry count for billing-api is 3
     Retry count for billing-api calls is 3.
```

Each result has its ID, type, area, date and status, the title, the first line of the
body, and the details.

- **nv ranks, it does not judge.** A text search returns the closest notes even when none
  answers the question. Read them. If nothing fits, say that nv has nothing on it; do not
  bend a near miss into an answer.
- When the first line is not enough to answer, run `nv note show <id>` for the whole body.
- When two notes disagree, trust the newer one.
- `outdated → #42` means the note was replaced: #42 says what is true now. Answer from #42.
- A commitment shows `todo`, `done` or `dropped` in place of `active`.
- Expired notes are hidden. Add `--all` when the user asks about something that is over.
- Say where the answer comes from: "From nv #42 (2026-10-06): …".

The user decides when to look outside nv (meetings, tickets, email). Do not go there on
your own because nv had no answer.
