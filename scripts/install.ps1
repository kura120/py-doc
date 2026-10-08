$ErrorActionPreference = 'Stop'

# Configuration
$Repo = "kura120/py-doc"
$BinaryName = "py-doc"
$InstallDir = "$env:USERPROFILE\.cargo\bin" # Use cargo bin path since it's commonly in user PATH

# Ensure target directory exists
if (!(Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

# Fetch the latest release tag
$ReleaseApi = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $ReleaseApi.tag_name

if (!$Tag) {
    Write-Error "Failed to fetch latest release version from GitHub."
}

$AssetName = "${BinaryName}-x86_64-pc-windows-msvc.zip"
$Url = "https://github.com/$Repo/releases/download/$Tag/$AssetName"

# Work in a private temporary directory, removed on any exit.
$WorkDir = Join-Path $env:TEMP ("py-doc-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $WorkDir | Out-Null
$ZipPath = Join-Path $WorkDir $AssetName

try {
    Write-Host "Downloading $BinaryName $Tag..."
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing

    # Verify the download against the checksum published with the release.
    $ChecksumPath = "$ZipPath.sha256"
    $HasChecksum = $true
    try {
        Invoke-WebRequest -Uri "$Url.sha256" -OutFile $ChecksumPath -UseBasicParsing
    } catch {
        $HasChecksum = $false
    }

    if ($HasChecksum) {
        $Expected = ((Get-Content $ChecksumPath -Raw).Trim() -split '\s+')[0]
        $Actual = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash
        if ($Expected -ne $Actual) {
            Write-Error "Checksum mismatch for ${AssetName}: expected $Expected, got $Actual."
        }
        Write-Host "Checksum verified."
    } else {
        Write-Warning "Release $Tag publishes no checksum for $AssetName; skipping verification."
    }

    Write-Host "Extracting..."
    Expand-Archive -Path $ZipPath -DestinationPath $WorkDir -Force

    Write-Host "Installing to $InstallDir..."
    Move-Item -Path (Join-Path $WorkDir "${BinaryName}.exe") -Destination "$InstallDir\${BinaryName}.exe" -Force
} finally {
    Remove-Item -Path $WorkDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Successfully installed $BinaryName $Tag!" -ForegroundColor Green
