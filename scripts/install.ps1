[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "LiteRofi"),
    [string]$ConfigDir = (Join-Path $env:APPDATA "LiteRofi"),
    [switch]$Autostart,
    [switch]$NoAutostart,
    [switch]$NoStartDaemon,
    [switch]$NoPath,
    [switch]$NoShortcuts,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"

function Write-Info([string]$Message) {
    if (-not $Quiet) {
        Write-Host $Message
    }
}

function Copy-TreeMissing([string]$SourceRoot, [string]$DestRoot) {
    if (-not (Test-Path -LiteralPath $SourceRoot)) { return 0 }
    $copied = 0
    $srcFull = (Resolve-Path -LiteralPath $SourceRoot).Path.TrimEnd('\')
    Get-ChildItem -LiteralPath $srcFull -Recurse -File | ForEach-Object {
        $rel = $_.FullName.Substring($srcFull.Length).TrimStart('\')
        $dst = Join-Path $DestRoot $rel
        $dstParent = Split-Path -Parent $dst
        if (-not (Test-Path -LiteralPath $dstParent)) {
            New-Item -ItemType Directory -Path $dstParent -Force | Out-Null
        }
        if (-not (Test-Path -LiteralPath $dst)) {
            Copy-Item -LiteralPath $_.FullName -Destination $dst
            $copied++
        }
    }
    return $copied
}

$PayloadRoot = $PSScriptRoot
$ExeSource = Join-Path $PayloadRoot "literofi.exe"
if (-not (Test-Path -LiteralPath $ExeSource)) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
    $Candidate = Join-Path $RepoRoot "target\release\literofi.exe"
    if (Test-Path -LiteralPath $Candidate) {
        $ExeSource = $Candidate
    } else {
        throw "literofi.exe not found next to install.ps1 or in target\release."
    }
}

$StudioSource = Join-Path $PayloadRoot "literofi-studio.exe"
if (-not (Test-Path -LiteralPath $StudioSource)) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
    $CandidateStudio = Join-Path $RepoRoot "target\release\literofi-studio.exe"
    if (Test-Path -LiteralPath $CandidateStudio) {
        $StudioSource = $CandidateStudio
    } else {
        $StudioSource = ""
    }
}

Write-Info "Installing LiteRofi to $InstallDir ..."
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
$ThemesDest = Join-Path $ConfigDir "themes"
New-Item -ItemType Directory -Path $ThemesDest -Force | Out-Null

$ExistingExe = Join-Path $InstallDir "literofi.exe"
if (Test-Path -LiteralPath $ExistingExe) {
    try {
        & $ExistingExe --stop-daemon | Out-Null
        Start-Sleep -Milliseconds 250
    } catch {
        # Best effort before replacing running binary
    }
}
$NormInstallDir = $InstallDir.TrimEnd('\')
foreach ($procName in @("literofi", "literofi-studio")) {
    Get-Process -Name $procName -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            if ($_.Path -and $_.Path.StartsWith($NormInstallDir, [System.StringComparison]::OrdinalIgnoreCase)) {
                Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
                Start-Sleep -Milliseconds 150
            }
        } catch {
            # Ignore inaccessible process paths
        }
    }
}

