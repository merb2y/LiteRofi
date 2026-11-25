[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "LiteRofi"),
    [string]$ConfigDir = (Join-Path $env:APPDATA "LiteRofi"),
    [switch]$PurgeConfig,
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

# Stop only literofi processes running from InstallDir
$NormalizedInstall = $InstallDir.TrimEnd('\')
Get-Process -Name "literofi" -ErrorAction SilentlyContinue | ForEach-Object {
    try {
        if ($_.Path -and $_.Path.StartsWith($NormalizedInstall, [System.StringComparison]::OrdinalIgnoreCase)) {
            Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
        }
    } catch {
        # Ignore inaccessible process paths
    }
}

# Remove User PATH entry
if (-not $NoPath) {
    $CurrentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not [string]::IsNullOrWhiteSpace($CurrentUserPath)) {
        $Filtered = $CurrentUserPath.Split(';') | Where-Object {
            (-not [string]::IsNullOrWhiteSpace($_)) -and
            (-not $_.TrimEnd('\').Equals($NormalizedInstall, [System.StringComparison]::OrdinalIgnoreCase))
        }
        [Environment]::SetEnvironmentVariable("Path", ($Filtered -join ';'), "User")
    }
}

# Remove Start Menu and Startup shortcuts
if (-not $NoShortcuts) {
    $MenuDir = Join-Path ([Environment]::GetFolderPath("Programs")) "LiteRofi"
    if (Test-Path -LiteralPath $MenuDir) {
        Remove-Item -LiteralPath $MenuDir -Recurse -Force
    }
    $AutoLink = Join-Path ([Environment]::GetFolderPath("Startup")) "LiteRofi Daemon.lnk"
    if (Test-Path -LiteralPath $AutoLink) {
        Remove-Item -LiteralPath $AutoLink -Force
    }
}

# Remove installation directory
if (Test-Path -LiteralPath $InstallDir) {
    Remove-Item -LiteralPath $InstallDir -Recurse -Force
    Write-Info "Removed $InstallDir"
}

# Optionally purge user config and themes
if ($PurgeConfig -and (Test-Path -LiteralPath $ConfigDir)) {
    Remove-Item -LiteralPath $ConfigDir -Recurse -Force
    Write-Info "Removed $ConfigDir"
} elseif (Test-Path -LiteralPath $ConfigDir) {
    Write-Info "Preserved user config and themes at $ConfigDir (use -PurgeConfig to remove)."
}

Write-Info "LiteRofi uninstalled."
