# nv read commands

Commands that only read. All of them take `--json` when you need the full fields; the
compact text is enough for almost every answer.

## Search

```
nv search "why did login tokens expire too early"
nv search "retry" --repo billing-api --type decision
nv search --project Billing --since 2026-10-01 --limit 50
nv search --type commitment --status done --limit 50
nv search "exam date" --all
```

| Filter | Keeps |
| --- | --- |
| `--area <area>` | `work`, `learning` or `personal` |
| `--type <type>` | `decision`, `commitment`, `how-to`, `fact` or `idea` |
| `--status <status>` | commitments that are `todo`, `done` or `dropped` |
| `--project <name>` | notes of that project; the whole name, in any case |
| `--repo <name>` | notes about that repo |
| `--ticket <id>` | notes about that ticket |
| `--person <id>` | notes about the person and commitments they own |
| `--planned <date>` | commitments planned for exactly that day |
| `--since <date>` | notes created on or after that day |
| `--all` | expired notes too |
| `--limit <n>` | how many notes to show, default 5 |

- All filters combine with AND. A query text is optional when there is a filter.
- With a text, the best match is first. With only filters, the newest is first.
- Active notes are always above outdated ones.
- Only a search with only filters ends with `showing 5 of 7, use --limit`. A text search
  ranks every note and has no such line.
- Nothing found: `No notes found.`

## One note

```
nv note show 42
```

The same lines as a search result, then the whole body. It also shows an expired note.
The details can hold `replaces: #17` (this note is the newer one) and `related: #5, #9`
(notes about the same thing; read them too when the question is wider).

## Today

```
nv today
```

```
Planned for today (2026-10-07)
#2  Book the exam slot · planned 2026-10-05, overdue
#1  Send retry numbers to Anna

Others owe you
#3  Anna Nowak: Review the retry PR · planned 2026-10-08

2 more of yours have no date: nv search --type commitment --status todo
```

Done, dropped, outdated and expired commitments are not in this view. Nothing runs it
automatically.

## People

```
nv people search "anna"
nv people list
```

```
#1  Anna Nowak · QA lead
    aliases: Anna, anna.nowak@contoso.com
#3  Anna Kowalska · backend developer
    aliases: Anna, Ania
```

`nv people search` finds by main name or alias. Use the ID in `--person`.

## History

```
nv history
nv history --note 42
nv history --person 7 --limit 50
```

The change log, newest first: who changed what and when (`claude` or `user`). Use it when
the user asks when a note changed or what was saved today.
