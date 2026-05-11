use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;

use anyhow::{Context, Result};
use dashmap::DashMap;
use globset::GlobSet;
use rayon::Scope;

use crate::model::{Node, NodeId, NodeKind, Tree};

mod generic;

#[cfg(target_os = "macos")]
mod darwin;

pub struct ScanOptions {
    pub one_file_system: bool,
    pub follow_symlinks: bool,
    pub excludes: Option<GlobSet>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            one_file_system: true,
            follow_symlinks: false,
            excludes: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

impl From<EntryKind> for NodeKind {
    fn from(k: EntryKind) -> Self {
        match k {
            EntryKind::Dir => NodeKind::Dir,
            EntryKind::File => NodeKind::File,
            EntryKind::Symlink => NodeKind::Symlink,
            EntryKind::Other => NodeKind::Other,
        }
    }
}

#[derive(Debug)]
pub struct EntryInfo {
    pub name: String,
    pub kind: EntryKind,
    pub apparent_size: u64,
    pub allocated_size: u64,
    pub dev: u64,
    pub ino: u64,
    pub nlink: u64,
}

pub trait DirLister: Send + Sync {
    fn list(&self, path: &Path) -> std::io::Result<Vec<EntryInfo>>;
}

#[cfg(target_os = "macos")]
fn make_lister() -> Box<dyn DirLister> {
    Box::new(darwin::DarwinLister::new())
}

#[cfg(not(target_os = "macos"))]
fn make_lister() -> Box<dyn DirLister> {
    Box::new(generic::GenericLister)
}

struct ScanCtx {
    tree: Arc<RwLock<Tree>>,
    inodes: DashMap<(u64, u64), ()>,
    lister: Box<dyn DirLister>,
    opts: ScanOptions,
    root_dev: u64,
    cancel: Arc<AtomicBool>,
}

/// Handle to an in-progress streaming scan. The `tree` is shared with the
/// scanner via an `RwLock`; the UI takes read locks for ~ms per frame while
/// the scanner takes write locks for ~ms per directory.
pub struct ScanHandle {
    pub tree: Arc<RwLock<Tree>>,
    pub cancel: Arc<AtomicBool>,
    pub done: Arc<AtomicBool>,
    pub error: Arc<Mutex<Option<anyhow::Error>>>,
    join: Option<JoinHandle<()>>,
}

impl ScanHandle {
    /// Signal the scan to stop at the next directory boundary. Idempotent.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Block until the scan thread finishes, propagating any error.
    pub fn join(mut self) -> Result<()> {
        if let Some(h) = self.join.take() {
            h.join()
                .map_err(|_| anyhow::anyhow!("scan thread panicked"))?;
        }
        if let Some(e) = self.error.lock().unwrap().take() {
            return Err(e);
        }
        Ok(())
    }
}

/// Start a scan in the background. The returned handle exposes the shared
/// `Tree` immediately; callers can read it (under the `RwLock`) while the
/// scan progresses. The `done` flag flips to `true` when the scan thread
/// exits, whether by completion or cancellation.
pub fn scan_streaming(root: &Path, opts: ScanOptions) -> Result<ScanHandle> {
    let root = root
        .canonicalize()
        .with_context(|| format!("canonicalize {:?}", root))?;
    let root_meta = std::fs::symlink_metadata(&root).with_context(|| format!("stat {:?}", root))?;
    use std::os::unix::fs::MetadataExt;
    let root_dev = root_meta.dev();
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    let tree = Arc::new(RwLock::new(Tree::new(root.clone(), root_name)));
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));

    let tree_for_thread = tree.clone();
    let cancel_for_thread = cancel.clone();
    let done_for_thread = done.clone();
    let root_for_thread = root.clone();

    let join = std::thread::Builder::new()
        .name("clidirstat-scan".into())
        .spawn(move || {
            let ctx = Arc::new(ScanCtx {
                tree: tree_for_thread,
                inodes: DashMap::new(),
                lister: make_lister(),
                opts,
                root_dev,
                cancel: cancel_for_thread,
            });
            rayon::scope(|s| {
                spawn_dir_task(s, root_for_thread, NodeId::ROOT, ctx);
            });
            done_for_thread.store(true, Ordering::Release);
        })
        .context("spawn scan thread")?;

    Ok(ScanHandle {
        tree,
        cancel,
        done,
        error,
        join: Some(join),
    })
}

