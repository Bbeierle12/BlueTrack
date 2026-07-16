# BlueTrack Autoresearch Progress
**Objective:** Optimize snapshot cloning latency in the worker thread (`snapshot/clone_vec` benchmark).
**Target File:** `src/model.rs` (or related snapshot code)
**Metric:** Latency (nanoseconds) for N=300 devices, measured by `eval_benchmark.sh`.

## Phases

### [ ] Phase T0: Baseline Measurement
- [ ] Run `./eval_benchmark.sh "baseline"` to establish current performance.
- [ ] Verify test suite passes (`cargo test`).
- [ ] Log baseline to `results.tsv` (wired to dashboard).

### [ ] Phase T1: Hypothesis Generation
- [ ] Review `src/model.rs` and `benches/enrich.rs`.
- [ ] Formulate 3 distinct hypotheses to reduce cloning latency (e.g., Arc/Rc sharing, String interning, Cow).
- [ ] Record hypotheses in this document.

### [ ] Phase T2: Parallel Experiment Execution
- [ ] Branch for Hypothesis A, implement, run `./eval_benchmark.sh "hyp_a"`, stash/commit.
- [ ] Branch for Hypothesis B, implement, run `./eval_benchmark.sh "hyp_b"`, stash/commit.
- [ ] Branch for Hypothesis C, implement, run `./eval_benchmark.sh "hyp_c"`, stash/commit.

### [ ] Phase T3: Verification Against Test Suite
- [ ] For each branch, ensure `cargo test` passes.
- [ ] Compare `results.tsv` to baseline. Identify the branch with the lowest latency.

### [ ] Phase T4: Automatic Merge-to-Main
- [ ] If the winner's latency is significantly better than baseline and all gates (tests) are green:
- [ ] Checkout main.
- [ ] Merge the winning branch.
- [ ] Run final `./eval_benchmark.sh "final_merge"`.
- [ ] Push to main (if applicable).

---
*State checkpoints will be updated here after each phase to survive context limits.*
