#!/usr/bin/env bash
set -euo pipefail

app=$1
output=$2
# Check the generated bundle, since Tauri merges Info.plist during packaging.
python3 - "$app/Contents/Info.plist" <<'PYTHON'
import plistlib
import sys

with open(sys.argv[1], "rb") as file:
    plist = plistlib.load(file)

schemes = {scheme for entry in plist.get("CFBundleURLTypes", [])
           for scheme in entry.get("CFBundleURLSchemes", [])}
documents = {uti for entry in plist.get("CFBundleDocumentTypes", [])
             if entry.get("CFBundleTypeRole") == "Viewer"
             for uti in entry.get("LSItemContentTypes", [])}
if not {"http", "https"} <= schemes or not {"public.html", "public.xhtml"} <= documents:
    raise SystemExit("Missing browser associations in the packaged Info.plist")
PYTHON

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

ditto "$app" "$stage/Link Selector.app"
/usr/bin/codesign --force --sign - "$stage/Link Selector.app"
/usr/bin/codesign --verify --strict "$stage/Link Selector.app"
ln -s /Applications "$stage/Applications"
mkdir -p "$(dirname "$output")"
hdiutil create -volname "Link Selector" -srcfolder "$stage" -fs HFS+ -format UDZO "$output"
hdiutil verify "$output"
