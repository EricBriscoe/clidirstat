use std::fs;

use criterion::{Criterion, criterion_group, criterion_main};

use clidirstat::scanner::{ScanOptions, scan};

fn make_tree(root: &std::path::Path, breadth: usize, depth: usize, file_size: u64) {
    if depth == 0 {
        return;
    }
    for i in 0..breadth {
        let p = root.join(format!("f{i}.bin"));
        let mut buf = vec![0u8; file_size as usize];
        for (j, b) in buf.iter_mut().enumerate() {
            *b = (j % 251) as u8;
        }
        fs::write(p, &buf).unwrap();
    }
    for i in 0..breadth {
        let sub = root.join(format!("d{i}"));
        fs::create_dir(&sub).unwrap();
        make_tree(&sub, breadth, depth - 1, file_size);
    }
}

fn bench_scan(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    // 5^4 = 625 files + 156 dirs, 1 KiB each → ~625 KiB
    make_tree(dir.path(), 5, 4, 1024);

    c.bench_function("scan_5x4_1k", |b| {
        b.iter(|| {
            let _ = scan(dir.path(), ScanOptions::default()).unwrap();
        });
    });
}

criterion_group!(benches, bench_scan);
criterion_main!(benches);
