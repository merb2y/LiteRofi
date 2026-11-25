[CmdletBinding()]
param(
    [string]$Version = "0.1.0",
    [string]$AssetSourceDir = (Join-Path $env:APPDATA "LiteRofi"),
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot
try {
    if (-not $SkipBuild) {
        Write-Host "Building release binaries (literofi + literofi-studio)..."
        & cargo build --release --bins
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release --bins failed with exit code $LASTEXITCODE"
        }
    }

    $ReleaseExe = Join-Path $RepoRoot "target\release\literofi.exe"
    $StudioExe = Join-Path $RepoRoot "target\release\literofi-studio.exe"
    if (-not (Test-Path -LiteralPath $ReleaseExe)) {
        throw "Release binary missing at $ReleaseExe"
    }
    if (-not (Test-Path -LiteralPath $StudioExe)) {
        throw "Studio binary missing at $StudioExe"
    }

    $DistDir = Join-Path $RepoRoot "dist"
    $PortableName = "LiteRofi-$Version-win64-portable"
    $PortableDir = Join-Path $DistDir $PortableName
    $PortableZip = Join-Path $DistDir "$PortableName.zip"
    $SetupExe = Join-Path $DistDir "LiteRofi-$Version-win64-Setup.exe"
    $InnoStage = Join-Path $DistDir "_inno_stage"
    $AssetsStage = Join-Path $InnoStage "assets"

    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
    foreach ($p in @($PortableDir, $InnoStage)) {
        if (Test-Path -LiteralPath $p) {
            Remove-Item -LiteralPath $p -Recurse -Force
        }
    }
    foreach ($f in @($PortableZip, $SetupExe)) {
        if (Test-Path -LiteralPath $f) {
            Remove-Item -LiteralPath $f -Force
        }
    }

    # Assemble complete asset bundle (themes, colors, images, launchers, wallust)
    New-Item -ItemType Directory -Path $AssetsStage -Force | Out-Null

    foreach ($sub in @("themes", "colors", "images", "launchers", "wallust")) {
        $srcSub = Join-Path $AssetSourceDir $sub
        if (Test-Path -LiteralPath $srcSub) {
            $dstSub = Join-Path $AssetsStage $sub
            Copy-Item -LiteralPath $srcSub -Destination $dstSub -Recurse -Force
        }
    }

    $BundleThemes = Join-Path $AssetsStage "themes"
    New-Item -ItemType Directory -Path $BundleThemes -Force | Out-Null

    # Merge repo bundled themes & official MIT Rofi themes if missing
    foreach ($dir in @((Join-Path $RepoRoot "themes"), (Join-Path $RepoRoot "reference\rofi\themes"))) {
        if (Test-Path -LiteralPath $dir) {
            Get-ChildItem -LiteralPath $dir -File | Where-Object { $_.Extension -in @(".rasi", ".rasinc") } | ForEach-Object {
                $dest = Join-Path $BundleThemes $_.Name
                if (-not (Test-Path -LiteralPath $dest)) {
                    Copy-Item -LiteralPath $_.FullName -Destination $dest
                }
            }
        }
    }

    $themeCount = (Get-ChildItem -LiteralPath $BundleThemes -Filter "*.rasi" -File -ErrorAction SilentlyContinue).Count
    $colorCount = (Get-ChildItem -LiteralPath (Join-Path $AssetsStage "colors") -Filter "*.rasi" -File -ErrorAction SilentlyContinue).Count
    Write-Host "Bundling $themeCount themes and $colorCount color palettes..."

    # 1. Stage portable bundle
    New-Item -ItemType Directory -Path $PortableDir -Force | Out-Null
    Copy-Item -LiteralPath $ReleaseExe -Destination (Join-Path $PortableDir "literofi.exe")
    Copy-Item -LiteralPath $StudioExe -Destination (Join-Path $PortableDir "literofi-studio.exe")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "scripts\install.ps1") -Destination (Join-Path $PortableDir "install.ps1")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "scripts\uninstall.ps1") -Destination (Join-Path $PortableDir "uninstall.ps1")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "README.md") -Destination (Join-Path $PortableDir "README.md")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "LICENSE") -Destination (Join-Path $PortableDir "LICENSE")
    foreach ($sub in @("themes", "colors", "images", "launchers", "wallust")) {
        $srcSub = Join-Path $AssetsStage $sub
        if (Test-Path -LiteralPath $srcSub) {
            Copy-Item -LiteralPath $srcSub -Destination (Join-Path $PortableDir $sub) -Recurse -Force
        }
    }

    Write-Host "Creating portable archive $PortableZip ..."
    Compress-Archive -Path (Join-Path $PortableDir "*") -DestinationPath $PortableZip -Force

    # 2. Build native Windows GUI Setup.exe via Inno Setup 6
    $Iscc = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1

    if (-not $Iscc) {
        throw "Inno Setup 6 (ISCC.exe) not found. Install Inno Setup 6 to build the GUI Setup.exe."
    }

    Copy-Item -LiteralPath $ReleaseExe -Destination (Join-Path $InnoStage "literofi.exe")
    Copy-Item -LiteralPath $StudioExe -Destination (Join-Path $InnoStage "literofi-studio.exe")
    $StudioIco = Join-Path $RepoRoot "studio\studio.ico"
    if (Test-Path -LiteralPath $StudioIco) {
        Copy-Item -LiteralPath $StudioIco -Destination (Join-Path $InnoStage "studio.ico")
        Copy-Item -LiteralPath $StudioIco -Destination (Join-Path $PortableDir "studio.ico")
    }
    Copy-Item -LiteralPath (Join-Path $RepoRoot "README.md") -Destination (Join-Path $InnoStage "README.md")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "LICENSE") -Destination (Join-Path $InnoStage "LICENSE")

    $IssPath = Join-Path $RepoRoot "scripts\literofi.iss"
    Write-Host "Building GUI Windows installer via Inno Setup ($Iscc)..."
    & $Iscc "/DAppVersion=$Version" "/DStageDir=$InnoStage" "/DOutputDir=$DistDir" $IssPath
    if ($LASTEXITCODE -ne 0) {
        throw "ISCC.exe exited with code $LASTEXITCODE"
    }
    if (-not (Test-Path -LiteralPath $SetupExe)) {
        throw "ISCC.exe finished without creating $SetupExe"
    }

    Remove-Item -LiteralPath $InnoStage -Recurse -Force

    Write-Host ""
    Write-Host "Packaging complete:"
    Get-ChildItem -LiteralPath $DistDir -File | Select-Object Name, Length | Format-Table -AutoSize
} finally {
    Pop-Location
}
