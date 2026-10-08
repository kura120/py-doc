# Optional: a Rust target triple (e.g. x86_64-pc-windows-msvc).
param([string]$Target = "")

$ErrorActionPreference = "Stop"

Write-Host "Building optimized release binary (opt-level=z, LTO, single codegen unit)..."
if ($Target) {
    cargo build --release --target $Target
    $BinPath = "target\$Target\release\py-doc.exe"
} else {
    cargo build --release
    $BinPath = "target\release\py-doc.exe"
}

if (!(Test-Path $BinPath)) {
    Write-Error "Build failed: binary not found at $BinPath"
    exit 1
}

Write-Host ""
Write-Host "Binary size before UPX:"
"{0:N2} MB" -f ((Get-Item $BinPath).Length / 1MB)

$upx = Get-Command upx -ErrorAction SilentlyContinue
if ($upx) {
    Write-Host ""
    Write-Host "Compressing with UPX..."
    upx --best --lzma $BinPath
    Write-Host ""
    Write-Host "Binary size after UPX:"
    "{0:N2} MB" -f ((Get-Item $BinPath).Length / 1MB)
} else {
    Write-Host ""
    Write-Host "UPX not found on PATH - skipping compression."
    Write-Host "Install it to shrink further, e.g.: scoop install upx  (or)  choco install upx"
}

Write-Host ""
Write-Host "Done: $BinPath"