#!/usr/bin/env bash
set -euo pipefail

repository=${1:?Provide the repository}
commit=${2:?Provide the commit SHA}
version=${3:?Provide the installer version}
notarize=${4:?Provide the notarization mode}
destination=${5:?Provide the staging directory}

run=$(bash "$(dirname "$0")/find-release-build.sh" "$repository" "$commit")
[[ -n "$run" ]] || exit 0

scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

assets=(
    "installer-windows-x64/Link-Selector-Windows-$version-x64-setup.exe"
    "installer-windows-arm64/Link-Selector-Windows-$version-arm64-setup.exe"
)
if [[ "$notarize" != true ]]; then
    assets+=("installer-macos/Link-Selector-macOS-$version-universal.dmg")
fi
download_names=()
for asset in "${assets[@]}"; do
    download_names+=(--name "${asset%%/*}")
done

# Download before selecting reuse. The source artifacts may vanish after discovery.
if ! gh run download "$run" --repo "$repository" "${download_names[@]}" --dir "$scratch/download" >&2; then
    exit 0
fi
mkdir "$scratch/installers"
for asset in "${assets[@]}"; do
    if [[ ! -s "$scratch/download/$asset" ]]; then
        echo "Missing or empty artifact file: $asset" >&2
        exit 0
    fi
    mv "$scratch/download/$asset" "$scratch/installers/"
done

if [[ "$notarize" == true ]]; then
    if ! gh run download "$run" --repo "$repository" --name macos-app --dir "$scratch/macos-app" >&2 ||
        [[ ! -s "$scratch/macos-app/macos-app.zip" ]]; then
        echo 'The macOS app for notarization is unavailable.' >&2
        exit 0
    fi
fi

mkdir -p "$destination"
mv "$scratch/installers" "$destination/"
if [[ "$notarize" == true ]]; then
    mv "$scratch/macos-app" "$destination/"
fi
printf '%s\n' "$run"
