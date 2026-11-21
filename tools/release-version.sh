#!/bin/sh
set -eu

# Check that a release tag is a semantic version that matches the app's version,
# and print the version without its optional leading "v" as `version=<x.y.z>`.
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tag=${1:?usage: release-version.sh <tag>}

# Semantic Versioning 2.0.0: MAJOR.MINOR.PATCH with optional pre-release and build parts.
identifier='[0-9A-Za-z-]+'
semver="(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-$identifier(\.$identifier)*)?(\+$identifier(\.$identifier)*)?"
if ! printf '%s\n' "$tag" | grep -Eq "^v?$semver\$"; then
    echo "tag is not a semantic version: $tag" >&2
    exit 1
fi
version=${tag#v}

# The package version must match, so the zip name and the app report the same version.
cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$repo_root/Cargo.toml" | head -n 1)
if [ "$version" != "$cargo_version" ]; then
    echo "tag $tag does not match Cargo.toml version $cargo_version" >&2
    exit 1
fi
plist_version=$(sed -n '/CFBundleShortVersionString/{n;s/.*<string>\(.*\)<\/string>.*/\1/p;}' \
    "$repo_root/packaging/macos/Info.plist")
if [ "$version" != "$plist_version" ]; then
    echo "tag $tag does not match Info.plist version $plist_version" >&2
    exit 1
fi
echo "version=$version"
