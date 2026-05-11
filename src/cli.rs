use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ThemeChoice {
    /// Auto-detect from the terminal (OSC 11 + DA1 fallback, ~100 ms timeout).
    Auto,
    /// Dark background: bright, ~70 % L hues.
    Dark,
    /// Light background: deeper, ~45 % L hues.
    Light,
    /// Grayscale only (also triggered by NO_COLOR=1).
    NoColor,
}

#[derive(Parser, Debug)]
#[command(
    name = "clidirstat",
    version,
    about = "Terminal disk usage analyzer with treemap visualization",
    long_about = "A WinDirStat-style disk usage analyzer that runs in your terminal. \
                  Shows a treemap visualization alongside a file tree to help you find \
                  what's eating your disk space."
)]
pub struct Args {
    /// Directory to scan (defaults to current directory).
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Show apparent (logical) file sizes instead of allocated (on-disk) sizes.
    #[arg(long)]
    pub apparent_size: bool,

    /// Cross mount points (default: stop at filesystem boundaries).
    #[arg(long)]
    pub cross_filesystems: bool,

    /// Follow symbolic links (default: off).
    #[arg(long)]
    pub follow_symlinks: bool,

    /// Exclude paths matching this glob. Can be repeated.
    #[arg(long, value_name = "GLOB")]
    pub exclude: Vec<String>,

    /// Skip TUI; print top entries to stdout.
    #[arg(long)]
    pub no_tui: bool,

    /// Benchmark mode: scan the path and print only elapsed time + totals.
    /// Pairs well with `hyperfine` for repeatable measurements.
    #[arg(long)]
    pub timeit: bool,

    /// Emit JSON (pairs with --no-tui).
    #[arg(long)]
    pub json: bool,

    /// Number of files to print in --no-tui mode.
    #[arg(long, default_value_t = 20)]
    pub top: usize,

    /// Worker threads (default: detected CPU count).
    #[arg(long, value_name = "N")]
    pub threads: Option<usize>,

    /// Color theme. `auto` queries the terminal background.
    #[arg(long, value_enum, default_value_t = ThemeChoice::Auto)]
    pub theme: ThemeChoice,
}
