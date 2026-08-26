# ADR 0001: SQL parser boundary

- Status: accepted
- Date: 2026-08-26
- Scope: Phase 2 offline PostgreSQL query contracts

## Decision

Use [`sqlparser` 0.62.0](https://docs.rs/sqlparser/0.62.0/sqlparser/) with only its
`std` feature for syntax, statement boundaries, AST nodes and source locations.
Do not treat its PostgreSQL dialect as PostgreSQL semantic validation.

The offline catalog admits only migration facts the jails compiler explicitly
proves. A statement or option outside that bounded subset is retained in the
catalog as a content-addressed opaque blocker. Phase 3 live checks use a real
server's prepared-statement description as the semantic authority.

## Same-corpus spike

The comparison corpus was:

```sql
CREATE TABLE items (id uuid PRIMARY KEY, label text NOT NULL);
SELECT id, label FROM items WHERE label = $1 LIMIT $2;
```

Both embedded parsers accepted both statements. The server-only design accepts
the query by definition only when a reachable PostgreSQL instance can parse,
analyze and describe it; it was not used for the offline build measurements.

Measurements were made on this workstation with Rust 1.97.1, isolated empty
target directories, `cargo build --release`, and a no-op repeat. The jails
baseline is commit `c8364bf`; the selected build is the working tree based on
`51a6bf0`.

| Choice | Clean build | Max RSS | No-op build | Binary | Resolved dependency effect |
|---|---:|---:|---:|---:|---|
| jails baseline | 21.18 s | 434,600 KB | 0.07 s / 44,940 KB | 9,831,512 B | baseline |
| jails + `sqlparser` | 40.12 s | 1,242,328 KB | 0.09 s / 44,628 KB | 10,249,896 B | + `sqlparser`, + `log` |
| isolated `pg_query` 6.2.0 spike | 53.67 s | 923,604 KB | 0.08 s / 43,916 KB | 4,101,840 B | 80 packages in the minimal spike |
| server-only description | no embedded parser | not measured | not measured | no parser contribution | PostgreSQL client/protocol plus a reachable server |

The `pg_query` binary is a minimal spike, not a jails binary, so its absolute
size is not compared with the two jails rows. Its package count, native build
surface and clean-build measurements are directly observed.

## Alternatives

### Pure-Rust `sqlparser`

- Apache-2.0; the project describes itself as an extensible SQL lexer/parser.
- PostgreSQL, MySQL and SQLite dialects are available behind a Rust API.
- `TokenWithSpan`, `Span` and `Spanned` expose source locations.
- It is a syntax parser. Its generic design is intentionally broader than one
  server grammar and it supplies no catalog-aware type analysis.
- It preserves jails' Rust-only target/toolchain surface. The selected feature
  set avoids serde, visitors and derive macros.

This is sufficient for exact statement-count and directive/source boundaries,
provided catalog semantics are opt-in and bounded.

### `libpg_query` through `pg_query`

- Uses PostgreSQL server parser source and therefore tracks a named PostgreSQL
  grammar generation more closely than a multi-dialect parser.
- `libpg_query` uses the PostgreSQL license for server source and BSD-3-Clause
  for its other code; the Rust wrapper is MIT.
- The Rust wrapper bundles C and adds `bindgen`, `prost-build`, `cc`, protobuf
  generation and a native compiler/libclang surface.
- Maintenance follows PostgreSQL major branches; the latest stable major gets
  active development and older supported branches receive critical fixes.
- Raw parse locations are available, but the produced PostgreSQL parse tree is
  still not catalog-aware semantic analysis.

This gives higher grammar fidelity but does not solve offline query typing, and
its native build/target cost is disproportionate to the syntax-only job.

### Server-only live description

- PostgreSQL's extended protocol `Describe` response supplies parameter and row
  descriptions for a prepared statement.
- It is the authoritative grammar, catalog, overload and type-resolution path.
- It requires a reachable matching server and database state, so it cannot meet
  `jails sql check --offline --frozen` or editor-without-services requirements.
- Result nullability still needs conservative analysis; SQLx documents using
  catalog constraints plus `EXPLAIN (VERBOSE, FORMAT JSON)` and notes that the
  inference is imperfect.

This is retained as the Phase 3 evidence upgrade, not the Phase 2 baseline.

## Consequences

- A successful parse means syntactically understood, not server-valid.
- Offline contracts may claim only facts linked to admitted migration objects.
- An opaque migration blocks affected semantic claims rather than silently
  degrading them to guesses.
- Live evidence can replace offline evidence without changing query identity or
  reader SQL bytes.

## Sources

- [`sqlparser` manifest and license](https://github.com/apache/datafusion-sqlparser-rs/blob/main/Cargo.toml)
- [`sqlparser::tokenizer::Span`](https://docs.rs/sqlparser/0.62.0/sqlparser/tokenizer/struct.Span.html)
- [`libpg_query` versions and licensing](https://github.com/pganalyze/libpg_query)
- [`pg_query` 6.2.0 manifest](https://github.com/pganalyze/pg_query.rs/blob/main/Cargo.toml)
- [PostgreSQL extended-query message flow](https://www.postgresql.org/docs/18/protocol-flow.html)
- [SQLx query-analysis and offline-mode FAQ](https://github.com/launchbadge/sqlx/blob/main/FAQ.md)
