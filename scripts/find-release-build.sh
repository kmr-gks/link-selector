#!/usr/bin/env bash
set -euo pipefail

repository=${1:?Provide the repository}
commit=${2:?Provide the commit SHA}
runs=$(gh api --method GET "repos/$repository/actions/workflows/build.yml/runs" \
    -f branch=main -f event=push -f head_sha="$commit" -f status=success -F per_page=20)
candidates=$(jq -r --arg commit "$commit" --arg repo "$repository" '
    .workflow_runs[]
    | select(.head_sha == $commit and .head_branch == "main" and .event == "push"
        and .status == "completed" and .conclusion == "success" and .head_repository.full_name == $repo)
    | .id
' <<< "$runs")
while IFS= read -r run_id; do
    [[ -n "$run_id" ]] || continue
    artifacts=$(gh api "repos/$repository/actions/runs/$run_id/artifacts?per_page=100")
    if jq -e '
        [.artifacts[] | select(.expired == false and .size_in_bytes > 0) | .name] as $available
        | (["installer-macos", "installer-windows-x64", "installer-windows-arm64", "macos-app"] - $available | length) == 0
    ' <<< "$artifacts" >/dev/null; then
        printf '%s\n' "$run_id"
        exit 0
    fi
done <<< "$candidates"