# Copy main binaries and companion files
Copy-Item -LiteralPath $ExeSource -Destination (Join-Path $InstallDir "literofi.exe") -Force
Copy-Item -LiteralPath $ExeSource -Destination (Join-Path $InstallDir "rofi.exe") -Force
if (-not [string]::IsNullOrWhiteSpace($StudioSource) -and (Test-Path -LiteralPath $StudioSource)) {
    Copy-Item -LiteralPath $StudioSource -Destination (Join-Path $InstallDir "literofi-studio.exe") -Force
}
$StudioIcoSrc = Join-Path $PayloadRoot "studio.ico"
if (-not (Test-Path -LiteralPath $StudioIcoSrc)) {
    $StudioIcoSrc = Join-Path (Split-Path -Parent $PSScriptRoot) "studio\studio.ico"
}
if (Test-Path -LiteralPath $StudioIcoSrc) {
    Copy-Item -LiteralPath $StudioIcoSrc -Destination (Join-Path $InstallDir "studio.ico") -Force
}
$IconsSrc = Join-Path $PayloadRoot "icons"
if (-not (Test-Path -LiteralPath $IconsSrc)) {
    $IconsSrc = Join-Path (Split-Path -Parent $PSScriptRoot) "studio\icons"
}
$IconsDest = Join-Path $InstallDir "icons"
if (Test-Path -LiteralPath $IconsSrc) {
    New-Item -ItemType Directory -Path $IconsDest -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $IconsSrc "*.ico") -Destination $IconsDest -Force
}
foreach ($extra in @("uninstall.ps1", "README.md", "LICENSE")) {
    $src = Join-Path $PayloadRoot $extra
    if (-not (Test-Path -LiteralPath $src)) {
        $repoCandidate = Join-Path (Split-Path -Parent $PSScriptRoot) $extra
        if (Test-Path -LiteralPath $repoCandidate) {
            $src = $repoCandidate
        }
    }
    if (Test-Path -LiteralPath $src) {
        Copy-Item -LiteralPath $src -Destination (Join-Path $InstallDir $extra) -Force
    }
}

