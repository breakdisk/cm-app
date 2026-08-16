#!/usr/bin/env bash
#
# Every service Dockerfile must build with a Rust at least as new as the
# workspace MSRV.
#
# These are two hand-maintained numbers in 23 files that have to agree, and
# nothing connected them. On 2026-08-15 a dependency bump raised the real floor
# to 1.94.1 while all 22 Dockerfiles still said `FROM rust:1.91-slim-bookworm`.
# `cargo check`, `cargo clippy` and the whole test matrix pass in that state —
# they use the runner's toolchain, not the image's — so CI goes green and every
# service image then fails to compile at GHCR build time, well after review.
#
# The reverse also matters: lowering the workspace MSRV without lowering the
# images is harmless, but raising an image past the MSRV silently means the
# declared MSRV is untested. Only the "image older than MSRV" direction is a
# build break, so that is what fails here; a newer image is reported and allowed.
#
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

MSRV="$(grep -E '^rust-version = ' Cargo.toml | head -1 | sed -E 's/.*"([0-9]+\.[0-9]+(\.[0-9]+)?)".*/\1/')"

if [ -z "$MSRV" ]; then
  echo "FAIL: could not read rust-version from the workspace Cargo.toml."
  exit 1
fi

echo "Workspace MSRV: $MSRV"

# Compare dotted versions numerically: returns 0 when $1 < $2.
older_than() {
  [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -1)" = "$1" ]
}

FAILURES=0
CHECKED=0

while IFS= read -r df; do
  # First stage tag is the compiling stage; runtime stages are debian/distroless.
  PIN="$(grep -oE '^FROM rust:[0-9]+\.[0-9]+(\.[0-9]+)?' "$df" | head -1 | sed -E 's/^FROM rust://')"
  [ -z "$PIN" ] && continue
  CHECKED=$((CHECKED + 1))
  if older_than "$PIN" "$MSRV"; then
    echo "FAIL: $df pins rust:$PIN, older than the workspace MSRV $MSRV."
    FAILURES=$((FAILURES + 1))
  fi
done < <(find services -name Dockerfile -type f | sort)

echo "Checked $CHECKED Dockerfile(s) with a rust: base image."

if [ "$FAILURES" -gt 0 ]; then
  echo
  echo "$FAILURES Dockerfile(s) would fail to compile the current Cargo.lock."
  echo "Raise the FROM rust:<version> pin, or lower the workspace rust-version."
  exit 1
fi

echo "Every service image builds with a Rust at least as new as the MSRV."
