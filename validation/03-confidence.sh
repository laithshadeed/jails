#!/usr/bin/env bash
#
# Workout 3 -- confidence matching with a versioned scoring policy.
#   brief: stacks/workouts/03-confidence.md
#
# Every score has to show its working: which rules fired, what each
# contributed, and why the winner beat the runner-up.
#
# jails features exercised:  list<T>, g enum, double fields, capitalized types
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start confidence

run jails add json
run jails add testkit

run jails g enum  Currency GBP EUR USD
run jails g value SourceRef system:string! externalId:string!
run jails g value CanonicalTransaction id:string! date:date amountMinor:long \
                                       currency:Currency source:SourceRef description:string

# The four outcomes a scored candidate can land in. A closed set, because
# "confirmed" vs "proposed" drives whether a human has to look at it.
run jails g enum MatchClassification CONFIRMED PROPOSED AMBIGUOUS UNMATCHED

# One rule's contribution to a score. `detail:string?` because most rules have
# nothing to add beyond the number, but reference-similarity wants to report
# the similarity it computed.
run jails g value ScoreEvidence rule:string! contribution:double detail:string?

# The scoring policy, versioned. Weights and thresholds are configurable and
# the version is stamped onto every decision, so a later rescore is
# explainable rather than mysterious.
run jails g value MatchPolicy version:string! amountWeight:double referenceWeight:double \
                              dateWeight:double confirmThreshold:double proposeThreshold:double \
                              ambiguityGap:double

# What gets attached to a decision so it can be audited later: which policy
# version ran, what it concluded, and the evidence behind it.
run jails g value DecisionMeta policyVersion:string! confidence:double \
                               classification:MatchClassification \
                               'evidence:list<ScoreEvidence>'

# One ranked candidate.
run jails g value ScoredCandidate candidateId:string! confidence:double \
                                  'evidence:list<ScoreEvidence>'

run jails g command Match

fixtures 03-confidence clear-winner.json close-race.json exact-tie.json malformed-policy.json

# ---- assertions ------------------------------------------------------------

section "g enum" "classification is a closed set"
has "$DOMAIN/MatchClassification.java" 'public enum MatchClassification' 'enum generated'
has "$DOMAIN/MatchClassification.java" 'CONFIRMED'                      'declares CONFIRMED'
has "$DOMAIN/MatchClassification.java" 'UNMATCHED'                      'declares UNMATCHED'

section "double fields" "scores and weights"
has   "$DOMAIN/ScoreEvidence.java" 'double contribution' 'contribution is a primitive double'
lacks "$DOMAIN/ScoreEvidence.java" 'Double contribution' 'not boxed'
has   "$DOMAIN/MatchPolicy.java"   'double ambiguityGap' 'policy thresholds are doubles'

section "? suffix" "optional detail"
has "$DOMAIN/ScoreEvidence.java" 'Optional<String> detail' 'rule detail is Optional'

section "list<T>" "evidence travels with the decision"
has "$DOMAIN/DecisionMeta.java"    'List<ScoreEvidence> evidence' 'DecisionMeta carries evidence'
has "$DOMAIN/ScoredCandidate.java" 'List<ScoreEvidence> evidence' 'candidate carries evidence'

section "capitalized types" "enum as a component"
has   "$DOMAIN/DecisionMeta.java" 'MatchClassification classification' 'enum used as a component type'
lacks "$DOMAIN/DecisionMeta.java" 'String classification'              'not stringly typed'

section wiring "command reachable"
has "$SRC/App.java" 'MatchCommand' 'App dispatches to MatchCommand'

build

verdict "workout 3 (confidence)"
