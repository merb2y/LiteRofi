# LiteRofi — Domain Glossary (C Rofi -> Rust LiteRofi)

Grounded in upstream `davatorium/rofi` (`reference/rofi/include/` and `reference/rofi/doc/default_theme.rasi`).

## 1. Core Mode & Menu Lifecycle (`include/mode.h`, `include/settings.h`)
| Upstream C Symbol | Meaning in Rofi | Rust Target Name (`literofi`) |
| --- | --- | --- |
| `Mode` (`struct rofi_mode`) | Interface implemented by every switcher (`dmenu`, `run`, `drun`, `window`, etc.) | `literofi_core::mode::Mode` trait |
| `ModeMode` | Dialog state machine (`MODE_EXIT=1000`, `NEXT_DIALOG=1001`, `RELOAD_DIALOG=1002`, `PREVIOUS_DIALOG=1003`, `RESET_DIALOG=1004`) | `literofi_core::mode::ModeState` enum |
| `MenuReturn` | Outcome flags of user action (`MENU_OK`, `MENU_CANCEL`, `MENU_NEXT`, `MENU_PREVIOUS`, `MENU_CUSTOM_INPUT`, `MENU_ENTRY_DELETE`, `MENU_QUICK_SWITCH`, `MENU_CUSTOM_COMMAND`, `MENU_COMPLETE`, `MENU_CUSTOM_ACTION`) | `literofi_core::mode::MenuReturn` / `ActionOutcome` |
| `Settings` (`config`) | Global CLI + `configuration {}` runtime options (`modes`, `menu_font`, `show_icons`, `location`, `matching_method`, `sorting_method`, etc.) | `literofi_core::config::Config` |
| `MatchingMethod` | `MM_NORMAL` (0), `MM_REGEX` (1), `MM_GLOB` (2), `MM_FUZZY` (3), `MM_PREFIX` (4) | `literofi_core::matcher::MatchingMethod` |
| `SortingMethod` | `SORT_NORMAL` (0), `SORT_FZF` (1), `SORT_FZF_V2` (2) | `literofi_core::matcher::SortingMethod` |
| `WindowLocation` | 9-point anchor bitmask (`WL_CENTER`, `WL_NORTH`, `WL_EAST`, `WL_SOUTH`, `WL_WEST`, `WL_NORTH_WEST`, `WL_NORTH_EAST`, `WL_SOUTH_EAST`, `WL_SOUTH_WEST`) | `literofi_core::config::WindowLocation` |

## 2. Theme & Style System (`include/rofi-types.h`, `include/theme.h`, `lexer/theme-parser.y`)
| Upstream C Symbol | Meaning in Rofi | Rust Target Name (`literofi`) |
| --- | --- | --- |
| `ThemeWidget` | Rule tree node (`*`, `window`, `element selected.normal`, etc.) storing property map + children | `literofi_theme::ThemeNode` / `ThemeSheet` |
| `Property` / `PropertyType` | Typed value: `P_INTEGER`, `P_DOUBLE`, `P_STRING`, `P_BOOLEAN`, `P_COLOR`, `P_IMAGE`, `P_PADDING`, `P_LINK`, `P_POSITION`, `P_HIGHLIGHT`, `P_LIST`, `P_ORIENTATION`, `P_CURSOR`, `P_INHERIT` | `literofi_theme::PropertyValue` |
| `RofiDistance` / `RofiDistanceUnit` | Expression AST for CSS units (`px`, `mm`, `em`, `%`, `ch`) + `calc()` ops (`+`, `-`, `/`, `*`, `%`, `min`, `max`, `round`, `floor`, `ceil`) + line style (`solid`, `dash`) | `literofi_theme::Distance` / `DistanceExpr` |
| `RofiPadding` | 4-edge box distance (`top`, `right`, `bottom`, `left`) for `padding`, `margin`, `border`, `border-radius` | `literofi_theme::Padding` / `Edges<Distance>` |
| `ThemeColor` | RGBA float channels (`red`, `green`, `blue`, `alpha` in `0.0..=1.0`) | `literofi_theme::Color` |
| `RofiHighlightColorStyle` | `RofiHighlightStyle` bitflags (`BOLD`, `UNDERLINE`, `ITALIC`, `COLOR`, `STRIKETHROUGH`, `UPPERCASE`, `LOWERCASE`, `CAPITALIZE`) + optional color | `literofi_theme::HighlightStyle` |
| `RofiOrientation` | `ROFI_ORIENTATION_VERTICAL`, `ROFI_ORIENTATION_HORIZONTAL` | `literofi_theme::Orientation` |
| `RofiImage` | `url(...)` or `linear-gradient(...)` with scaling (`none`, `both`, `height`, `width`) & direction | `literofi_theme::ImageSource` |

