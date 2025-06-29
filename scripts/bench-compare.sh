#!/usr/bin/env bash
# Compare current performance with a baseline

BASELINE=${1:-main}
echo "Comparing with baseline: $BASELINE"

# Run benchmarks and compare
cargo bench --bench syzygy_benchmarks -- --baseline $BASELINE

# Check result
if [ $? -eq 0 ]; then
    echo "✅ Comparison complete"
    echo "Check the output above for performance changes"
else
    echo "❌ Comparison failed"
    echo "Make sure baseline '$BASELINE' exists"
    exit 1
fi