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
$ZipPath = Join-Path $env:TEMP $AssetName

Write-Host "Downloading $BinaryName $Tag..."
Invoke-WebRequest -Uri $Url -OutFile $ZipPath

Write-Host "Extracting..."
Expand-Archive -Path $ZipPath -DestinationPath $env:TEMP -Force

Write-Host "Installing to $InstallDir..."
Move-Item -Path "$env:TEMP\${BinaryName}.exe" -Destination "$InstallDir\${BinaryName}.exe" -Force

# Clean up
Remove-Item -Path $ZipPath -Force

Write-Host "Successfully installed $BinaryName!" -ForegroundColor Green