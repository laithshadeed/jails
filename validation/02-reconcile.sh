#!/usr/bin/env bash
#
# Workout 2 -- exact one-to-one bank/ledger reconciliation.
#   brief: stacks/workouts/02-reconcile.md
#
# Match on (currency, amountMinor, normalisedReference). Everything unmatched
# or ambiguous has to be reported in its own bucket, not swallowed.
#
# jails features exercised:  list<T> field types (the new one), g enum,
#                            capitalized types, !/? suffixes
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start reconcile

run jails add json
run jails add testkit

# Carried forward from workout 1 -- reconcile consumes CanonicalTransactions,
# never raw provider shapes.
run jails g enum  Currency GBP EUR USD
run jails g value SourceRef system:string! externalId:string!
run jails g value CanonicalTransaction id:string! date:date amountMinor:long \
                                       currency:Currency source:SourceRef description:string
run jails g value ValidationIssue path:string! code:string! message:string! source:SourceRef?

# One confirmed pairing. Deliberately its own type rather than a Map entry or
# a String pair: the brief asks for `{bankId, ledgerId}` in the output JSON.
run jails g value Match bankId:string! ledgerId:string!

# Why a group could not be resolved to one pairing -- e.g. two ledger rows
# with identical keys. Both sides are lists because ambiguity is many-to-many.
run jails g value Ambiguity 'bankIds:list<string>' 'ledgerIds:list<string>' reason:string!

# The result envelope: five buckets, every one a list. This is the shape that
# needs `list<T>`, and it recurs in workouts 3, 4, 7, 8, 9 and 10.
#
# `list<Match>` must produce List<Match> with the component defensively copied
# (List.copyOf) so the record is genuinely immutable, and default to an empty
# list rather than null -- callers should never have to null-check a bucket.
run jails g value ReconcileResult 'matched:list<Match>' \
                                  'unmatchedBank:list<string>' \
                                  'unmatchedLedger:list<string>' \
                                  'ambiguous:list<Ambiguity>' \
                                  'invalid:list<ValidationIssue>'

run jails g command Reconcile

fixtures 02-reconcile happy.json ambiguous.json many-to-one.json unmatched-both.json

# ---- assertions ------------------------------------------------------------

section "list<T>" "collection field types"
has "$DOMAIN/ReconcileResult.java" 'List<Match> matched'                'list<Match> -> List<Match>'
has "$DOMAIN/ReconcileResult.java" 'List<String> unmatchedBank'         'list<string> -> List<String>'
has "$DOMAIN/ReconcileResult.java" 'List<Ambiguity> ambiguous'          'nested own-type list'
has "$DOMAIN/ReconcileResult.java" 'List<ValidationIssue> invalid'      'issues bucket'
has "$DOMAIN/ReconcileResult.java" 'import java.util.List;'             'List imported'

section "list<T> semantics" "immutable and never null"
# A record component holding a caller's mutable list is not actually
# immutable, and a null bucket forces every consumer to guard.
has "$DOMAIN/ReconcileResult.java" 'List.copyOf|copyOf\(' 'components defensively copied'
has "$DOMAIN/ReconcileResult.java" 'List.of\(\)|isEmpty|== null'        'null list defaults to empty'

section "carried forward" "workout 1 types still generate"
has "$DOMAIN/CanonicalTransaction.java" 'Currency currency' 'canonical shape intact'
has "$DOMAIN/Match.java"                'String bankId'     'Match is a value type'

section wiring "command reachable"
has "$SRC/App.java" 'ReconcileCommand' 'App dispatches to ReconcileCommand'

build

# `list<T>` needs an element type; a bare `list` is meaningless and should be
# an error rather than silently becoming List<Object>.
section errors "malformed collection types rejected"
rejects "bare list rejected"          jails g value Bad items:list
rejects "unknown element type rejected" jails g value Bad 'items:list<nope>'

verdict "workout 2 (reconcile)"
