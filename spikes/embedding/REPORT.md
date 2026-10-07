# Embedding spike report

2026-10-07 · `bge-small-en-v1.5` through `fastembed` 7.1.0 (`ort` 2.0.0-rc.13, ONNX Runtime 1.28)

**Result: it works.** The model loads from local files only, runs with the network removed,
finds the expected note in the top 3 for 9 of 10 queries, and a cold query costs about 0.35 s.
Nothing blocks the real CLI. The query prefix made no difference to the hit rate: **do not use it**.

Machine: AMD Ryzen 5 5500U (12 threads), Linux x86_64, rustc 1.98.1, CPU only.

## Search quality

A = query as is. B = query with `Represent this sentence for searching relevant passages: `.
Notes were embedded as `"{title}\n{body}"` with no prefix. Scores are cosine (dot product).

| Query | Expected | Top 3 A (no prefix) | Top 3 B (prefix) | Hit A | Hit B |
| --- | --- | --- | --- | --- | --- |
| how many times do we retry billing calls | 1, 2 | #2 (0.86), #1 (0.85), #8 (0.68) | #2 (0.85), #1 (0.84), #8 (0.65) | yes | yes |
| why did login tokens expire too early | 4 | #4 (0.80), #12 (0.64), #8 (0.63) | #4 (0.78), #12 (0.62), #8 (0.59) | yes | yes |
| who can approve QA while Anna is away | 5 | #5 (0.75), #9 (0.63), #23 (0.63) | #5 (0.74), #9 (0.60), #23 (0.58) | yes | yes |
| how do I connect to the staging postgres | 6 | #6 (0.76), #18 (0.71), #17 (0.64) | #6 (0.76), #18 (0.70), #17 (0.61) | yes | yes |
| what did I promise to do tomorrow | 8 | #9 (0.62), #23 (0.58), #15 (0.58) | #9 (0.59), #15 (0.54), #23 (0.52) | **no** | **no** |
| http client that cannot stream large responses | 7 | #7 (0.74), #8 (0.61), #16 (0.61) | #7 (0.78), #8 (0.62), #16 (0.59) | yes | yes |
| docker setup needed to run tests on my mac | 17 | #17 (0.81), #21 (0.59), #15 (0.52) | #17 (0.82), #21 (0.59), #7 (0.53) | yes | yes |
| when is my exam | 14 | #14 (0.72), #23 (0.65), #15 (0.61) | #14 (0.69), #23 (0.63), #15 (0.59) | yes | yes |
| sqlite database locked error | 11 | #13 (0.60), #11 (0.57), #18 (0.56) | #13 (0.64), #11 (0.61), #18 (0.57) | yes | yes |
| present gift for my grandfather | 20 | #20 (0.76), #15 (0.54), #24 (0.51) | #20 (0.73), #15 (0.52), #23 (0.49) | yes | yes |

**Hit rate: A 9/10, B 9/10.**

### Recommendation: no prefix

- Same hit rate, and the same note at rank 1 in all 10 queries.
- The prefix did not widen the gap between rank 1 and rank 2 in any consistent way.
- It costs about 1.4 ms more per query (more tokens) and is one more thing to keep in sync
  with the model if `nv model reindex` ever switches models.

This is 10 queries on 25 notes, so it only shows that the prefix is not needed, not that it
never helps. Worth one more look once there are a few hundred real notes.

### What the results say about ranking

- **The one miss is not an embedding problem.** "what did I promise to do tomorrow" needs
  the note type (`commitment`), the owner (me) and the planned date. All three top results
  are commitments, but the vector cannot know that "tomorrow" means 2026-10-08. This query
  belongs to `nv today` / `--type commitment` filters, as the design already says.
- **Scores are squeezed into a narrow band.** Right answers score 0.57–0.86, unrelated notes
  0.50–0.65. A fixed score cut-off to detect "Nothing Found" will not be reliable; use rank
  and the gap to the next result instead.
- **"sqlite database locked error" found the right note only at rank 2** (0.57 vs 0.60 for
  "Use SQLite for nv"). This is where FTS5 keyword search should help: combining both
  rankings is needed, vector search alone is not enough.
