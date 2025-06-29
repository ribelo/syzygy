#!/usr/bin/env bash
# Run all benchmarks and save results

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
echo "Running benchmarks with timestamp: $TIMESTAMP"

# Run benchmarks and save baseline
cargo bench --bench syzygy_benchmarks -- --save-baseline $TIMESTAMP

# Check if benchmarks succeeded
if [ $? -eq 0 ]; then
    echo "✅ Benchmarks completed successfully"
    echo "Results saved to baseline: $TIMESTAMP"
    
    # Save a copy of the HTML report
    if [ -d "target/criterion" ]; then
        cp -r target/criterion benchmark_results/reports/$TIMESTAMP
        echo "HTML reports copied to benchmark_results/reports/$TIMESTAMP"
    fi
else
    echo "❌ Benchmarks failed"
    exit 1
fi