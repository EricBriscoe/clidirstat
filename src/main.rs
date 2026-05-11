use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use globset::{Glob, GlobSetBuilder};
use humansize::{BINARY, format_size};

use clidirstat::cli::{self, ThemeChoice};
use clidirstat::model::{NodeId, NodeKind, Tree};
use clidirstat::scanner::{self, ScanOptions};
use clidirstat::theme::Theme;
use clidirstat::tui;

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("clidirstat: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<()> {
    let args = cli::Args::parse();

    if let Some(n) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .context("init rayon")?;
    }

    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    let excludes = build_excludes(&args.exclude)?;

    let opts = ScanOptions {
        one_file_system: !args.cross_filesystems,
        follow_symlinks: args.follow_symlinks,
        excludes,
    };

    if args.timeit {
        let started = Instant::now();
        let tree = scanner::scan(&path, opts).context("scan failed")?;
        let elapsed = started.elapsed();
        let files = tree.len().saturating_sub(1);
        let total = tree.get(NodeId::ROOT).size;
        let errs = if tree.error_count > 0 {
            format!("  ({} errors)", tree.error_count)
        } else {
            String::new()
        };
        println!(
            "{:.3?}  {}  {} files{errs}",
            elapsed,
            format_size(total, BINARY),
            files,
        );
        return Ok(());
    }

    if args.no_tui {
        let started = Instant::now();
        let tree = scanner::scan(&path, opts).context("scan failed")?;
        let elapsed = started.elapsed();
        if args.json {
            print_json(&tree, args.top)?;
        } else {
            print_summary(&tree, args.top, elapsed);
        }
    } else {
        // Detect theme BEFORE entering raw mode (OSC queries need stdin/stdout).
        let theme = match args.theme {
            ThemeChoice::Auto => Theme::detect(),
            ThemeChoice::Dark => Theme::Dark,
            ThemeChoice::Light => Theme::Light,
            ThemeChoice::NoColor => Theme::NoColor,
        };
        let handle = scanner::scan_streaming(&path, opts).context("scan failed")?;
        tui::run(handle, theme).context("tui crashed")?;
    }
    Ok(())
}

fn build_excludes(patterns: &[String]) -> Result<Option<globset::GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        builder.add(Glob::new(p).with_context(|| format!("invalid glob: {p}"))?);
    }
    Ok(Some(builder.build()?))
}

/// Sorted (id, size, kind) for every non-root node, descending by size.
fn ranked_entries(tree: &Tree) -> Vec<(NodeId, u64, NodeKind)> {
    let mut all: Vec<(NodeId, u64, NodeKind)> = tree
        .nodes()
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, n)| (NodeId(i as u32), n.size, n.kind))
        .collect();
    all.sort_by_key(|(_, s, _)| std::cmp::Reverse(*s));
    all
}

fn print_summary(tree: &Tree, top: usize, elapsed: std::time::Duration) {
    let root = tree.get(NodeId::ROOT);
    let errs = if tree.error_count > 0 {
        format!(", {} errors", tree.error_count)
    } else {
        String::new()
    };
    println!(
        "{}  {}  ({} files, {:.2?}{errs})",
        format_size(root.size, BINARY),
        tree.root_path.display(),
        tree.len(),
        elapsed,
    );

    for (id, size, _kind) in ranked_entries(tree)
        .into_iter()
        .filter(|(_, _, k)| !matches!(k, NodeKind::Dir))
        .take(top)
    {
        let p = tree.path_of(id);
        println!("{:>10}  {}", format_size(size, BINARY), p.display());
    }
}

fn print_json(tree: &Tree, top: usize) -> Result<()> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Out<'a> {
        root: &'a std::path::Path,
        total: u64,
        files: usize,
        errors: u64,
        top: Vec<Item>,
    }
    #[derive(Serialize)]
    struct Item {
        path: std::path::PathBuf,
        size: u64,
        kind: &'static str,
    }

    let items: Vec<Item> = ranked_entries(tree)
        .into_iter()
        .take(top)
        .map(|(id, size, kind)| Item {
            path: tree.path_of(id),
            size,
            kind: kind.as_str(),
        })
        .collect();
    let out = Out {
        root: &tree.root_path,
        total: tree.get(NodeId::ROOT).size,
        files: tree.len(),
        errors: tree.error_count,
        top: items,
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
