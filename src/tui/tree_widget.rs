use std::collections::HashSet;

use humansize::{BINARY, format_size};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::model::{NodeId, NodeKind, Tree};
use crate::util::truncate_to_width;

pub struct TreeView<'a> {
    pub tree: &'a Tree,
    pub root: NodeId,
    pub selected: NodeId,
    pub expanded: &'a HashSet<NodeId>,
    pub by_alloc: bool,
    pub focused: bool,
    pub scroll: u16,
}

pub struct TreeRow {
    pub id: NodeId,
    pub depth: u16,
    pub is_dir: bool,
    pub expanded: bool,
}

impl<'a> TreeView<'a> {
    /// Visible rows in display order under the current root.
    pub fn rows(&self) -> Vec<TreeRow> {
        let mut out = Vec::new();
        walk_visible(
            self.tree,
            self.root,
            0,
            self.expanded,
            self.by_alloc,
            &mut out,
        );
        out
    }
}

fn walk_visible(
    tree: &Tree,
    id: NodeId,
    depth: u16,
    expanded: &HashSet<NodeId>,
    by_alloc: bool,
    out: &mut Vec<TreeRow>,
) {
    let node = tree.get(id);
    let is_dir = matches!(node.kind, NodeKind::Dir);
    let exp = expanded.contains(&id);
    out.push(TreeRow {
        id,
        depth,
        is_dir,
        expanded: exp,
    });
    if is_dir && exp {
        for child in tree.sorted_children(id, by_alloc) {
            walk_visible(tree, child, depth + 1, expanded, by_alloc, out);
        }
    }
}

impl<'a> Widget for TreeView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        // Header row: pane title + focus indicator.
        let header_style = if self.focused {
            Style::default()
                .fg(Color::Rgb(255, 220, 100))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Rgb(140, 140, 150))
                .add_modifier(Modifier::BOLD)
        };
        let header = if self.focused { "▎ Tree" } else { "  Tree" };
        buf.set_string(area.x, area.y, header, header_style);

        let body = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(1),
        };
        if body.width == 0 || body.height == 0 {
            return;
        }
        let rows = self.rows();
        let start = self.scroll as usize;
        let visible = rows.iter().skip(start).take(body.height as usize);
        for (i, row) in visible.enumerate() {
            let node = self.tree.get(row.id);
            let marker = if row.is_dir {
                if row.expanded { "▾ " } else { "▸ " }
            } else {
                "  "
            };
            let indent: String = "  ".repeat(row.depth as usize);
            let size_str = format_size(node.size(self.by_alloc), BINARY);
            let size_w = size_str.chars().count() as u16;
            let name_max = body
                .width
                .saturating_sub(size_w + 2 + indent.chars().count() as u16 + 2)
                .max(1);
            let name = truncate_to_width(&node.name, name_max as usize);
            let y = body.y + i as u16;

            let (name_style, size_style) = if row.id == self.selected {
                let s = Style::default()
                    .bg(Color::Rgb(60, 60, 90))
                    .fg(Color::Rgb(250, 250, 255))
                    .add_modifier(Modifier::BOLD);
                (s, s)
            } else if row.is_dir {
                (
                    Style::default().fg(Color::Rgb(140, 200, 255)),
                    Style::default().fg(Color::Rgb(180, 180, 190)),
                )
            } else {
                (
                    Style::default().fg(Color::Rgb(200, 200, 210)),
                    Style::default().fg(Color::Rgb(140, 140, 150)),
                )
            };

            // Paint full-width row bg for selected line, so the highlight
            // reads as a bar rather than a fragment.
            if row.id == self.selected {
                for x in body.x..body.x + body.width {
                    buf[(x, y)].set_char(' ').set_style(name_style);
                }
            }
            let line = Line::from(vec![Span::styled(
                format!("{indent}{marker}{name}"),
                name_style,
            )]);
            buf.set_line(body.x, y, &line, body.width.saturating_sub(size_w + 1));
            let size_x = body.x + body.width.saturating_sub(size_w);
            buf.set_string(size_x, y, &size_str, size_style);
        }
    }
}
