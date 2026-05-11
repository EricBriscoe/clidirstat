# Changelog

All notable changes to this project will be documented in this file. Format
follows [Keep a Changelog](https://keepachangelog.com/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.2.0] - unreleased

### Added
- **Live treemap during scan**: scanner runs on a background thread; UI
  reads through `Arc<RwLock<Tree>>` and repaints at ~5 fps as entries land,
  no waiting on a final spinner.
- **Recursive treemap rendering**: every file becomes its own colored tile
  (WinDirStat-style), not just direct children of the focused directory.
- **Half-block rendering**: 2× vertical resolution via `▀` glyphs.
- **Cushion shading**: every tile gets a top/left highlight + bottom/right
  shadow so adjacent same-category tiles read as separate.
- **Auto theme detection**: OSC 11 background query (`terminal-colorsaurus`)
  picks a dark or light palette automatically. Honours `NO_COLOR`. Override
  with `--theme dark|light|no-color`.
- **`--timeit` flag**: print only elapsed time + totals; pairs with
  `hyperfine` for repeatable benchmarks.
- **Soft-cancel on quit**: pressing `q` mid-scan signals workers to stop
  at the next directory boundary and joins them cleanly.

### Changed
- Treemap colour palette redesigned for 10 categories (Code, Image, Video,
  Audio, Docs, Archive, Binary, Data, Cache, Other) with separate dark/light
  variants.
- Cache subtrees (`.cache`, `node_modules`, `target/`, `__pycache__`,
  `.gradle`, etc.) automatically de-emphasised to grey.
- Pane borders removed; replaced with a 1-cell gutter for a cleaner look.
- Selected-tile outline now respects the half-block coordinate system so it
  always aligns to the tile boundary, even for 1-virtual-pixel tiles.

## [0.1.0] - 2026-05-10

### Added
- Initial release: `getattrlistbulk(2)` fast-path scanner on macOS, squarified
  treemap + tree-pane TUI, `--no-tui` and `--json` output modes, hardlink
  dedup, mount-point boundary, Time Machine snapshot skip.
