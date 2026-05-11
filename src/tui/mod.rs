use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use humansize::{BINARY, format_size};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::model::{NodeId, NodeKind, Tree};
use crate::scanner::ScanHandle;
use crate::theme::Theme;
use crate::util::truncate_to_width;

fn truncate_middle(s: &str, max_cells: usize) -> String {
    truncate_to_width(s, max_cells)
}

mod tree_widget;
mod treemap_widget;

use tree_widget::TreeView;
use treemap_widget::TreemapWidget;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn run(handle: ScanHandle, theme: Theme) -> Result<()> {
    let mut term = ratatui::try_init()?;
    let result = run_app(&mut term, handle, theme);
    ratatui::restore();
    result
}

fn run_app(term: &mut ratatui::DefaultTerminal, handle: ScanHandle, theme: Theme) -> Result<()> {
    let tree = handle.tree.clone();
    let cancel = handle.cancel.clone();
    let done = handle.done.clone();
    let mut app = App::new(tree, cancel.clone(), done, theme);
    loop {
        term.draw(|f| app.render(f))?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && app.handle_key(key.code, key.modifiers)
        {
            break;
        }
    }
    // Soft-cancel the scanner; wait for it to drain so the process exits clean.
    cancel.store(true, Ordering::Relaxed);
    handle.join()?;
    Ok(())
}

#[derive(PartialEq, Eq)]
enum Focus {
    Tree,
    Treemap,
}

struct App {
    tree: Arc<RwLock<Tree>>,
    done: Arc<AtomicBool>,
    selected: NodeId,
    expanded: HashSet<NodeId>,
    zoom_stack: Vec<NodeId>,
    focus: Focus,
    scroll: u16,
    show_help: bool,
    started_at: Instant,
    theme: Theme,
    /// Optional handle the App owns to issue cancellations when navigating
    /// far away from the scanned tree. Currently unused but reserved.
    _cancel: Arc<AtomicBool>,
}

