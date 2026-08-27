# bugs.md — open defects found by dogfooding jails

**No numbered report is currently open.** What follows is the convention and
the coverage note, so the next pass starts from B57.

Binary: `jails 0.1.0`, built and installed from this checkout. Every report
below was reproduced from an empty directory with the commands as written, in a
disposable project under a scratch directory. **No jails source, test, build or
doc file is modified while reproducing.**

**A closed report is *deleted* from this file, not marked done.**
`git log -p -- bugs.md` is where a closed one and the run that closed it live.
Numbers are stable and never reused, so a `bugs.md B33` citation in the source
still resolves to a subject.

---

## Never covered

Recorded so the gaps in this file are visible rather than implied.
`testd` and `--affected`; `test --engine warm`; `jails run` cold start;
`sql check --live`, `introspect`, `pull`, `contract check`, `editor`,
`request`, `runner`, `logs`, `console`; a generated application run end to end
against a live database. No `gradle` binary is on PATH, so a Gradle claim can
only be exercised through a checkout that ships its own wrapper -- which
`minicom/minicom-15-01-2026/spring` and `minicom/old/mc-01-06-2026/spring` both
do, under `JAVA_HOME` pointing at JDK 21.
