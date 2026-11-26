#!/usr/bin/env bash
set -euo pipefail

scripts="$(cd "$(dirname "$0")" && pwd)"
find_build="$scripts/find-release-build.sh"
gh() {
    local state destination name
    if [[ $1 == run && $2 == download ]]; then
        [[ $3 == 90 ]] || return 1
        destination=${!#}
        while [[ $# -gt 1 ]]; do
            if [[ $1 == --name ]]; then
                name=$2
                case "$name" in
                    installer-macos) file=Link-Selector-macOS-1.2.3-universal.dmg ;;
                    installer-windows-x64) file=Link-Selector-Windows-1.2.3-x64-setup.exe ;;
                    installer-windows-arm64)
                        case ${DOWNLOAD_STATE:-} in
                            expired) return 1 ;;
                            partial) shift 2; continue ;;
                        esac
                        file=Link-Selector-Windows-1.2.3-arm64-setup.exe ;;
                    macos-app)
                        [[ ${DOWNLOAD_STATE:-} != app-expired ]] || return 1
                        mkdir -p "$destination"
                        printf 'app' > "$destination/macos-app.zip"
                        return ;;
                    *) return 2 ;;
                esac
                mkdir -p "$destination/$name"
                printf 'installer' > "$destination/$name/$file"
                shift
            fi
            shift
        done
        return 0
    fi
    case "${4:-}" in
        repos/owner/repo/actions/workflows/build.yml/runs)
            jq -n '
                def candidate($fields):
                  {head_sha:"123",head_branch:"main",event:"push",status:"completed",
                   conclusion:"success",head_repository:{full_name:"owner/repo"}} + $fields;
                {workflow_runs:[
                    candidate({id:99,head_sha:"other"}),
                    candidate({id:98,event:"pull_request"}),
                    candidate({id:97,head_branch:"feature"}),
                    candidate({id:96,conclusion:"failure"}),
                    candidate({id:95,head_repository:{full_name:"fork/repo"}}),
                    candidate({id:94,status:"in_progress"}),
                    candidate({id:93}), candidate({id:92}), candidate({id:91}), candidate({id:90})
                ]}'
            return
            ;;
    esac
    if [[ ${GH_DEBUG:-} == api ]]; then printf 'debug: requesting build artifacts\n' >&2; fi
    case "$2" in
        */93/artifacts*)
            case ${FIRST_BUILD_STATE:-expired} in
                deleted) printf 'gh: Not Found (HTTP 404)\n' >&2; return 1 ;;
                unavailable) printf 'gh: Service Unavailable (HTTP 503)\n' >&2; return 1 ;;
                *) state=expired ;;
            esac
            ;;
        */92/artifacts*) state=empty ;;
        */91/artifacts*) state=missing ;;
        */90/artifacts*) state=${FINAL_BUILD_STATE:-complete} ;;
        *) echo "Unexpected artifact lookup: $2" >&2; return 2 ;;
    esac
    jq -n --arg state "$state" '
        {artifacts: (["installer-macos","installer-windows-x64","macos-app"]
            + (if $state == "missing" then [] else ["installer-windows-arm64"] end)
            | map({name:.,expired:($state == "expired"),size_in_bytes:(if $state == "empty" then 0 else 10 end)}))}
    '
}
export -f gh

selected=$(bash "$find_build" owner/repo 123)
[[ "$selected" == 90 ]]
selected=$(FINAL_BUILD_STATE=missing bash "$find_build" owner/repo 123)
[[ -z "$selected" ]]


scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

# The API still reports all artifacts, but expiry during transfer must select a tag build.
for state in expired partial app-expired; do
    selected=$(DOWNLOAD_STATE=$state bash "$scripts/stage-release-build.sh" owner/repo 123 1.2.3 true "$scratch/$state")
    [[ -z "$selected" && ! -e "$scratch/$state" ]]
done
selected=$(bash "$scripts/stage-release-build.sh" owner/repo 123 1.2.3 false "$scratch/plain")
[[ $selected == 90 && -s "$scratch/plain/installers/Link-Selector-macOS-1.2.3-universal.dmg" ]]
[[ ! -e "$scratch/plain/macos-app" ]]
selected=$(bash "$scripts/stage-release-build.sh" owner/repo 123 1.2.3 true "$scratch/signed")
[[ $selected == 90 && -s "$scratch/signed/macos-app/macos-app.zip" ]]
[[ -s "$scratch/signed/installers/Link-Selector-Windows-1.2.3-arm64-setup.exe" ]]
[[ ! -e "$scratch/signed/installers/Link-Selector-macOS-1.2.3-universal.dmg" ]]


selected=$(GH_DEBUG=api FIRST_BUILD_STATE=deleted bash "$find_build" owner/repo 123)
[[ "$selected" == 90 ]]
selected=$(FIRST_BUILD_STATE=deleted FINAL_BUILD_STATE=missing bash "$scripts/stage-release-build.sh" owner/repo 123 1.2.3 false "$scratch/deleted")
[[ -z "$selected" && ! -e "$scratch/deleted" ]]
if FIRST_BUILD_STATE=unavailable bash "$find_build" owner/repo 123; then
    echo 'A metadata service outage was incorrectly accepted.' >&2
    exit 1
fi
