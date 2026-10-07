#!/usr/bin/env bash
# Setup step (network allowed): download bge-small-en-v1.5 ONNX model + tokenizer files
# from Hugging Face into <repo root>/models/bge-small-en-v1.5/.
# nv itself never downloads anything at runtime.
set -euo pipefail

REPO="BAAI/bge-small-en-v1.5"
REV="${REV:-main}"
BASE="https://huggingface.co/${REPO}/resolve/${REV}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="${ROOT}/models/bge-small-en-v1.5"

# remote path -> local file name
FILES=(
  "onnx/model.onnx:model.onnx"
  "tokenizer.json:tokenizer.json"
  "config.json:config.json"
  "special_tokens_map.json:special_tokens_map.json"
  "tokenizer_config.json:tokenizer_config.json"
)

mkdir -p "${DEST}"
for entry in "${FILES[@]}"; do
  remote="${entry%%:*}"
  local_name="${entry##*:}"
  if [[ -s "${DEST}/${local_name}" ]]; then
    echo "have  ${local_name}"
    continue
  fi
  echo "fetch ${remote}"
  curl --fail --location --silent --show-error \
    --output "${DEST}/${local_name}.part" "${BASE}/${remote}"
  mv "${DEST}/${local_name}.part" "${DEST}/${local_name}"
done

echo
echo "Model files in ${DEST}:"
ls -l "${DEST}"
( cd "${DEST}" && sha256sum model.onnx tokenizer.json )
