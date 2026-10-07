# nv

A local, offline knowledge store for Claude Code. Claude saves short notes (decisions,
commitments, how-tos, facts, ideas) about work, learning and personal life, and finds
them again with keyword + semantic search. Nothing leaves the laptop.

- Design and every decision: [`docs/design.md`](docs/design.md)
- Embedding spike: [`spikes/embedding/REPORT.md`](spikes/embedding/REPORT.md)
- Rules for working on the code: [`CLAUDE.md`](CLAUDE.md)

## Install

```
spikes/embedding/fetch-model.sh   # once, needs the network: model into models/
./install.sh --dry-run            # see what it will do
./install.sh                      # binary, model, Claude Code skill, permission
```

`install.sh` puts `nv` in `~/.local/bin`, the model in `~/.nv/models`, the skill in
`~/.claude/skills/nv`, and adds `Bash(nv:*)` to `~/.claude/settings.json`. It adds no
hook. The first build downloads ONNX Runtime once; after that nv never uses the network.

Data lives in one folder: `NV_HOME`, default `~/.nv` (`nv.db` and `models/`).

## Commands

```
nv add --title T --area work|learning|personal [--type …] [--repo R] [--ticket K]
       [--person ID] [--source-kind K --source-ref R] [--expires-on DATE]
       [--owner ID] [--planned-for DATE]            < body on stdin
nv search ["query"] [--area] [--type] [--repo] [--ticket] [--person ID]
          [--since DATE] [--planned DATE] [--all] [--limit N]
nv today

nv note show|edit|replace|delete|link …
nv commitment done|drop|postpone|today …
nv people add|edit|list|search|alias|merge …
nv history [--note ID] [--person ID]      nv history undo [CHANGE-ID]
nv model info|reindex
```

Every command takes `--help`; most take `--json`.

## Develop

```
cargo test                 # fast, offline, fake embedder
cargo test -- --ignored    # the real model, needs models/bge-small-en-v1.5
cargo fmt && cargo clippy --all-targets -- -D warnings
```

Test first, always: see `CLAUDE.md`.

## Before relying on it at work

- Build and run the tests on the work laptop's OS. Only Linux x86_64 was tested; on macOS
  and Windows check the static ONNX Runtime link and the model path.
- Check that `nv` is a free command name there (`which nv` / `where nv`).
- Check company rules for using Claude Code with project data.
