#!/usr/bin/env bash
#
# Workout 4 -- cash application from an out-of-order event stream.
#   brief: stacks/workouts/04-cash-application.md
#
# JSONL events arrive in any order and may be delivered twice. Payments move
# through a state machine, every transition is audited, and an approved
# decision must never silently change.
#
# jails features exercised:  Json.readJsonl (the new one), `instant` field
#                            type (also new), g enum with many constants,
#                            list<T>, capitalized types
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start cashapply

# This workout's input is JSONL -- one JSON object per line, not a JSON array.
# `Json` needs a readJsonl(Path) -> List<JsonNode> to go with readTree: an
# event log is the canonical JSONL use case, and splitting on newlines by hand
# is exactly the plumbing this is meant to remove.
run jails add json
run jails add testkit

run jails g enum  Currency GBP EUR USD

# The payment state machine. Seven constants -- worth checking the generator
# handles a long list and multi-word names, since these become
# AWAITING_REMITTANCE etc.
run jails g enum PaymentState RECEIVED AWAITING_REMITTANCE AWAITING_INVOICE \
                              PROPOSED APPROVED REJECTED UNRESOLVED

# The event types the stream carries. Closed set: an unrecognised event type
# is a validation issue, not a silently ignored line.
run jails g enum EventType PAYMENT_RECEIVED INVOICE_CREATED REMITTANCE_RECEIVED \
                           PAYMENT_REVIEWED PAYMENT_APPROVED PAYMENT_REJECTED

run jails g value SourceRef system:string! externalId:string!
run jails g value ValidationIssue path:string! code:string! message:string! source:SourceRef?

# One payment applied to one invoice.
run jails g value Allocation invoiceId:string! amountMinor:long currency:Currency

# A payment and everything currently allocated against it.
run jails g value Payment id:string! state:PaymentState amountMinor:long \
                          currency:Currency 'allocations:list<Allocation>'

# One state transition, append-only.
#
# `at:instant` is the new field type: the brief's audit entry is an Instant,
# not a LocalDateTime. An audit timestamp is a moment on a global timeline --
# LocalDateTime has no offset and is the wrong type for it. `datetime` already
# maps to LocalDateTime, so this needs its own token.
# `from`/`to` were the original spelling. jails refuses both -- they are
# PostgreSQL reserved words and would make the generated SQL invalid -- and
# names the fix, which is a domain-specific pair. The brief's meaning is
# unchanged; only the columns are nameable now.
run jails g value AuditEntry entityId:string! movedFrom:string! movedTo:string! at:instant

run jails g value CashApplyResult 'payments:list<Payment>' \
                                  'audit:list<AuditEntry>' \
                                  'issues:list<ValidationIssue>'

run jails g command CashApply

fixtures 04-cash-application payment-first.jsonl out-of-order.jsonl \
                             duplicate-delivery.jsonl conflicting-event-id.jsonl \
                             approval-lock.jsonl

# ---- assertions ------------------------------------------------------------

section "Json.readJsonl" "one object per line"
has "$ADAPTERS/Json.java" 'readJsonl'          'readJsonl generated'
has "$ADAPTERS/Json.java" 'List<JsonNode>'     'returns parsed nodes, not raw strings'
# Trailing newlines and blank lines are normal in an appended-to log; they are
# not parse errors.
has "$ADAPTERS/Json.java" 'isBlank|isEmpty'    'blank lines skipped'
has "$TEST/adapters/JsonTest.java" 'readJsonl' 'readJsonl is covered by a test'

section "instant type" "audit timestamps"
has   "$DOMAIN/AuditEntry.java" 'Instant at'              'instant -> java.time.Instant'
has   "$DOMAIN/AuditEntry.java" 'import java.time.Instant;' 'Instant imported'
lacks "$DOMAIN/AuditEntry.java" 'LocalDateTime'           'not confused with datetime'

section "g enum" "long constant lists"
has "$DOMAIN/PaymentState.java" 'AWAITING_REMITTANCE' 'multi-word constant kept verbatim'
has "$DOMAIN/PaymentState.java" 'UNRESOLVED'          'last constant present'
has "$DOMAIN/EventType.java"    'PAYMENT_APPROVED'    'event types generated'

section "composition" "enums and lists together"
has "$DOMAIN/Payment.java"         'PaymentState state'         'state is the enum'
has "$DOMAIN/Payment.java"         'List<Allocation> allocations' 'allocations is a list'
has "$DOMAIN/CashApplyResult.java" 'List<AuditEntry> audit'     'audit trail in the result'

section wiring "command reachable"
has "$SRC/App.java" 'CashApplyCommand' 'App dispatches to CashApplyCommand'

build

verdict "workout 4 (cash application)"
