#!/bin/sh
set -eu

# Cross-build the Windows release with MinGW-w64 and stage Texcel.exe with its licenses.
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=x86_64-pc-windows-gnu
output=${1:-"$repo_root/target/package/windows"}
prefix=x86_64-w64-mingw32

for tool in "$prefix-gcc" "$prefix-windres"; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "missing $tool; install mingw-w64 first" >&2
        exit 1
    fi
done
if [ ! -f "$repo_root/assets/icons/app_icon.ico" ]; then
    echo "missing app_icon.ico; run the documented icon converter first" >&2
    exit 1
fi

# Add the Rust standard library for the Windows target when rustup manages the toolchain.
if command -v rustup >/dev/null 2>&1 \
    && ! rustup target list --installed | grep -qx "$target"; then
    rustup target add "$target"
fi

# Link with MinGW and embed the icon with its resource compiler.
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$prefix-gcc" \
RC="$prefix-windres" \
    cargo build --locked --release --target "$target" \
    --manifest-path "$repo_root/Cargo.toml" --bin texcel

mkdir -p "$output"
cp "$repo_root/target/$target/release/texcel.exe" "$output/Texcel.exe"
# Ship the license text of Texcel and of every crate built into the Windows release.
cargo run --locked --quiet --manifest-path "$repo_root/Cargo.toml" \
    --bin texcel-licenses -- "$output/licenses" --target "$target"
echo "created $output"
