#!/usr/bin/env bash
#
# Workout 10 -- the orchestrator: versioned tools, policies, replay, override.
#   brief: stacks/workouts/10-orchestrator.md
#
# Work items are routed to tools, deterministic evidence is collected first,
# and a versioned policy decides auto-resolve / propose / escalate. Restricted
# actions always need a human. The audit package explains the whole chain.
#
# Everything the previous nine workouts introduced, composed. If this script
# passes, the jails feature set covers the gym.
#
# jails features exercised:  g sealed, g enum, list<T>, map<K,V>, instant,
#                            g repo, ?/! suffixes, add sqlite/http/fake
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start orchestrator

run jails add json
run jails add testkit
run jails add sqlite
run jails add http
run jails add fake

run jails g enum Currency GBP EUR USD

# What the orchestrator may do with a work item.
run jails g enum ActionClass AUTO_RESOLVE PROPOSE ESCALATE

# The nine versioned tools from the brief. An enum because "which tool ran" is
# stamped into the audit package and must be a closed, spellable set.
run jails g enum ToolName NORMALISE_TRANSACTIONS EXACT_RECONCILE RANK_MATCH_CANDIDATES \
                          APPLY_CASH RECONCILE_VAT RECONCILE_INTERCOMPANY \
                          INVESTIGATE_VARIANCE VALIDATE_JOURNAL \
                          GENERATE_GROUNDED_EXPLANATION

run jails g value SourceRef system:string! externalId:string!
run jails g value ValidationIssue path:string! code:string! message:string! source:SourceRef?

# A tool invocation and its result, versioned. Replay means being able to
# re-run this exact call and get the same answer.
run jails g value ToolCall tool:ToolName toolVersion:string! \
                           inputHash:string! calledAt:instant

# Tool outcomes carry different payloads, so this is a sealed type, not an
# enum: a success has output, a failure has a reason, a timeout has a
# duration. A switch over it is exhaustiveness-checked.
run jails g sealed ToolOutcome Succeeded Failed TimedOut

# The routing policy, versioned like everything else.
run jails g value OrchestrationPolicy version:string! \
                                      'restrictedActions:list<string>' \
                                      autoResolveThreshold:double

# A decision, with everything needed to explain or supersede it.
#
# `supersededBy:string?` is how "retain superseded decisions" is modelled:
# nothing is deleted, a decision just points at the one that replaced it.
run jails g value Decision id:string! workItemId:string! action:ActionClass \
                           policyVersion:string! 'toolCalls:list<ToolCall>' \
                           decidedAt:instant supersededBy:string?

# A human override. The reason is mandatory -- `string!` is the requirement
# "human override with mandatory reason" expressed as a type, so an empty
# justification cannot be constructed.
run jails g value Override decisionId:string! actor:string! reason:string! at:instant

# The audit package: the full chain, with every version that shaped it.
run jails g value AuditPackage workItemId:string! 'decisions:list<Decision>' \
                               'overrides:list<Override>' \
                               'versions:map<string,string>' \
                               'issues:list<ValidationIssue>'

# Persisted, replayable, append-only.
run jails g repo Decision
run jails g repo AuditPackage

run jails g command Orchestrate

fixtures 10-orchestrator work-items.json policy.json propose.json \
                         restricted-journal.json retry-idempotent.json \
                         stale-decision.json human-override.json \
                         tool-failure.json model-timeout.json audit-package.json

# ---- assertions ------------------------------------------------------------

section "g sealed" "tool outcomes carry different payloads"
has "$DOMAIN/ToolOutcome.java" 'sealed interface ToolOutcome' 'sealed interface'
has "$DOMAIN/ToolOutcome.java" 'record Succeeded'             'success variant'
has "$DOMAIN/ToolOutcome.java" 'record TimedOut'              'timeout variant'
has "$DOMAIN/ToolOutcome.java" 'permits'                      'permits clause'

section "g enum" "action classes and tools"
has "$DOMAIN/ActionClass.java" 'AUTO_RESOLVE'                    'auto-resolve'
has "$DOMAIN/ActionClass.java" 'ESCALATE'                        'escalate'
has "$DOMAIN/ToolName.java"    'GENERATE_GROUNDED_EXPLANATION'   'all nine tools generated'
has "$DOMAIN/Decision.java"    'ActionClass action'              'action is the enum'

section "versioning" "every decision is explainable"
has "$DOMAIN/Decision.java" 'String policyVersion'      'policy version stamped'
has "$DOMAIN/ToolCall.java" 'String toolVersion'        'tool version stamped'
has "$DOMAIN/ToolCall.java" 'ToolName tool'             'tool identity is the enum'
has "$DOMAIN/AuditPackage.java" 'Map<String, ?String> versions' 'version map on the package'

section "? suffix" "supersede rather than delete"
has "$DOMAIN/Decision.java" 'Optional<String> supersededBy' 'superseded pointer is Optional'

section "! suffix" "mandatory override reason"
has "$DOMAIN/Override.java" 'reason.*(isEmpty|isBlank)' 'blank override reason rejected'
has "$DOMAIN/Override.java" 'Instant at'                'override is timestamped'

section "g repo" "replayable persistence"
exists "$APP/DecisionRepository.java"            'decision port'
exists "$ADAPTERS/SqliteDecisionRepository.java" 'decision adapter'
lacks "$APP/DecisionRepository.java" 'java.sql'  'port free of JDBC types'

section "capabilities stack" "all five together"
exists "$ADAPTERS/Json.java"    'json'
exists "$ADAPTERS/Database.java" 'sqlite'
exists "$API/Server.java"       'http'
exists "$TEST/testkit/Cli.java" 'testkit'
exists "$TEST/testkit/Fake.java" 'fake'

section wiring "command reachable"
has "$SRC/App.java" 'OrchestrateCommand' 'App dispatches to OrchestrateCommand'

build

verdict "workout 10 (orchestrator)"
