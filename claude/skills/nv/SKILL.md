---
name: nv
description: The user's local knowledge store (the nv command). Use it to save decisions, commitments, how-tos, facts and ideas the moment they come up, and to search before answering anything nv may know - what was decided, what was promised, who someone is, how something is set up, what is planned today.
---

# nv: the user's knowledge store

nv keeps short notes about the user's work, learning and personal life in a local
database, and finds them again by meaning and keyword. You write the notes and you read
them. The user rarely runs nv by hand, so if you do not save something, it is lost, and
if you do not search, the user gets an answer that ignores what they already told you.

nv cannot think. It stores, ranks and applies fixed rules. Deciding what matters, writing
a note that makes sense in a month, and judging search results are your job.

## When to save

Save right away, without asking for approval, when the user will be glad to find it in a
month and it is hard to find anywhere else.

| Save | Do not save |
| --- | --- |
| Decisions, commitments, ideas | Routine steps that worked (install, build, tests passed) |
| Why things happened: root causes of bugs | What git already shows (renames, small changes) |
| People: availability, preferences, roles | What the repo already documents (README, docs, comments) |
| Setup and how-to knowledge needed again | Secrets: passwords, tokens, API keys, private keys |
| Limits and traps of tools and libraries | A duplicate: edit the existing note instead |
| The lesson, not the story | Personal things said in passing; personal notes only when asked |

The user can also just say "save this".

If a detail is missing that the note needs (which Anna, which date, which repo), ask
before saving. A note with a guess in it is worse than a short question.

Say in one line what you saved ("Saved to nv: #42 Retry 5 times"). Do not ask for
permission first.

## How to save

Fields are flags. The body comes from stdin; always use a quoted heredoc (`<<'EOF'`), so
code, quotes and `$` arrive unchanged.

```
nv add --title "Retry count for billing-api is 5" --area work --type decision \
  --repo billing-api --ticket PAY-1234 --person 7 \
  --source-kind meeting --source-ref "Sprint planning, 2026-10-06" <<'EOF'
Retry count for billing-api calls is 5 (was 3), agreed with Anna Nowak because of
timeouts in PAY-1234. Code: `retry(max = 5)` in src/client.rs.
EOF
```

- `--title` (required): short, with the words you would search for later.
- `--area` (required), exactly one: `work` (job, project, team, tickets, HR), `learning`
  (general knowledge, wherever it was learned), `personal` (own life, side projects).
