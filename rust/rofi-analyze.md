# Rust migration plan: rofi (C)

2 packages, 50 source files (28731 lines), 12 test files, 0 UI files, grouped into 9 units.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 1 | 5 | 0 | 3 |

> 41 files had parse errors; their units are marked partial.

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `source/widgets/` | 3642 | 15 | 0 | 0 | C ABI | Pointers and manual memory (404); String offsets and slicing (5); Null references (165) |
| 2 | `source/wayland/` | 2383 | 15 | 0 | 0 | C ABI | Pointers and manual memory (412); String offsets and slicing (7); Null references (181) |

### `source/widgets/`

3642 lines in 7 files, wave 1, priority 15, readiness 0, bridge **C ABI**.

- 5 parse errors: results for this unit are partial.
- 7 system libraries dependencies not in the crate table: check them by hand.
- 35 imports use aliases or virtual modules and were not followed.
- Pointers and manual memory x404: `source/widgets/box.c:47`, `source/widgets/box.c:50`, `source/widgets/box.c:52`
- String offsets and slicing x5: `source/widgets/textbox.c:350`, `source/widgets/textbox.c:573`, `source/widgets/textbox.c:574`
- Null references x165: `source/widgets/box.c:66`, `source/widgets/box.c:83`, `source/widgets/box.c:102`
- Integer width and overflow x85: `source/widgets/box.c:53`, `source/widgets/box.c:68`, `source/widgets/box.c:85`
- Division and modulo x64: `source/widgets/box.c:179`, `source/widgets/box.c:249`, `source/widgets/icon.c:65`

### `source/wayland/`

2383 lines in 2 files, wave 1, priority 15, readiness 0, bridge **C ABI**.

