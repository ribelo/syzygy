#!/usr/bin/env bash
# Watch for changes and re-run benchmarks

echo "Watching for changes and running benchmarks..."
echo "Press Ctrl+C to stop"

# Check if cargo-watch is installed
if ! command -v cargo-watch &> /dev/null; then
    echo "cargo-watch is not installed. Install it with:"
    echo "  cargo install cargo-watch"
    exit 1
fi

# Run benchmarks on file changes
cargo watch -x 'bench --bench syzygy_benchmarks'