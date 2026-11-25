//! Box, grid/listview, scrollbar, and window gravity layout engine
//! ported from `source/widgets/box.c` and `source/widgets/listview.c`.

use literofi_core::config::WindowLocation;
use literofi_theme::Orientation;

/// Integer-pixel/float rectangle in window or screen coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(self, px: f32, py: f32) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && px >= self.x
            && px < self.x + self.w
            && py >= self.y
            && py < self.y + self.h
    }

    pub fn inset(self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            x: self.x + left,
            y: self.y + top,
            w: (self.w - left - right).max(0.0),
            h: (self.h - top - bottom).max(0.0),
        }
    }
}

/// Child layout constraints fed into [`layout_box`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxChildSpec {
    pub enabled: bool,
    pub expand: bool,
    pub desired_w: i32,
    pub desired_h: i32,
}

/// Lays out children along `orientation` matching `source/widgets/box.c`.
///
/// Disabled children receive `Rect::ZERO`. Expanding children evenly share
/// remaining space using Rofi's progressive integer remainder distribution.
pub fn layout_box(
    container: Rect,
    orientation: Orientation,
    padding: (i32, i32, i32, i32), // top, right, bottom, left
    spacing: i32,
    children: &[BoxChildSpec],
) -> Vec<Rect> {
    let mut out = vec![Rect::ZERO; children.len()];
    let (pad_t, pad_r, pad_b, pad_l) = padding;
    let cx = container.x as i32;
    let cy = container.y as i32;
    let rem_width = ((container.w as i32) - pad_l - pad_r).max(0);
    let rem_height = ((container.h as i32) - pad_t - pad_b).max(0);

    let mut active_widgets = 0i32;
    let mut expanding_widgets = 0i32;
    let mut fixed_sum = 0i32;

    for c in children {
        if !c.enabled {
            continue;
        }
        active_widgets += 1;
        if c.expand {
            expanding_widgets += 1;
        } else {
            let fixed = match orientation {
                Orientation::Horizontal => c.desired_w.max(0),
                Orientation::Vertical => c.desired_h.max(0),
            };
            fixed_sum += fixed;
        }
    }

    if active_widgets == 0 {
        return out;
    }

    let total_spacing = (active_widgets - 1).max(0) * spacing;
    let max_size = fixed_sum + total_spacing;

    match orientation {
        Orientation::Horizontal => {
            let mut left = cx + pad_l;
            let mut rem = (rem_width - max_size).max(0);
            let mut exp_idx = 0i32;
            for (i, c) in children.iter().enumerate() {
                if !c.enabled {
                    continue;
                }
                if c.expand {
                    let w = rem / (expanding_widgets - exp_idx);
                    out[i] = Rect::new(
                        left as f32,
                        (cy + pad_t) as f32,
                        w as f32,
                        rem_height as f32,
                    );
                    left += w + spacing;
                    rem -= w;
                    exp_idx += 1;
                } else {
                    let w = c.desired_w.max(0);
                    out[i] = Rect::new(
                        left as f32,
                        (cy + pad_t) as f32,
                        w as f32,
                        rem_height as f32,
                    );
                    left += w + spacing;
                }
            }
        }
        Orientation::Vertical => {
            let mut top = cy + pad_t;
            let mut rem = (rem_height - max_size).max(0);
            let mut exp_idx = 0i32;
            for (i, c) in children.iter().enumerate() {
                if !c.enabled {
                    continue;
                }
                if c.expand {
                    let h = rem / (expanding_widgets - exp_idx);
                    out[i] = Rect::new((cx + pad_l) as f32, top as f32, rem_width as f32, h as f32);
                    top += h + spacing;
                    rem -= h;
                    exp_idx += 1;
                } else {
                    let h = c.desired_h.max(0);
                    out[i] = Rect::new((cx + pad_l) as f32, top as f32, rem_width as f32, h as f32);
                    top += h + spacing;
                }
            }
        }
    }

    out
}

/// Grid / listview layout specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListViewLayoutSpec {
    pub area: Rect,
    pub columns: usize,
    pub lines: usize,
    pub spacing_x: f32,
    pub spacing_y: f32,
    pub flow: Orientation,
    pub show_scrollbar: bool,
    pub scrollbar_width: f32,
    pub total_items: usize,
    pub scroll_offset: usize,
}

