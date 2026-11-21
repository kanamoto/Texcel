param(
    [string]$Output = "target/package/windows"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$Manifest = Join-Path $RepoRoot "Cargo.toml"

# Build for the explicit host target, so release output sits in target/<triple>/release
# like the other packaging scripts. The build script embeds the icon resource.
$HostLine = rustc -vV | Where-Object { $_ -like "host: *" } | Select-Object -First 1
if (-not $HostLine) {
    throw "rustc did not report a host target"
}
$Target = $HostLine.Substring("host: ".Length).Trim()
cargo build --locked --release --target $Target --manifest-path $Manifest --bin texcel
if ($LASTEXITCODE -ne 0) {
    throw "Release build failed"
}
$ReleaseExe = Join-Path $RepoRoot "target/$Target/release/texcel.exe"

# Stage the executable.
New-Item -ItemType Directory -Force -Path $Output | Out-Null
Copy-Item -LiteralPath $ReleaseExe -Destination (Join-Path $Output "Texcel.exe") -Force

# Ship the license text of Texcel and of every crate built into it.
cargo run --locked --quiet --manifest-path $Manifest --bin texcel-licenses -- (Join-Path $Output "licenses") --target $Target
if ($LASTEXITCODE -ne 0) {
    throw "License bundle failed"
}
Write-Output "Created $Output"
