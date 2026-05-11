use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::{EntryInfo, EntryKind};

#[cfg(not(target_os = "macos"))]
pub struct GenericLister;

#[cfg(not(target_os = "macos"))]
impl super::DirLister for GenericLister {
    fn list(&self, path: &Path) -> std::io::Result<Vec<EntryInfo>> {
        list_dir(path)
    }
}

pub(super) fn list_dir(path: &Path) -> std::io::Result<Vec<EntryInfo>> {
    let mut out = Vec::new();
    let rd = std::fs::read_dir(path)?;
    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = match entry.metadata() {
            // metadata() doesn't follow symlinks for DirEntry; it uses lstat
            Ok(m) => m,
            Err(_) => continue,
        };
        let ft = meta.file_type();
        let kind = if ft.is_dir() {
            EntryKind::Dir
        } else if ft.is_symlink() {
            EntryKind::Symlink
        } else if ft.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        };
        out.push(EntryInfo {
            name,
            kind,
            size: meta.blocks().saturating_mul(512),
            dev: meta.dev(),
            ino: meta.ino(),
            nlink: meta.nlink(),
        });
    }
    Ok(out)
}