/// Synchronous convenience wrapper for `--no-tui` callers and tests: starts
/// a streaming scan, waits for completion, returns the owned `Tree`.
pub fn scan(root: &Path, opts: ScanOptions) -> Result<Tree> {
    let handle = scan_streaming(root, opts)?;
    let tree_arc = handle.tree.clone();
    handle.join()?;
    let tree = Arc::try_unwrap(tree_arc)
        .map_err(|_| anyhow::anyhow!("tree references leaked from sync scan"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("tree lock poisoned"))?;
    Ok(tree)
}

fn spawn_dir_task<'scope>(
    scope: &Scope<'scope>,
    path: PathBuf,
    parent_id: NodeId,
    ctx: Arc<ScanCtx>,
) {
    scope.spawn(move |scope| {
        process_dir(scope, path, parent_id, ctx);
    });
}

fn process_dir<'scope>(scope: &Scope<'scope>, path: PathBuf, parent_id: NodeId, ctx: Arc<ScanCtx>) {
    if ctx.cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(entries) = ctx.lister.list(&path) else {
        let mut t = ctx.tree.write().unwrap();
        t.error_count += 1;
        t.get_mut(parent_id).had_error = true;
        return;
    };

    let mut subdir_paths: Vec<(PathBuf, NodeId)> = Vec::new();
    {
        let mut t = ctx.tree.write().unwrap();
        for entry in entries {
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            if is_time_machine_snapshot(&entry.name) {
                t.skipped_count += 1;
                continue;
            }
            let child_path = path.join(&entry.name);
            if let Some(glob) = ctx.opts.excludes.as_ref()
                && glob.is_match(&child_path)
            {
                t.skipped_count += 1;
                continue;
            }
            let cross_dev = entry.dev != 0 && entry.dev != ctx.root_dev;
            if ctx.opts.one_file_system && cross_dev && matches!(entry.kind, EntryKind::Dir) {
                t.skipped_count += 1;
                continue;
            }

            let (apparent, allocated) = if entry.nlink > 1 && !matches!(entry.kind, EntryKind::Dir)
            {
                if ctx.inodes.insert((entry.dev, entry.ino), ()).is_some() {
                    (0, 0)
                } else {
                    (entry.apparent_size, entry.allocated_size)
                }
            } else {
                (entry.apparent_size, entry.allocated_size)
            };

            let follow_symlink =
                ctx.opts.follow_symlinks && matches!(entry.kind, EntryKind::Symlink);
            let is_dir = matches!(entry.kind, EntryKind::Dir);
            let node = Node {
                name: entry.name.clone(),
                parent: Some(parent_id),
                kind: entry.kind.into(),
                children: Vec::new(),
                apparent_size: apparent,
                allocated_size: allocated,
                had_error: false,
            };
            let id = t.push(parent_id, node);
            // Live aggregation: roll non-dir sizes up to every ancestor.
            // Directories start at 0 and accumulate via their own children's
            // bump_ancestors calls, so they get the right totals without
            // double-counting.
            if !is_dir {
                t.bump_ancestors(id, apparent, allocated);
            }
            if is_dir || follow_symlink {
                subdir_paths.push((child_path, id));
            }
        }
    }

    for (child_path, id) in subdir_paths {
        spawn_dir_task(scope, child_path, id, ctx.clone());
    }
}

fn is_time_machine_snapshot(name: &str) -> bool {
    name == ".com.apple.TimeMachine.localsnapshots"
        || name.starts_with(".com.apple.TimeMachine.")
        || name == ".MobileBackups"
}