- `--type` when it is clear: `decision` (something was decided), `commitment` (someone
  promised an action), `how-to` (steps to get something done), `fact` (true, no action
  and no decision; a person's preference is a fact), `idea` (a maybe, nothing decided).
- `--project`, `--repo`, `--ticket`, `--person <id>`: when the note is about them.
  `--repo`, `--ticket` and `--person` can be given many times.
- `--source-kind` with `--source-ref`: where it came from. Kinds: `meeting`, `chat`,
  `email`, `ticket`, `web`, `repo`, `doc`. Leave both out for the user's own thought.
  "Anna on Teams" is source `chat` plus person Anna: people are not the source.
- `--expires-on <date>`: for facts with an end, the last day they are true. After that
  day the note is hidden from search.

```
nv add --title "Anna Nowak is on vacation" --area work --type fact \
  --person 7 --expires-on 2026-10-16 <<'EOF'
Anna Nowak is on vacation from 2026-10-12 to 2026-10-16. Piotr Zielinski approves QA
while she is away.
EOF
```

## Writing rules

1. **The body must make sense alone**, without this conversation. Not "As discussed,
   we'll use 5", but "Retry count for billing-api calls is 5 (was 3), agreed with Anna
   Nowak because of timeouts in PAY-1234."
2. **Keep specifics**: names, numbers, paths, versions, dates. Search finds them, and
   they are what makes a note useful.
3. **Write notes and search queries in English**, also when the user speaks Ukrainian or
   Polish. The search model is English only.
4. **Turn relative dates into real dates** before calling nv. "Tomorrow", "next week",
   "Friday" mean nothing in a month. Use today's date from your context and write
   `2026-10-08`, in the text and in every date flag.

## Never save secrets

No passwords, tokens, API keys or private keys. Save how to get access instead: "Staging
database password: ask DevOps in #infra-help". nv refuses text that looks like a secret.
When it does, do not reword the secret to get past the check; write the note without it.

## People

People are addressed by ID. A name is not unique: there can be two Annas.

```
nv people search "anna"
nv people add "Anna Nowak" --role "QA lead" --alias "Anna" --alias "anna.nowak@contoso.com"
nv people alias 7 "Anna N."
nv people merge 7 12
nv people list
```

- Before linking a person, run `nv people search`. It shows every match with role and
  aliases, so you see which Anna this is.
- One match that fits the context: use that ID. Several matches and the context does not
  decide: ask the user. Never guess between two people.
- No match: add the person, with the role if you know it.
- A new form of a known name (an email, a Teams name, "Anna N."): add it as an alias.
- The same person twice: `nv people merge <keep-id> <other-id>`.

## Commitments

A note that holds a promise of action is a commitment: the user promised something, or
someone promised something to the user. This is what nv is most needed for, because a
passing "I'll do it" is otherwise forgotten.

```
nv add --title "Send retry numbers to Anna" --area work --type commitment \
  --planned-for 2026-10-08 --person 7 <<'EOF'
I promised Anna Nowak on the daily of 2026-10-07 to send the retry numbers for
billing-api from last week.
EOF
nv add --title "Review the retry PR" --area work --type commitment --owner 7 <<'EOF'
Anna Nowak promised on 2026-10-07 to review the retry PR for billing-api (PAY-1234).
EOF
nv commitment done 43
nv commitment drop 44
nv commitment postpone 43 2026-10-10
nv today
```

- `--owner <person-id>` when someone else promised. Leave it out when the user promised.
- `--planned-for <date>` when a day is known. Later the date moves only with `postpone`.
- When the user says it is finished, run `done`. When it will not happen, `drop`. Both
  are final; if the work is needed again, save a new commitment.
- `nv today` shows what the user planned for today (overdue included) and what others
  owe them. Run it when the user asks what is planned, what is open, or what they are
  waiting for. Nothing runs it automatically.

## Edit, replace or delete

Ask one question: is the old version still true today?

- **Yes**, it only needs a typo fixed or a detail added: edit the same note.
- **No**, something else is true now: replace it. The old note stays, marked outdated,
  and points to the new one.
- **It was never true**, a mistake: delete it.

```
nv note edit 42 --title "Retry count for billing-api is 5" --ticket PAY-1234
nv note edit 42 --body <<'EOF'
Retry count for billing-api calls is 5 (was 3). Applies to POST calls only.
EOF
nv note replace 42 --title "Retry count for billing-api is 7" --area work --type decision <<'EOF'
Retry count for billing-api calls is 7 (was 5), decided on 2026-11-03 after the
incident in PAY-1300.
EOF
nv note delete 45
nv note link 42 51
nv note show 42
```

On edit, each flag replaces that field; `--repo`, `--ticket` and `--person` replace the
whole list. The body changes only with `--body`.

## When to search

Search before you answer a question nv may know: what was decided, why something
happened, who someone is, how something is set up, what was promised. Search also before
you save, so that you edit an existing note instead of making a duplicate. No permission
is needed for searching.

```
nv search "how many times do we retry billing calls"
nv search "staging postgres access" --area work --limit 3
nv search --ticket PAY-1234
nv search --person 7 --type commitment
nv search --planned 2026-10-08
nv search "exam date" --all
```

- Write the query in English, as a question or as keywords.
- Exact questions are filter questions. "What did I promise for tomorrow" is
  `--planned <date>` with a real date, not a text query. A ticket is `--ticket`, a person
  is `--person <id>`. A search with only filters is fine.
- `--since <date>` keeps notes created on or after that day. `--all` includes expired
  notes.
- **nv ranks, it does not judge.** It returns the closest notes even when none answers
  the question. Read them. If nothing fits, say that nv has nothing on it; do not bend a
  near miss into an answer.
- Each result shows its date, type and status. When two notes disagree, trust the newer
  one. A line with `outdated → #42` means the note was replaced: read #42 with
  `nv note show 42`.
- Search shows titles. Run `nv note show <id>` for the body before you rely on a note.

The user decides when to look outside nv (meetings, tickets, email). Do not go there on
your own because nv had no answer.

## When something went wrong

Every change is in the change log and can be taken back.

```
nv history
nv history --note 42
nv history undo
nv history undo 31
```

`nv history undo` without an ID takes back the newest change. Only the last change of a
note can be undone; nv names the change to undo first when there is a newer one.
