# bugs.md — open defects found by dogfooding jails

**No report is open.** B46-B51 and B57-B60 are closed and deleted; each was
re-reproduced from an empty directory against the binary built from HEAD rather
than assumed from the diff.

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
`request`, `runner`, `logs`, `console`.

**No longer in this list: a generated application run end to end against a
live database.** `minicom/minicom-org/spring` was started against a real
PostgreSQL and driven over HTTP -- sign-in, an admin message, a customer
reply the admin then reads, a mark-as-read, and a customer request asking for
`sender_type=ADMIN` that is still stored as `CUSTOMER`. It was started with
`mvn spring-boot:run` rather than `jails run`, so **`jails run` cold start is
still uncovered** and is the half that remains.

No `gradle` binary is on PATH, so a Gradle claim can only be exercised through
a checkout that ships its own wrapper. Three generations have now been
observed: `minicom/old/mc-01-06-2026/spring` (8.5 / Boot 2.7.18 / JDK 21),
`minicom/minicom-15-01-2026/spring`, and the same checkout after `jails
modernize` took it to Gradle 9.7 / Boot 4.1 / JDK 26, where `./gradlew build`
runs 60 unit and 23 integration tests green.
