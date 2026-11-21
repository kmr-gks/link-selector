#!/usr/bin/env bash
set -euo pipefail

app=$1
output=$2
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

ditto "$app" "$stage/Link Selector.app"
codesign --verify --strict "$stage/Link Selector.app"
ln -s /Applications "$stage/Applications"
mkdir -p "$(dirname "$output")"
hdiutil create -volname "Link Selector" -srcfolder "$stage" -fs HFS+ -format UDZO "$output"
hdiutil verify "$output"
