<#
.SYNOPSIS
    LiteRofi automated theme visual & static audit tool.
.DESCRIPTION
    Scans a directory of `.rasi` themes, checks for unsupported/risky Rofi CSS features,
    renders each theme via `literofi.exe --dump-png`, inspects the rendered pixels for
    anomalies (blank/invisible window, off-screen clipping, low contrast, crash/error),
    and writes a prioritized Markdown + JSON report.
.EXAMPLE
    .\scripts\audit-themes.ps1
    .\scripts\audit-themes.ps1 -ThemesDir "C:\Users\merby\AppData\Roaming\LiteRofi\themes" -OutDir ".\dist\audit"
#>
param(
    [string]$ThemesDir = "$env:APPDATA\LiteRofi\themes",
    [string]$LiteRofiExe = "$env:LOCALAPPDATA\LiteRofi\literofi.exe",
    [string]$OutDir = "",
    [string]$Mode = "drun",
    [string]$ReferencePngDir = "",
    [switch]$KeepPngs
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $LiteRofiExe)) {
    $fallback = Join-Path $PSScriptRoot "..\target\release\literofi.exe"
    if (Test-Path $fallback) {
        $LiteRofiExe = (Resolve-Path $fallback).Path
    } else {
        Write-Error "literofi.exe not found at '$LiteRofiExe' or '$fallback'."
        exit 1
    }
}

if (-not (Test-Path $ThemesDir)) {
    Write-Error "Themes directory not found: $ThemesDir"
    exit 1
}

if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path $PSScriptRoot "..\dist\theme-audit"
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$pngDir = Join-Path $OutDir "pngs"
New-Item -ItemType Directory -Force -Path $pngDir | Out-Null

$rasiFiles = Get-ChildItem -Path $ThemesDir -Filter "*.rasi" -File -Recurse | Sort-Object FullName
Write-Host "Auditing $($rasiFiles.Count) themes from '$ThemesDir' using '$LiteRofiExe' (mode=$Mode)..."

