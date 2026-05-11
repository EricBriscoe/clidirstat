#!/usr/bin/env bash
# Build a synthetic directory tree for testing & screenshots of clidirstat.
# ~235 MiB across ten extension categories plus cache subtrees, hardlinks
# and symlinks, designed so the treemap shows a clear visual hierarchy:
# Movies (video, coral) ~51% · Photos (image, orchid) ~11% · everything
# else <10% each, with .cache and node_modules de-emphasised to grey.
set -euo pipefail

ROOT="${1:-/tmp/clidirstat-testing}"

echo "Building demo tree at $ROOT"
rm -rf "$ROOT"
mkdir -p "$ROOT"

# mk <size> <path>: size like 40M / 200K
mk() {
  local size="$1" path="$2"
  local count="${size%[MK]}" unit="${size: -1}"
  local target="$ROOT/$path"
  mkdir -p "$(dirname "$target")"
  case "$unit" in
    M) dd if=/dev/urandom of="$target" bs=1m count="$count" 2>/dev/null ;;
    K) dd if=/dev/urandom of="$target" bs=1k count="$count" 2>/dev/null ;;
    *) echo "bad size $size" >&2; exit 1 ;;
  esac
}

# Movies (video, coral red): dominant tile, ~51%
mk 40M  Movies/vacation.mp4
mk 30M  Movies/interview.mov
mk 20M  Movies/trailer.webm
mk 30M  Movies/archive/old_recording.mkv

# Photos (image, orchid)
mk 8M   Photos/2024/img_001.jpg
mk 6M   Photos/2024/img_002.jpg
mk 4M   Photos/2024/img_003.heic
mk 3M   Photos/2024/img_004.png
mk 5M   Photos/2025/shot.png
mk 200K Photos/2025/design.svg
mk 100K Photos/logo.svg

# Music (audio, violet)
mk 2M   Music/album/track01.flac
mk 3M   Music/album/track02.flac
mk 1M   Music/album/track03.mp3
mk 1M   Music/album/track04.mp3
mk 4K   Music/album/playlist.m3u

# Code (sky blue) + a node_modules + a target/ (both should de-emphasise)
mk 60K  Code/projects/webapp/src/main.rs
mk 40K  Code/projects/webapp/src/lib.rs
mk 25K  Code/projects/webapp/src/utils.rs
mk 1K   Code/projects/webapp/Cargo.toml
mk 8K   Code/projects/webapp/README.md
mk 2M   Code/projects/webapp/node_modules/react/index.js
mk 1M   Code/projects/webapp/node_modules/lodash/index.js
mk 3M   Code/projects/webapp/node_modules/webpack/lib.js
mk 5M   Code/projects/webapp/target/debug/big_artifact.rlib
mk 8M   Code/projects/webapp/target/release/binary
mk 30K  Code/projects/api/server.py
mk 25K  Code/projects/api/handlers.py
mk 2K   Code/projects/api/requirements.txt
mk 4K   Code/scripts/deploy.sh
mk 2K   Code/scripts/backup.sh

# Documents (docs, pale yellow)
mk 5M   Documents/report.pdf
mk 1M   Documents/invoice.pdf
mk 80K  Documents/notes.md
mk 200K Documents/meeting.docx
mk 2M   Documents/presentation.pptx
mk 500K Documents/spreadsheet.xlsx

# Archives (orange)
mk 5M   Archives/backup-2025-01.tar.gz
mk 6M   Archives/backup-2025-02.tar.gz
mk 3M   Archives/old-project.zip
mk 4M   Archives/installer.dmg

# Data (sage green)
mk 2M   Data/users.json
mk 3M   Data/events.json
mk 1M   Data/analytics.csv
mk 8K   Data/config.yaml
mk 12K  Data/schema.json

# Binaries (salmon)
mk 2M   Binaries/helper.bin
mk 1M   Binaries/libstuff.dylib
mk 800K Binaries/tool

# .cache (grey de-emphasis)
mk 8M   .cache/webpack-cache/chunk1.bin
mk 6M   .cache/webpack-cache/chunk2.bin
mk 4M   .cache/webpack-cache/chunk3.bin
mk 200K .cache/thumbnails/thumb1.png
mk 150K .cache/thumbnails/thumb2.png

# Other (muted khaki): no or unknown extensions
mk 4K   unknown_stuff/file_no_ext
mk 8K   unknown_stuff/data.weird

# Hardlink (should be deduped and counted once)
ln "$ROOT/Movies/vacation.mp4" "$ROOT/Movies/vacation.hardlink.mp4"

# Symlinks
ln -s vacation.mp4 "$ROOT/Movies/vacation.symlink.mp4"
ln -s /etc/hosts   "$ROOT/external_link"

# A tiny empty directory to confirm rendering doesn't blow up on 0-byte nodes
mkdir -p "$ROOT/empty_dir"

echo "Done. Tree size:"
du -sh "$ROOT"
echo
echo "Try:"
echo "  ./target/release/clidirstat $ROOT"
echo "  ./target/release/clidirstat $ROOT --timeit"
echo "  ./target/release/clidirstat $ROOT --no-tui --top 10"
