#!/usr/bin/env bash
# Run all benchmarks and save results

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
echo "Running benchmarks with timestamp: $TIMESTAMP"

# Run benchmarks and save baseline (with reduced timing for CI/quick runs)
cargo bench --bench syzygy_benchmarks -- --warm-up-time 1 --measurement-time 2 --save-baseline $TIMESTAMP

# Check if benchmarks succeeded
if [ $? -eq 0 ]; then
    echo "✅ Benchmarks completed successfully"
    echo "Results saved to baseline: $TIMESTAMP"
    
    # Save a copy of the HTML reports
    if [ -d "target/criterion" ]; then
        mkdir -p "benchmark_results/reports/$TIMESTAMP"
        cp -r target/criterion/* "benchmark_results/reports/$TIMESTAMP/"
        echo "HTML reports copied to benchmark_results/reports/$TIMESTAMP"
        echo "View main report at: benchmark_results/reports/$TIMESTAMP/report/index.html"
    fi
else
    echo "❌ Benchmarks failed"
    exit 1
fi