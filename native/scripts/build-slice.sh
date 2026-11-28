#!/bin/bash
# Builds one architecture of the app binary and prints where it landed.
# Usage: scripts/build-slice.sh <aarch64-apple-darwin|x86_64-apple-darwin> [profile]
# The profile is "release" (what ships) unless given; CI's pull request checks
# use "ci", the same build without the slow whole-program link step.
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET="$1"
PROFILE="${2:-release}"
# Match LSMinimumSystemVersion in bundle.sh, for the slice and the C code it builds.
export MACOSX_DEPLOYMENT_TARGET=12.0
rustup target add "$TARGET" >/dev/null 2>&1 || true
cargo build --profile "$PROFILE" -p billable --target "$TARGET" >&2
echo "target/$TARGET/$PROFILE/billable"
