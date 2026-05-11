# clidirstat

A terminal-native disk-usage analyzer with a **squarified treemap** side-by-side
with a navigable file tree, like WinDirStat for the CLI.

![clidirstat scanning ~/Downloads](docs/hero.png)

Every file becomes a coloured tile. Tiles are coloured by extension category
(blue = code, coral = video, orchid = images, sage = data, …), with **cushion
shading** on every tile so adjacent files separate visually even when they
share a category. Cache subtrees (`.cache`, `node_modules`, `target/`,
`__pycache__`, …) automatically de-emphasise to grey so the visual weight
matches "you probably want to clean this".

## Why another disk-usage tool?

The market splits in two:
- **Fast, flat output**: `dust`, `dirstat-rs`, `du`. Numbers, no spatial layout.
- **Interactive trees**: `ncdu`, `gdu`, `dua-cli`. Drill in, but you're scanning a list.

Only `diskonaut` offers a terminal treemap, and it's slow on large trees.
`clidirstat` is the first tool to combine **diskonaut's UX** with
**`dirstat-rs`-class speed** on macOS, by using the native `getattrlistbulk(2)`
syscall to pull ~200 directory entries *with full stat data* per syscall.

On `/usr/share` (~20 K files): **68 ms** for clidirstat vs. **163 ms** for
The benchmark reports the same total as `du -sh` in about one-third the time.

## Install

> v0.2 is **macOS** (Intel + Apple Silicon) and **Linux** (x86_64 + arm64).
> Windows is planned for v0.3.

### Shell one-liner

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/EricBriscoe/clidirstat/releases/latest/download/clidirstat-installer.sh | sh
```

Downloads the right binary for your platform, verifies its SHA-256, and drops
it into `~/.local/bin` (override with `--prefix=/usr/local`).

### Homebrew

```sh
brew install EricBriscoe/clidirstat/clidirstat
```

### Cargo

```sh
cargo install clidirstat
```

Or build the latest from this repo:

```sh
cargo install --git https://github.com/EricBriscoe/clidirstat
```

### Pre-built binaries

Grab a tarball directly from the [releases page](https://github.com/EricBriscoe/clidirstat/releases/latest).
Each release includes:

- `clidirstat-aarch64-apple-darwin.tar.xz`
- `clidirstat-x86_64-apple-darwin.tar.xz`
- `clidirstat-x86_64-unknown-linux-gnu.tar.xz`
- `clidirstat-aarch64-unknown-linux-gnu.tar.xz`
- a `dist-manifest.json` and signed checksum file

### From source

```sh
git clone https://github.com/EricBriscoe/clidirstat
cd clidirstat
cargo build --release
./target/release/clidirstat --help
```

Requires Rust 1.85+ (uses edition 2024). The `rust-toolchain.toml` will pull
the right version automatically with rustup.

## Usage

```sh
clidirstat                          # scan cwd, launch TUI
clidirstat ~/Library                # scan a specific path
clidirstat --no-tui ~/Downloads     # print top-20 to stdout
clidirstat --no-tui --json ~        # machine-readable
clidirstat --apparent-size ~        # use logical sizes instead of allocated
clidirstat --exclude '**/node_modules' .
clidirstat ~ --timeit               # benchmark: scan and print elapsed only
clidirstat ~ --theme light          # force light-bg palette
```

### Keys

| Key                | Action                              |
| ------------------ | ----------------------------------- |
| `j` `k` / arrows   | move selection                      |
| `l` / `→`          | expand directory                    |
| `h` / `←`          | collapse / go up                    |
| `Enter`            | zoom into selected directory        |
| `Esc` / `Backspace`| zoom out                            |
| `Tab`              | toggle focus (tree ↔ treemap)       |
| `a`                | toggle apparent vs allocated size   |
| `g` / `G`          | top / bottom                        |
| `?`                | help overlay                        |
| `q`                | quit                                |

The treemap fills in **during the scan** instead of waiting on a spinner.
Quitting before the scan completes soft-cancels the workers and the process
exits in under 200 ms.

## Benchmarks

`hyperfine` matrix on Apple Silicon, warm cache:

| Corpus              | clidirstat | `du -sh` |
| ------------------- | ---------- | -------- |
| `/usr/share` (267 MiB, 20 K files) | **68 ms** | 163 ms   |
| `~/.cargo`   (340 MiB, 20 K files) | **47 ms** | …         |

Full benchmark vs. `dirstat-rs` / `diskonaut` lands in `BENCHMARKS.md` for the
v0.2.0 release.

For a reproducible benchmark, the repo ships a fixture generator:

```sh
./scripts/make-demo-tree.sh /tmp/clidirstat-testing   # ~227 MiB synthetic tree
hyperfine --warmup 1 './target/release/clidirstat /tmp/clidirstat-testing --timeit'
```

## Design notes

- **Read-only.** No delete or move. Find the bytes, then `rm` yourself.
- **Sizes**: allocated (on-disk) by default, matching `du`. `--apparent-size`
  for logical bytes. APFS clones inflate apparent size; the default
  sidesteps this.
- **Mount points**: `--cross-filesystems` opts into crossing them; default
  stops at the boundary like GNU `du`'s default.
- **Symlinks**: never followed unless `--follow-symlinks`.
- **Hardlinks**: counted once via `(dev, ino)` dedup on the generic backend.
  The Darwin fast path skips the extra `fstatat` for nlink to keep the
  syscall count down; if you have a hardlink-heavy tree and want exact
  totals, set `CLIDIRSTAT_DISABLE_FASTPATH=1` (coming v0.3).
- **Time Machine snapshots**: `.com.apple.TimeMachine.*` directories skipped
  automatically; otherwise reported totals balloon by hundreds of GB.
- **Theme**: auto-detected from terminal background via OSC 11 (with a
  100 ms timeout + DA1 fallback so Apple Terminal doesn't hang). Override
  with `--theme dark|light|no-color`. Honours `NO_COLOR`.

## How it works

- **Scanner**: parallel walk via `rayon::scope` with a shared `RwLock<Tree>`.
  Per directory, the scanner takes the write lock once, batch-inserts all
  entries, and bubbles their sizes up through the parent chain so the
  tree's directory totals are always live. The UI takes a read lock per
  frame at ~5 fps.
- **Darwin fast path** (`src/scanner/darwin.rs`): one `getattrlistbulk(2)`
  call returns ~200 entries with name, type, inode, dev, apparent size, and
  allocated size in a single syscall. We parse the packed attribute buffer
  inline. Falls back to `readdir + lstat` per-directory on filesystems
  that return `ENOTSUP`.
- **Treemap**: squarified layout (Bruls/Huijsen/van Wijk 1999) rendered at
  **2× vertical resolution** via half-block glyphs (`▀` U+2580: top-half is
  the foreground colour, bottom-half is the background colour). Every leaf
  gets a top/left highlight + bottom/right shadow cushion so adjacent tiles
  always read as separate even when they share a category colour.
- **Theme**: `terminal-colorsaurus` queries OSC 11 once at startup before
  raw mode engages; per-category RGB tables are tuned for ~70 % L on dark
  backgrounds and ~45 % L on light backgrounds.

## Contributing

Bug reports and PRs welcome. For larger changes, please open an issue first
to discuss the approach.

```sh
cargo test --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## License

[MIT](./LICENSE)
