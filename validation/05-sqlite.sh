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
#   app/TransactionRepository.java        interface: findById, findAll, save,
#                                         deleteById -- no JDBC types in the
#                                         signature, so the app layer stays
#                                         persistence-ignorant
#   adapters/SqliteTransactionRepository.java
#                                         implements it over java.sql, with
#                                         PreparedStatements (never string
#                                         concatenation) and try-with-resources
#   test/.../SqliteTransactionRepositoryTest.java
#                                         round-trips against an in-memory
#                                         database from `add sqlite`
#
# The <Name> is the entity it stores, so the fields come from the record that
# already exists rather than being redeclared here.
run jails g repo Transaction

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
has "$APP/TransactionRepository.java"      'interface TransactionRepository' 'repository is an interface'
has "$APP/TransactionRepository.java"      'findById|findAll'                'declares CRUD reads'
# The point of the port: app code must not import java.sql.
lacks "$APP/TransactionRepository.java"    'java.sql'                        'port is free of JDBC types'
exists "$ADAPTERS/SqliteTransactionRepository.java" 'SQLite implementation generated'
has "$ADAPTERS/SqliteTransactionRepository.java" 'implements TransactionRepository' 'adapter implements the port'
has "$ADAPTERS/SqliteTransactionRepository.java" 'prepareStatement'          'uses PreparedStatement'
has "$ADAPTERS/SqliteTransactionRepository.java" 'try \(' 'try-with-resources'

section "g repo" "no ORM, no framework"
for forbidden in hibernate jakarta.persistence JpaRepository org.springframework; do
  lacks "$ADAPTERS/SqliteTransactionRepository.java" "$forbidden" "no $forbidden"
done

section "g repo" "companion test round-trips"
exists "$TEST/adapters/SqliteTransactionRepositoryTest.java" 'repository test generated'
has "$TEST/adapters/SqliteTransactionRepositoryTest.java" 'inMemory|:memory:' 'tests against a temp database'

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
