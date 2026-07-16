# Kill Test Documentation

Before firing the real autoresearch run, we verify that the evaluation harness (`eval_benchmark.sh`) correctly catches bad changes.

## Test 1: Verification Gate (Correctness)
**Action:** Introduce a compile error or a logic error that breaks `cargo test`.
**Expected Result:** `./eval_benchmark.sh` fails immediately and does not record a metric to `results.tsv`.

## Test 2: Performance Gate (Latency)
**Action:** Inject artificial latency (e.g., `std::thread::sleep`) into the target function (`clone` in `src/model.rs`).
**Expected Result:** `./eval_benchmark.sh` records a significantly higher latency than baseline, meaning this hypothesis would be rejected in Phase T3.

Let's execute these manually to verify the harness.
