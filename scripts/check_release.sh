#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release_dir="$(mktemp -d "${TMPDIR:-/tmp}/kusd-release.XXXXXX")"
silverscript_commit="${SILVERSCRIPT_COMMIT:-3ed973335b59269293564805cc2c58a14595ec03}"
trap 'rm -rf "$release_dir"' EXIT

rsync -a \
  --exclude='.git/' \
  --exclude='.env' \
  --exclude='*.env.local' \
  --exclude='*.local.json' \
  --exclude='*.local.txt' \
  --exclude='.venv/' \
  --exclude='target/' \
  --exclude='vendor/' \
  --exclude='__pycache__/' \
  "$project_root/" "$release_dir/"

git clone --quiet https://github.com/kaspanet/silverscript.git \
  "$release_dir/vendor/silverscript"
git -C "$release_dir/vendor/silverscript" checkout --quiet --detach \
  "$silverscript_commit"

(
  cd "$release_dir"
  if [[ "${KUSD_ALLOW_LOCK_UPDATE:-0}" == "1" ]]; then
    cargo test --all-targets
  else
    cargo test --locked --all-targets
  fi
  python3 -m py_compile scripts/*.py
)

echo "Reproducible validation succeeded in a temporary workspace."
