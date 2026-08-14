# jails

Rails-CLI-inspired scaffolding tool for Spring Boot / plain Maven projects.
Full spec: `prompt.md` (non-goals, command list, field-type table — read it
before adding anything not already there; this is a deliberately small v1,
not a place to grow a plugin system).

## Layout

- `src/main.rs` — clap derive CLI, dispatch only.
- `src/new.rs` — `new` (start.spring.io wrapper, real network) and `new-cli`
  (hand-written pom/App/AppTest, no network).
- `src/generate.rs` — all Java templates (`format!`, no template engine) +
  `generate`/`destroy`. `ArtifactKind` is a `clap::ValueEnum` — keep it that
  way, see gotcha below.
- `src/add.rs` — `add <capability>` (csv/sqlite/json): grows an existing
  project by a whole slice (dependency + code + test). `Capability` is a `clap::ValueEnum` for
  the same completion reason as `ArtifactKind`.
- `src/pom.rs` — the only code that *edits* a file the user owns. Flavor
  and release-level detection, plus a comment-preserving dependency splice.
  `TARGET_RELEASE` lives here.
- `src/run.rs` — `test`/`build`/`run`, shells to `mvn`/`mvnd`.
- `tests/common/mod.rs` + `tests/cli.rs` — integration tests against the
  real compiled binary (`CARGO_BIN_EXE_jails`).

## Workflow (every change, no exceptions)

```
cargo build && cargo test && cargo install --path .
```
Tests must stay green before installing. A Stop hook runs this
automatically (see `.claude/settings.json`) — don't skip it manually even
though the hook exists, since the hook only fires on turn end, not mid-turn.

## Gotchas hit so far

- **Generated projects target Java 27** (`pom::TARGET_RELEASE`), which is
  not GA until 2026-09-15. mise's java registry carries *no* JDK 27 build
  of any vendor, so the EA build is symlinked in — see `mise.toml`. This
  shell does not run mise's activation hook, so `java` on a bare PATH is
  still 26; use `mise exec` or an explicit `JAVA_HOME` when something has
  to compile at release 27.
- **Tier-3 tests gate on `real_java_supports_target_release()`, not just
  on a JDK being present.** A JDK older than the target rejects
  `--release N` outright, so presence is not enough. Without the gate the
  suite goes red on any machine that hasn't installed the new JDK yet.
- **`base_package()` falls back to the shallowest .java file.** It used to
  require `*Application.java`, which only Spring projects have — `new-cli`
  projects have `App.java`, so `add` failed on exactly the projects it's
  most useful for.
- **Commons CSV renamed `Builder.build()` to `Builder.get()` in 1.13.**
  The pinned version and the generated call have to move together; a unit
  test in `add.rs` asserts they do, because the mismatch only surfaces as
  a compile error in the real-toolchain tier.
- **Don't use preview features in generated Java.** Structured concurrency
  is on its seventh preview and primitive patterns their fifth as of JDK
  27 — anything preview needs `--enable-preview` wired into both compile
  and surefire and breaks on the next JDK. String templates (`STR."..."`)
  were withdrawn and do not exist at all.

- **clap `alias` vs `visible_alias`**: hidden `alias` is invisible to
  `clap_complete`'s bash generator — `jails g <TAB>` fell back to top-level
  subcommand names instead of `generate`'s completions. Always use
  `visible_alias` for anything meant to be typed interactively.
- **Free-form `String` args don't tab-complete.** Any arg with a closed
  value set (like `generate`'s `kind`) must be a `clap::ValueEnum`, not a
  `String` matched by hand — that's the only way `clap_complete` can emit a
  static completion list.
- **This machine's `mvnd` daemon is flaky under JDK 26** (native-library
  extraction bug, unrelated to jails). `run.rs` still prefers `mvnd` for
  real usage (per spec), but the two real-compile tests in `tests/cli.rs`
  pin to plain `mvn` — see `real_path_without_mvnd()` in
  `tests/common/mod.rs`. Don't "fix" those tests back to the default PATH;
  they'll flake.
- **`mvn`'s own launcher script shells out to `uname`/`dirname`/`ls`/`expr`.**
  If you isolate PATH for a test (mocked mvn or real-mvn-only), you can
  strip specific binaries (e.g. `mvnd`) out of PATH, but you can't reduce
  PATH to *just* the tool directory — the real `mvn` script breaks with
  "command not found" for coreutils. Mocked fake-mvn scripts don't have
  this problem (they're a single `#!/bin/sh` line with no external calls).
- **Spring Boot 4.x moved `@AutoConfigureMockMvc`** from
  `org.springframework.boot.test.autoconfigure.web.servlet` to
  `org.springframework.boot.webmvc.test.autoconfigure`, no back-compat
  shim. `generate.rs::mockmvc_autoconfigure_import()` sniffs the parent POM
  version and picks the right one — don't hardcode the import again.
- **Tests never call start.spring.io.** `generate_scaffold_produces_a_
  project_that_compiles_and_passes_tests` and friends use a hand-written,
  version-pinned fixture (`write_spring_fixture` in `tests/common/mod.rs`)
  instead. Keep it that way — don't reintroduce a network dependency into
  the test suite.
- **All unit tests share one test binary** (this is a bin crate, not
  lib+bin), so `#[cfg(test)]` modules across `src/*.rs` run in the same
  process. Any test that calls `std::env::set_current_dir` MUST hold
  `crate::CWD_LOCK` (defined in `main.rs`) for the duration, or parallel
  tests race on the process-global cwd.
- **`cargo clippy` errors with E0514 (crate compiled by incompatible
  rustc)** in this environment — a toolchain/rustup mismatch between
  `cargo build`'s and clippy's driver, not a real code issue. Don't chase
  it; `cargo build`/`cargo test` are the real signal here.
- **`cargo init` gave edition `"2024"`**, not `"2026"` as prompt.md
  aspirationally says — edition 2026 doesn't exist yet as of this
  writing. Leave it on 2024.
- Install target is `~/.cargo/bin/jails` via `cargo install --path .`
  (already on PATH) — not a symlink into `~/.local/bin` or `~/bin`, which
  is how some other tools in `~/code/my-dotfiles` are wired. Don't
  "helpfully" switch install methods without asking; it was a deliberate
  choice among options.
- Bash completion is registered in
  `~/code/my-dotfiles/home/.bashrc.d/60-completions.sh`, guarded the same
  way as `gym`: `command -v jails &>/dev/null && source <(jails completion
  bash)`. That's a separate repo — changes there aren't tracked by this
  project's git history.

## Testing philosophy (see prompt.md's own bar)

Three tiers, don't blur them:
1. **Unit tests** (colocated `#[cfg(test)] mod tests` per file) — pure
   functions and filesystem-only logic, no Maven, no subprocess.
2. **Mocked-mvn integration tests** — a fake `mvn`/`mvnd` shell script that
   just logs argv, for verifying `run.rs`'s command construction (which
   binary, which flags) without needing real Maven.
3. **Real-toolchain integration tests** — actually invoke `mvn`/`javac`
   against a fixture project, gated on `mvn`/`java` being on PATH (skip
   gracefully, don't fail, if absent). This is the only tier that answers
   prompt.md's literal question: "does it produce a project that
   compiles/passes tests?" Don't let tier 2 masquerade as tier 3.
