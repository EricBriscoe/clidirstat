//! Squarified treemap layout with terminal cell aspect-ratio correction.
//!
//! Cells are ~2:1 tall in most terminals. The algorithm internally scales the
//! short edge by 2.0 when the row runs vertically so the *visual* output is
//! roughly squared; we then map back to integer cell coords for rendering.

use crate::model::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }
    pub fn area(&self) -> u32 {
        self.w as u32 * self.h as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub rect: Rect,
    pub id: NodeId,
}

const CELL_ASPECT: f64 = 2.0;

/// Layout `items` (weight, id) into `bounds`. Items must be sorted descending
/// by weight. Zero-weight items are dropped to keep ratios finite.
pub fn squarify(bounds: Rect, items: &[(u64, NodeId)]) -> Vec<Tile> {
    if bounds.w == 0 || bounds.h == 0 {
        return Vec::new();
    }
    let items: Vec<(u64, NodeId)> = items.iter().copied().filter(|(w, _)| *w > 0).collect();
    if items.is_empty() {
        return Vec::new();
    }
    let total: u128 = items.iter().map(|(w, _)| *w as u128).sum();

    let mut tiles = Vec::with_capacity(items.len());
    let mut remaining = bounds;
    let mut rem_total = total;
    let mut idx = 0;

    while idx < items.len() && remaining.w > 0 && remaining.h > 0 && rem_total > 0 {
        let row_end = pick_row(&items[idx..], remaining, rem_total);
        let row = &items[idx..idx + row_end];
        layout_row(row, &mut remaining, rem_total, &mut tiles);
        let row_sum: u128 = row.iter().map(|(w, _)| *w as u128).sum();
        rem_total = rem_total.saturating_sub(row_sum);
        idx += row_end;
    }

    tiles
}

fn pick_row(items: &[(u64, NodeId)], rect: Rect, total: u128) -> usize {
    let short = rect.w.min(rect.h) as f64;
    if short == 0.0 {
        return items.len();
    }
    let area_per_weight = (rect.area() as f64) / (total as f64);

    // For vertical rows, scale short by CELL_ASPECT so visual squareness is
    // preferred (terminal cells are ~2× taller than wide).
    let s = if rect.w <= rect.h {
        short
    } else {
        short * CELL_ASPECT
    };

    let mut sum = 0u128;
    let mut min_w_weight = u64::MAX;
    let mut best_worst = f64::INFINITY;

    for (i, (w, _)) in items.iter().enumerate() {
        sum = sum.saturating_add(*w as u128);
        min_w_weight = min_w_weight.min(*w);
        let row_area = sum as f64 * area_per_weight;
        let max_w = *w as f64 * area_per_weight;
        let min_w = min_w_weight as f64 * area_per_weight;
        // Classic squarify worst-ratio:
        //   max( s² · max_w / row_area² ,  row_area² / (s² · min_w) )
        let worst =
            (s * s * max_w / (row_area * row_area)).max(row_area * row_area / (s * s * min_w));
        if worst > best_worst {
            return i;
        }
        best_worst = worst;
    }
    items.len()
}