$renderMeta = @()
foreach ($f in $rasiFiles) {
    $root = (Get-Item -LiteralPath $ThemesDir).FullName.TrimEnd('\', '/')
    $relative = $f.FullName.Substring($root.Length + 1)
    $pngPath = Join-Path $pngDir ([IO.Path]::ChangeExtension($relative, ".png"))
    New-Item -ItemType Directory -Path (Split-Path -Parent $pngPath) -Force | Out-Null
    if (Test-Path $pngPath) { Remove-Item -Force $pngPath }
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $stderr = ""
    try {
        $out = & $LiteRofiExe -show $Mode -show-icons -theme $f.FullName --dump-png $pngPath 2>&1
        $exitCode = $LASTEXITCODE
        if ($out) { $stderr = ($out | Out-String).Trim() }
    } catch {
        $exitCode = -1
        $stderr = $_.Exception.Message
    }
    $sw.Stop()
    $renderMeta += [PSCustomObject]@{
        name      = [IO.Path]::ChangeExtension($relative, $null).TrimEnd('.')
        rasi      = $f.FullName
        png       = $pngPath
        exit_code = $exitCode
        ms        = [int]$sw.ElapsedMilliseconds
        stderr    = $stderr
        has_png   = (Test-Path $pngPath)
    }
}

$metaJsonPath = Join-Path $OutDir "render-meta.json"
$renderMeta | ConvertTo-Json -Depth 4 | Set-Content -Path $metaJsonPath -Encoding UTF8

$pyScript = @'
import json, math, os, re, struct, sys, zlib

meta_path, out_dir = sys.argv[1:3]
reference_dir = sys.argv[3] if len(sys.argv) > 3 else ""
with open(meta_path, "r", encoding="utf-8-sig") as f:
    items = json.load(f)
if isinstance(items, dict):
    items = [items]

def read_png_rgb(path):
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("Not a PNG")
    pos = 8
    w = h = bit_depth = color_type = 0
    idat = bytearray()
    while pos < len(data):
        ln = struct.unpack(">I", data[pos:pos+4])[0]
        ty = data[pos+4:pos+8]
        ch = data[pos+8:pos+8+ln]
        pos += 12 + ln
        if ty == b"IHDR":
            w, h, bit_depth, color_type = struct.unpack(">IIBB", ch[:10])
        elif ty == b"IDAT":
            idat.extend(ch)
        elif ty == b"IEND":
            break
    raw = zlib.decompress(bytes(idat))
    bpp = 3 if color_type == 2 else 4
    stride = w * bpp
    rows = []
    p = 0
    for y in range(h):
        flt = raw[p]; p += 1
        row = bytearray(raw[p:p+stride]); p += stride
        if flt == 1:
            for i in range(bpp, stride):
                row[i] = (row[i] + row[i - bpp]) & 0xff
        elif flt == 2 and y > 0:
            prev = rows[-1]
            for i in range(stride):
                row[i] = (row[i] + prev[i]) & 0xff
        elif flt == 3:
            prev = rows[-1] if y > 0 else bytes(stride)
            for i in range(stride):
                a = row[i - bpp] if i >= bpp else 0
                row[i] = (row[i] + ((a + prev[i]) >> 1)) & 0xff
        elif flt == 4:
            prev = rows[-1] if y > 0 else bytes(stride)
            for i in range(stride):
                a = row[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                pa = abs(b - c); pb = abs(a - c); pc = abs(a + b - 2*c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                row[i] = (row[i] + pr) & 0xff
        rows.append(row)
    return w, h, bpp, rows

def backdrop_rgb(x, y, w, h):
    tx = x / max(1, w)
    ty = y / max(1, h)
    r = int(36.0 + 28.0 * tx + 18.0 * (1.0 - ty))
    g = int(52.0 + 36.0 * (1.0 - ty) + 14.0 * tx)
    b = int(78.0 + 44.0 * ty + 20.0 * (1.0 - tx))
    return r, g, b

def analyze_png(path):
    w, h, bpp, rows = read_png_rgb(path)
    step = 2
    min_x, min_y, max_x, max_y = w, h, -1, -1
    diff_cnt = 0
    total_cnt = 0
    lums = []
    for y in range(0, h, step):
        row = rows[y]
        for x in range(0, w, step):
            total_cnt += 1
            idx = x * bpp
            r, g, b = row[idx], row[idx+1], row[idx+2]
            br, bg, bb = backdrop_rgb(x, y, w, h)
            if abs(r - br) + abs(g - bg) + abs(b - bb) > 14:
                diff_cnt += 1
                if x < min_x: min_x = x
                if y < min_y: min_y = y
                if x > max_x: max_x = x
                if y > max_y: max_y = y
                lums.append(0.299 * r + 0.587 * g + 0.114 * b)
    coverage = diff_cnt / max(1, total_cnt)
    if max_x < min_x:
        return {"w": w, "h": h, "coverage": 0.0, "bbox": None, "lum_std": 0.0}
    mean_l = sum(lums) / len(lums)
    var_l = sum((v - mean_l) ** 2 for v in lums) / len(lums)
    return {
        "w": w, "h": h,
        "coverage": round(coverage, 4),
        "bbox": [min_x, min_y, max_x - min_x + 1, max_y - min_y + 1],
        "lum_std": round(math.sqrt(var_l), 2),
        "touches_edge": (min_x <= 2 or min_y <= 2 or max_x >= w - 3 or max_y >= h - 3),
    }

def analyze_rasi(path):
    txt = open(path, "r", encoding="utf-8", errors="replace").read()
    # Strip C-style comments
    clean = re.sub(r"/\*.*?\*/", "", txt, flags=re.S)
    clean = re.sub(r"//.*", "", clean)
    flags = []
    fullscreen = bool(re.search(r"\bfullscreen\s*:\s*true\b", clean, re.I))
    # Check url(...) background-image
    for m in re.finditer(r"url\s*\(\s*[\"']([^\"']+)[\"']", clean, re.I):
        u = m.group(1)
        if u.startswith("/") or u.startswith("~"):
            flags.append(f"linux_asset_path:{u}")
    # Check @import
    base_dir = os.path.dirname(path)
    for m in re.finditer(r"@import\s+[\"']([^\"']+)[\"']", clean, re.I):
        imp = m.group(1)
        cand = imp if os.path.isabs(imp) else os.path.join(base_dir, imp)
        if not os.path.exists(cand) and not os.path.exists(cand + ".rasi"):
            flags.append(f"missing_import:{imp}")
    # Check risky / partial Rofi features
    checks = [
        (r"\b@media\b", "uses_media_query"),
        (r"linear-gradient\s*\(", "uses_linear_gradient"),
        (r"\bcursor-outline\b", "uses_cursor_outline"),
        (r"\btext-outline\b", "uses_text_outline"),
        (r"\btab-stops\b", "uses_tab_stops"),
        (r"\bblink\s*:", "uses_blink"),
        (r"\btransparency\s*:", "uses_x11_transparency"),
        (r"\bplaceholder-markup\b", "uses_placeholder_markup"),
        (r"\bicon-current-entry\b", "uses_icon_current_entry"),
        (r"\btextbox-current-entry\b", "uses_textbox_current_entry"),
        (r"\bmode-switcher\b|\bsidebar\b", "uses_mode_switcher"),
        (r"\bscrollbar\s*:\s*true\b", "uses_scrollbar"),
        (r"\bcalc\s*\(", "uses_calc"),
        (r"\bmm\b", "uses_mm_unit"),
    ]
    for pat, tag in checks:
        if re.search(pat, clean, re.I):
            flags.append(tag)
    return fullscreen, flags

results = []
for it in items:
    name = it["name"]
    fullscreen, rasi_flags = analyze_rasi(it["rasi"])
    severity = "OK"
    issues = []

    if it["exit_code"] != 0 or not it["has_png"]:
        severity = "CRITICAL"
        issues.append(f"render_failed(exit={it['exit_code']}): {it['stderr'][:140]}")
        px = None
    else:
        px = analyze_png(it["png"])
        if px["coverage"] < 0.004 or px["bbox"] is None:
            severity = "CRITICAL"
            issues.append(f"blank_or_invisible_window(coverage={px['coverage']})")
        else:
            bw, bh = px["bbox"][2], px["bbox"][3]
            if bw < 50 or bh < 14:
                severity = "CRITICAL"
                issues.append(f"collapsed_window({bw}x{bh})")
            if px["lum_std"] < 6.0:
                severity = "WARN" if severity == "OK" else severity
                issues.append(f"very_low_contrast(lum_std={px['lum_std']})")
            if px["touches_edge"] and not fullscreen and not any(
                k in name.lower() for k in ("oneline", "twolines", "bar", "dmenu", "sidebar", "dock", "fullscreen", "windows11", "tokyonight")
            ):
                severity = "WARN" if severity == "OK" else severity
                issues.append(f"touches_screen_edge(bbox={px['bbox']})")

    reference_size = None
    if reference_dir:
        reference = os.path.join(reference_dir, name + ".png")
        if os.path.isfile(reference):
            with open(reference, "rb") as image:
                header = image.read(24)
            if header.startswith(b"\x89PNG\r\n\x1a\n") and len(header) == 24:
                reference_size = list(struct.unpack(">II", header[16:24]))
            if not reference_size or (px and [px["w"], px["h"]] != reference_size):
                issues.append(f"reference_size_differs({reference_size})")
        else:
            issues.append("reference_png_missing")
        if issues and severity == "OK":
            severity = "WARN"

    for fl in rasi_flags:
        if fl.startswith("missing_import:") or fl.startswith("linux_asset_path:"):
            severity = "WARN" if severity == "OK" else severity
            issues.append(fl)

    results.append({
        "name": name,
        "severity": severity,
        "ms": it["ms"],
        "issues": issues,
        "features": rasi_flags,
        "pixel": px,
        "reference_size": reference_size,
    })

order = {"CRITICAL": 0, "WARN": 1, "OK": 2}
results.sort(key=lambda r: (order[r["severity"]], r["name"].lower()))

json_out = os.path.join(out_dir, "report.json")
with open(json_out, "w", encoding="utf-8") as f:
    json.dump(results, f, indent=2, ensure_ascii=False)

crit = [r for r in results if r["severity"] == "CRITICAL"]
warn = [r for r in results if r["severity"] == "WARN"]
ok = [r for r in results if r["severity"] == "OK"]

md_lines = [
    f"# LiteRofi Theme Audit Report",
    f"",
    f"- Total themes: **{len(results)}**",
    f"- CRITICAL: **{len(crit)}** | WARN: **{len(warn)}** | OK: **{len(ok)}**",
    f"",
]
if crit:
    md_lines.append("## CRITICAL")
    for r in crit:
        md_lines.append(f"- **{r['name']}**: {'; '.join(r['issues'])}")
    md_lines.append("")
if warn:
    md_lines.append("## WARN")
    for r in warn:
        md_lines.append(f"- **{r['name']}**: {'; '.join(r['issues'])}")
    md_lines.append("")

md_out = os.path.join(out_dir, "report.md")
with open(md_out, "w", encoding="utf-8") as f:
    f.write("\n".join(md_lines) + "\n")

print(f"AUDIT_SUMMARY total={len(results)} critical={len(crit)} warn={len(warn)} ok={len(ok)}")
for r in crit + warn:
    print(f"[{r['severity']}] {r['name']}: {'; '.join(r['issues'])}")
'@

$pyPath = Join-Path $OutDir "analyze.py"
Set-Content -Path $pyPath -Value $pyScript -Encoding UTF8
python $pyPath $metaJsonPath $OutDir $ReferencePngDir
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if (-not $KeepPngs) {
    Remove-Item -Recurse -Force $pngDir -ErrorAction SilentlyContinue
}
Write-Host "Report written to: $(Join-Path $OutDir 'report.md')"