## 3. Standard Rofi Widget Tree (`source/widgets/`, `doc/default_theme.rasi`)
| Widget Selector | Role in Rofi |
| --- | --- |
| `window` | Top-level popup container (`location`, `anchor`, `x-offset`, `y-offset`, `width`, `height`, `transparency`) |
| `mainbox` | Root flex box (`children: [ inputbar, message, listview, mode-switcher ]`) |
| `inputbar` | Input container (`children: [ prompt, textbox-prompt-colon, entry, num-filtered-rows, num-rows, case-indicator ]`) |
| `prompt` / `entry` / `case-indicator` | Prompt label, editable query box, case-sensitivity indicator |
| `message` / `textbox` | Optional `-mesg` / script message banner |
| `listview` | Scrollable N-column grid/list (`columns`, `lines`, `fixed-height`, `dynamic`, `cycle`, `scrollbar`, `reverse`) |
| `scrollbar` | Scroll thumb & handle track |
| `element` (`element-icon`, `element-text`) | Row item with states `{normal, selected, alternate}.{normal, urgent, active}` |
| `mode-switcher` / `button` | Clickable mode tabs (`sidebar-mode`) |
| `overlay` / `error-message` | Inline overlay / fatal mode error box |

## 4. `dmenu` / `script` Row Protocol (`include/modes/dmenuscriptshared.h`)
| Field / Flag | Meaning | Rust Field (`DmenuScriptEntry`) |
| --- | --- | --- |
| `\0icon\x1f<val>` | Icon name/path (+ fallback chain) | `icons: Vec<String>` |
| `\0display\x1f<val>` | Visible display override (distinct from returned `entry` text) | `display: Option<String>` |
| `\0meta\x1f<val>` | Hidden extra search keywords | `meta: Option<String>` |
| `\0info\x1f<val>` | Hidden payload exported via `ROFI_INFO` in `script` mode | `info: Option<String>` |
| `\0nonselectable\x1ftrue` | Row shown but cannot be accepted | `nonselectable: bool` |
| `\0permanent\x1ftrue` | Always shown regardless of filter | `permanent: bool` |
| `\0urgent\x1ftrue` / `\0active\x1ftrue` | Applies `.urgent` / `.active` theme state | `urgent: bool`, `active: bool` |

## 5. Keybindings & Exit Codes (`include/keyb.h`, `source/rofi.c`)
- Actions: `ACCEPT_ENTRY`, `ACCEPT_ALT`, `ACCEPT_CUSTOM`, `ACCEPT_CUSTOM_ALT`, `CANCEL`, `MODE_NEXT`, `MODE_PREVIOUS`, `MODE_COMPLETE`, `TOGGLE_CASE_SENSITIVITY`, `DELETE_ENTRY`, `ROW_UP`/`DOWN`/`LEFT`/`RIGHT`/`TAB`/`FIRST`/`LAST`, `PAGE_PREV`/`NEXT`, `ELEMENT_NEXT`/`PREV`, `SELECT_ELEMENT_1..10`, `ENTRY_HISTORY_UP`/`DOWN`, `MATCHER_UP`/`DOWN`, `TOGGLE_SORT`, `CUSTOM_1..19`.
- Exit codes (`dmenu` / launcher): `0` = accepted entry, `1` = cancelled / Esc, `10..28` = `CUSTOM_1..CUSTOM_19` (`10 + custom_index - 1`).
