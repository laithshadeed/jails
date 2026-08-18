#!/usr/bin/env bash
#
# Workout 9 -- variance analysis with a grounded narrative.
#   brief: stacks/workouts/09-variance-agent.md
#
# Deterministic code owns every accounting number and builds an evidence
# bundle. The model only writes prose, and that prose is verified against the
# bundle -- invented vendors, unsupported amounts and unknown IDs are
# rejected. No real LLM: tests drive a scripted double.
#
# jails features exercised:  add fake, list<T>, g enum, instant,
#                            g sealed (the new one)
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start variance

run jails add json
run jails add testkit

# A scripted test double for any interface, driven by a lambda. This is how
# the model provider gets stubbed -- including the failure modes the brief
# demands: a timeout and malformed output.
run jails add fake

run jails g enum Currency GBP EUR USD

# Absolute vs percentage thresholds, and whether both must trip or either.
run jails g enum MaterialityMode EITHER BOTH

# One contributing driver, with the transactions behind it. `evidenceId` is
# what the narrative must cite -- prose referencing anything not in this list
# is what verification rejects.
run jails g value Driver evidenceId:string! dimension:string! label:string! \
                         contributionMinor:long 'sourceTransactionIds:list<string>'

# The evidence bundle handed to the model. Versioned, because a narrative is
# only meaningful against the bundle it was generated from.
run jails g value EvidenceBundle evidenceVersion:string! account:string! \
                                 currentMinor:long comparisonMinor:long \
                                 varianceMinor:long 'drivers:list<Driver>'

# Thresholds, with per-account overrides.
run jails g value MaterialityPolicy version:string! mode:MaterialityMode \
                                    absoluteMinor:long percentage:double \
                                    'overrides:map<string,long>'

# The headline feature.
#
# `g sealed <Name> <Variant>...` should emit a sealed interface plus one record
# per variant, which is the modern-Java way to model a closed set of outcomes
# that carry different data -- an enum cannot, because each case has its own
# payload. Verification here has exactly that shape:
#
#   Verified(String narrative)          -- prose cleared for use
#   Unsupported(List<String> claims)    -- cites amounts the bundle cannot back
#   Unknown(List<String> ids)           -- cites evidence IDs that do not exist
#   Timeout(Duration waited)            -- the model never answered
#
# Sealed means a switch over it is checked for exhaustiveness: add a fifth
# outcome later and every consumer fails to compile until it handles it.
run jails g sealed VerificationResult Verified Unsupported Unknown Timeout

# What is retained afterwards. Model output and human-edited text are stored
# SEPARATELY -- the brief is explicit -- along with every version involved.
run jails g value NarrativeRecord modelOutput:string! humanEdited:string? \
                                  modelVersion:string! promptVersion:string! \
                                  evidenceVersion:string! verifiedAt:instant

run jails g command Variance

fixtures 09-variance-agent material.json immaterial.json driver-grouping.json \
                          grounded-narrative.json unsupported-narrative.json \
                          model-timeout.json zero-and-negative-baseline.json

# ---- assertions ------------------------------------------------------------

section "g sealed" "closed set with per-variant payloads"
has "$DOMAIN/VerificationResult.java" 'sealed interface VerificationResult' 'sealed interface generated'
has "$DOMAIN/VerificationResult.java" 'permits'                             'permits clause lists variants'
has "$DOMAIN/VerificationResult.java" 'record Verified'                     'Verified variant'
has "$DOMAIN/VerificationResult.java" 'record Timeout'                      'Timeout variant'
# The reason to use sealed at all: exhaustiveness. The generated test should
# switch over it without a default arm.
exists "$TEST/domain/VerificationResultTest.java" 'sealed type has a companion test'
has "$TEST/domain/VerificationResultTest.java" 'switch'                     'test switches over the variants'

section "add fake" "scripted model double"
exists "$TEST/testkit/Fake.java" 'fake helper generated'
has "$TEST/testkit/Fake.java" 'interface|Proxy|lambda|Function' 'drives any interface from a lambda'

section "evidence bundle" "deterministic layer owns the numbers"
has "$DOMAIN/EvidenceBundle.java" 'List<Driver> drivers'   'drivers list'
has "$DOMAIN/EvidenceBundle.java" 'long varianceMinor'     'variance is integer minor units'
has "$DOMAIN/EvidenceBundle.java" 'String evidenceVersion' 'bundle is versioned'
has "$DOMAIN/Driver.java" 'List<String> sourceTransactionIds' 'driver names its transactions'

section "? suffix" "human edit is optional"
# Absent means nobody edited it -- different from an empty string, which would
# mean somebody deleted the text.
has "$DOMAIN/NarrativeRecord.java" 'Optional<String> humanEdited' 'human edit is Optional'
has "$DOMAIN/NarrativeRecord.java" 'String modelOutput'           'original model output retained'

section "map<K,V>" "per-account overrides"
has "$DOMAIN/MaterialityPolicy.java" 'Map<String, ?Long> overrides' 'overrides map generated'
has "$DOMAIN/MaterialityPolicy.java" 'MaterialityMode mode'         'either/both is an enum'

section "no real model" "nothing reaches the network"
for forbidden in HttpClient api.anthropic openai java.net.URL; do
  lacks "$DOMAIN/NarrativeRecord.java" "$forbidden" "no $forbidden in the domain"
done

section wiring "command reachable"
has "$SRC/App.java" 'VarianceCommand' 'App dispatches to VarianceCommand'

build

verdict "workout 9 (variance agent)"
