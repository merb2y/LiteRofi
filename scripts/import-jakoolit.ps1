[CmdletBinding()]
param(
    [string]$ConfigDir = (Join-Path $env:APPDATA "LiteRofi"),
    [string]$ActivateTheme = ""
)

$ErrorActionPreference = "Stop"

$BaseRaw = "https://raw.githubusercontent.com/JaKooLit/Hyprland-Dots/main/config/rofi"
$ApiThemes = "https://api.github.com/repos/JaKooLit/Hyprland-Dots/contents/config/rofi/themes"

$ThemesDir = Join-Path $ConfigDir "themes"
$WallustDir = Join-Path $ConfigDir "wallust"
New-Item -ItemType Directory -Path $ThemesDir, $WallustDir -Force | Out-Null

Write-Host "Downloading JaKooLit/Hyprland-Dots Rofi themes into $ConfigDir ..."
Invoke-WebRequest -Uri "$BaseRaw/0-shared-fonts.rasi" -OutFile (Join-Path $ConfigDir "0-shared-fonts.rasi")
Invoke-WebRequest -Uri "$BaseRaw/wallust/colors-rofi.rasi" -OutFile (Join-Path $WallustDir "colors-rofi.rasi")

$ThemeList = Invoke-RestMethod -Uri $ApiThemes -Headers @{ "User-Agent" = "LiteRofi-Importer" }
$Count = 0
foreach ($item in $ThemeList) {
    if ($item.type -eq "file" -and $item.name -like "*.rasi") {
        $dest = Join-Path $ThemesDir $item.name
        Invoke-WebRequest -Uri $item.download_url -OutFile $dest
        $Count++
    }
}

if (-not [string]::IsNullOrWhiteSpace($ActivateTheme)) {
    $ConfigFile = Join-Path $ConfigDir "config.rasi"
    if (Test-Path -LiteralPath $ConfigFile) {
        $lines = Get-Content -LiteralPath $ConfigFile
        $replaced = $false
        $out = foreach ($line in $lines) {
            if ($line.TrimStart() -like "@theme*" -and (-not $replaced)) {
                $replaced = $true
                "@theme `"$ActivateTheme`""
            } else {
                $line
            }
        }
        if (-not $replaced) {
            $out += "@theme `"$ActivateTheme`""
        }
        [System.IO.File]::WriteAllText($ConfigFile, ($out -join "`n") + "`n", [System.Text.UTF8Encoding]::new($false))
    }
    Write-Host "Activated theme '$ActivateTheme' in $ConfigFile"
}

Write-Host "Done! Imported $Count JaKooLit themes + wallust/colors-rofi.rasi into $ConfigDir."
Write-Host "Switch themes anytime with: literofi -show theme (or Start Menu -> LiteRofi Theme Selector)."
