#!/usr/bin/env bash
set -euo pipefail

package="$(cd "$(dirname "$0")" && pwd)/package-macos.sh"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

app="$scratch/Input App.app"
output="$scratch/Release Files/Link Selector.dmg"
mkdir -p "$app/Contents" "$scratch/bin"
cat > "$app/Contents/Info.plist" <<'PLIST'
<plist version="1.0"><dict>
<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLSchemes</key>
<array><string>http</string><string>https</string></array></dict></array>
<key>CFBundleDocumentTypes</key><array><dict><key>CFBundleTypeRole</key><string>Viewer</string>
<key>LSItemContentTypes</key><array><string>public.html</string><string>public.xhtml</string></array>
</dict></array></dict></plist>
PLIST

export PACKAGE_TEST_CALLS="$scratch/calls"
export PATH="$scratch/bin:$PATH"
cat > "$scratch/bin/packaging-tool" <<'TOOL'
#!/usr/bin/env bash
set -euo pipefail
case "${0##*/}:$1" in
    ditto:*)
        printf 'copy app\n' >> "$PACKAGE_TEST_CALLS"
        cp -R "$1" "$2"
        ;;
    codesign:--verify)
        [[ "$2" == --deep && "$3" == --strict && -d "$4" ]]
        printf 'verify app\n' >> "$PACKAGE_TEST_CALLS"
        ;;
    codesign:--force)
        if [[ "$2" == --sign ]]; then
            [[ "$3" == - && -d "$4" ]]
            printf 'ad hoc app\n' >> "$PACKAGE_TEST_CALLS"
        else
            [[ "$2" == --options && "$3" == runtime && "$4" == --timestamp ]]
            [[ "$5" == --sign && "$6" == "$MACOS_SIGNING_IDENTITY" && -d "${!#}" ]]
            if [[ -n "${MACOS_SIGNING_KEYCHAIN:-}" ]]; then
                [[ "$7" == --keychain && "$8" == "$MACOS_SIGNING_KEYCHAIN" ]]
            fi
            printf 'sign app\n' >> "$PACKAGE_TEST_CALLS"
        fi
        ;;
    codesign:--timestamp)
        [[ "$2" == --sign && "$3" == "$MACOS_SIGNING_IDENTITY" && -f "${!#}" ]]
        if [[ -n "${MACOS_SIGNING_KEYCHAIN:-}" ]]; then
            [[ "$4" == --keychain && "$5" == "$MACOS_SIGNING_KEYCHAIN" ]]
        fi
        printf 'sign image\n' >> "$PACKAGE_TEST_CALLS"
        ;;
    hdiutil:create)
        [[ "$4" == -srcfolder && -d "$5/Link Selector.app" ]]
        [[ $(readlink "$5/Applications") == /Applications ]]
        printf 'create image\n' >> "$PACKAGE_TEST_CALLS"
        printf 'new image' > "${!#}"
        ;;
    hdiutil:verify)
        [[ -s "$2" ]]
        printf 'verify image\n' >> "$PACKAGE_TEST_CALLS"
        ;;
    xcrun:notarytool)
        [[ "$2" == submit && -f "$3" && "$4" == --keychain-profile && "$5" == "$MACOS_NOTARY_PROFILE" ]]
        if [[ -n "${MACOS_SIGNING_KEYCHAIN:-}" ]]; then
            [[ "$6" == --keychain && "$7" == "$MACOS_SIGNING_KEYCHAIN" ]]
        fi
        [[ "${!#}" == json ]]
        printf 'submit image\n' >> "$PACKAGE_TEST_CALLS"
        printf '{"id":"test-submission","status":"%s"}\n' "${PACKAGE_TEST_NOTARY_STATUS:-Accepted}"
        ;;
    xcrun:stapler)
        [[ -f "$3" ]]
        printf '%s ticket\n' "$2" >> "$PACKAGE_TEST_CALLS"
        ;;
    spctl:--assess)
        [[ "$2" == --type && "$3" == open && "$4" == --context && "$5" == context:primary-signature && -f "$6" ]]
        printf 'assess image\n' >> "$PACKAGE_TEST_CALLS"
        ;;
    *) exit 2 ;;
esac
TOOL
chmod +x "$scratch/bin/packaging-tool"
for tool in ditto codesign hdiutil xcrun spctl; do
    ln -s packaging-tool "$scratch/bin/$tool"
done

bash "$package" "$app" "$output" --adhoc
[[ $(cat "$output") == 'new image' ]]
diff -u - "$PACKAGE_TEST_CALLS" <<'CALLS'
copy app
ad hoc app
verify app
create image
verify image
CALLS

: > "$PACKAGE_TEST_CALLS"
if MACOS_SIGNING_IDENTITY= MACOS_NOTARY_PROFILE= bash "$package" "$app" "$output" --notarize > "$scratch/error" 2>&1; then
    echo 'Notarized packaging accepted missing credentials.' >&2
    exit 1
fi
grep -Fq 'MACOS_SIGNING_IDENTITY' "$scratch/error"
[[ ! -s "$PACKAGE_TEST_CALLS" ]]

export MACOS_SIGNING_IDENTITY='Developer ID Application: Test'
export MACOS_NOTARY_PROFILE='test profile'
export MACOS_SIGNING_KEYCHAIN="$scratch/release signing.keychain-db"
printf 'previous image' > "$output"
if PACKAGE_TEST_NOTARY_STATUS=Invalid bash "$package" "$app" "$output" --notarize > "$scratch/error" 2>&1; then
    echo 'Packaging accepted a rejected notarization.' >&2
    exit 1
fi
grep -Fq 'Notarization test-submission: Invalid' "$scratch/error"
[[ $(cat "$output") == 'previous image' ]]
[[ $(tail -n 1 "$PACKAGE_TEST_CALLS") == 'submit image' ]]

: > "$PACKAGE_TEST_CALLS"
bash "$package" "$app" "$output" --notarize
[[ $(cat "$output") == 'new image' ]]
diff -u - "$PACKAGE_TEST_CALLS" <<'CALLS'
copy app
sign app
verify app
create image
sign image
submit image
staple ticket
validate ticket
assess image
verify image
CALLS
