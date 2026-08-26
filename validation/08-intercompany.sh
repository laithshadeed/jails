#!/usr/bin/env bash
#
# Workout 8 -- intercompany receivable/payable matching.
#   brief: stacks/workouts/08-intercompany.md
#
# Entity pairs are canonicalised so (A,B) and (B,A) are one pair. Residuals
# are calculated, corrections are drafted and never posted, and segregation of
# duties is enforced.
#
# jails features exercised:  list<T>, g enum, ? on own types, map<K,V>,
#                            composition -- no new features
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start intercompany

run jails add json
run jails add testkit
run jails add sqlite

run jails g enum Currency GBP EUR USD

# The five classifications from the brief.
run jails g enum PairOutcome EXACT_MATCH MISSING_COUNTERPART AMOUNT_MISMATCH \
                             DATE_MISMATCH AMBIGUOUS

# Which side of the pair a record sits on.
run jails g enum Direction RECEIVABLE PAYABLE

run jails g value SourceRef system:string! externalId:string!
run jails g value ValidationIssue path:string! code:string! message:string! source:SourceRef?

# One entity's side of a transaction.
run jails g value EntityRecord id:string! entity:string! counterparty:string! \
                               direction:Direction amountMinor:long currency:Currency \
                               date:date

# The canonicalised pair key: (A,B) and (B,A) must produce the same value, so
# a pair is a type rather than a concatenated string.
run jails g value EntityPair firstEntity:string! secondEntity:string!

# Tolerances, versioned like every other policy in the gym.
run jails g value MatchTolerance version:string! amountToleranceMinor:long dateToleranceDays:int

# Deterministic FX rates, supplied as input -- the stretch goal. `map<...>`
# because rates arrive keyed by currency pair.
run jails g value FxRates asOf:date 'rates:map<string,double>'

# One classified pair, with the residual left over after matching.
run jails g value PairResult pair:EntityPair outcome:PairOutcome \
                             'receivableIds:list<string>' 'payableIds:list<string>' \
                             residualMinor:long currency:Currency

# Drafted, never posted -- and balanced, so a correction cannot itself break
# the books.
run jails g value DraftEntry entity:string! account:string! amountMinor:long \
                             currency:Currency reason:string! draftedAt:instant

run jails g value IntercompanyResult 'results:list<PairResult>' \
                                     'corrections:list<DraftEntry>' \
                                     'issues:list<ValidationIssue>'

run jails g command Intercompany

fixtures 08-intercompany exact.json missing-counterpart.json amount-mismatch.json \
                         residual.json segregation.json stale-approval.json

# ---- assertions ------------------------------------------------------------

section "g enum" "five outcomes plus direction"
has "$DOMAIN/PairOutcome.java" 'MISSING_COUNTERPART' 'declares the missing case'
has "$DOMAIN/PairOutcome.java" 'AMBIGUOUS'           'declares the ambiguous case'
has "$DOMAIN/Direction.java"   'RECEIVABLE'          'direction is a closed set'

section "own types as components" "pair key is a type"
has   "$DOMAIN/PairResult.java" 'EntityPair pair'  'pair is a value type'
lacks "$DOMAIN/PairResult.java" 'String pair'      'not a concatenated string key'
has   "$DOMAIN/EntityRecord.java" 'Direction direction' 'direction used as a component'

section "map<K,V>" "supplied FX rates"
has "$DOMAIN/FxRates.java" 'Map<String, ?Double> rates' 'map<string,double> -> Map<String, Double>'
has "$DOMAIN/FxRates.java" 'Map.copyOf|copyOf'          'rates are defensively copied'

section "list<T>" "many-to-many pairings"
has "$DOMAIN/PairResult.java" 'List<String> receivableIds' 'receivable side is a list'
has "$DOMAIN/PairResult.java" 'List<String> payableIds'    'payable side is a list'

section "int type" "day tolerance"
has   "$DOMAIN/MatchTolerance.java" 'int dateToleranceDays'     'int unboxes to a primitive'
lacks "$DOMAIN/MatchTolerance.java" 'Integer dateToleranceDays' 'not boxed'

section "instant" "draft timestamps"
has "$DOMAIN/DraftEntry.java" 'Instant draftedAt' 'drafted-at is an Instant'

section wiring "command reachable"
has "$SRC/App.java" 'IntercompanyCommand' 'App dispatches to IntercompanyCommand'

build

verdict "workout 8 (intercompany)"
