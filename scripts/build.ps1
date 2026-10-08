# Usage: build.ps1 [-Target <triple>] [-Upx]
#   -Target  optional Rust target triple (e.g. x86_64-pc-windows-msvc)
#   -Upx     also compress the binary with UPX. Off by default: packed
#            executables are often flagged by antivirus tools on Windows.
param(
    [string]$Target = "",
    [switch]$Upx
)

$ErrorActionPreference = "Stop"

Write-Host "Building optimized release binary (opt-level=z, LTO, single codegen unit)..."
if ($Target) {
    cargo build --release --locked --target $Target
    $BinPath = "target\$Target\release\py-doc.exe"
} else {
    cargo build --release --locked
    $BinPath = "target\release\py-doc.exe"
}

if ($LASTEXITCODE -ne 0 -or !(Test-Path $BinPath)) {
    Write-Error "Build failed: binary not found at $BinPath"
    exit 1
}

Write-Host ""
Write-Host "Binary size:"
"{0:N2} MB" -f ((Get-Item $BinPath).Length / 1MB)

if ($Upx) {
    if (Get-Command upx -ErrorAction SilentlyContinue) {
        Write-Host ""
        Write-Host "Compressing with UPX..."
        upx --best --lzma $BinPath
        Write-Host ""
        Write-Host "Binary size after UPX:"
        "{0:N2} MB" -f ((Get-Item $BinPath).Length / 1MB)
    } else {
        Write-Error "-Upx was given but UPX is not on PATH. Install it, e.g.: scoop install upx  (or)  choco install upx"
        exit 1
    }
}

Write-Host ""
Write-Host "Done: $BinPath"
