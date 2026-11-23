#!/usr/bin/env bash
set -euo pipefail

find_build="$(cd "$(dirname "$0")" && pwd)/find-release-build.sh"
gh() {
    local state
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
    case "$2" in
        */93/artifacts*) state=expired ;;
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
