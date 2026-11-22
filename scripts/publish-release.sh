#!/usr/bin/env bash
set -euo pipefail

tag=${1:?Provide a version tag}
repository=${2:?Provide the GitHub repository}
version=${tag#v}
files=("Link-Selector-macOS-$version-universal.dmg" "Link-Selector-Windows-$version-x64-setup.exe")

if existing=$(gh release view "$tag" --repo "$repository" --json isDraft,assets \
    --jq '.isDraft, (.assets[].name)'); then
    if [[ ${existing%%$'\n'*} != false ]]; then
        echo "Release $tag is still a draft." >&2
        exit 1
    fi
    for file in "${files[@]}" SHA256SUMS.txt; do
        if ! grep -Fxq -- "$file" <<< "$existing"; then
            echo "Published release $tag is missing $file." >&2
            exit 1
        fi
    done
    echo "Release $tag already contains the expected assets."
    exit 0
fi

sha256sum "${files[@]}" > SHA256SUMS.txt
release_flags=(--verify-tag --generate-notes)
if [[ "$tag" == *-* ]]; then
    release_flags+=(--prerelease)
fi
gh release create "$tag" "${files[@]}" SHA256SUMS.txt \
    --repo "$repository" "${release_flags[@]}"