fn layout_row(row: &[(u64, NodeId)], rect: &mut Rect, total: u128, out: &mut Vec<Tile>) {
    if row.is_empty() || rect.w == 0 || rect.h == 0 {
        return;
    }
    let row_sum: u128 = row.iter().map(|(w, _)| *w as u128).sum();
    let area_per_weight = (rect.area() as f64) / (total as f64);
    let row_area = row_sum as f64 * area_per_weight;

    if rect.w <= rect.h {
        // Row runs left-to-right; height = row_area / w.
        let raw_h = (row_area / rect.w as f64).round() as u16;
        let row_h = raw_h.max(1).min(rect.h);
        let mut x = rect.x;
        let mut remaining_w = rect.w;
        let n = row.len() as u16;
        for (i, (wgt, id)) in row.iter().enumerate() {
            let mine_area = *wgt as f64 * area_per_weight;
            let reserved_for_others = n.saturating_sub(i as u16 + 1);
            let max_w_here = remaining_w.saturating_sub(reserved_for_others).max(1);
            let w = if i + 1 == row.len() {
                remaining_w
            } else {
                (mine_area / row_h as f64).round() as u16
            };
            let w = w.max(1).min(max_w_here);
            if remaining_w == 0 {
                break;
            }
            out.push(Tile {
                rect: Rect::new(x, rect.y, w, row_h),
                id: *id,
            });
            x = x.saturating_add(w);
            remaining_w = remaining_w.saturating_sub(w);
        }
        rect.y = rect.y.saturating_add(row_h);
        rect.h = rect.h.saturating_sub(row_h);
    } else {
        // Row runs top-to-bottom; width = row_area / h.
        let raw_w = (row_area / rect.h as f64).round() as u16;
        let row_w = raw_w.max(1).min(rect.w);
        let mut y = rect.y;
        let mut remaining_h = rect.h;
        let n = row.len() as u16;
        for (i, (wgt, id)) in row.iter().enumerate() {
            let mine_area = *wgt as f64 * area_per_weight;
            let reserved_for_others = n.saturating_sub(i as u16 + 1);
            let max_h_here = remaining_h.saturating_sub(reserved_for_others).max(1);
            let h = if i + 1 == row.len() {
                remaining_h
            } else {
                (mine_area / row_w as f64).round() as u16
            };
            let h = h.max(1).min(max_h_here);
            if remaining_h == 0 {
                break;
            }
            out.push(Tile {
                rect: Rect::new(rect.x, y, row_w, h),
                id: *id,
            });
            y = y.saturating_add(h);
            remaining_h = remaining_h.saturating_sub(h);
        }
        rect.x = rect.x.saturating_add(row_w);
        rect.w = rect.w.saturating_sub(row_w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(n: u32) -> NodeId {
        NodeId(n)
    }

    #[test]
    fn empty_inputs() {
        assert!(squarify(Rect::new(0, 0, 0, 10), &[(1, nid(1))]).is_empty());
        assert!(squarify(Rect::new(0, 0, 10, 0), &[(1, nid(1))]).is_empty());
        assert!(squarify(Rect::new(0, 0, 10, 10), &[]).is_empty());
        assert!(squarify(Rect::new(0, 0, 10, 10), &[(0, nid(1))]).is_empty());
    }

    #[test]
    fn rows_fill_bounds_exactly() {
        let tiles = squarify(
            Rect::new(0, 0, 40, 20),
            &[(50, nid(1)), (30, nid(2)), (20, nid(3))],
        );
        let total_area: u32 = tiles.iter().map(|t| t.rect.area()).sum();
        assert_eq!(total_area, 40 * 20);
        assert_eq!(tiles.len(), 3);
        // No overlap: paint a grid and assert each cell painted exactly once.
        let mut grid = vec![0u8; 40 * 20];
        for t in &tiles {
            for dy in 0..t.rect.h {
                for dx in 0..t.rect.w {
                    let idx = (t.rect.y + dy) as usize * 40 + (t.rect.x + dx) as usize;
                    grid[idx] += 1;
                }
            }
        }
        assert!(grid.iter().all(|&c| c == 1), "overlap or gap detected");
    }

    #[test]
    fn weights_proportional() {
        let tiles = squarify(Rect::new(0, 0, 100, 50), &[(80, nid(1)), (20, nid(2))]);
        let area1 = tiles.iter().find(|t| t.id == nid(1)).unwrap().rect.area();
        let area2 = tiles.iter().find(|t| t.id == nid(2)).unwrap().rect.area();
        assert_eq!(area1 + area2, 100 * 50);
        let ratio = area1 as f64 / area2 as f64;
        assert!(
            (3.5..=4.5).contains(&ratio),
            "expected ~4:1 ratio for 80:20 weights, got {ratio}"
        );
    }

    #[test]
    fn zero_among_non_zero_does_not_panic() {
        let tiles = squarify(
            Rect::new(0, 0, 40, 20),
            &[(100, nid(1)), (0, nid(2)), (50, nid(3))],
        );
        assert!(tiles.iter().all(|t| t.id != nid(2)));
        assert!(!tiles.is_empty());
    }

    #[test]
    fn narrow_rect_does_not_panic() {
        let _ = squarify(Rect::new(0, 0, 1, 10), &[(10, nid(1)), (5, nid(2))]);
        let _ = squarify(Rect::new(0, 0, 10, 1), &[(10, nid(1)), (5, nid(2))]);
        let _ = squarify(Rect::new(0, 0, 1, 1), &[(10, nid(1)), (5, nid(2))]);
    }
}