impl App {
    fn new(
        tree: Arc<RwLock<Tree>>,
        cancel: Arc<AtomicBool>,
        done: Arc<AtomicBool>,
        theme: Theme,
    ) -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(NodeId::ROOT);
        Self {
            tree,
            done,
            selected: NodeId::ROOT,
            expanded,
            zoom_stack: Vec::new(),
            focus: Focus::Tree,
            scroll: 0,
            show_help: false,
            started_at: Instant::now(),
            theme,
            _cancel: cancel,
        }
    }

    fn current_root(&self) -> NodeId {
        self.zoom_stack.last().copied().unwrap_or(NodeId::ROOT)
    }

    fn render(&self, f: &mut ratatui::Frame) {
        let tree = self.tree.read().unwrap();
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        // Two panes with a 1-cell gutter between them; no boxed borders.
        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Length(1),
                Constraint::Min(20),
            ])
            .split(chunks[0]);

        let root = self.current_root();
        let treemap = TreemapWidget {
            tree: &tree,
            root,
            selected: self.selected,
            theme: self.theme,
        };
        f.render_widget(treemap, main[0]);

        // Draw the vertical gutter as a column of dim verticals.
        let gutter_rgb = match self.theme {
            Theme::Light => Color::Rgb(200, 200, 210),
            _ => Color::Rgb(60, 60, 70),
        };
        let gutter_style = Style::default().fg(gutter_rgb);
        for y in main[1].y..main[1].y + main[1].height {
            f.buffer_mut()[(main[1].x, y)]
                .set_char('│')
                .set_style(gutter_style);
        }

        let tree_view = TreeView {
            tree: &tree,
            root,
            selected: self.selected,
            expanded: &self.expanded,
            focused: self.focus == Focus::Tree,
            scroll: self.scroll,
        };
        f.render_widget(tree_view, main[2]);

        self.render_status_bar(&tree, chunks[1], f);

        if self.show_help {
            self.render_help(f, area);
        }
    }

    fn render_status_bar(&self, tree: &Tree, area: ratatui::layout::Rect, f: &mut ratatui::Frame) {
        let root = self.current_root();
        let total = tree.get(root).size;
        let sel = tree.get(self.selected);
        let sel_size = sel.size;
        let scanning = !self.done.load(Ordering::Acquire);
        let pct = if total > 0 {
            (sel_size as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        // Left: scan indicator + selected name + size + percent of root.
        let left = if scanning {
            let frame_idx =
                (self.started_at.elapsed().as_millis() / 100) as usize % SPINNER_FRAMES.len();
            format!(
                "  {} scanning…  {}  {}  {:.1}%",
                SPINNER_FRAMES[frame_idx],
                truncate_middle(&sel.name, 36),
                format_size(sel_size, BINARY),
                pct,
            )
        } else {
            format!(
                "  {}  {}  {:.1}%",
                truncate_middle(&sel.name, 40),
                format_size(sel_size, BINARY),
                pct,
            )
        };

        // Right: minimal keymap hints.
        let right = "q quit · ? help · Tab focus · Enter zoom  ";

        let (bar_bg, strong_fg, muted_fg) = match self.theme {
            Theme::Light => (
                Color::Rgb(232, 232, 236),
                Color::Rgb(30, 30, 40),
                Color::Rgb(100, 100, 110),
            ),
            _ => (
                Color::Rgb(20, 20, 24),
                Color::Rgb(230, 230, 235),
                Color::Rgb(140, 140, 150),
            ),
        };
        let line = Line::from(vec![
            Span::styled(left, Style::default().fg(strong_fg)),
            Span::raw(" "),
            Span::styled(right, Style::default().fg(muted_fg)),
        ]);
        let bar = Paragraph::new(line).style(Style::default().bg(bar_bg));
        f.render_widget(bar, area);
    }

    fn render_help(&self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let w = 50.min(area.width.saturating_sub(4));
        let h = 14.min(area.height.saturating_sub(4));
        let x = area.x + (area.width - w) / 2;
        let y = area.y + (area.height - h) / 2;
        let rect = ratatui::layout::Rect::new(x, y, w, h);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Help ")
            .style(Style::default().bg(Color::Black).fg(Color::White));
        let lines = vec![
            Line::from(""),
            Line::from("  j/↓     down"),
            Line::from("  k/↑     up"),
            Line::from("  h/←     collapse / go up"),
            Line::from("  l/→     expand"),
            Line::from("  Enter   zoom into selected dir"),
            Line::from("  Esc/Bs  zoom out"),
            Line::from("  Tab     toggle focus"),
            Line::from("  g/G     top / bottom"),
            Line::from("  ?       toggle this help"),
            Line::from("  q       quit"),
        ];
        let para = Paragraph::new(lines).block(block);
        f.render_widget(ratatui::widgets::Clear, rect);
        f.render_widget(para, rect);
    }

    /// Returns `true` if the app should exit.
    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if self.show_help {
            match code {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.show_help = false;
                }
                _ => {}
            }
            return false;
        }
        match code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Treemap,
                    Focus::Treemap => Focus::Tree,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Char('g') => self.move_to_top(),
            KeyCode::Char('G') => self.move_to_bottom(),
            KeyCode::Right | KeyCode::Char('l') => self.expand_selected(),
            KeyCode::Left | KeyCode::Char('h') => self.collapse_selected(),
            KeyCode::Enter => self.zoom_in(),
            KeyCode::Esc | KeyCode::Backspace => self.zoom_out(),
            _ => {}
        }
        false
    }

    fn visible_rows(&self) -> Vec<NodeId> {
        let tree = self.tree.read().unwrap();
        let tv = TreeView {
            tree: &tree,
            root: self.current_root(),
            selected: self.selected,
            expanded: &self.expanded,
            focused: false,
            scroll: 0,
        };
        tv.rows().into_iter().map(|r| r.id).collect()
    }

    fn move_selection(&mut self, delta: i32) {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return;
        }
        let idx = rows.iter().position(|&r| r == self.selected).unwrap_or(0);
        let next = (idx as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
        self.selected = rows[next];
        self.adjust_scroll(next);
    }

    fn adjust_scroll(&mut self, idx: usize) {
        let idx = idx as u16;
        if idx < self.scroll {
            self.scroll = idx;
        }
        if idx > self.scroll + 30 {
            self.scroll = idx - 30;
        }
    }

    fn move_to_top(&mut self) {
        let rows = self.visible_rows();
        if let Some(first) = rows.first() {
            self.selected = *first;
            self.scroll = 0;
        }
    }

    fn move_to_bottom(&mut self) {
        let rows = self.visible_rows();
        if let Some(last) = rows.last() {
            self.selected = *last;
            let idx = rows.len().saturating_sub(1) as u16;
            self.scroll = idx.saturating_sub(20);
        }
    }

    fn expand_selected(&mut self) {
        let is_dir = matches!(
            self.tree.read().unwrap().get(self.selected).kind,
            NodeKind::Dir
        );
        if is_dir {
            self.expanded.insert(self.selected);
        }
    }

    fn collapse_selected(&mut self) {
        if self.expanded.contains(&self.selected) {
            self.expanded.remove(&self.selected);
            return;
        }
        let parent = self.tree.read().unwrap().get(self.selected).parent;
        if let Some(p) = parent {
            self.selected = p;
        }
    }

    fn zoom_in(&mut self) {
        let is_dir = matches!(
            self.tree.read().unwrap().get(self.selected).kind,
            NodeKind::Dir
        );
        if is_dir && self.selected != self.current_root() {
            self.zoom_stack.push(self.selected);
            self.expanded.insert(self.selected);
        }
    }

    fn zoom_out(&mut self) {
        if let Some(prev) = self.zoom_stack.pop() {
            self.selected = prev;
        }
    }
}
