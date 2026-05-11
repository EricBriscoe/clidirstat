//! WinDirStat-style treemap renderer.
//!
//! Renders **recursively to the leaf**; every file becomes a coloured tile,
//! coloured by extension category. Sub-directories are not given their own
//! fill; their boundaries are implied by per-depth darkening of the contained
//! leaves, which gives the WinDirStat "tubs of light" feel without burning
//! cells on borders.
//!
//! Uses **half-block** glyphs (`▀` U+2580) to double vertical resolution: each
//! terminal cell encodes two virtual pixels: the upper half via the
//! foreground colour, the lower half via the background colour.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect as TuiRect;
use ratatui::style::{Color, Modifier, Style};
// `Modifier` is used in overlay_label for bolding the path header below.
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthChar;

use crate::fs_categories::{self, Category};
use crate::model::{NodeId, NodeKind, Tree};
use crate::theme::Theme;
use crate::treemap::{Rect, squarify};

pub struct TreemapWidget<'a> {
    pub tree: &'a Tree,
    pub root: NodeId,
    pub selected: NodeId,
    pub by_alloc: bool,
    pub theme: Theme,
}

impl<'a> Widget for TreemapWidget<'a> {
    fn render(self, area: TuiRect, buf: &mut Buffer) {
        if area.width < 2 || area.height < 1 {
            return;
        }
        // Virtual canvas at 2× vertical resolution. Each pair of virtual rows
        // collapses into one terminal cell rendered with `▀`.
        let vw = area.width;
        let vh = area.height.saturating_mul(2);
        let bg = self.theme.canvas_bg();
        let mut canvas: Vec<(u8, u8, u8)> = vec![bg; vw as usize * vh as usize];
        // Per-pixel bevel: positive = brighten (highlight), negative = darken
        // (shadow). Every leaf adds a 1-virt-pixel highlight on its top/left
        // and a 1-virt-pixel shadow on its bottom/right, so adjacent tiles
        // always meet at a hard light→dark transition.
        let mut bevel: Vec<i16> = vec![0; vw as usize * vh as usize];

        let bounds = Rect::new(0, 0, vw, vh);
        let mut selected_rect: Option<Rect> = None;
        paint(
            self.tree,
            self.root,
            bounds,
            0,
            false,
            self.selected,
            self.by_alloc,
            self.theme,
            &mut canvas,
            &mut bevel,
            vw,
            vh,
            &mut selected_rect,
        );

        // Outline the selected tile at virtual-pixel resolution so it lines
        // up exactly with the underlying half-block tile, even when the tile
        // is only one or two virtual pixels tall.
        let mut outline = vec![false; vw as usize * vh as usize];
        if let Some(r) = selected_rect {
            mark_outline(&mut outline, vw, vh, r);
        }

        flush(
            &canvas,
            &bevel,
            &outline,
            self.theme.outline(),
            vw,
            vh,
            area,
            buf,
        );
        overlay_label(self.tree, self.root, area, buf, self.by_alloc, self.theme);

        if vw == 0 || vh == 0 {
            let msg = "(empty)";
            let x = area.x + area.width / 2 - (msg.len() as u16) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(x, y, msg, Style::default().fg(Color::DarkGray));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint(
    tree: &Tree,
    node_id: NodeId,
    rect: Rect,
    depth: u8,
    in_cache_subtree: bool,
    selected: NodeId,
    by_alloc: bool,
    theme: Theme,
    canvas: &mut [(u8, u8, u8)],
    bevel: &mut [i16],
    vw: u16,
    vh: u16,
    selected_rect: &mut Option<Rect>,
) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    if node_id == selected {
        *selected_rect = Some(rect);
    }

    let node = tree.get(node_id);
    match node.kind {
        NodeKind::Dir => {
            let in_cache = in_cache_subtree || fs_categories::is_cache_dir(&node.name);
            let children: Vec<(u64, NodeId)> = tree
                .sorted_children(node_id, by_alloc)
                .into_iter()
                .filter_map(|id| {
                    let s = tree.get(id).size(by_alloc);
                    if s == 0 { None } else { Some((s, id)) }
                })
                .collect();
            if children.is_empty() {
                fill_rect(
                    canvas,
                    vw,
                    vh,
                    rect,
                    depth_color(theme, Category::Other, depth, 0.0),
                );
                add_bevel(bevel, vw, vh, rect, 60);
                return;
            }
            // Directory perimeter gets an extra bevel "ridge" so nested
            // directories are visually grouped, on top of each leaf's bevel.
            if depth > 0 {
                add_bevel(bevel, vw, vh, rect, 35);
            }
            let tiles = squarify(rect, &children);
            for tile in tiles.iter() {
                paint(
                    tree,
                    tile.id,
                    tile.rect,
                    depth.saturating_add(1),
                    in_cache,
                    selected,
                    by_alloc,
                    theme,
                    canvas,
                    bevel,
                    vw,
                    vh,
                    selected_rect,
                );
            }
        }
        NodeKind::File | NodeKind::Symlink | NodeKind::Other => {
            let cat = if in_cache_subtree {
                Category::Cache
            } else {
                fs_categories::classify(&node.name)
            };
            // Mild per-depth shading still helps on the bevel-free interior
            // pixels of large tiles.
            let color = depth_color(theme, cat, depth, 0.0);
            fill_rect(canvas, vw, vh, rect, color);
            add_bevel(bevel, vw, vh, rect, 60);
        }
    }
}

/// Paint a 1-virtual-pixel-thick bevel around `rect`: top + left edges
/// lighten by `+strength`, bottom + right edges darken by `-strength`.
/// Accumulates per-pixel and is clamped at flush time. Skipped on tiles too
/// small to fit a meaningful bevel.
fn add_bevel(bevel: &mut [i16], vw: u16, vh: u16, rect: Rect, strength: i16) {
    if rect.w < 2 || rect.h < 2 || rect.x >= vw || rect.y >= vh {
        return;
    }
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x.saturating_add(rect.w).saturating_sub(1).min(vw - 1);
    let y1 = rect.y.saturating_add(rect.h).saturating_sub(1).min(vh - 1);
    let stride = vw as usize;
    // Top edge: lighten.
    let top_row = y0 as usize * stride;
    for x in x0..=x1 {
        let i = top_row + x as usize;
        bevel[i] = bevel[i].saturating_add(strength);
    }
    // Left edge: lighten.
    for y in y0..=y1 {
        let i = y as usize * stride + x0 as usize;
        bevel[i] = bevel[i].saturating_add(strength);
    }
    // Bottom edge: darken.
    if y1 > y0 {
        let bot_row = y1 as usize * stride;
        for x in x0..=x1 {
            let i = bot_row + x as usize;
            bevel[i] = bevel[i].saturating_sub(strength);
        }
    }
    // Right edge: darken.
    if x1 > x0 {
        for y in y0..=y1 {
            let i = y as usize * stride + x1 as usize;
            bevel[i] = bevel[i].saturating_sub(strength);
        }
    }
}

fn depth_color(theme: Theme, cat: Category, depth: u8, bias: f32) -> (u8, u8, u8) {
    // On dark backgrounds, darken deeper levels for nesting depth cues.
    // On light backgrounds, *lighten* deeper levels for the same effect
    // (we want depth to recede toward the bg).
    let (r, g, b) = theme.rgb(cat);
    let depth_step = depth as f32 * 0.06;
    let factor = match theme {
        Theme::Light => (1.0 + depth_step).min(1.35) + bias,
        _ => (1.0 - depth_step).max(0.60) + bias,
    };
    let factor = factor.clamp(0.40, 1.45);
    let clamp = |v: f32| v.clamp(0.0, 255.0) as u8;
    (
        clamp(r as f32 * factor),
        clamp(g as f32 * factor),
        clamp(b as f32 * factor),
    )
}

fn fill_rect(canvas: &mut [(u8, u8, u8)], vw: u16, vh: u16, rect: Rect, color: (u8, u8, u8)) {
    let x_end = (rect.x + rect.w).min(vw);
    let y_end = (rect.y + rect.h).min(vh);
    for y in rect.y..y_end {
        let row_off = y as usize * vw as usize;
        for x in rect.x..x_end {
            canvas[row_off + x as usize] = color;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn flush(
    canvas: &[(u8, u8, u8)],
    bevel: &[i16],
    outline: &[bool],
    outline_color: (u8, u8, u8),
    vw: u16,
    vh: u16,
    area: TuiRect,
    buf: &mut Buffer,
) {
    let term_h = vh / 2;
    for ty in 0..term_h {
        let top_row = ty as usize * 2 * vw as usize;
        let bot_row = (ty as usize * 2 + 1) * vw as usize;
        for tx in 0..vw {
            let top_i = top_row + tx as usize;
            let bot_i = bot_row + tx as usize;
            let top = if outline[top_i] {
                outline_color
            } else {
                apply_bevel(canvas[top_i], bevel[top_i])
            };
            let bot = if outline[bot_i] {
                outline_color
            } else {
                apply_bevel(canvas[bot_i], bevel[bot_i])
            };
            let cell = &mut buf[(area.x + tx, area.y + ty)];
            cell.set_char('▀');
            cell.set_fg(Color::Rgb(top.0, top.1, top.2));
            cell.set_bg(Color::Rgb(bot.0, bot.1, bot.2));
        }
    }
}

fn apply_bevel(color: (u8, u8, u8), bevel: i16) -> (u8, u8, u8) {
    if bevel == 0 {
        return color;
    }
    // Clamp accumulated bevel so deep stacks don't oversaturate, then scale
    // into a ±28 % brightness factor.
    let b = bevel.clamp(-90, 90);
    let factor = 1.0 + (b as f32) * 0.0031;
    let clamp = |v: f32| v.clamp(0.0, 255.0) as u8;
    (
        clamp(color.0 as f32 * factor),
        clamp(color.1 as f32 * factor),
        clamp(color.2 as f32 * factor),
    )
}

/// Paint a 1-virtual-pixel-thick perimeter around `rect` into the outline
/// mask. The mask is rendered at the same half-cell resolution as the tile
/// canvas, so a single-virtual-row tile gets a single-half-cell outline.
fn mark_outline(outline: &mut [bool], vw: u16, vh: u16, rect: Rect) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect
        .x
        .saturating_add(rect.w)
        .saturating_sub(1)
        .min(vw.saturating_sub(1));
    let y1 = rect
        .y
        .saturating_add(rect.h)
        .saturating_sub(1)
        .min(vh.saturating_sub(1));
    if x0 >= vw || y0 >= vh {
        return;
    }
    let set = |o: &mut [bool], x: u16, y: u16| {
        if x < vw && y < vh {
            o[y as usize * vw as usize + x as usize] = true;
        }
    };
    // Top + bottom edges.
    for x in x0..=x1 {
        set(outline, x, y0);
        set(outline, x, y1);
    }
    // Left + right edges (corners overlap, harmless).
    for y in y0..=y1 {
        set(outline, x0, y);
        set(outline, x1, y);
    }
}

/// Draws the currently-focused root path along the top edge of the pane.
fn overlay_label(
    tree: &Tree,
    root: NodeId,
    area: TuiRect,
    buf: &mut Buffer,
    by_alloc: bool,
    theme: Theme,
) {
    if area.width < 6 {
        return;
    }
    let node = tree.get(root);
    let label = format!(
        " {} · {} ",
        tree.path_of(root).display(),
        humansize::format_size(node.size(by_alloc), humansize::BINARY),
    );
    let max = area.width as usize;
    let trimmed = trim_label(&label, max);
    let (fg, bg) = match theme {
        Theme::Light => ((20, 20, 24), (240, 240, 244)),
        _ => ((240, 240, 240), (28, 28, 32)),
    };
    let style = Style::default()
        .fg(Color::Rgb(fg.0, fg.1, fg.2))
        .bg(Color::Rgb(bg.0, bg.1, bg.2))
        .add_modifier(Modifier::BOLD);
    buf.set_string(area.x, area.y, &trimmed, style);
}

fn trim_label(s: &str, max_cells: usize) -> String {
    let total: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total <= max_cells {
        return format!("{s:width$}", width = max_cells);
    }
    if max_cells == 0 {
        return String::new();
    }
    // Keep the right side (filename) visible; ellipsize the middle.
    let chars: Vec<char> = s.chars().collect();
    let head = 1;
    let tail = max_cells.saturating_sub(2);
    let mut out = String::new();
    out.extend(chars.iter().take(head));
    out.push('…');
    out.extend(chars.iter().rev().take(tail).rev());
    out
}