/// Positioned visible item cells and optional scrollbar geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct ListViewLayout {
    pub cells: Vec<(usize, Rect)>,
    pub scrollbar_track: Option<Rect>,
    pub scrollbar_thumb: Option<Rect>,
    pub page_size: usize,
}

/// Computes visible item cell rectangles and scrollbar track/thumb geometry.
pub fn layout_listview(spec: &ListViewLayoutSpec) -> ListViewLayout {
    let cols = spec.columns.max(1);
    let rows = spec.lines.max(1);
    let page_size = cols * rows;

    let need_sb = spec.show_scrollbar && spec.area.w > spec.scrollbar_width.max(2.0);
    let sb_reserve = if need_sb {
        spec.scrollbar_width.max(2.0) + spec.spacing_x.max(0.0)
    } else {
        0.0
    };

    let content_w = (spec.area.w - sb_reserve).max(0.0);
    let content_h = spec.area.h.max(0.0);

    let cell_w = if cols > 1 {
        ((content_w - spec.spacing_x * (cols - 1) as f32) / cols as f32).max(1.0)
    } else {
        content_w
    };
    let cell_h = if rows > 1 {
        ((content_h - spec.spacing_y * (rows - 1) as f32) / rows as f32).max(1.0)
    } else {
        content_h
    };

    let max_offset = spec.total_items.saturating_sub(1);
    let start = if spec.total_items == 0 {
        0
    } else {
        spec.scroll_offset.min(max_offset)
    };
    let visible_count = spec.total_items.saturating_sub(start).min(page_size);

    let mut cells = Vec::with_capacity(visible_count);
    for slot in 0..visible_count {
        let (col, row) = match spec.flow {
            Orientation::Vertical => (slot / rows, slot % rows),
            Orientation::Horizontal => (slot % cols, slot / cols),
        };
        let x = spec.area.x + col as f32 * (cell_w + spec.spacing_x);
        let y = spec.area.y + row as f32 * (cell_h + spec.spacing_y);
        cells.push((start + slot, Rect::new(x, y, cell_w, cell_h)));
    }

    let (scrollbar_track, scrollbar_thumb) = if need_sb {
        let track_x = spec.area.x + spec.area.w - spec.scrollbar_width.max(2.0);
        let track = Rect::new(
            track_x,
            spec.area.y,
            spec.scrollbar_width.max(2.0),
            content_h,
        );
        let total = spec.total_items.max(1) as f32;
        let visible_frac = if spec.total_items <= page_size {
            1.0
        } else {
            (page_size as f32 / total).clamp(0.08, 1.0)
        };
        let thumb_h = (content_h * visible_frac).max(20.0).min(content_h);
        let scrollable_items = (spec.total_items.saturating_sub(page_size)).max(1) as f32;
        let scroll_frac = if spec.total_items <= page_size {
            0.0
        } else {
            (start as f32 / scrollable_items).clamp(0.0, 1.0)
        };
        let thumb_y = spec.area.y + (content_h - thumb_h) * scroll_frac;
        let thumb = Rect::new(track.x, thumb_y, track.w, thumb_h);
        (Some(track), Some(thumb))
    } else {
        (None, None)
    };

    ListViewLayout {
        cells,
        scrollbar_track,
        scrollbar_thumb,
        page_size,
    }
}

