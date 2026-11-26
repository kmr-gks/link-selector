#!/usr/bin/env bash
set -euo pipefail

status=0
bash "$(dirname "$0")/publish-release.sh" "$GITHUB_REF_NAME" "$GITHUB_REPOSITORY" --check || status=$?
case "$status" in
    0) echo 'published=true' >> "$GITHUB_OUTPUT" ;;
    10) echo 'published=false' >> "$GITHUB_OUTPUT" ;;
    *) exit "$status" ;;
esac
