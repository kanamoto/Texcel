#!/bin/sh
set -eu

# Build the release binary, bundle it with its ICNS icon as Texcel.app, and put the
# licenses folder beside the app, where users can open it as on the other platforms.
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=${1:-"$repo_root/target/package/mac"}
app="$output/Texcel.app"
contents="$app/Contents"

if [ ! -f "$repo_root/assets/icons/app_icon.icns" ]; then
    echo "missing app_icon.icns; run the documented icon converter first" >&2
    exit 1
fi

# Build for the explicit host target, so release output sits in target/<triple>/release
# like the Windows cross-build.
target=$(rustc -vV | sed -n 's/^host: //p')
cargo build --locked --release --target "$target" \
    --manifest-path "$repo_root/Cargo.toml" --bin texcel
binary="$repo_root/target/$target/release/texcel"

# Start from an empty bundle so files from an earlier layout never stay inside it.
rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp "$binary" "$contents/MacOS/Texcel"
cp "$repo_root/assets/icons/app_icon.icns" "$contents/Resources/app_icon.icns"
cp "$repo_root/packaging/macos/Info.plist" "$contents/Info.plist"
# Ship the license text of Texcel and of every crate built into it.
cargo run --locked --quiet --manifest-path "$repo_root/Cargo.toml" \
    --bin texcel-licenses -- "$output/licenses" --target "$target"
chmod 755 "$contents/MacOS/Texcel"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"
echo "created $app"
