#!/usr/bin/env python3
"""LiteRofi Theme Studio — standalone visual theme & color palette gallery."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import threading
import time
import urllib.parse
import webbrowser
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

PORT = 47821
HOST = "127.0.0.1"

APPDATA = Path(os.environ.get("APPDATA") or (Path.home() / "AppData" / "Roaming"))
LOCALAPPDATA = Path(os.environ.get("LOCALAPPDATA") or (Path.home() / "AppData" / "Local"))

CONFIG_ROOT = APPDATA / "LiteRofi"
THEMES_DIR = CONFIG_ROOT / "themes"
COLORS_DIR = CONFIG_ROOT / "colors"
CONFIG_RASI = CONFIG_ROOT / "config.rasi"
FAVORITES_FILE = CONFIG_ROOT / "studio-favorites.json"
CACHE_DIR = LOCALAPPDATA / "LiteRofi" / "studio-cache"
STUDIO_DIR = Path(__file__).resolve().parent

LITEROFI_EXE_CANDIDATES = [
    LOCALAPPDATA / "LiteRofi" / "literofi.exe",
    STUDIO_DIR.parent / "target" / "release" / "literofi.exe",
    STUDIO_DIR.parent / "literofi.exe",
]

BUILTIN_PALETTES: list[dict[str, str]] = [
    {"name": "Catppuccin Mocha", "bg": "#1e1e2e", "bg_alt": "#313244", "fg": "#cdd6f4", "selected": "#89b4fa", "active": "#a6e3a1", "urgent": "#f38ba8", "border": "#89b4fa"},
    {"name": "Catppuccin Macchiato", "bg": "#24273a", "bg_alt": "#363a4f", "fg": "#cad3f5", "selected": "#c6a0f6", "active": "#a6da95", "urgent": "#ed8796", "border": "#c6a0f6"},
    {"name": "Catppuccin Latte", "bg": "#eff1f5", "bg_alt": "#ccd0da", "fg": "#4c4f69", "selected": "#1e66f5", "active": "#40a02b", "urgent": "#d20f39", "border": "#1e66f5"},
    {"name": "Tokyo Night", "bg": "#1a1b26", "bg_alt": "#24283b", "fg": "#c0caf5", "selected": "#7aa2f7", "active": "#9ece6a", "urgent": "#f7768e", "border": "#7aa2f7"},
    {"name": "Dracula", "bg": "#282a36", "bg_alt": "#44475a", "fg": "#f8f8f2", "selected": "#bd93f9", "active": "#50fa7b", "urgent": "#ff5555", "border": "#bd93f9"},
    {"name": "Nord", "bg": "#2e3440", "bg_alt": "#3b4252", "fg": "#eceff4", "selected": "#88c0d0", "active": "#a3be8c", "urgent": "#bf616a", "border": "#88c0d0"},
    {"name": "Gruvbox Dark", "bg": "#282828", "bg_alt": "#3c3836", "fg": "#ebdbb2", "selected": "#fabd2f", "active": "#b8bb26", "urgent": "#fb4934", "border": "#fabd2f"},
    {"name": "Rose Pine", "bg": "#191724", "bg_alt": "#26233a", "fg": "#e0def4", "selected": "#c4a7e7", "active": "#9ccfd8", "urgent": "#eb6f92", "border": "#c4a7e7"},
    {"name": "Everforest Dark", "bg": "#2d353b", "bg_alt": "#3d484d", "fg": "#d3c6aa", "selected": "#a7c080", "active": "#83c092", "urgent": "#e67e80", "border": "#a7c080"},
    {"name": "One Dark", "bg": "#282c34", "bg_alt": "#353b45", "fg": "#abb2bf", "selected": "#61afef", "active": "#98c379", "urgent": "#e06c75", "border": "#61afef"},
    {"name": "Kanagawa", "bg": "#1f1f28", "bg_alt": "#2a2a37", "fg": "#dcd7ba", "selected": "#7e9cd8", "active": "#98bb6c", "urgent": "#c34043", "border": "#7e9cd8"},
    {"name": "Solarized Dark", "bg": "#002b36", "bg_alt": "#073642", "fg": "#eee8d5", "selected": "#268bd2", "active": "#859900", "urgent": "#dc322f", "border": "#268bd2"},
    {"name": "Cyberpunk Neon", "bg": "#0d0f1a", "bg_alt": "#1a1f36", "fg": "#f0f4ff", "selected": "#ff2a85", "active": "#00f0ff", "urgent": "#ffe600", "border": "#00f0ff"},
    {"name": "Monochrome OLED", "bg": "#09090b", "bg_alt": "#18181b", "fg": "#fafafa", "selected": "#e4e4e7", "active": "#a1a1aa", "urgent": "#ef4444", "border": "#52525b"},
]

RENDER_LOCKS_GUARD = threading.Lock()
RENDER_LOCKS: dict[str, threading.Lock] = {}
RENDER_EXECUTOR = concurrent.futures.ThreadPoolExecutor(max_workers=4)

LIVE_PREVIEW_LOCK = threading.Lock()
LIVE_PREVIEW_PROC: subprocess.Popen[bytes] | None = None

PRERENDER_STATE = {
    "running": False,
    "done": 0,
    "total": 0,
    "failed": 0,
}
PRERENDER_LOCK = threading.Lock()

LAST_PING = time.monotonic()
AUTO_SHUTDOWN = True


def find_literofi_exe() -> Path | None:
    for cand in LITEROFI_EXE_CANDIDATES:
        if cand.is_file():
            return cand
    found = shutil.which("literofi.exe") or shutil.which("literofi")
    return Path(found) if found else None


def classify_theme(name: str) -> str:
    low = name.lower()
    if low.startswith("lab-"):
        return "Lab"
    if low.startswith("kool_"):
        return "KooL"
    if low.startswith("adi_"):
        return "adi1090x"
    if low.startswith("gh0stzk_"):
        return "gh0stzk"
    if low.startswith("ml4w_"):
        return "ML4W"
    if "catppuccin" in low:
        return "Catppuccin"
    if low in {"windows95", "cyberpunk-hud", "c64"} or low.startswith("breaking-"):
        return "Retro"
    if any(
        low.startswith(p)
        for p in ("rounded-", "squared-", "spotlight", "launchpad", "windows11", "nord", "dracula", "rose-pine", "tokyonight", "saint-")
    ):
        return "Modern"
    return "Klasik"


def read_active_theme() -> str:
    if not CONFIG_RASI.is_file():
        return ""
    text = CONFIG_RASI.read_text(encoding="utf-8", errors="ignore").lstrip("\ufeff")
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("@theme"):
            m = re.search(r'@theme\s+"([^"]+)"', s)
            if m:
                return Path(m.group(1)).stem
    return ""


def read_active_palette() -> str:
    wallust = CONFIG_ROOT / "wallust" / "colors-rofi.rasi"
    if not wallust.is_file():
        return ""
    first = wallust.read_text(encoding="utf-8", errors="ignore").splitlines()[:3]
    for line in first:
        m = re.search(r"LiteRofi Color Palette:\s*(.+?)\s*\*/", line)
        if m:
            return m.group(1).strip()
    return ""


def load_favorites() -> set[str]:
    if not FAVORITES_FILE.is_file():
        return set()
    try:
        data = json.loads(FAVORITES_FILE.read_text(encoding="utf-8"))
        if isinstance(data, list):
            return {str(x) for x in data}
    except Exception:
        pass
    return set()


def save_favorites(favs: set[str]) -> None:
    CONFIG_ROOT.mkdir(parents=True, exist_ok=True)
    FAVORITES_FILE.write_text(json.dumps(sorted(favs), indent=2), encoding="utf-8")


def list_themes() -> list[dict[str, Any]]:
    favs = load_favorites()
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    items: list[dict[str, Any]] = []
    if THEMES_DIR.is_dir():
        for p in sorted(THEMES_DIR.glob("*.rasi"), key=lambda x: x.stem.lower()):
            name = p.stem
            png = CACHE_DIR / f"{name}.png"
            try:
                mtime = p.stat().st_mtime
            except OSError:
                mtime = 0.0
            items.append(
                {
                    "name": name,
                    "category": classify_theme(name),
                    "hasPreview": png.is_file() and png.stat().st_size > 256,
                    "isFavorite": name in favs,
                    "mtime": mtime,
                }
            )
    return items


def _parse_hex6(val: str) -> str | None:
    s = val.strip().rstrip(";").strip()
    m = re.match(r"^#([0-9a-fA-F]{6})([0-9a-fA-F]{2})?$", s)
    if m:
        return "#" + m.group(1).lower()
    m = re.match(r"^rgba?\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)", s)
    if m:
        r, g, b = (max(0, min(255, int(m.group(i)))) for i in (1, 2, 3))
        return f"#{r:02x}{g:02x}{b:02x}"
    return None


def _parse_palette_file(path: Path) -> dict[str, str] | None:
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except Exception:
        return None
    kv: dict[str, str] = {}
    for m in re.finditer(r"^\s*([a-zA-Z0-9_-]+)\s*:\s*([^;]+);", text, re.MULTILINE):
        key = m.group(1).strip().lower()
        hx = _parse_hex6(m.group(2))
        if hx:
            kv[key] = hx

    bg = kv.get("background") or kv.get("base00") or kv.get("normal-background")
    fg = kv.get("foreground") or kv.get("base05") or kv.get("normal-foreground")
    if not bg or not fg:
        return None
    bg_alt = kv.get("background-alt") or kv.get("lightbg") or kv.get("base01") or kv.get("alternate-normal-background") or bg
    selected = kv.get("selected") or kv.get("selected-normal-background") or kv.get("blue") or kv.get("base0d") or kv.get("lightfg") or fg
    active = kv.get("active") or kv.get("active-background") or kv.get("green") or kv.get("base0b") or kv.get("blue") or selected
    urgent = kv.get("urgent") or kv.get("urgent-background") or kv.get("red") or kv.get("base08") or selected
    border = kv.get("border-color") or selected

    stem = path.stem
    pretty = re.sub(r"^base16-", "", stem)
    pretty = " ".join(w.capitalize() for w in re.split(r"[-_]+", pretty) if w)
    if stem.lower().startswith("base16-"):
        pretty = f"Base16 {pretty}"

    try:
        mtime = path.stat().st_mtime
    except OSError:
        mtime = 0.0

    return {
        "name": pretty,
        "bg": bg,
        "bg_alt": bg_alt,
        "fg": fg,
        "selected": selected,
        "active": active,
        "urgent": urgent,
        "border": border,
        "mtime": mtime,
    }


def list_palettes() -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = [{**p, "mtime": 0.0} for p in BUILTIN_PALETTES]
    seen = {p["name"].lower() for p in out}
    if COLORS_DIR.is_dir():
        extras: list[dict[str, str]] = []
        for p in sorted(COLORS_DIR.glob("*.rasi"), key=lambda x: x.stem.lower()):
            parsed = _parse_palette_file(p)
            if parsed and parsed["name"].lower() not in seen:
                seen.add(parsed["name"].lower())
                extras.append(parsed)
        extras.sort(key=lambda x: x["name"].lower())
        out.extend(extras)
    return out


def apply_theme(theme_name: str) -> None:
    target_rasi = THEMES_DIR / f"{theme_name}.rasi"
    if not target_rasi.is_file():
        raise FileNotFoundError(f"Tema bulunamadi: {theme_name}")
    CONFIG_ROOT.mkdir(parents=True, exist_ok=True)
    if CONFIG_RASI.is_file():
        existing = CONFIG_RASI.read_text(encoding="utf-8", errors="ignore").lstrip("\ufeff")
    else:
        existing = (
            'configuration {\n'
            '    modes: "drun,run,window,combi,filebrowser,keys";\n'
            '    show-icons: true;\n'
            '    font: "Segoe UI 11";\n'
            '    drun-display-format: "{name}";\n'
            '    daemon-hotkey: "Alt+Space";\n'
            '    daemon-mode: "drun";\n'
            '}\n'
        )

    selected = f"themes/{theme_name}.rasi" if theme_name.casefold() == CONFIG_RASI.stem.casefold() else theme_name
    directive = f'@theme "{selected}"'
    replaced = False
    out_lines: list[str] = []
    for line in existing.splitlines():
        trimmed = line.lstrip()
        if trimmed.startswith("@theme") and not trimmed.startswith("/*") and not trimmed.startswith("//"):
            if not replaced:
                out_lines.append(directive)
                replaced = True
        else:
            out_lines.append(line)
    if not replaced:
        if out_lines and out_lines[-1] != "":
            out_lines.append("")
        out_lines.append(directive)
    CONFIG_RASI.write_text("\n".join(out_lines) + "\n", encoding="utf-8")


def _hex_to_rgb(h: str) -> tuple[int, int, int]:
    h = h.lstrip("#")
    return int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)


def apply_palette(palette_name: str) -> None:
    palettes = list_palettes()
    match = next((p for p in palettes if p["name"].lower() == palette_name.lower()), None)
    if not match:
        raise FileNotFoundError(f"Palet bulunamadi: {palette_name}")

    r, g, b = _hex_to_rgb(match["bg"])
    bg_rgba = f"rgba({r},{g},{b},0.88)"
    wallust_text = (
        f"/* LiteRofi Color Palette: {match['name']} */\n"
        "* {\n"
        f"    active-background: {match['active']};\n"
        f"    active-foreground: {match['fg']};\n"
        f"    normal-background: {match['bg']};\n"
        f"    normal-foreground: {match['fg']};\n"
        f"    urgent-background: {match['urgent']};\n"
        f"    urgent-foreground: {match['fg']};\n"
        f"    alternate-active-background: {match['bg_alt']};\n"
        f"    alternate-active-foreground: {match['fg']};\n"
        f"    alternate-normal-background: {match['bg']};\n"
        f"    alternate-normal-foreground: {match['fg']};\n"
        f"    alternate-urgent-background: {match['bg']};\n"
        f"    alternate-urgent-foreground: {match['fg']};\n"
        f"    selected-active-background: {match['selected']};\n"
        f"    selected-active-foreground: {match['bg']};\n"
        f"    selected-normal-background: {match['selected']};\n"
        f"    selected-normal-foreground: {match['bg']};\n"
        f"    selected-urgent-background: {match['urgent']};\n"
        f"    selected-urgent-foreground: {match['bg']};\n"
        f"    background-color: {match['bg']};\n"
        f"    background: {bg_rgba};\n"
        f"    background-alt: {match['bg_alt']};\n"
        f"    foreground: {match['fg']};\n"
        f"    selected: {match['selected']};\n"
        f"    active: {match['active']};\n"
        f"    urgent: {match['urgent']};\n"
        f"    border-color: {match['border']};\n"
        f"    color0: {match['bg']};\n"
        f"    color1: {match['bg_alt']};\n"
        f"    color2: {match['bg_alt']};\n"
        f"    color3: {match['urgent']};\n"
        f"    color4: {match['selected']};\n"
        f"    color5: {match['border']};\n"
        f"    color6: {match['active']};\n"
        f"    color7: {match['fg']};\n"
        f"    color8: {match['bg_alt']};\n"
        f"    color9: {match['bg_alt']};\n"
        f"    color10: {match['active']};\n"
        f"    color11: {match['active']};\n"
        f"    color12: {match['selected']};\n"
        f"    color13: {match['urgent']};\n"
        f"    color14: {match['fg']};\n"
        f"    color15: {match['fg']};\n"
        "}\n"
    )
    wallust_path = CONFIG_ROOT / "wallust" / "colors-rofi.rasi"
    wallust_path.parent.mkdir(parents=True, exist_ok=True)
    wallust_path.write_text(wallust_text, encoding="utf-8")

    adi_text = (
        f"/* LiteRofi Color Palette: {match['name']} */\n"
        "* {\n"
        f"    background:     {match['bg']}FF;\n"
        f"    background-alt: {match['bg_alt']}FF;\n"
        f"    foreground:     {match['fg']}FF;\n"
        f"    selected:       {match['selected']}FF;\n"
        f"    active:         {match['active']}FF;\n"
        f"    urgent:         {match['urgent']}FF;\n"
        "}\n"
    )
    launchers = CONFIG_ROOT / "launchers"
    if launchers.is_dir():
        for ent in launchers.iterdir():
            shared = ent / "shared" / "colors.rasi"
            if shared.is_file():
                shared.write_text(adi_text, encoding="utf-8")


def _get_render_lock(name: str) -> threading.Lock:
    with RENDER_LOCKS_GUARD:
        lk = RENDER_LOCKS.get(name)
        if lk is None:
            lk = threading.Lock()
            RENDER_LOCKS[name] = lk
        return lk


def render_preview_png(name: str, force: bool = False) -> Path | None:
    rasi = THEMES_DIR / f"{name}.rasi"
    if not rasi.is_file():
        return None
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    out_png = CACHE_DIR / f"{name}.png"
    with _get_render_lock(name):
        if not force and out_png.is_file() and out_png.stat().st_size > 256:
            return out_png
        exe = find_literofi_exe()
        if not exe:
            return None
        tmp_png = CACHE_DIR / f".{name}.{os.getpid()}.{threading.get_ident()}.png"
        creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        try:
            proc = subprocess.run(
                [str(exe), "-show", "drun", "-show-icons", "-theme", str(rasi), "--dump-png", str(tmp_png)],
                capture_output=True,
                timeout=12,
                creationflags=creationflags,
            )
            if proc.returncode == 0 and tmp_png.is_file() and tmp_png.stat().st_size > 256:
                os.replace(tmp_png, out_png)
                return out_png
        except Exception:
            pass
        finally:
            if tmp_png.exists():
                try:
                    tmp_png.unlink()
                except Exception:
                    pass
    return out_png if (out_png.is_file() and out_png.stat().st_size > 256) else None


def start_prerender_missing(force: bool = False) -> bool:
    with PRERENDER_LOCK:
        if PRERENDER_STATE["running"]:
            return False
        themes = list_themes()
        targets = [t["name"] for t in themes if force or not t["hasPreview"]]
        PRERENDER_STATE["running"] = True
        PRERENDER_STATE["done"] = 0
        PRERENDER_STATE["total"] = len(targets)
        PRERENDER_STATE["failed"] = 0

    def _worker() -> None:
        try:
            futs = [RENDER_EXECUTOR.submit(render_preview_png, n, force) for n in targets]
            for fut in concurrent.futures.as_completed(futs):
                ok = False
                try:
                    ok = fut.result() is not None
                except Exception:
                    ok = False
                with PRERENDER_LOCK:
                    PRERENDER_STATE["done"] += 1
                    if not ok:
                        PRERENDER_STATE["failed"] += 1
        finally:
            with PRERENDER_LOCK:
                PRERENDER_STATE["running"] = False

    threading.Thread(target=_worker, daemon=True).start()
    return True


def close_live_preview() -> None:
    global LIVE_PREVIEW_PROC
    with LIVE_PREVIEW_LOCK:
        proc = LIVE_PREVIEW_PROC
        LIVE_PREVIEW_PROC = None
        if proc is not None and proc.poll() is None:
            try:
                proc.terminate()
                proc.wait(timeout=1.0)
            except Exception:
                try:
                    proc.kill()
                except Exception:
                    pass


def launch_live_preview(theme_name: str) -> None:
    global LIVE_PREVIEW_PROC
    exe = find_literofi_exe()
    if not exe:
        raise FileNotFoundError("literofi.exe bulunamadi")
    rasi = THEMES_DIR / f"{theme_name}.rasi"
    if not rasi.is_file():
        raise FileNotFoundError(f"Tema bulunamadi: {theme_name}")
    close_live_preview()
    creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    with LIVE_PREVIEW_LOCK:
        LIVE_PREVIEW_PROC = subprocess.Popen(
            [str(exe), "-show", "drun", "-show-icons", "-theme", str(rasi)],
            creationflags=creationflags,
        )


class StudioHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args: Any) -> None:
        return

    def _send_json(self, obj: Any, status: int = 200) -> None:
        data = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def _read_json(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length", "0") or "0")
        raw = self.rfile.read(length) if length > 0 else b"{}"
        return json.loads(raw.decode("utf-8"))

    def do_GET(self) -> None:
        global LAST_PING
        LAST_PING = time.monotonic()
        parsed = urllib.parse.urlsplit(self.path)
        path = parsed.path
        qs = urllib.parse.parse_qs(parsed.query)

        if path in ("/", "/index.html"):
            html_path = STUDIO_DIR / "index.html"
            data = html_path.read_bytes()
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(data)
            return

        if path == "/api/state":
            themes = list_themes()
            palettes = list_palettes()
            preview_count = sum(1 for t in themes if t["hasPreview"])
            self._send_json(
                {
                    "activeTheme": read_active_theme(),
                    "activePalette": read_active_palette(),
                    "themes": themes,
                    "palettes": palettes,
                    "previewCount": preview_count,
                    "totalThemes": len(themes),
                    "prerender": dict(PRERENDER_STATE),
                }
            )
            return

        if path == "/api/prerender-status":
            with PRERENDER_LOCK:
                self._send_json(dict(PRERENDER_STATE))
            return

        if path == "/api/ping":
            self._send_json({"ok": True})
            return

        if path.startswith("/api/preview/") and path.endswith(".png"):
            raw_name = urllib.parse.unquote(path[len("/api/preview/") : -4])
            if "/" in raw_name or "\\" in raw_name or ".." in raw_name:
                self._send_json({"error": "invalid name"}, 400)
                return
            render_flag = qs.get("render", ["0"])[0] == "1"
            force_flag = qs.get("force", ["0"])[0] == "1"
            png = CACHE_DIR / f"{raw_name}.png"
            if force_flag or (render_flag and not (png.is_file() and png.stat().st_size > 256)):
                png_res = render_preview_png(raw_name, force=force_flag)
                if png_res is None:
                    self.send_response(HTTPStatus.NOT_FOUND)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                png = png_res
            if not (png.is_file() and png.stat().st_size > 256):
                self.send_response(HTTPStatus.NOT_FOUND)
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            data = png.read_bytes()
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "image/png")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(data)
            return

        self._send_json({"error": "not found"}, 404)

    def do_POST(self) -> None:
        global LAST_PING
        LAST_PING = time.monotonic()
        path = urllib.parse.urlsplit(self.path).path
        try:
            body = self._read_json()
        except Exception as e:
            self._send_json({"error": f"bad json: {e}"}, 400)
            return

        try:
            if path == "/api/apply-theme":
                name = str(body.get("name", "")).strip()
                apply_theme(name)
                self._send_json({"ok": True, "activeTheme": name})
                return

            if path == "/api/apply-palette":
                name = str(body.get("name", "")).strip()
                apply_palette(name)
                self._send_json({"ok": True, "activePalette": name})
                return

            if path == "/api/live-preview":
                name = str(body.get("name", "")).strip()
                launch_live_preview(name)
                self._send_json({"ok": True})
                return

            if path == "/api/favorite":
                name = str(body.get("name", "")).strip()
                favs = load_favorites()
                if name in favs:
                    favs.remove(name)
                    is_fav = False
                else:
                    favs.add(name)
                    is_fav = True
                save_favorites(favs)
                self._send_json({"ok": True, "name": name, "isFavorite": is_fav})
                return

            if path == "/api/prerender":
                force = bool(body.get("force", False))
                started = start_prerender_missing(force=force)
                with PRERENDER_LOCK:
                    self._send_json({"ok": True, "started": started, "prerender": dict(PRERENDER_STATE)})
                return

            if path == "/api/clear-cache":
                if CACHE_DIR.is_dir():
                    for f in CACHE_DIR.glob("*.png"):
                        try:
                            f.unlink()
                        except Exception:
                            pass
                self._send_json({"ok": True})
                return

            self._send_json({"error": "not found"}, 404)
        except Exception as e:
            self._send_json({"error": str(e)}, 500)


def find_chromium_app_browser() -> str | None:
    pf = os.environ.get("ProgramFiles", r"C:\Program Files")
    pf86 = os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")
    local = str(LOCALAPPDATA)
    candidates = [
        os.path.join(pf86, "Microsoft", "Edge", "Application", "msedge.exe"),
        os.path.join(pf, "Microsoft", "Edge", "Application", "msedge.exe"),
        os.path.join(pf, "Google", "Chrome", "Application", "chrome.exe"),
        os.path.join(pf86, "Google", "Chrome", "Application", "chrome.exe"),
        os.path.join(local, "Google", "Chrome", "Application", "chrome.exe"),
        os.path.join(pf, "BraveSoftware", "Brave-Browser", "Application", "brave.exe"),
        os.path.join(local, "BraveSoftware", "Brave-Browser", "Application", "brave.exe"),
        os.path.join(local, "Vivaldi", "Application", "vivaldi.exe"),
        os.path.join(local, "Programs", "Opera", "opera.exe"),
        os.path.join(local, "Programs", "Opera GX", "opera.exe"),
    ]
    for c in candidates:
        if os.path.isfile(c):
            return c
    return None


def open_app_window(url: str) -> None:
    browser = find_chromium_app_browser()
    if browser:
        profile_dir = LOCALAPPDATA / "LiteRofi" / "studio-profile"
        profile_dir.mkdir(parents=True, exist_ok=True)
        subprocess.Popen(
            [
                browser,
                f"--app={url}",
                "--window-size=1280,820",
                f"--user-data-dir={profile_dir}",
            ]
        )
    else:
        webbrowser.open(url)


def is_port_in_use(host: str, port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(0.3)
        return s.connect_ex((host, port)) == 0


def main() -> int:
    global AUTO_SHUTDOWN, LAST_PING
    parser = argparse.ArgumentParser(description="LiteRofi Theme Studio")
    parser.add_argument("--port", type=int, default=PORT)
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--no-autoshutdown", action="store_true")
    args = parser.parse_args()

    AUTO_SHUTDOWN = not args.no_autoshutdown
    url = f"http://{HOST}:{args.port}/"

    if is_port_in_use(HOST, args.port):
        if not args.no_browser:
            open_app_window(url)
        return 0

    server = ThreadingHTTPServer((HOST, args.port), StudioHandler)
    LAST_PING = time.monotonic()

    if AUTO_SHUTDOWN:
        def _watchdog() -> None:
            # Give browser 45s to open initially, then 35s idle timeout after window closes
            time.sleep(25)
            while True:
                time.sleep(5)
                if time.monotonic() - LAST_PING > 35:
                    server.shutdown()
                    break

        threading.Thread(target=_watchdog, daemon=True).start()

    if not args.no_browser:
        threading.Timer(0.25, lambda: open_app_window(url)).start()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
