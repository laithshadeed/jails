# CLI baseline fixtures

These fixtures measure jails' own project discovery and preparation overhead; they do not run a JVM or start containers.

- `small`: one Maven module and 5 Java sources.
- `medium`: one Maven module and 60 Java sources.
- `multi-module`: a three-module Maven reactor with 20 Java sources per module. Measurements run from `web`, so reactor discovery and active-module selection are both exercised.
- `phase1-loop`: one plain Java main plus three eligible JUnit cases. The opt-in Phase 1 benchmark uses it for resident warm-test and direct-JVM lifecycle measurements; it never starts a container or network service.

Run the repeatable baseline with:

```sh
cargo test --test baseline record_cli_baseline -- --ignored --nocapture
```

Run the Phase 1 test/run loop baseline with:

```sh
JAILS_BENCH_SAMPLES=20 cargo test --test baseline record_phase1_loop_baseline -- --ignored --nocapture
```

The Phase 1 output uses the `jails.phase1-loop-baseline.v1` schema. Its warm-test
samples reuse one resident daemon after an unmeasured Maven build and daemon
prime. Its application samples start a new jails process and direct JVM each
time while reusing a current runtime-classpath cache. Selection and first-result
figures are explicitly upper bounds because v1 emits a complete report rather
than streaming per-selector or per-case events.

`JAILS_BENCH_SAMPLES` selects the sample count (default 30, minimum 5). Every result names the fixture, cold/warm state, sample count, cache reason, p50, p95, and median absolute deviation. Cold samples use a fresh fixture copy with no `.jails` or build output. Warm samples reuse one copy after an unmeasured priming invocation. Each measured sample still starts a new jails process. Fixture copying is outside the timed interval.

CI should retain the emitted JSON lines from at least three runs on the same runner class and compare p95 only within the same fixture and state. The median absolute deviation records within-run variance. Do not combine these process-only baselines with JVM startup, build-tool startup, or container cold starts.
