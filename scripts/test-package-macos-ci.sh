#!/usr/bin/env bash
set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/package-macos-ci.sh"
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
cp "$script" "$scratch/package-macos-ci.sh"
unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_BASE64
unset MACOS_REQUIRE_NOTARIZATION MACOS_SIGNING_IDENTITY MACOS_SIGNING_KEYCHAIN MACOS_NOTARY_PROFILE
export RUNNER_TEMP="$scratch"
export TEST_SIGNING_LOG="$scratch/log"
export TEST_IDENTITY=0123456789ABCDEF0123456789ABCDEF01234567

cat > "$scratch/package-macos.sh" <<'SH'
#!/usr/bin/env bash
set -eu
[[ $1 == 'test app' && $2 == 'test.dmg' ]]
printf '%s\n' "$3" >> "$TEST_SIGNING_LOG"
if [[ $3 == --notarize ]]; then
    [[ $MACOS_SIGNING_IDENTITY == "$TEST_IDENTITY" && $MACOS_NOTARY_PROFILE == link-selector-notary ]]
    [[ -f "$MACOS_SIGNING_KEYCHAIN" ]]
fi
SH

security() {
    printf '%s\n' "$1" >> "$TEST_SIGNING_LOG"
    case "$1" in
        create-keychain) touch "${!#}" ;;
        import) [[ $(cat "$2") == certificate ]] ;;
        find-identity) printf '  1) %s "Developer ID Application: Test"\n' "$TEST_IDENTITY" ;;
    esac
}

xcrun() {
    [[ $1 == notarytool && $2 == store-credentials && $3 == link-selector-notary ]]
    [[ $4 == --keychain && -f $5 && $6 == --key && $(cat "$7") == api-key ]]
    [[ $8 == --key-id && $9 == test-key && ${10} == --issuer && ${11} == test-issuer ]]
    printf '%s\n' "$2" >> "$TEST_SIGNING_LOG"
}
export -f security xcrun

run() {
    bash "$scratch/package-macos-ci.sh" 'test app' test.dmg
}

warning=$(GITHUB_REF_TYPE=tag run)
[[ "$warning" == "::warning::Developer ID credentials are absent. The macOS DMG is ad-hoc signed and unnotarized." ]]
[[ $(cat "$TEST_SIGNING_LOG") == --adhoc ]]
rm "$TEST_SIGNING_LOG"

if MACOS_REQUIRE_NOTARIZATION=true run; then
    echo 'Required notarization should reject a missing configuration.' >&2
    exit 1
fi
if APPLE_API_KEY=test-key run; then
    echo 'A partial signing configuration must fail before packaging.' >&2
    exit 1
fi
[[ ! -e "$TEST_SIGNING_LOG" ]]

APPLE_CERTIFICATE=$(printf certificate | base64) \
APPLE_CERTIFICATE_PASSWORD=test-password APPLE_API_KEY=test-key APPLE_API_ISSUER=test-issuer \
APPLE_API_KEY_BASE64=$(printf api-key | base64) MACOS_REQUIRE_NOTARIZATION=true run
expected=$'create-keychain\nset-keychain-settings\nunlock-keychain\nimport\nset-key-partition-list\nfind-identity\nstore-credentials\n--notarize\ndelete-keychain'
[[ $(cat "$TEST_SIGNING_LOG") == "$expected" ]]
[[ -z $(find "$scratch" -name 'link-selector-signing.*' -print) ]]
