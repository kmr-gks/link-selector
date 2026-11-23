#!/usr/bin/env bash
set -euo pipefail

app=${1:?Provide the path to a built macOS app}
output=${2:?Provide the path for the output DMG}
mode=${3:---adhoc}
case "$mode" in
    --adhoc) ;;
    --notarize)
        : "${MACOS_SIGNING_IDENTITY:?Provide the Developer ID Application signing identity}"
        : "${MACOS_NOTARY_PROFILE:?Provide the notarytool keychain profile}"
        identity=(--sign "$MACOS_SIGNING_IDENTITY")
        profile=(--keychain-profile "$MACOS_NOTARY_PROFILE")
        if [[ -n ${MACOS_SIGNING_KEYCHAIN:-} ]]; then
            identity+=(--keychain "$MACOS_SIGNING_KEYCHAIN")
            profile+=(--keychain "$MACOS_SIGNING_KEYCHAIN")
        fi
        ;;
    *)
        echo "Unknown macOS packaging mode: $mode" >&2
        exit 2
        ;;
esac
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

output_directory=$(dirname "$output")
mkdir -p "$output_directory"
stage=$(mktemp -d "$output_directory/.package-macos.XXXXXX")
trap 'rm -rf "$stage"' EXIT
contents="$stage/contents"
bundle="$contents/Link Selector.app"
image="$stage/$(basename "$output")"
mkdir "$contents"

ditto "$app" "$bundle"
if [[ "$mode" == --notarize ]]; then
    codesign --force --options runtime --timestamp "${identity[@]}" "$bundle"
else
    codesign --force --sign - "$bundle"
fi
codesign --verify --deep --strict "$bundle"
ln -s /Applications "$contents/Applications"
hdiutil create -volname "Link Selector" -srcfolder "$contents" -fs HFS+ -format UDZO "$image"

if [[ "$mode" == --notarize ]]; then
    codesign --timestamp "${identity[@]}" "$image"
    # The notary service issues tickets for the outer DMG and its nested app.
    xcrun notarytool submit "$image" "${profile[@]}" \
        --wait --timeout 30m --output-format json > "$stage/notarization.json"
    python3 - "$stage/notarization.json" <<'PYTHON'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    result = json.load(file)
if result.get("status") != "Accepted":
    raise SystemExit(f"Notarization {result.get('id')}: {result.get('status')}")
PYTHON
    xcrun stapler staple "$image"
    xcrun stapler validate "$image"
    spctl --assess --type open --context context:primary-signature "$image"
fi

hdiutil verify "$image"
mv -f -- "$image" "$output"
