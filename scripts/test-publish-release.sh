#!/usr/bin/env bash
set -euo pipefail

publish=$(cd "$(dirname "$0")" && pwd)/publish-release.sh
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
cd "$scratch"
export macos=Link-Selector-macOS-1.2.3-universal.dmg
export windows=Link-Selector-Windows-1.2.3-x64-setup.exe
printf 'macOS' > "$macos"
printf 'Windows' > "$windows"

gh() {
    if [[ $1 == release && $2 == view ]]; then
        case "$RELEASE_STATE" in
            missing) return 1 ;;
            published) printf '%s\n' false "$macos" "$windows" SHA256SUMS.txt ;;
            incomplete) printf '%s\n' false "$macos" SHA256SUMS.txt ;;
            draft) printf '%s\n' true "$macos" "$windows" SHA256SUMS.txt ;;
            *) return 2 ;;
        esac
    elif [[ $1 == release && $2 == create ]]; then
        printf '%s\n' "$@" >> create.log
    else
        return 2
    fi
}
export -f gh

RELEASE_STATE=missing bash "$publish" v1.2.3 owner/repo
[[ $(sed -n '3p' create.log) == v1.2.3 ]]
sha256sum -c SHA256SUMS.txt
rm create.log SHA256SUMS.txt

RELEASE_STATE=published bash "$publish" v1.2.3 owner/repo
[[ ! -e create.log && ! -e SHA256SUMS.txt ]]
if RELEASE_STATE=incomplete bash "$publish" v1.2.3 owner/repo; then
    echo 'A release with missing assets was incorrectly accepted.' >&2
    exit 1
fi
if RELEASE_STATE=draft bash "$publish" v1.2.3 owner/repo; then
    echo 'A draft release was incorrectly accepted.' >&2
    exit 1
fi
[[ ! -e create.log && ! -e SHA256SUMS.txt ]]
