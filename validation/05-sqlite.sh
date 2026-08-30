#!/usr/bin/env bash
#
# Workout 5 -- SQLite persistence, append-only audit, deterministic replay.
#   brief: stacks/workouts/05-sqlite.md
#
# Repository interfaces live in app/, their SQLite implementations in
# adapters/, and the domain stays ignorant of both. Migrations are versioned
# and a second `db migrate` is a no-op.
#
# jails features exercised:  g repo (the new one), nested subcommands,
#                            add sqlite, list<T>
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start ledgerdb

run jails add json
run jails add testkit

# sqlite-jdbc, a Database record, a Migrations runner that tracks applied
# scripts in schema_migrations, and 001_init.sql. Plain JDBC -- no ORM, which
# the gym bans outright.
run jails add sqlite

run jails g enum  Currency GBP EUR USD
run jails g value SourceRef system:string! externalId:string!
run jails g value CanonicalTransaction id:string! date:date amountMinor:long \
                                       currency:Currency source:SourceRef description:string

# The headline feature for this workout.
#
# `g repo <Name>` should emit the port/adapter pair the brief asks for, which
# is otherwise three hand-written files every single time:
#
#   app/CanonicalTransactionRepository.java        interface: findById, findAll, save,
#                                         deleteById -- no JDBC types in the
#                                         signature, so the app layer stays
#                                         persistence-ignorant
#   adapters/SqliteCanonicalTransactionRepository.java
#                                         implements it over java.sql, with
#                                         PreparedStatements (never string
#                                         concatenation) and try-with-resources
#   test/.../SqliteCanonicalTransactionRepositoryTest.java
#                                         round-trips against an in-memory
#                                         database from `add sqlite`
#
# The <Name> is the entity it stores, so the fields come from the record that
# already exists rather than being redeclared here.
# `Transaction` here was a name this workout never declared -- the record
# above is `CanonicalTransaction`, and the comment says the argument *is*
# the entity it stores. jails refused it correctly.
run jails g repo CanonicalTransaction

# Work items are the queue a human reviews.
run jails g enum  WorkItemState OPEN IN_REVIEW APPROVED REJECTED
run jails g value WorkItem id:string! entityId:string! state:WorkItemState reason:string!
run jails g repo  WorkItem

# `db migrate` / `db reset` / `work list` / `audit show <id>` are nested
# subcommands -- two words, not one. jails generates the leaf; App's dispatcher
# has to route on the first word and hand the rest down.
run jails g command Db
run jails g command Work
run jails g command Audit

fixtures 05-sqlite import-batch.json replay-sequence.jsonl work-items.json stale-projection.json

# ---- assertions ------------------------------------------------------------

section "g repo" "port in app/, adapter in adapters/"
has "$APP/CanonicalTransactionRepository.java"      'interface CanonicalTransactionRepository' 'repository is an interface'
has "$APP/CanonicalTransactionRepository.java"      'findById|findAll'                'declares CRUD reads'
# The point of the port: app code must not import java.sql.
lacks "$APP/CanonicalTransactionRepository.java"    'java.sql'                        'port is free of JDBC types'
exists "$ADAPTERS/SqliteCanonicalTransactionRepository.java" 'SQLite implementation generated'
has "$ADAPTERS/SqliteCanonicalTransactionRepository.java" 'implements CanonicalTransactionRepository' 'adapter implements the port'
has "$ADAPTERS/SqliteCanonicalTransactionRepository.java" 'prepareStatement'          'uses PreparedStatement'
has "$ADAPTERS/SqliteCanonicalTransactionRepository.java" 'try \(' 'try-with-resources'

section "g repo" "no ORM, no framework"
lacks "$ADAPTERS/SqliteCanonicalTransactionRepository.java" 'org.springframework' 'no framework imports'

section "g repo" "companion test round-trips"
exists "$TEST/adapters/SqliteCanonicalTransactionRepositoryTest.java" 'repository test generated'
has "$TEST/adapters/SqliteCanonicalTransactionRepositoryTest.java" 'inMemory|:memory:' 'tests against a temp database'

section "add sqlite" "migrations"
exists "$PROJECT/src/main/resources/db/migration/001_init.sql" 'first migration'
has "$ADAPTERS/Migrations.java" 'schema_migrations' 'applied scripts are tracked'

section "second repo" "generator is not single-use"
exists "$APP/WorkItemRepository.java"              'second port'
exists "$ADAPTERS/SqliteWorkItemRepository.java"   'second adapter'

section wiring "nested subcommands"
has "$SRC/App.java" 'DbCommand'    'App dispatches to DbCommand'
has "$SRC/App.java" 'AuditCommand' 'App dispatches to AuditCommand'

build

verdict "workout 5 (sqlite)"
