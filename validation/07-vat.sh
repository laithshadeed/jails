#!/usr/bin/env bash
#
# Workout 7 -- three-way VAT reconciliation with lineage.
#   brief: stacks/workouts/07-vat.md
#
# Compare general ledger, prepared return and authority per VAT box. Every
# calculated amount has to name the source records behind it, so an aggregate
# can always be taken apart again.
#
# jails features exercised:  list<T>, g enum, capitalized types, instant,
#                            no new features -- this one is a composition test
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start vat

run jails add json
run jails add testkit
run jails add sqlite

run jails g enum  Currency GBP EUR USD

# The four outcomes per box. Naming them as an enum is what stops "reconciled"
# and "reconcilied" both existing in the output.
run jails g enum VatOutcome RECONCILED GL_VERSUS_RETURN RETURN_VERSUS_AUTHORITY ALL_THREE_DISAGREE

run jails g value SourceRef system:string! externalId:string!
run jails g value ValidationIssue path:string! code:string! message:string! source:SourceRef?

# Lineage: an aggregated figure plus the transaction IDs that produced it.
# `'sourceTransactionIds:list<string>'` is the requirement "preserve source
# lineage through aggregation" expressed as a type -- an aggregate that cannot
# name its inputs is not auditable.
run jails g value BoxTotal box:string! amountMinor:long currency:Currency \
                          'sourceTransactionIds:list<string>'

# One box compared across all three legs.
run jails g value BoxComparison box:string! outcome:VatOutcome \
                                generalLedger:BoxTotal \
                                preparedReturn:BoxTotal? \
                                authority:BoxTotal? \
                                differenceMinor:long

# Rounding tolerance is configurable AND versioned: a decision made under one
# tolerance must stay explainable after the tolerance changes.
run jails g value TolerancePolicy version:string! toleranceMinor:long

# A draft correction. Drafted, never posted -- the brief is explicit that
# nothing automated may post a journal.
run jails g value DraftCorrection box:string! amountMinor:long currency:Currency \
                                  reason:string! draftedAt:instant

run jails g value VatResult 'comparisons:list<BoxComparison>' \
                            'corrections:list<DraftCorrection>' \
                            'issues:list<ValidationIssue>' \
                            policyVersion:string!

run jails g command VatReconcile

fixtures 07-vat reconciled.json all-disagree.json late-posting.json \
                edited-after-snapshot.json tolerance-boundary.json missing-box.json

# ---- assertions ------------------------------------------------------------

section "lineage" "aggregates name their sources"
has "$DOMAIN/BoxTotal.java" 'List<String> sourceTransactionIds' 'lineage list generated'
has "$DOMAIN/BoxTotal.java" 'long amountMinor'                  'money stays integer minor units'

section "optional legs" "? on an own type"
# A box can be missing from the return or the authority extract entirely --
# which is different from being present and zero.
has "$DOMAIN/BoxComparison.java" 'Optional<BoxTotal> preparedReturn' 'return leg is Optional'
has "$DOMAIN/BoxComparison.java" 'Optional<BoxTotal> authority'      'authority leg is Optional'
has "$DOMAIN/BoxComparison.java" 'BoxTotal generalLedger'            'GL leg is required'
lacks "$DOMAIN/BoxComparison.java" 'Optional<BoxTotal> generalLedger' 'GL is not Optional'

section "g enum" "four outcomes"
has "$DOMAIN/VatOutcome.java" 'ALL_THREE_DISAGREE'      'declares the three-way case'
has "$DOMAIN/BoxComparison.java" 'VatOutcome outcome'   'outcome is the enum'

section "instant" "draft timestamps"
has "$DOMAIN/DraftCorrection.java" 'Instant draftedAt' 'drafted-at is an Instant'

section "composition" "result envelope"
has "$DOMAIN/VatResult.java" 'List<BoxComparison> comparisons' 'comparisons bucket'
has "$DOMAIN/VatResult.java" 'List<DraftCorrection> corrections' 'corrections bucket'
has "$DOMAIN/VatResult.java" 'List<ValidationIssue> issues'      'issues bucket'

section wiring "command reachable"
has "$SRC/App.java" 'VatReconcileCommand' 'App dispatches to VatReconcileCommand'

build

verdict "workout 7 (vat)"
