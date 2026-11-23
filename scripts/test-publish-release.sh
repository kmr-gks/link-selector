#!/usr/bin/env bash
set -euo pipefail

publish=$(cd "$(dirname "$0")" && pwd)/publish-release.sh
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
cd "$scratch"
export macos=Link-Selector-macOS-1.2.3-universal.dmg
export windows=Link-Selector-Windows-1.2.3-x64-setup.exe
export arm64=Link-Selector-Windows-1.2.3-arm64-setup.exe
printf 'macOS' > "$macos"
printf 'Windows x64' > "$windows"
printf 'Windows Arm64' > "$arm64"

gh() {
    if [[ $1 == release && $2 == view ]]; then
        local json
        local state=${RELEASE_STATE:-missing}
        if [[ $state == interrupted ]]; then
            if [[ -e edit.log ]]; then state=published
            elif [[ -e create.log ]]; then state=managed
            else state=missing
            fi
        fi
        case "$state" in
            missing) return 1 ;;
            published|incomplete|missing-arm64|draft|managed|starter|empty)
                json=$(jq -n --arg mac "$macos" --arg win "$windows" --arg arm "$arm64" --arg state "$state" '
                    {isDraft: ($state == "draft" or $state == "managed"),
                     body: (if $state == "managed" then "<!-- link-selector:managed-release -->\n\nGenerated notes" else "" end),
                     assets: ([
                        {name: $mac, state: (if $state == "starter" then "starter" else "uploaded" end),
                         size: (if $state == "starter" or $state == "empty" then 0 else 10 end)},
                        {name: "SHA256SUMS.txt", state: "uploaded", size: 10}
                    ] + (if $state == "incomplete" then [] else [{name: $win, state: "uploaded", size: 10}] end)
                       + (if $state == "missing-arm64" then [] else [{name: $arm, state: "uploaded", size: 10}] end))}') || return 2
                jq -r "${!#}" <<< "$json" | awk '{ sub(/\r$/, ""); printf "%s\r\n", $0 }' ;;
            *) return 2 ;;
        esac
    elif [[ $1 == release && $2 == create ]]; then
        printf '%s\n' "$@" >> create.log
    elif [[ $1 == release && $2 == upload ]]; then
        printf '%s\n' "$@" > upload.log
        [[ ${FAIL_UPLOAD:-false} != true ]]
    elif [[ $1 == release && $2 == edit ]]; then
        printf '%s\n' "$@" > edit.log
    else
        return 2
    fi
}
export -f gh

printf '## 変更点\n\n- リンクの選択画面を改善しました。\n' > notes.md
if RELEASE_STATE=missing bash "$publish" v1.2.3 owner/repo missing.md; then
    echo 'Publishing without versioned release notes should fail.' >&2
    exit 1
fi
[[ ! -e create.log && ! -e upload.log ]]
RELEASE_STATE=missing bash "$publish" v1.2.3 owner/repo notes.md
[[ $(sed -n '3p' create.log) == v1.2.3 ]]
sha256sum -c SHA256SUMS.txt
grep -Fxq -- '--draft' create.log
grep -Fxq -- '--generate-notes' create.log
grep -Fxq -- '<!-- link-selector:managed-release -->' create.log
grep -Fxq -- '## 変更点' create.log
grep -Fxq -- '- リンクの選択画面を改善しました。' create.log
grep -Fxq -- '--clobber' upload.log
grep -Fxq -- '--draft=false' edit.log
rm create.log upload.log edit.log SHA256SUMS.txt

# The early check succeeds with no installers in the working directory.
mkdir check-only
(
    cd check-only
    RELEASE_STATE=published bash "$publish" v1.2.3 owner/repo --check
    for state in missing managed; do
        status=0
        RELEASE_STATE=$state bash "$publish" v1.2.3 owner/repo --check || status=$?
        [[ $status == 10 ]]
    done
    for state in incomplete draft; do
        status=0
        RELEASE_STATE=$state bash "$publish" v1.2.3 owner/repo --check || status=$?
        [[ $status == 1 ]]
    done
    [[ ! -e create.log && ! -e upload.log && ! -e SHA256SUMS.txt ]]
)
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
for broken in missing-arm64 starter empty; do
    if RELEASE_STATE=$broken bash "$publish" v1.2.3 owner/repo; then
        echo "A release with a $broken upload was incorrectly accepted." >&2
        exit 1
    fi
done
[[ ! -e create.log && ! -e SHA256SUMS.txt ]]

# An interrupted upload leaves the workflow's draft. The retry fills and publishes it.
if FAIL_UPLOAD=true RELEASE_STATE=interrupted bash "$publish" v1.2.3 owner/repo notes.md; then
    echo 'A failed upload incorrectly published a release.' >&2
    exit 1
fi
[[ -e create.log && ! -e edit.log ]]
create_attempt=$(cat create.log)
RELEASE_STATE=interrupted bash "$publish" v1.2.3 owner/repo
[[ $(cat create.log) == "$create_attempt" && -e edit.log ]]
grep -Fxq -- "$macos" upload.log
grep -Fxq -- "$windows" upload.log
grep -Fxq -- "$arm64" upload.log
grep -Fxq -- SHA256SUMS.txt upload.log
sha256sum -c SHA256SUMS.txt
rm upload.log edit.log create.log SHA256SUMS.txt

RELEASE_STATE=draft bash "$publish" v1.2.3 owner/repo && exit 1
[[ ! -e upload.log && ! -e edit.log ]]
