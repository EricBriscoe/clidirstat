# CLAUDE.md: project context for Claude sessions

`clidirstat` is a terminal disk-usage analyzer with a WinDirStat-style
recursive treemap. Public root readme is intentionally minimal; the design
context that doesn't belong in either user-facing docs or code comments
lives here.

## What it competes with

- `dust`, `dirstat-rs`: fast but flat. No spatial layout.
- `ncdu`, `gdu`, `dua-cli`: interactive tree view, no treemap.
- `diskonaut`: only existing TUI treemap; slow on large trees, sparse maintenance.

The wedge: **diskonaut's UX with `dirstat-rs`-class speed, by using macOS's
`getattrlistbulk(2)` to batch directory reads with full stat data**.

## Invariants

- **Read-only.** No delete, move, or rename operations. Users `rm` themselves
  after finding offenders. If anyone proposes adding mutation, push back.
  This is a deliberate scope choice, not an oversight.
- **Allocated size by default.** Matches `du`; `--apparent-size` flips it.
  APFS clones / compressed files make apparent diverge wildly from
  allocated, and "what's on my disk" is the allocated answer.
- **Stop at mount-point boundaries by default** (`--cross-filesystems` to opt
  in). Network mounts, time-machine snapshots, and `/System` firmlinks
  otherwise wreck both runtime and reported totals.
- **Symlinks are never followed** unless `--follow-symlinks`.
- **Hardlinks are deduped by `(dev, ino)`** on the generic backend. The
  Darwin fast path skips the extra `fstatat` to keep syscall count down, so
  a hardlink-heavy tree reports the inflated total there. If we ever ship a
  `CLIDIRSTAT_DISABLE_FASTPATH` env var, route through `scanner/generic.rs`.
- **Time Machine snapshots are silently skipped** (`.com.apple.TimeMachine.*`,
  `.MobileBackups`). Without this, root scans report hundreds of GB of
  phantom space. See `scanner/mod.rs::is_time_machine_snapshot`.
- **Theme detection runs once at startup, before raw mode.** OSC 11 needs
  stdin/stdout in cooked mode. See `src/theme.rs::Theme::detect`.

## Architecture map

| Concern                                  | File                                          |
| ---------------------------------------- | --------------------------------------------- |
| CLI flag parsing                         | `src/cli.rs`                                  |
| Tree arena, size aggregation, `bump_ancestors` | `src/model.rs`                          |
| Scanner dispatch + RwLock + cancel       | `src/scanner/mod.rs`                          |
| macOS `getattrlistbulk(2)` fast path     | `src/scanner/darwin.rs`                       |
| Portable `readdir` fallback              | `src/scanner/generic.rs`                      |
| Squarified treemap layout (pure fn)      | `src/treemap.rs`                              |
| Treemap renderer (half-block + cushion)  | `src/tui/treemap_widget.rs`                   |
| File-tree pane                           | `src/tui/tree_widget.rs`                      |
| Event loop + App state                   | `src/tui/mod.rs`                              |
| Extension → Category buckets             | `src/fs_categories.rs`                        |
| Dark/light/no-color RGB tables           | `src/theme.rs`                                |

## Rendering pipeline (the part not obvious from any single file)

1. Scanner pushes nodes into `Arc<RwLock<Tree>>`. For non-dir leaves it
   calls `Tree::bump_ancestors` so directory totals are always live.
2. UI thread takes a read lock per frame at ~5 fps.
3. `TreemapWidget::render` builds a virtual canvas at `width × (height*2)`.
4. `paint` recurses to leaves, filling pixels with extension-category RGB,
   and stamping bevel highlights/shadows along every tile's edges.
5. `flush` collapses each pair of virtual rows into one terminal cell via
   `▀` (top-half = fg, bottom-half = bg), applying the bevel as a brightness
   multiplier.
6. Outline (selected tile) is painted as a parallel `Vec<bool>` mask at
   virtual-pixel resolution and overrides the cell colour during flush,
   so it aligns to the half-cell where the tile is rendered.

## Performance budget

- v0.2 target: **within 1.3× of `dirstat-rs`** on a 1 M-file corpus,
  **≥3× faster than diskonaut** on the same.
- Real numbers on Apple Silicon, warm cache:
  - `/usr/share` (20 K files): clidirstat 68 ms, `du -sh` 163 ms.
  - `~/.cargo` (20 K files): clidirstat 47 ms.
- Profile via `cargo build --release && hyperfine --warmup 1 './target/release/clidirstat <path> --timeit'`.
- Hot loops: `Tree::sorted_children` (clones + sorts per render frame);
  squarify recursion in `treemap_widget::paint`; the per-pixel bevel
  multiplication in `flush`. Don't add allocations to these without
  benchmarking.

## Things to never do

- **Don't follow symlinks by default.** Following them can recurse forever.
- **Don't add transitively-cached state to `Tree`** that the scanner doesn't
  maintain incrementally. Either bump it in `bump_ancestors` or compute it
  fresh in the read path.
- **Don't block on terminal queries after raw mode is engaged.** OSC
  responses come in over the same stdin that the event loop is reading.
- **Don't trust file extension as the only category signal in a cache
  subtree.** `is_cache_dir` short-circuits to `Category::Cache` regardless
  of extension so a 4 GB `.dylib` under `target/` doesn't dominate the
  visual hierarchy.

## Release pipeline

- `dist-workspace.toml` configures cargo-dist v0.31; it is currently scoped to
  the `shell` installer only (curl-pipe). No Homebrew tap; users wanting
  brew can re-enable it by adding `tap = "..."` + `publish-jobs = ["homebrew"]`
  to dist-workspace.toml and re-running `dist generate`.
- `.github/workflows/release.yml` is generated by `dist generate`. Don't
  hand-edit it.
- Bumping the version: edit `Cargo.toml`, update `CHANGELOG.md`,
  `git tag v<N>`, `git push --tags`.
- `cargo install clidirstat` requires a separate `cargo publish`; it isn't
  automated yet.
