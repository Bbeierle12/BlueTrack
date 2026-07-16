#!/bin/bash
set -e

# Run tests first to ensure correctness
cargo test || { echo "Tests failed"; exit 1; }

# Run benchmark
cargo bench --bench enrich "snapshot/clone_vec" > /dev/null

# Extract mean latency for N=300 in nanoseconds
ESTIMATE_FILE="target/criterion/snapshot/clone_vec/300/new/estimates.json"
if [ ! -f "$ESTIMATE_FILE" ]; then
    echo "Benchmark output not found!"
    exit 1
fi

LATENCY_NS=$(grep -oP '"point_estimate":\s*\K[0-9.]+' "$ESTIMATE_FILE" | head -n 1)

# Ensure results.tsv exists with headers
if [ ! -f results.tsv ]; then
    echo -e "timestamp\tcommit\tlatency_ns\tcomment" > results.tsv
fi

TIMESTAMP=$(date -Iseconds)
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "working-tree")
COMMENT=${1:-"auto-run"}

echo -e "${TIMESTAMP}\t${COMMIT}\t${LATENCY_NS}\t${COMMENT}" >> results.tsv
echo "Latency (N=300): ${LATENCY_NS} ns"
