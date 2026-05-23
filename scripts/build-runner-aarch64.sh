#!/usr/bin/env bash
# Cross-build the exact-runner binary for aarch64-unknown-linux-gnu (Pi
# 3/4/5 with 64-bit OS) from a darwin or linux dev box.
#
# Uses cargo-zigbuild because zig ships a cross libc + linker, so we don't
# need to install a separate aarch64 toolchain. Both tools come from the
# nix devshell — enter it (`nix develop` or `direnv allow`) before running.
#
# Output: target/aarch64-unknown-linux-gnu/release/exact-runner
#
# Optional scp: BENCH_PI_HOST=pi@bench.lan ./scripts/build-runner-aarch64.sh
#   copies the binary to ~/exact-runner on the Pi after building.

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="aarch64-unknown-linux-gnu"
ARTIFACT="target/${TARGET}/release/exact-runner"

if ! command -v cargo-zigbuild >/dev/null 2>&1; then
  echo "cargo-zigbuild not found. Are you in the nix devshell?" >&2
  echo "Run: nix develop  (or 'direnv allow' once in the repo root)" >&2
  exit 1
fi

echo "==> cargo zigbuild -p exact-runner --release --target ${TARGET}"
cargo zigbuild -p exact-runner --release --target "${TARGET}"

ls -lh "${ARTIFACT}"
file "${ARTIFACT}" || true

if [[ -n "${BENCH_PI_HOST:-}" ]]; then
  echo "==> scp to ${BENCH_PI_HOST}:~/exact-runner"
  scp "${ARTIFACT}" "${BENCH_PI_HOST}:~/exact-runner"
fi
