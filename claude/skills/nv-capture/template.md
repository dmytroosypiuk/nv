# The fields of a note, and good notes

A note needs only a title, a body and an area. Fill the other fields when you know them.

| Field | Flag | When |
| --- | --- | --- |
| title | `--title` (required) | Short, with the words you would search for later |
| body | stdin (required) | One sentence to a few paragraphs; makes sense alone |
| area | `--area` (required) | `work`: job, project, team, tickets, HR. `learning`: general knowledge, wherever it was learned. `personal`: own life, side projects |
| type | `--type` | When it is clear, see below |
| project | `--project` | Work notes, when known; always the same spelling |
| repos, tickets | `--repo`, `--ticket` | When the note is about them; can be given many times |
| people | `--person <id>` | People the note is about; can be given many times |
| source | `--source-kind` + `--source-ref` | Where it came from. Leave both out for the user's own thought |
| expires on | `--expires-on <date>` | Facts with a known end: the last day they are true |
| owner | `--owner <id>` | Commitments only: who promised. Leave out when the user promised |
| planned date | `--planned <date>` | Commitments only: the day it is planned for |

**Types:** `decision` (something was decided), `commitment` (someone promised an action),
`how-to` (steps to get something done), `fact` (true, no action and no decision; a
person's preference or a root cause is a fact), `idea` (a maybe, nothing decided).

**Source kinds:** `meeting`, `chat`, `email`, `ticket`, `web`, `repo`, `doc`. "Anna on
Teams" is source `chat` plus person Anna: people are not the source.

## Writing rules

1. The body must make sense alone. Not "As discussed, we'll use 5", but "Retry count for
   billing-api calls is 5 (was 3), agreed with Anna Nowak because of timeouts in PAY-1234."
2. Keep specifics: names, numbers, paths, versions, dates. Search finds them.
3. Write in English, also when the user speaks Ukrainian or Polish.
4. Real dates only: `2026-10-08`, never "tomorrow", in the text and in the flags.
5. Write the lesson, not the story of how you found it.

## Good notes

A root cause (a fact):

```
nv add --title "Login tokens expired early: local time compared with UTC" --area work \
  --type fact --repo auth-service --ticket AUTH-311 <<'EOF'
Login tokens expired too early because token.rs computed expires_at from local time and
compared it with UTC. Fixed on 2026-10-07 by using UTC in both places.
EOF
```

A fact with an end, from an email:

```
nv add --title "December vacation requests: enter in Workday by 14 November" --area work \
  --type fact --source-kind email --source-ref "HR, 2026-10-07" \
  --expires-on 2026-11-14 <<'EOF'
Vacation requests for December 2026 must be entered in Workday by 2026-11-14.
EOF
```

A commitment someone else owns:

```
nv add --title "Review the retry PR" --area work --type commitment --owner 7 \
  --planned 2026-10-09 --repo billing-api --ticket PAY-1234 <<'EOF'
Anna Nowak promised at sprint planning on 2026-10-06 to review the retry PR for
billing-api by Friday 2026-10-09.
EOF
```

A how-to without the secret:

```
nv add --title "How to get access to the staging database" --area work --type how-to <<'EOF'
Ask DevOps in #infra-help for the staging database password; it changes every month.
Connect through the bastion host, not directly.
EOF
```

A trap of a tool, in area learning:

```
nv add --title "SQLite FTS5: porter tokenizer is English only" --area learning \
  --type fact <<'EOF'
The porter tokenizer of SQLite FTS5 stems English words only ("retries" finds "retry").
Text in other languages is matched word for word.
EOF
```
