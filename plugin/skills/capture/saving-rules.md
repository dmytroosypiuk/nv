# What to save

Save a note only if the user will be glad to find it in a month and it is hard to find
anywhere else.

| Save | Do not save |
| --- | --- |
| Decisions, commitments, ideas | Routine steps that worked (install, build, tests passed) |
| Why things happened: root causes of bugs | What git already shows (renames, small changes) |
| People: availability, preferences, roles | What the repo already documents (README, docs, comments) |
| Setup and how-to knowledge needed again | Secrets: passwords, tokens, API keys, private keys |
| Limits and traps of tools and libraries | A duplicate: edit the existing note instead |
| The lesson, not the story: "Library X does not support streaming" | Personal things said in passing; personal notes only when asked |

The user can also just say "save this" or "remember"; then save it, whatever it is,
except a secret.

## One note, one thing

A message with a decision and two promises becomes three notes: one decision and two
commitments. Each must make sense alone.

## Ask or save

Ask about missing facts, never about whether to save.

- A fact the note needs is missing: which of two Annas, which day "next week" means,
  which repo. Ask one short question, then save.
- Everything is clear: save and say so in one line. Do not ask "should I save this?".

## Facts with an end

A fact that stops being true on a known day gets `--expires-on <date>`: the last day it
is true. "Anna is on vacation until Friday 2026-10-16" expires on 2026-10-16. After that
day the note is hidden from search. No known end: no expiry; replace the note when the
situation changes.

## Secrets

- Never: passwords, tokens, API keys, private keys, recovery codes, connection strings
  with a password in them.
- Instead, save how to get access: "Staging database password: ask DevOps in #infra-help",
  "The API key is in the team vault under billing-api/staging".
- nv refuses obvious forms: `ghp_…`, `sk-…`, `password=…`, "password is hunter2", private
  key blocks, long random strings. It cannot see a short password in other words, a PIN
  or a recovery phrase. The check is a safety net; leaving secrets out is your job.
- When nv refuses a note, do not reword the secret to get past the check. Write the note
  without it.

## The same thing again

Search before you save. When a note on the same thing exists:

- still true, and a detail is new: edit it;
- no longer true: replace it;
- about the same thing but a different point: save the new note and link the two.