- The two retry notes (#1 active, #2 outdated in real life) score almost the same, and the
  older one is first. The "active above outdated" rule has to come from the database, as designed.

## Timings

Release build, CPU, default thread count (all cores). Stable across 5 runs.

| Measure | Value |
| --- | --- |
| Model load (read files + create ONNX session + tokenizer) | 330–355 ms |
| Embed all 25 notes (one batch) | 310–330 ms, about 13 ms per note |
| Average query A, embedding + search over 25 vectors | 6.3 ms |
| Average query B, embedding + search over 25 vectors | 7.7 ms |
| Cold start: new process, load model, embed one query | about 345 ms wall time |
| Peak memory (VmHWM) | 330 MB |
| Release binary, stripped | 27.3 MB (26 MiB) |
| Model folder | 128 MiB (`model.onnx` 133 MB, the rest under 1 MB) |

The brute-force search itself is not measurable at 25 notes (microseconds). At 384 floats per
note it stays far below a millisecond for thousands of notes.

## Offline check

How it was verified:

1. The binary and `models/bge-small-en-v1.5/` were copied to an empty folder outside the repo,
   to show nothing else is needed.
2. It was run with `unshare -rn` (a new network namespace with only a loopback device that is
   down), `HF_HUB_OFFLINE=1`, and `HOME` pointing at an empty folder. No Hugging Face cache
   and no `.fastembed_cache` exist on this machine. `curl https://huggingface.co` in the same
   namespace fails with "Could not connect".
3. Result: exit code 0, same 9/10 and same timings as with the network. No files were created.

```
HF_HUB_OFFLINE=1 HOME=/nonexistent unshare -rn ./target/release/embedding-spike
```

The build also makes this hard to break: `fastembed` is compiled with `default-features = false`,
so the `hf-hub` crate (the model downloader) is not in the binary at all. The only HTTP client
in the dependency tree (`ureq`) is a build dependency of `ort-sys`, used to download ONNX
Runtime at build time. `strace` is not installed here, so network syscalls were not traced;
the empty network namespace is the proof.

### ONNX Runtime linking

**Static.** With the `ort-download-binaries-rustls-tls` feature, `ort-sys` downloads a prebuilt
ONNX Runtime static library at **build time** (cached in `~/.cache/ort.pyke.io`) and links it
into the binary. No `libonnxruntime.so` is needed. `ldd` shows only system libraries:

```
libc.so.6  libm.so.6  libstdc++.so.6  libgcc_s.so.1
```

### What must ship

```
nv                                  the binary
models/bge-small-en-v1.5/
  model.onnx                        133 MB
  tokenizer.json                    711 KB
  config.json
  special_tokens_map.json
  tokenizer_config.json
```

All four tokenizer/config files are required by `TokenizerFiles`; fastembed reads
`model_max_length` and `pad_token` from them. Nothing else.

## Problems found

None of these block the real CLI.

1. **Pooling is not set by default in the user-defined API.** `UserDefinedEmbeddingModel::new`
   leaves `pooling: None`. BGE needs the CLS token, so the code must call
   `.with_pooling(Pooling::Cls)`. Easy to forget, and wrong pooling gives vectors that look
   fine but rank worse. The real CLI should have a test that pins a known vector or ranking.
2. **Building needs the network once.** `cargo build` downloads the ONNX Runtime static
   library. This is a build step, so it fits the hard rule, but a corporate laptop that
   builds from source needs access to the pyke.io download server or a pre-filled `~/.cache/ort.pyke.io`.
   Shipping a prebuilt binary avoids it.
3. **`libstdc++` is a dynamic dependency.** Present on every normal Linux desktop, missing in
   minimal containers. "Single binary" is true in practice, not in the strict static sense.
4. **`ort` is still a release candidate** (2.0.0-rc.13). fastembed pins it, and the API used
   here is small, but pin exact versions in `Cargo.lock` and expect small breaks on upgrade.
5. **Memory peak is 330 MB** for a 133 MB model: the file is read into a `Vec` and ONNX Runtime
   keeps its own copy. Fine for a short-lived CLI process on a laptop.
6. **Every `nv search` pays about 0.35 s to load the model.** Acceptable for Claude calling a
   CLI. Commands that do not need vectors (`nv today`, `nv note show`, filter-only search)
   must not load the model. `nv add` should not wait for it either, which matches the design
   (embed in the background).
7. **Only Linux x86_64 was tested.** The design mentions macOS (Colima); the build and the
   static link should be checked there before relying on it.

## Not tested

- A quantized model (for example the `Xenova/bge-small-en-v1.5` export, roughly a quarter of the size): smaller and faster to load, with
  a possible quality cost. Worth a try if size or load time becomes a problem.
- Long notes. All test notes are one or two sentences; the model cuts input at 512 tokens.
- Thread count. The default uses all cores; `with_intra_threads` can cap it.
- More than 25 notes, and combined FTS5 + vector ranking.

## How to run

```
./fetch-model.sh                      # once, needs network
cargo build --release
./target/release/embedding-spike      # full experiment
./target/release/embedding-spike --cold "when is my exam"
```

`NV_MODEL_DIR` and `NV_NOTES` override the default paths.
