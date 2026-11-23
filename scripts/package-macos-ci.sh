#!/usr/bin/env bash
set -euo pipefail

app=${1:?Provide the app bundle}
output=${2:?Provide the DMG path}
packager="$(dirname "$0")/package-macos.sh"
credentials=(APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_BASE64)
provided=0
missing=()
for credential in "${credentials[@]}"; do
    if [[ -n ${!credential:-} ]]; then
        provided=$((provided + 1))
    else
        missing+=("$credential")
    fi
done

if [[ "$provided" == 0 && ${MACOS_REQUIRE_NOTARIZATION:-false} != true ]]; then
    if [[ ${GITHUB_REF_TYPE:-} == tag ]]; then
        echo '::warning::Developer ID credentials are absent. The macOS DMG is ad-hoc signed and unnotarized.'
    fi
    exec bash "$packager" "$app" "$output" --adhoc
fi
if [[ "$provided" != "${#credentials[@]}" ]]; then
    printf 'Missing macOS signing settings: %s\n' "${missing[*]}" >&2
    exit 1
fi

umask 077
scratch=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/link-selector-signing.XXXXXXXX")
keychain="$scratch/signing.keychain-db"
trap 'security delete-keychain "$keychain" >/dev/null 2>&1 || true; rm -rf -- "$scratch"' EXIT
password=$(openssl rand -hex 32)

printf '%s' "$APPLE_CERTIFICATE" | base64 -D > "$scratch/certificate.p12"
printf '%s' "$APPLE_API_KEY_BASE64" | base64 -D > "$scratch/notary-key.p8"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$scratch/certificate.p12" -k "$keychain" -f pkcs12 -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null

identity=$(security find-identity -v -p codesigning "$keychain" | awk '/"Developer ID Application:/{ print $2 }')
if [[ ! "$identity" =~ ^[[:xdigit:]]{40}$ ]]; then
    echo 'Expected exactly one valid Developer ID Application identity in the certificate.' >&2
    exit 1
fi

profile=link-selector-notary
xcrun notarytool store-credentials "$profile" --keychain "$keychain" \
    --key "$scratch/notary-key.p8" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER"

MACOS_SIGNING_IDENTITY="$identity" MACOS_SIGNING_KEYCHAIN="$keychain" MACOS_NOTARY_PROFILE="$profile" \
    bash "$packager" "$app" "$output" --notarize
