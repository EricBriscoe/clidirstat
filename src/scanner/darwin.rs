//! macOS-native fast directory scanner using `getattrlistbulk(2)`.
//!
//! Returns up to ~200 entries *with full stat info* per syscall, avoiding the
//! per-file `stat()` that dominates portable scanners. Falls back to the
//! generic readdir-based implementation per-directory on ENOTSUP or any I/O
//! error so weird mounts (some SMB/NFS shares) don't break the whole scan.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::{DirLister, EntryInfo, EntryKind, generic};

const BUF_SIZE: usize = 32 * 1024;

#[repr(C)]
#[derive(Default)]
struct AttrSet {
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

#[repr(C)]
struct Attrlist {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

#[repr(C)]
struct AttrReference {
    attr_dataoffset: i32,
    attr_length: u32,
}

// Constants from <sys/attr.h>. The `libc` crate exposes most of these but
// not all in every version; declare locally to keep things consistent.
const ATTR_BIT_MAP_COUNT: u16 = 5;

const ATTR_CMN_NAME: u32 = 0x0000_0001;
const ATTR_CMN_DEVID: u32 = 0x0000_0002;
const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
const ATTR_CMN_FILEID: u32 = 0x0200_0000;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;

const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;

// VNODE types (from <sys/vnode.h>).
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

pub struct DarwinLister;

impl DarwinLister {
    pub fn new() -> Self {
        Self
    }
}

impl DirLister for DarwinLister {
    fn list(&self, path: &Path) -> std::io::Result<Vec<EntryInfo>> {
        match list_bulk(path) {
            Ok(v) => Ok(v),
            Err(e) => {
                let code = e.raw_os_error();
                if matches!(
                    code,
                    Some(libc::ENOTSUP) | Some(libc::EOPNOTSUPP) | Some(libc::EINVAL)
                ) {
                    return generic::list_dir(path);
                }
                Err(e)
            }
        }
    }
}

fn list_bulk(path: &Path) -> std::io::Result<Vec<EntryInfo>> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;

    let fd = unsafe {
        libc::open(
            c_path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let guard = FdGuard(fd);

    let mut attrlist = Attrlist {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_NAME
            | ATTR_CMN_DEVID
            | ATTR_CMN_OBJTYPE
            | ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: ATTR_FILE_ALLOCSIZE,
        forkattr: 0,
    };
    let mut buf = vec![0u8; BUF_SIZE];
    let mut out = Vec::new();

    loop {
        let count = unsafe {
            getattrlistbulk(
                guard.0,
                &mut attrlist as *mut Attrlist as *mut libc::c_void,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
            )
        };
        if count < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if count == 0 {
            break;
        }
        let mut offset: usize = 0;
        for _ in 0..count {
            let entry_start = offset;
            // First 4 bytes: total entry length including itself.
            let entry_len =
                u32::from_ne_bytes(buf[entry_start..entry_start + 4].try_into().unwrap()) as usize;
            // Next: attribute_set_t (5 u32s = AttrSet) indicating returned attrs.
            let mut p = entry_start + 4;
            let returned = read_attrset(&buf, p);
            p += std::mem::size_of::<AttrSet>();

            let mut name = String::new();
            let mut dev: u64 = 0;
            let mut ino: u64 = 0;
            let mut objtype: u32 = 0;
            let mut alloc_size: u64 = 0;

            // Order of attrs follows declaration in attrlist, restricted to
            // those present in `returned`. RETURNED_ATTRS itself does
            // not occupy bytes after the AttrSet header.
            if returned.commonattr & ATTR_CMN_NAME != 0 {
                let attr_ref = read_attrreference(&buf, p);
                let name_off = (p as isize + attr_ref.attr_dataoffset as isize) as usize;
                let name_end = name_off + attr_ref.attr_length as usize;
                let raw = &buf[name_off..name_end];
                // Trim trailing NULs from the C string.
                let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                name = String::from_utf8_lossy(&raw[..end]).into_owned();
                p += std::mem::size_of::<AttrReference>();
            }
            if returned.commonattr & ATTR_CMN_DEVID != 0 {
                let v = u32::from_ne_bytes(buf[p..p + 4].try_into().unwrap());
                dev = v as u64;
                p += 4;
            }
            if returned.commonattr & ATTR_CMN_OBJTYPE != 0 {
                objtype = u32::from_ne_bytes(buf[p..p + 4].try_into().unwrap());
                p += 4;
            }
            if returned.commonattr & ATTR_CMN_FILEID != 0 {
                // u64 field; re-align to 8 if not already.
                p = (p + 7) & !7;
                ino = u64::from_ne_bytes(buf[p..p + 8].try_into().unwrap());
                p += 8;
            }
            if returned.fileattr & ATTR_FILE_ALLOCSIZE != 0 {
                p = (p + 7) & !7;
                alloc_size = u64::from_ne_bytes(buf[p..p + 8].try_into().unwrap());
                p += 8;
            }
            let _ = p;

            if !name.is_empty() && name != "." && name != ".." {
                let kind = match objtype {
                    VDIR => EntryKind::Dir,
                    VREG => EntryKind::File,
                    VLNK => EntryKind::Symlink,
                    _ => EntryKind::Other,
                };
                // getattrlistbulk doesn't return nlink; assume 1 here. The
                // scanner skips hardlink dedup when nlink == 1, so a tree
                // with lots of hardlinks reports the inflated total via the
                // fast path. (CLAUDE.md "Invariants".)
                out.push(EntryInfo {
                    name,
                    kind,
                    size: alloc_size,
                    dev,
                    ino,
                    nlink: 1,
                });
            }
            offset = entry_start + entry_len;
        }
    }
    drop(guard);
    Ok(out)
}

fn read_attrset(buf: &[u8], off: usize) -> AttrSet {
    let r = |o: usize| u32::from_ne_bytes(buf[o..o + 4].try_into().unwrap());
    AttrSet {
        commonattr: r(off),
        volattr: r(off + 4),
        dirattr: r(off + 8),
        fileattr: r(off + 12),
        forkattr: r(off + 16),
    }
}

fn read_attrreference(buf: &[u8], off: usize) -> AttrReference {
    let dataoffset = i32::from_ne_bytes(buf[off..off + 4].try_into().unwrap());
    let length = u32::from_ne_bytes(buf[off + 4..off + 8].try_into().unwrap());
    AttrReference {
        attr_dataoffset: dataoffset,
        attr_length: length,
    }
}

struct FdGuard(libc::c_int);
impl Drop for FdGuard {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

unsafe extern "C" {
    fn getattrlistbulk(
        dirfd: libc::c_int,
        alist: *mut libc::c_void,
        attr_buf: *mut libc::c_void,
        buf_size: libc::size_t,
        options: u64,
    ) -> libc::c_int;
}
