#!/usr/bin/env sh

set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

if ! command -v node >/dev/null 2>&1; then
  echo "AriaType bootstrap requires Node.js 18 or newer." >&2
  echo "Install Node.js, then rerun ./bootstrap.sh." >&2
  exit 1
fi

exec node "$ROOT_DIR/scripts/bootstrap.mjs" "$@"

