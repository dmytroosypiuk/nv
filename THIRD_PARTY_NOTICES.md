# Third-party notices

nv is MIT licensed (see `LICENSE`). A release of nv contains or downloads the following
work of others.

## The embedding model: bge-small-en-v1.5

- By BAAI (Beijing Academy of Artificial Intelligence), MIT License.
- Source: https://huggingface.co/BAAI/bge-small-en-v1.5
- The release downloads the ONNX model and the tokenizer files unchanged, at setup time.
  Their SHA-256 sums are pinned in `spikes/embedding/model.sha256`. The model archive of a
  release has a `NOTICE` file with this information.

## ONNX Runtime

- By Microsoft, MIT License. Source: https://github.com/microsoft/onnxruntime
- Linked statically into the `nv` binary through the `ort` crate (pyke), which downloads
  the prebuilt library at build time.

## Rust crates

- The crates nv is built from are listed in `Cargo.lock`. Each is under its own licence,
  mostly MIT or Apache-2.0. SQLite, bundled through `rusqlite`, is in the public domain.