# Install bundled themes, colors, images, launchers, and wallust without overwriting user files
$InstalledAssetCount = 0
$BundleZip = Join-Path $PayloadRoot "bundle.zip"
$ThemesZip = Join-Path $PayloadRoot "themes.zip"
if (Test-Path -LiteralPath $BundleZip) {
    $TmpBundle = Join-Path ([System.IO.Path]::GetTempPath()) ("literofi_bundle_" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $TmpBundle -Force | Out-Null
    try {
        Expand-Archive -LiteralPath $BundleZip -DestinationPath $TmpBundle -Force
        $InstalledAssetCount += Copy-TreeMissing -SourceRoot $TmpBundle -DestRoot $ConfigDir
    } finally {
        Remove-Item -LiteralPath $TmpBundle -Recurse -Force -ErrorAction SilentlyContinue
    }
} elseif (Test-Path -LiteralPath $ThemesZip) {
    $TmpThemes = Join-Path ([System.IO.Path]::GetTempPath()) ("literofi_themes_" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $TmpThemes -Force | Out-Null
    try {
        Expand-Archive -LiteralPath $ThemesZip -DestinationPath $TmpThemes -Force
        $InstalledAssetCount += Copy-TreeMissing -SourceRoot $TmpThemes -DestRoot $ThemesDest
    } finally {
        Remove-Item -LiteralPath $TmpThemes -Recurse -Force -ErrorAction SilentlyContinue
    }
} else {
    foreach ($sub in @("themes", "colors", "images", "launchers", "wallust")) {
        $localSub = Join-Path $PayloadRoot $sub
        if (Test-Path -LiteralPath $localSub) {
            $InstalledAssetCount += Copy-TreeMissing -SourceRoot $localSub -DestRoot (Join-Path $ConfigDir $sub)
        }
    }
    foreach ($dir in @((Join-Path (Split-Path -Parent $PSScriptRoot) "themes"), (Join-Path (Split-Path -Parent $PSScriptRoot) "reference\rofi\themes"))) {
        if (Test-Path -LiteralPath $dir) {
            $InstalledAssetCount += Copy-TreeMissing -SourceRoot $dir -DestRoot $ThemesDest
        }
    }
}

# Create starter config.rasi if missing
$ConfigFile = Join-Path $ConfigDir "config.rasi"
if (-not (Test-Path -LiteralPath $ConfigFile)) {
    $StarterConfig = @"
configuration {
    modes: "drun,run,window,combi,filebrowser";
    show-icons: true;
    font: "Segoe UI 11";
    drun-display-format: "{name}";

    /* Global background daemon hotkey & default mode (`literofi --daemon`) */
    daemon-hotkey: "Alt+Space";
    daemon-mode: "drun";
}

/* Drop any GitHub Rofi .rasi theme into %APPDATA%\LiteRofi\themes\ and select it below */
@theme "mango-minimal"
"@
    [System.IO.File]::WriteAllText($ConfigFile, $StarterConfig, [System.Text.UTF8Encoding]::new($false))
}

# Add InstallDir to User PATH
if (-not $NoPath) {
    $CurrentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $Segments = @()
    if (-not [string]::IsNullOrWhiteSpace($CurrentUserPath)) {
        $Segments = $CurrentUserPath.Split(';') | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    }
    $NormalizedTarget = $InstallDir.TrimEnd('\')
    $AlreadyPresent = $false
    foreach ($seg in $Segments) {
        if ($seg.TrimEnd('\').Equals($NormalizedTarget, [System.StringComparison]::OrdinalIgnoreCase)) {
            $AlreadyPresent = $true
            break
        }
    }
    if (-not $AlreadyPresent) {
        $NewPath = ($Segments + $InstallDir) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        Write-Info "Added $InstallDir to User PATH."
    }
}

# Create Start Menu, Desktop, and Startup shortcuts
if (-not $NoShortcuts) {
    $WshShell = New-Object -ComObject WScript.Shell
    $ProgramsDir = [Environment]::GetFolderPath("Programs")
    $DesktopDir = [Environment]::GetFolderPath("Desktop")
    $MenuDir = Join-Path $ProgramsDir "LiteRofi"
    New-Item -ItemType Directory -Path $MenuDir -Force | Out-Null

    $ExePath = Join-Path $InstallDir "literofi.exe"
    $StudioPath = Join-Path $InstallDir "literofi-studio.exe"
    $StudioIcoPath = Join-Path $InstallDir "studio.ico"
    $StudioIconLoc = if (Test-Path -LiteralPath $StudioIcoPath) { "$StudioIcoPath,0" } else { "$ExePath,0" }
    function Resolve-IconLoc([string]$IcoName) {
        $p = Join-Path $InstallDir "icons\$IcoName"
        if (Test-Path -LiteralPath $p) { return "$p,0" }
        return "$ExePath,0"
    }
    $UninstallPath = Join-Path $InstallDir "uninstall.ps1"

    $Shortcuts = @(
        @{ Name = "LiteRofi.lnk"; Target = $ExePath; Args = "-show drun"; Desc = "Launch LiteRofi application launcher"; Style = 7; Icon = (Resolve-IconLoc "literofi.ico") },
        @{ Name = "LiteRofi System Menu.lnk"; Target = $ExePath; Args = "-show menu"; Desc = "Open LiteRofi unified system menu (Ctrl+Alt+Space)"; Style = 7; Icon = (Resolve-IconLoc "literofi.ico") },
        @{ Name = "LiteRofi Launchpad.lnk"; Target = $ExePath; Args = "-show drun -theme launchpad"; Desc = "Launch LiteRofi 4x3 Launchpad grid"; Style = 7; Icon = (Resolve-IconLoc "literofi.ico") },
        @{ Name = "LiteRofi Daemon.lnk"; Target = $ExePath; Args = "--start-daemon"; Desc = "Start LiteRofi background hotkey daemon (Alt+Space)"; Style = 7; Icon = (Resolve-IconLoc "literofi.ico") },
        @{ Name = "LiteRofi Theme Selector.lnk"; Target = $ExePath; Args = "-show theme"; Desc = "Choose and switch active LiteRofi .rasi theme"; Style = 7; Icon = (Resolve-IconLoc "themes.ico") },
        @{ Name = "LiteRofi Color Palette.lnk"; Target = $ExePath; Args = "-show colors"; Desc = "Choose and preview LiteRofi color palettes"; Style = 7; Icon = (Resolve-IconLoc "palette.ico") },
        @{ Name = "LiteRofi Wallpaper.lnk"; Target = $ExePath; Args = "-show wallpaper"; Desc = "Open LiteRofi slanted wallpaper carousel (Alt+W)"; Style = 7; Icon = (Resolve-IconLoc "themes.ico") },
        @{ Name = "LiteRofi Music.lnk"; Target = $ExePath; Args = "-show music -theme music-player"; Desc = "Control Spotify, YouTube Music, and system media playback"; Style = 7; Icon = (Resolve-IconLoc "music.ico") },
        @{ Name = "LiteRofi Power Menu.lnk"; Target = $ExePath; Args = "-show power -theme powermenu"; Desc = "Lock, Suspend, Logout, Reboot, or Shutdown Windows"; Style = 7; Icon = (Resolve-IconLoc "power.ico") },
        @{ Name = "LiteRofi Games.lnk"; Target = $ExePath; Args = "-show games -theme game-launcher"; Desc = "Browse and launch installed Steam & Epic games with cover art"; Style = 7; Icon = (Resolve-IconLoc "games.ico") },
        @{ Name = "LiteRofi Settings.lnk"; Target = $ExePath; Args = "--settings"; Desc = "Open LiteRofi config.rasi settings and hotkeys"; Style = 7; Icon = (Resolve-IconLoc "settings.ico") },
        @{ Name = "LiteRofi Themes Folder.lnk"; Target = $ExePath; Args = "--themes-dir"; Desc = "Open LiteRofi themes folder"; Style = 7; Icon = (Resolve-IconLoc "settings.ico") },
        @{ Name = "Uninstall LiteRofi.lnk"; Target = "powershell.exe"; Args = "-NoProfile -ExecutionPolicy Bypass -File `"$UninstallPath`""; Desc = "Uninstall LiteRofi"; Style = 1; Icon = (Resolve-IconLoc "literofi.ico") }
    )
    if (Test-Path -LiteralPath $StudioPath) {
        $Shortcuts += @{ Name = "LiteRofi Theme Studio.lnk"; Target = $StudioPath; Args = ""; Desc = "Visual Theme & Color Palette Gallery for LiteRofi"; Style = 1; Icon = $StudioIconLoc }
    }

    foreach ($sc in $Shortcuts) {
        $link = $WshShell.CreateShortcut((Join-Path $MenuDir $sc.Name))
        $link.TargetPath = $sc.Target
        $link.Arguments = $sc.Args
        $link.WorkingDirectory = $InstallDir
        $link.Description = $sc.Desc
        $link.WindowStyle = $sc.Style
        $link.IconLocation = $sc.Icon
        $link.Save()
    }

    # Desktop shortcut for Theme Studio
    if ((Test-Path -LiteralPath $StudioPath) -and (Test-Path -LiteralPath $DesktopDir)) {
        $deskLink = $WshShell.CreateShortcut((Join-Path $DesktopDir "LiteRofi Theme Studio.lnk"))
        $deskLink.TargetPath = $StudioPath
        $deskLink.Arguments = ""
        $deskLink.WorkingDirectory = $InstallDir
        $deskLink.Description = "Visual Theme & Color Palette Gallery for LiteRofi"
        $deskLink.IconLocation = $StudioIconLoc
        $deskLink.Save()
    }

    $StartupDir = [Environment]::GetFolderPath("Startup")
    $AutoStartupLink = Join-Path $StartupDir "LiteRofi Daemon.lnk"
    if ($Autostart -or (-not $NoAutostart)) {
        $autoLink = $WshShell.CreateShortcut($AutoStartupLink)
        $autoLink.TargetPath = $ExePath
        $autoLink.Arguments = "--start-daemon"
        $autoLink.WorkingDirectory = $InstallDir
        $autoLink.Description = "LiteRofi background hotkey listener (Alt+Space)"
        $autoLink.WindowStyle = 7
        $autoLink.Save()
    }
}

# Start background daemon immediately unless suppressed
if (-not $NoStartDaemon) {
    $ExePath = Join-Path $InstallDir "literofi.exe"
    try {
        Start-Process -FilePath $ExePath -ArgumentList "--start-daemon" -WorkingDirectory $InstallDir -WindowStyle Hidden -Wait | Out-Null
        Write-Info "LiteRofi background daemon started (Alt+Space)."
    } catch {
        Write-Info "Note: could not auto-start daemon: $($_.Exception.Message)"
    }
}

Write-Info ""
Write-Info "LiteRofi installed successfully!"
Write-Info "  Binaries : $InstallDir\literofi.exe, literofi-studio.exe"
Write-Info "  Config   : $ConfigFile"
Write-Info "  Assets   : $ConfigDir ($InstalledAssetCount new files installed)"
Write-Info "  Hotkey   : Alt+Space"