/// Computes listview cell rectangles using Rofi's fixed natural `element_height`
/// (`source/widgets/listview.c:503-544, 669-697`) without stretching rows to fill
/// extra vertical space.
pub fn layout_listview_fixed_row_height(
    spec: &ListViewLayoutSpec,
    element_height: f32,
    fixed_columns: bool,
) -> ListViewLayout {
    let menu_cols = spec.columns.max(1);
    let content_h = spec.area.h.max(0.0);
    let elem_h = element_height.max(1.0);
    let rows = if content_h <= 0.0 {
        spec.lines.max(1)
    } else {
        ((content_h + spec.spacing_y.max(0.0)) / (elem_h + spec.spacing_y.max(0.0)))
            .floor()
            .max(1.0) as usize
    };
    let max_elements = menu_cols * rows;

    let cols = if !fixed_columns && spec.total_items < max_elements && spec.total_items > 0 {
        match spec.flow {
            Orientation::Vertical => spec.total_items.div_ceil(rows).clamp(1, menu_cols),
            Orientation::Horizontal => spec.total_items.min(menu_cols).max(1),
        }
    } else {
        menu_cols
    };
    let page_size = menu_cols * rows;

    let need_sb = spec.show_scrollbar && spec.area.w > spec.scrollbar_width.max(2.0);
    let sb_reserve = if need_sb {
        spec.scrollbar_width.max(2.0) + spec.spacing_x.max(0.0)
    } else {
        0.0
    };

    let content_w = (spec.area.w - sb_reserve).max(0.0);
    let cell_w = if cols > 1 {
        ((content_w - spec.spacing_x * (cols - 1) as f32) / cols as f32).max(1.0)
    } else {
        content_w
    };
    let cell_h = elem_h.min(content_h.max(1.0));

    let max_offset = spec.total_items.saturating_sub(1);
    let start = if spec.total_items == 0 {
        0
    } else {
        spec.scroll_offset.min(max_offset)
    };
    let visible_count = spec.total_items.saturating_sub(start).min(page_size);

    let mut cells = Vec::with_capacity(visible_count);
    for slot in 0..visible_count {
        let (col, row) = match spec.flow {
            Orientation::Vertical => (slot / rows, slot % rows),
            Orientation::Horizontal => (slot % cols, slot / cols),
        };
        let x = spec.area.x + col as f32 * (cell_w + spec.spacing_x);
        let y = spec.area.y + row as f32 * (cell_h + spec.spacing_y);
        cells.push((start + slot, Rect::new(x, y, cell_w, cell_h)));
    }

    let (scrollbar_track, scrollbar_thumb) = if need_sb {
        let track_x = spec.area.x + spec.area.w - spec.scrollbar_width.max(2.0);
        let track = Rect::new(
            track_x,
            spec.area.y,
            spec.scrollbar_width.max(2.0),
            content_h,
        );
        let total = spec.total_items.max(1) as f32;
        let visible_frac = if spec.total_items <= page_size {
            1.0
        } else {
            (page_size as f32 / total).clamp(0.08, 1.0)
        };
        let thumb_h = (content_h * visible_frac).max(20.0).min(content_h);
        let scrollable_items = (spec.total_items.saturating_sub(page_size)).max(1) as f32;
        let scroll_frac = if spec.total_items <= page_size {
            0.0
        } else {
            (start as f32 / scrollable_items).clamp(0.0, 1.0)
        };
        let thumb_y = spec.area.y + (content_h - thumb_h) * scroll_frac;
        let thumb = Rect::new(track.x, thumb_y, track.w, thumb_h);
        (Some(track), Some(thumb))
    } else {
        (None, None)
    };

    ListViewLayout {
        cells,
        scrollbar_track,
        scrollbar_thumb,
        page_size,
    }
}

fn location_factors(loc: WindowLocation) -> (f32, f32) {
    let bits = loc as u8;
    let fx = if bits & (WindowLocation::West as u8) != 0 {
        0.0
    } else if bits & (WindowLocation::East as u8) != 0 {
        1.0
    } else {
        0.5
    };
    let fy = if bits & (WindowLocation::North as u8) != 0 {
        0.0
    } else if bits & (WindowLocation::South as u8) != 0 {
        1.0
    } else {
        0.5
    };
    (fx, fy)
}

/// Computes popup window screen rectangle matching Rofi's `location` + `anchor` + offsets.
#[allow(clippy::too_many_arguments)]
pub fn compute_window_rect(
    workarea: Rect,
    width: f32,
    height: f32,
    location: WindowLocation,
    anchor: WindowLocation,
    x_offset: f32,
    y_offset: f32,
    fullscreen: bool,
) -> Rect {
    if fullscreen {
        return workarea;
    }
    let w = width.max(64.0).min(workarea.w);
    let h = height.max(14.0).min(workarea.h);
    let (lx, ly) = location_factors(location);
    let (ax, ay) = location_factors(anchor);

    let x = workarea.x + workarea.w * lx - w * ax + x_offset;
    let y = workarea.y + workarea.h * ly - h * ay + y_offset;
    Rect::new(x.round(), y.round(), w.round(), h.round())
}
