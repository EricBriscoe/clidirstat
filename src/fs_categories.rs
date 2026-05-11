#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Code,
    Image,
    Video,
    Audio,
    Docs,
    Archive,
    Binary,
    Data,
    Cache,
    Other,
}

const CODE: &[&str] = &[
    "rs", "go", "py", "js", "jsx", "ts", "tsx", "c", "h", "hpp", "cpp", "cc", "java", "kt",
    "swift", "rb", "php", "lua", "sh", "bash", "zsh", "fish", "pl", "scala", "clj", "ex", "exs",
    "erl", "ml", "hs", "elm", "dart", "css", "scss", "html", "vue", "sql",
];

const IMAGE: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp", "heic", "raw", "svg", "ico", "psd",
];

const VIDEO: &[&str] = &[
    "mp4", "mov", "mkv", "avi", "webm", "flv", "wmv", "m4v", "ts", "mts",
];

const AUDIO: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "aac", "m4a", "opus", "alac", "ape",
];

const DOCS: &[&str] = &[
    "txt", "md", "rst", "tex", "doc", "docx", "pdf", "rtf", "odt", "epub", "ppt", "pptx", "odp",
];

const ARCHIVE: &[&str] = &[
    "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "tgz", "tbz", "txz", "dmg", "iso", "pkg", "deb",
    "rpm",
];

const BINARY: &[&str] = &[
    "exe", "dll", "so", "dylib", "a", "lib", "o", "bin", "app", "wasm",
];

const DATA: &[&str] = &[
    "json", "csv", "tsv", "parquet", "arrow", "feather", "yaml", "yml", "toml", "xml", "xls",
    "xlsx", "ods", "db", "sqlite", "sqlite3",
];

/// Directory names that should de-emphasize their entire subtree by colouring
/// leaves with `Category::Cache` regardless of extension.
pub const CACHE_DIR_NAMES: &[&str] = &[
    ".cache",
    "node_modules",
    ".npm",
    "Caches",
    ".gradle",
    "target", // Rust build artefacts
    "build",
    ".next",
    ".turbo",
    ".pytest_cache",
    "__pycache__",
];

pub fn is_cache_dir(name: &str) -> bool {
    CACHE_DIR_NAMES.contains(&name)
}

pub fn classify(name: &str) -> Category {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return Category::Other;
    };
    if ext.is_empty() {
        return Category::Other;
    }
    let lower = ext.to_ascii_lowercase();
    let e = lower.as_str();
    if CODE.contains(&e) {
        Category::Code
    } else if IMAGE.contains(&e) {
        Category::Image
    } else if VIDEO.contains(&e) {
        Category::Video
    } else if AUDIO.contains(&e) {
        Category::Audio
    } else if DOCS.contains(&e) {
        Category::Docs
    } else if ARCHIVE.contains(&e) {
        Category::Archive
    } else if BINARY.contains(&e) {
        Category::Binary
    } else if DATA.contains(&e) {
        Category::Data
    } else {
        Category::Other
    }
}
