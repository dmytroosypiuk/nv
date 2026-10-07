# nv write commands

Every change goes to the change log, so `nv history undo` can take it back. Answers are
one line: `Saved #42`, `Edited #42`, `Deleted #42`, `Saved #43, replaces #42`. A planned
or expiry date is in the answer with its weekday: `Saved #43, planned Thu 2026-10-08`,
`Saved #44, expires Sat 2026-11-14`, `Postponed #43 to Mon 2026-10-12`. Check that
weekday against what the user said.

## Add

```
nv add --title "Anna Nowak is on vacation" --area work --type fact \
  --person 7 --expires-on 2026-10-16 <<'EOF'
Anna Nowak is on vacation from 2026-10-12 to 2026-10-16. Piotr Zielinski approves QA
while she is away.
EOF
```

Flags: `--title`, `--area`, `--type`, `--project`, `--repo`, `--ticket`, `--person`,
`--source-kind` with `--source-ref`, `--expires-on`, `--owner`, `--planned`. `--owner`
and `--planned` are for `--type commitment` only. An unknown person ID is refused.

## Edit: the note is still true

```
nv note edit 42 --title "Retry count for billing-api is 5" --add-ticket PAY-1300
nv note edit 42 --add-repo gateway --add-person 9 --remove-ticket PAY-1234
nv note edit 42 --remove-repo gateway --remove-person 9
nv note edit 42 --repo billing-api --ticket PAY-1234 --person 7
nv note edit 42 --body <<'EOF'
Retry count for billing-api calls is 5 (was 3). Applies to POST calls only.
EOF
nv note edit 43 --planned 2026-10-10 --owner 7
```

- Each flag replaces that field. The body changes only with `--body`, from stdin.
- `--add-repo`, `--add-ticket`, `--add-person` add one value and keep the others.
  `--remove-…` takes one out; a value the note does not have is refused.
- Plain `--repo`, `--ticket` and `--person` replace the whole list. Use them only when
  you mean to; they cannot be mixed with the add and remove form of the same list.
- `--planned` works on a commitment that is still todo.
- A field cannot be cleared with edit. Replace the note when a field must go.

## Replace: something else is true now

```
nv note replace 42 --title "Retry count for billing-api is 7" <<'EOF'
Retry count for billing-api calls is 7 (was 5), decided on 2026-11-03 after the
incident in PAY-1300.
EOF
nv note replace 42 --title "Retry count for billing-api is 7" --ticket PAY-1300 \
  --source-kind chat --source-ref "Teams, 2026-11-03" <<'EOF'
Retry count for billing-api calls is 7 (was 5), decided on 2026-11-03.
EOF
```

- `--title` and the body are new. Area, type, project, repos, tickets, people and source
  are copied from the old note; flags you give override the copied fields. A list flag
  replaces the whole copied list.
- A commitment that stays a commitment keeps its owner and planned date, and starts as
  todo again. The expiry date is never copied: give `--expires-on` when the new note has
  an end.
- The old note stays, marked outdated, and points to the new one. An outdated note cannot
  be replaced again: replace the newer note.

## Delete and link

```
nv note delete 45
nv note link 42 51
```

- Delete only a note that was never true. A note that replaced another cannot be deleted.
- Link two notes that are about the same thing when neither replaces the other. The link
  has no direction and shows on both as `related`.

## Commitments

```
nv add --title "Book the exam slot" --area learning --type commitment \
  --planned 2026-10-12 <<'EOF'
I will book the slot for the AZ-204 exam on Monday 2026-10-12.
EOF
nv commitment done 43
nv commitment drop 44
nv commitment postpone 43 2026-10-10
```

- `done` and `drop` are final. If the work is needed again, save a new commitment.
- `postpone` moves the planned date and also gives a first date to a commitment without
  one.

## People

```
nv people search "anna"
nv people add "Anna Nowak" --role "QA lead" --alias "Anna" --alias "anna.nowak@contoso.com"
nv people alias 7 "Anna N."
nv people edit 7 --role "QA manager"
nv people merge 7 12
```

- Search before you add, so that nobody is saved twice.
- A new form of a known name (an email, a Teams name, "Anna N."): add it as an alias.
- The same person twice: `nv people merge <keep-id> <other-id>`.

## Undo

```
nv history --limit 5
nv history undo
nv history undo 31
```

Without an ID, undo takes back the newest change. Only the last change of a note can be
undone; when there is a newer one, nv names the change to undo first.