- 2 parse errors: results for this unit are partial.
- 13 system libraries dependencies not in the crate table: check them by hand.
- 17 imports use aliases or virtual modules and were not followed.
- Pointers and manual memory x412: `source/wayland/display.c:84`, `source/wayland/display.c:86`, `source/wayland/display.c:87`
- String offsets and slicing x7: `source/wayland/display.c:1714`, `source/wayland/display.c:1717`, `source/wayland/display.c:1888`
- Null references x181: `source/wayland/display.c:127`, `source/wayland/display.c:132`, `source/wayland/display.c:132`
- Division and modulo x18: `source/wayland/display.c:417`, `source/wayland/display.c:479`, `source/wayland/display.c:711`
- Integer width and overflow x16: `source/wayland/display.c:210`, `source/wayland/display.c:862`, `source/wayland/display.c:1546`
- Reflection and prototypes x1: `source/wayland/display.c:72`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. rofi (`.`, portable, 12 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `include/widgets/` | ready | 197 | 83 | C ABI | Pointers and manual memory (30) |
| 0 | `include/modes/` | caution | 89 | 75 | C ABI | Pointers and manual memory (9) |
| 0 | `include/` | caution | 1276 | 0 | C ABI | Pointers and manual memory (262); Reflection and prototypes (3) |
| 1 | `config/` | caution | 180 | 87 | C ABI | Null references (14) |
| 1 | `source/modes/` | keep | 8081 | 0 | C ABI | Pointers and manual memory (1711); String offsets and slicing (24) |
| 1 | `source/wayland/` | caution | 2383 | 0 | C ABI | Pointers and manual memory (412); String offsets and slicing (7) |
| 1 | `source/widgets/` | caution | 3642 | 0 | C ABI | Pointers and manual memory (404); String offsets and slicing (5) |
| 1 | `source/xcb/` | keep | 2765 | 0 | C ABI | Pointers and manual memory (406); String offsets and slicing (6) |
| 2 | `source/` | keep | 10118 | 0 | C ABI | Pointers and manual memory (1566); String offsets and slicing (58) |

## system libraries dependencies

| Package | Units | Rust | Fit | Note |
| --- | ---: | --- | --- | --- |
| `<glib.h>` | 5 | ? | unknown |  |
| `<cairo.h>` | 2 | ? | unknown |  |
| `<libgwater-wayland.h>` | 2 | ? | unknown |  |
| `<nkutils-bindings.h>` | 2 | ? | unknown |  |
| `<pango/pango.h>` | 2 | ? | unknown |  |
| `<wayland-client.h>` | 2 | ? | unknown |  |
| `<xcb/xcb.h>` | 2 | ? | unknown |  |
| `<xkbcommon/xkbcommon.h>` | 2 | ? | unknown |  |
| `<config.h>` | 1 | ? | unknown |  |
| `<glib/gstdio.h>` | 1 | ? | unknown |  |
| `<gmodule.h>` | 1 | ? | unknown |  |
| `<libgwater-xcb.h>` | 1 | ? | unknown |  |
| `<libsn/sn.h>` | 1 | ? | unknown |  |
| `<mode.h>` | 1 | ? | unknown |  |
| `<rofi.h>` | 1 | ? | unknown |  |
| `<wayland-cursor.h>` | 1 | ? | unknown |  |
| `<widgets/box.h>` | 1 | ? | unknown |  |
| `<widgets/icon.h>` | 1 | ? | unknown |  |
| `<widgets/listview.h>` | 1 | ? | unknown |  |
| `<widgets/scrollbar.h>` | 1 | ? | unknown |  |
| `<widgets/textbox.h>` | 1 | ? | unknown |  |
| `<widgets/widget.h>` | 1 | ? | unknown |  |
| `<xcb-imdkit/imclient.h>` | 1 | ? | unknown |  |
| `<xcb/xcb_ewmh.h>` | 1 | ? | unknown |  |
| `<xcb/xcb_keysyms.h>` | 1 | ? | unknown |  |
| `<xkbcommon/xkbcommon-compose.h>` | 1 | ? | unknown |  |
| `<fcntl.h>` | 1 | std::fs, std::process, libc | good |  |
| `<sys/mman.h>` | 1 | memmap2 | good |  |
| `<sys/types.h>` | 1 | std::fs, std::process, libc | good |  |
| `<unistd.h>` | 1 | std::fs, std::process, libc | good |  |

## Risk guide

- **String offsets and slicing** (High): JS indexes strings in UTF-16 code units, Rust in UTF-8 bytes. Offsets that cross the boundary must be converted, or non-ASCII text gets wrong ranges silently.
- **Exceptions** (Low): Thrown errors become `Result`. Errors crossing the bridge must map back to the same JS error shape.
- **Reflection and prototypes** (Medium): Proxy, Reflect, Symbol and prototype tricks need a redesign, not a translation.
- **Integer width and overflow** (Medium): Python ints never overflow; C, Go and Java ints wrap or are undefined on overflow. Pick i64, i128 or a bignum crate deliberately, and use wrapping_* where the source wraps.
- **Division and modulo** (Medium): Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined.
- **Pointers and manual memory** (High): Pointer arithmetic and manual memory become slices, indices and owned types. Code that exposes pointers across the boundary needs `unsafe`, isolated and explained.
- **Null references** (Medium): Any reference may be null in the source: decide per parameter whether it is `Option` and what null means, instead of mapping every value to `Option`.
- **Platform-specific C (asm, volatile, bitfields, packing)** (High): This C depends on its target: register access, memory layout, the compiler's type sizes or its assembly. A recording on this machine says nothing about the target, and battle-tested driver or firmware code gains little from a rewrite: keep it in C and port the portable logic around it.

## Whole app

Shape: **cli**.

| Entry point | Kind | Found from |
| --- | --- | --- |
| `source/rofi.c` | cli | `int main(` at source/rofi.c:989 |

### Every dependency

| Dependency | Declared | Used by | Rust | Fit | Note |
| --- | --- | --- | --- | --- | --- |
| `<glib.h>` | no | 8 | ? | unknown |  |
| `<pango/pango.h>` | no | 5 | ? | unknown |  |
| `<xcb/xcb.h>` | no | 5 | ? | unknown |  |
| `<cairo.h>` | no | 4 | ? | unknown |  |
| `<xkbcommon/xkbcommon.h>` | no | 4 | ? | unknown |  |
| `<config.h>` | no | 3 | ? | unknown |  |
| `<gio/gio.h>` | no | 3 | ? | unknown |  |
| `<glib/gstdio.h>` | no | 3 | ? | unknown |  |
| `<gmodule.h>` | no | 3 | ? | unknown |  |
| `<nkutils-bindings.h>` | no | 3 | ? | unknown |  |
| `<rofi.h>` | no | 3 | ? | unknown |  |
| `<wayland-client.h>` | no | 3 | ? | unknown |  |
| `<xcb/xcb_ewmh.h>` | no | 3 | ? | unknown |  |
| `<glib-unix.h>` | no | 2 | ? | unknown |  |
| `<libgwater-wayland.h>` | no | 2 | ? | unknown |  |
| `<libsn/sn.h>` | no | 2 | ? | unknown |  |
| `<widgets/widget.h>` | no | 2 | ? | unknown |  |
| `<xcb/xcb_icccm.h>` | no | 2 | ? | unknown |  |
| `<xcb/xcb_keysyms.h>` | no | 2 | ? | unknown |  |
| `<cairo-xcb.h>` | no | 1 | ? | unknown |  |
| `<gdk-pixbuf/gdk-pixbuf.h>` | no | 1 | ? | unknown |  |
| `<gio/gdesktopappinfo.h>` | no | 1 | ? | unknown |  |
| `<gio/gunixinputstream.h>` | no | 1 | ? | unknown |  |
| `<glob.h>` | no | 1 | ? | unknown |  |
| `<helper.h>` | no | 1 | ? | unknown |  |
| `<libgwater-xcb.h>` | no | 1 | ? | unknown |  |
| `<mode.h>` | no | 1 | ? | unknown |  |
| `<modes/modes.h>` | no | 1 | ? | unknown |  |
| `<pango/pango-fontmap.h>` | no | 1 | ? | unknown |  |
| `<pango/pangocairo.h>` | no | 1 | ? | unknown |  |
| `<pwd.h>` | no | 1 | ? | unknown |  |
| `<strings.h>` | no | 1 | ? | unknown |  |
| `<sys/file.h>` | no | 1 | ? | unknown |  |
| `<sysexits.h>` | no | 1 | ? | unknown |  |
| `<theme.h>` | no | 1 | ? | unknown |  |
| `<wayland-cursor.h>` | no | 1 | ? | unknown |  |
| `<wayland-version.h>` | no | 1 | ? | unknown |  |
| `<widgets/box.h>` | no | 1 | ? | unknown |  |
| `<widgets/icon.h>` | no | 1 | ? | unknown |  |
| `<widgets/listview.h>` | no | 1 | ? | unknown |  |
| `<widgets/scrollbar.h>` | no | 1 | ? | unknown |  |
| `<widgets/textbox.h>` | no | 1 | ? | unknown |  |
| `<xcb-imdkit/encoding.h>` | no | 1 | ? | unknown |  |
| `<xcb-imdkit/imclient.h>` | no | 1 | ? | unknown |  |
| `<xcb/randr.h>` | no | 1 | ? | unknown |  |
| `<xcb/xcb_atom.h>` | no | 1 | ? | unknown |  |
| `<xcb/xcb_aux.h>` | no | 1 | ? | unknown |  |
| `<xcb/xcb_cursor.h>` | no | 1 | ? | unknown |  |
| `<xcb/xinerama.h>` | no | 1 | ? | unknown |  |
| `<xcb/xkb.h>` | no | 1 | ? | unknown |  |
| `<xcb/xproto.h>` | no | 1 | ? | unknown |  |
| `<xkbcommon/xkbcommon-compose.h>` | no | 1 | ? | unknown |  |
| `<xkbcommon/xkbcommon-x11.h>` | no | 1 | ? | unknown |  |
| `<dirent.h>` (also `<fcntl.h>`, `<sys/stat.h>`, `<sys/types.h>`, `<unistd.h>`) | no | 4 | std::fs, std::process, libc | good |  |
| `<sys/mman.h>` | no | 1 | memmap2 | good |  |

### Pieces, in port order

- wave 0: `include/` (1276 lines)
- wave 0: `include/modes/` (89 lines)
- wave 0: `include/widgets/` (197 lines)
- wave 1: `config/` (180 lines)
- wave 1: `source/wayland/` (2383 lines)
- wave 1: `source/widgets/` (3642 lines)

### Decisions for the user

**D1** (shape): The app is a cli, entered through source/rofi.c. Port every entry point, in that shape?

> Yes: a clap front door (`app front-door`) takes every command over first

**D2** (keep source/): source/ stays in the source language (Depends on the target (hardware registers, memory layout, inline assembly): keep in C, since a recording on this machine cannot vouch for the target). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D3** (keep source/modes/): source/modes/ stays in the source language (Depends on the target (hardware registers, memory layout, inline assembly): keep in C, since a recording on this machine cannot vouch for the target). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D4** (keep source/xcb/): source/xcb/ stays in the source language (Depends on the target (hardware registers, memory layout, inline assembly): keep in C, since a recording on this machine cannot vouch for the target). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D5** (dependency <glib.h>): Which crate replaces <glib.h> (used by 8 units)?

> Research it against primary sources into rust/research/<glib.h>.md before porting what uses it

**D6** (dependency <pango/pango.h>): Which crate replaces <pango/pango.h> (used by 5 units)?

> Research it against primary sources into rust/research/<pango-pango.h>.md before porting what uses it

**D7** (dependency <xcb/xcb.h>): Which crate replaces <xcb/xcb.h> (used by 5 units)?

> Research it against primary sources into rust/research/<xcb-xcb.h>.md before porting what uses it

**D8** (dependency <cairo.h>): Which crate replaces <cairo.h> (used by 4 units)?

> Research it against primary sources into rust/research/<cairo.h>.md before porting what uses it

**D9** (dependency <xkbcommon/xkbcommon.h>): Which crate replaces <xkbcommon/xkbcommon.h> (used by 4 units)?

> Research it against primary sources into rust/research/<xkbcommon-xkbcommon.h>.md before porting what uses it

**D10** (dependency <config.h>): Which crate replaces <config.h> (used by 3 units)?

> Research it against primary sources into rust/research/<config.h>.md before porting what uses it

**D11** (dependency <gio/gio.h>): Which crate replaces <gio/gio.h> (used by 3 units)?

> Research it against primary sources into rust/research/<gio-gio.h>.md before porting what uses it

**D12** (dependency <glib/gstdio.h>): Which crate replaces <glib/gstdio.h> (used by 3 units)?

> Research it against primary sources into rust/research/<glib-gstdio.h>.md before porting what uses it

**D13** (dependency <gmodule.h>): Which crate replaces <gmodule.h> (used by 3 units)?

> Research it against primary sources into rust/research/<gmodule.h>.md before porting what uses it

**D14** (dependency <nkutils-bindings.h>): Which crate replaces <nkutils-bindings.h> (used by 3 units)?

> Research it against primary sources into rust/research/<nkutils-bindings.h>.md before porting what uses it

**D15** (dependency <rofi.h>): Which crate replaces <rofi.h> (used by 3 units)?

> Research it against primary sources into rust/research/<rofi.h>.md before porting what uses it

**D16** (dependency <wayland-client.h>): Which crate replaces <wayland-client.h> (used by 3 units)?

> Research it against primary sources into rust/research/<wayland-client.h>.md before porting what uses it

**D17** (dependency <xcb/xcb_ewmh.h>): Which crate replaces <xcb/xcb_ewmh.h> (used by 3 units)?

> Research it against primary sources into rust/research/<xcb-xcb_ewmh.h>.md before porting what uses it

**D18** (dependency <glib-unix.h>): Which crate replaces <glib-unix.h> (used by 2 units)?

> Research it against primary sources into rust/research/<glib-unix.h>.md before porting what uses it

**D19** (dependency <libgwater-wayland.h>): Which crate replaces <libgwater-wayland.h> (used by 2 units)?

> Research it against primary sources into rust/research/<libgwater-wayland.h>.md before porting what uses it

**D20** (dependency <libsn/sn.h>): Which crate replaces <libsn/sn.h> (used by 2 units)?

> Research it against primary sources into rust/research/<libsn-sn.h>.md before porting what uses it

**D21** (dependency <widgets/widget.h>): Which crate replaces <widgets/widget.h> (used by 2 units)?

> Research it against primary sources into rust/research/<widgets-widget.h>.md before porting what uses it

**D22** (dependency <xcb/xcb_icccm.h>): Which crate replaces <xcb/xcb_icccm.h> (used by 2 units)?

> Research it against primary sources into rust/research/<xcb-xcb_icccm.h>.md before porting what uses it

**D23** (dependency <xcb/xcb_keysyms.h>): Which crate replaces <xcb/xcb_keysyms.h> (used by 2 units)?

> Research it against primary sources into rust/research/<xcb-xcb_keysyms.h>.md before porting what uses it

**D24** (dependency <cairo-xcb.h>): Which crate replaces <cairo-xcb.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<cairo-xcb.h>.md before porting what uses it

**D25** (dependency <gdk-pixbuf/gdk-pixbuf.h>): Which crate replaces <gdk-pixbuf/gdk-pixbuf.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<gdk-pixbuf-gdk-pixbuf.h>.md before porting what uses it

**D26** (dependency <gio/gdesktopappinfo.h>): Which crate replaces <gio/gdesktopappinfo.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<gio-gdesktopappinfo.h>.md before porting what uses it

**D27** (dependency <gio/gunixinputstream.h>): Which crate replaces <gio/gunixinputstream.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<gio-gunixinputstream.h>.md before porting what uses it

**D28** (dependency <glob.h>): Which crate replaces <glob.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<glob.h>.md before porting what uses it

**D29** (dependency <helper.h>): Which crate replaces <helper.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<helper.h>.md before porting what uses it

**D30** (dependency <libgwater-xcb.h>): Which crate replaces <libgwater-xcb.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<libgwater-xcb.h>.md before porting what uses it

**D31** (dependency <mode.h>): Which crate replaces <mode.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<mode.h>.md before porting what uses it

**D32** (dependency <modes/modes.h>): Which crate replaces <modes/modes.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<modes-modes.h>.md before porting what uses it

**D33** (dependency <pango/pango-fontmap.h>): Which crate replaces <pango/pango-fontmap.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<pango-pango-fontmap.h>.md before porting what uses it

**D34** (dependency <pango/pangocairo.h>): Which crate replaces <pango/pangocairo.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<pango-pangocairo.h>.md before porting what uses it

**D35** (dependency <pwd.h>): Which crate replaces <pwd.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<pwd.h>.md before porting what uses it

**D36** (dependency <strings.h>): Which crate replaces <strings.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<strings.h>.md before porting what uses it

**D37** (dependency <sys/file.h>): Which crate replaces <sys/file.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<sys-file.h>.md before porting what uses it

**D38** (dependency <sysexits.h>): Which crate replaces <sysexits.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<sysexits.h>.md before porting what uses it

**D39** (dependency <theme.h>): Which crate replaces <theme.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<theme.h>.md before porting what uses it

**D40** (dependency <wayland-cursor.h>): Which crate replaces <wayland-cursor.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<wayland-cursor.h>.md before porting what uses it

**D41** (dependency <wayland-version.h>): Which crate replaces <wayland-version.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<wayland-version.h>.md before porting what uses it

**D42** (dependency <widgets/box.h>): Which crate replaces <widgets/box.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<widgets-box.h>.md before porting what uses it

**D43** (dependency <widgets/icon.h>): Which crate replaces <widgets/icon.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<widgets-icon.h>.md before porting what uses it

**D44** (dependency <widgets/listview.h>): Which crate replaces <widgets/listview.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<widgets-listview.h>.md before porting what uses it

**D45** (dependency <widgets/scrollbar.h>): Which crate replaces <widgets/scrollbar.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<widgets-scrollbar.h>.md before porting what uses it

**D46** (dependency <widgets/textbox.h>): Which crate replaces <widgets/textbox.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<widgets-textbox.h>.md before porting what uses it

**D47** (dependency <xcb-imdkit/encoding.h>): Which crate replaces <xcb-imdkit/encoding.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-imdkit-encoding.h>.md before porting what uses it

**D48** (dependency <xcb-imdkit/imclient.h>): Which crate replaces <xcb-imdkit/imclient.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-imdkit-imclient.h>.md before porting what uses it

**D49** (dependency <xcb/randr.h>): Which crate replaces <xcb/randr.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-randr.h>.md before porting what uses it

**D50** (dependency <xcb/xcb_atom.h>): Which crate replaces <xcb/xcb_atom.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-xcb_atom.h>.md before porting what uses it

**D51** (dependency <xcb/xcb_aux.h>): Which crate replaces <xcb/xcb_aux.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-xcb_aux.h>.md before porting what uses it

**D52** (dependency <xcb/xcb_cursor.h>): Which crate replaces <xcb/xcb_cursor.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-xcb_cursor.h>.md before porting what uses it

**D53** (dependency <xcb/xinerama.h>): Which crate replaces <xcb/xinerama.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-xinerama.h>.md before porting what uses it

**D54** (dependency <xcb/xkb.h>): Which crate replaces <xcb/xkb.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-xkb.h>.md before porting what uses it

**D55** (dependency <xcb/xproto.h>): Which crate replaces <xcb/xproto.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xcb-xproto.h>.md before porting what uses it

**D56** (dependency <xkbcommon/xkbcommon-compose.h>): Which crate replaces <xkbcommon/xkbcommon-compose.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xkbcommon-xkbcommon-compose.h>.md before porting what uses it

**D57** (dependency <xkbcommon/xkbcommon-x11.h>): Which crate replaces <xkbcommon/xkbcommon-x11.h> (used by 1 unit)?

> Research it against primary sources into rust/research/<xkbcommon-xkbcommon-x11.h>.md before porting what uses it

