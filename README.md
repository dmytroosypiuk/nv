# nv

A local, offline knowledge store for Claude Code. Claude saves short notes (decisions,
commitments, how-tos, facts, ideas) about work, learning and personal life, and finds
them again with keyword + semantic search. Nothing leaves the laptop.

- Design and every decision: [`docs/design.md`](docs/design.md)
- Embedding spike: [`spikes/embedding/REPORT.md`](spikes/embedding/REPORT.md)
- Rules for working on the code: [`CLAUDE.md`](CLAUDE.md)

## Install

nv is a Claude Code plugin. Add this repository as a marketplace and install the plugin:

```
claude plugin marketplace add dmytroosypiuk/nv
claude plugin install nv@nv-marketplace
```

Then allow what a plugin cannot allow for you, in `~/.claude/settings.json`
(`claude/settings.snippet.json` has the same rules):

```json
{ "permissions": { "allow": ["Bash(nv:*)", "Skill(nv:capture)", "Skill(nv:recall)"] } }
```

**Without typing commands:** put this in `~/.claude/settings.json` (or in a project's
`.claude/settings.json`) and Claude Code installs the plugin at the next session start:

```json
{
  "extraKnownMarketplaces": {
    "nv-marketplace": { "source": { "source": "github", "repo": "dmytroosypiuk/nv" } }
  },
  "enabledPlugins": { "nv@nv-marketplace": true },
  "permissions": { "allow": ["Bash(nv:*)", "Skill(nv:capture)", "Skill(nv:recall)"] }
}
```

**What the first session does.** The plugin's `SessionStart` hook downloads `nv` (31 MB)
and the embedding model (128 MB) from the GitHub release of this repository, checks their
SHA-256 against the release's `SHA256SUMS`, and puts them in `~/.nv/bin` and
`~/.nv/models`. The first session start waits for it (up to 10 minutes); later ones do not
download anything. The hook also tells Claude, in the context of every session, that `nv`
exists and when to use the two skills:

- `nv:capture` saves a note the moment a decision, a promise, a root cause or a how-to comes up.
- `nv:recall` searches the notes before Claude answers about the past or about plans.

**nv itself never uses the network.** Only the setup step above does, once per version.
`NV_NO_DOWNLOAD=1` turns even that off (then put the files in place yourself; see
`plugin/scripts/ensure-nv.sh`).

**Supported:** Linux x86_64 (glibc 2.35 or newer: Ubuntu 22.04, Debian 12, Fedora 36, ...)
and macOS on Apple silicon. Not Windows, not Linux arm64: build from source (`cargo build
--release`, the first build downloads ONNX Runtime once) and use the developer install below.

Data lives in one folder: `NV_HOME`, default `~/.nv` (`nv.db` and `models/`).

### Developer install

From a checkout, with the model in `models/` (`spikes/embedding/fetch-model.sh`, once):

```
./install.sh --dry-run    # see what it will do
./install.sh              # build; binary, model; the plugin from this folder; permissions
```

`install.sh` also removes the standalone skills and the `# nv:start` block that earlier
versions put into `~/.claude`.

### Release

Bump the version in `Cargo.toml`, `plugin/VERSION` and `plugin/.claude-plugin/plugin.json`
(a test keeps them equal), then tag `v<version>` and push the tag: `.github/workflows/release.yml`
builds both platforms, packs the model, and publishes the release. A manual run of that
workflow only builds and publishes nothing.

## Commands

```
nv add --title T --area work|learning|personal [--type …] [--repo R] [--ticket K]
       [--person ID] [--source-kind K --source-ref R] [--expires-on DATE]
       [--owner ID] [--planned DATE]                < body on stdin
nv search ["query"] [--area] [--type] [--repo] [--ticket] [--person ID]
          [--since DATE] [--planned DATE] [--all] [--limit N]
nv today
nv date [--days N]                        today and the next days with weekdays

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

- Check that the plugin installs there: a work laptop may not allow a third-party
  marketplace or a download hook. Only Linux x86_64 was tested; the macOS build is made by
  CI and has not been run by anyone yet.
- Check that `nv` is a free command name there (`which nv` / `where nv`).
- Check company rules for using Claude Code with project data.
