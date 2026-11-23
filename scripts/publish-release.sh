#!/usr/bin/env bash
set -euo pipefail

tag=${1:?Provide a version tag}
repository=${2:?Provide the GitHub repository}
version=${tag#v}
files=(
    "Link-Selector-macOS-$version-universal.dmg"
    "Link-Selector-Windows-$version-x64-setup.exe"
    "Link-Selector-Windows-$version-arm64-setup.exe"
)

create_draft=false
marker='<!-- link-selector:managed-release -->'

if existing=$(gh release view "$tag" --repo "$repository" --json isDraft,body \
    --jq '.isDraft, (.body // "")'); then
    existing=${existing//$'\r'/}
    if [[ ${existing%%$'\n'*} == false ]]; then
        # Query names alone so lines in release notes cannot satisfy the asset check.
        uploaded=$(gh release view "$tag" --repo "$repository" --json assets \
            --jq '.assets[] | select(.state == "uploaded" and .size > 0) | .name')
        uploaded=${uploaded//$'\r'/}
        for file in "${files[@]}" SHA256SUMS.txt; do
            if ! grep -Fxq -- "$file" <<< "$uploaded"; then
                echo "Published release $tag is missing a complete upload of $file." >&2
                exit 1
            fi
        done
        echo "Release $tag already contains the expected assets."
        exit 0
    fi
    notes=${existing#*$'\n'}
    if [[ ${notes%%$'\n'*} != "$marker" ]]; then
        echo "Release $tag is a draft outside this workflow." >&2
        exit 1
    fi
else
    create_draft=true
fi

# The prepare job uses the same asset check before downloading or building anything.
if [[ ${3:-} == --check ]]; then
    exit 10
fi

sha256sum "${files[@]}" > SHA256SUMS.txt
if [[ "$create_draft" == true ]]; then
    notes_file=${3:?Provide the Japanese release notes file}
    if [[ ! -s "$notes_file" ]] || ! grep -q '[^[:space:]]' "$notes_file"; then
        echo "Release notes are missing or empty: $notes_file" >&2
        exit 1
    fi
    release_flags=(--verify-tag --generate-notes --draft --notes "$marker"$'\n\n'"$(cat "$notes_file")")
    if [[ "$tag" == *-* ]]; then
        release_flags+=(--prerelease)
    fi
    gh release create "$tag" --repo "$repository" "${release_flags[@]}"
fi
gh release upload "$tag" "${files[@]}" SHA256SUMS.txt --clobber --repo "$repository"
gh release edit "$tag" --draft=false --repo "$repository"
