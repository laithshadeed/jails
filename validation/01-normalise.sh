#!/usr/bin/env bash
#
# Workout 1 -- multi-source transaction normalisation.
#   brief: stacks/workouts/01-normalise.md
#
# Two provider shapes in, one canonical shape out, with record-level errors
# reported rather than dropped.
#
# jails features exercised:  g enum, capitalized field types, !/? suffixes
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start normalise

# Jackson + the java.time module, plus adapters/Json.java. Needs `readTree`
# specifically: malformed.json has a bare string and an array sitting inside
# the record arrays, and binding the whole document to a type would lose every
# valid sibling along with them.
run jails add json

# testkit/Fixtures (classpath fixture loading), Cli (drive a command
# in-process and capture its streams), Clocks and Ids (deterministic time and
# identifiers). Fixtures is the one this workout leans on.
run jails add testkit

# A plain Java enum. The brief's canonical shape has a Currency component, and
# unsupported currencies (JPY appears in the fixtures) must be rejected as a
# ValidationIssue -- which needs a closed set, not a String.
run jails g enum Currency GBP EUR USD

# Provenance. Both components are required and non-blank: a record whose
# provenance is an empty string is not traceable, which is the whole point of
# carrying it.
run jails g value SourceRef system:string! externalId:string!

# The canonical shape every later workout consumes.
#   id:string!            blank IDs are a documented validation issue
#   amountMinor:long      integer minor units -- never floating point
#   currency:Currency     capitalized -> a type this project owns
#   source:SourceRef      ditto; composition, not a stringly-typed field
#   description:string    bare: required, but blank is ALLOWED. Two fixtures
#                         carry "description": "" and the brief never forbids it
run jails g value CanonicalTransaction id:string! date:date amountMinor:long \
                                       currency:Currency source:SourceRef description:string

# A record-level rejection. `source:SourceRef?` -> Optional<SourceRef>: an
# issue can predate knowing which record it came from (a non-object element
# has no provenance to report).
run jails g value ValidationIssue path:string! code:string! message:string! source:SourceRef?

# The `normalise` subcommand: run(out, err, args) returning an exit code, with
# App wired to dispatch to it.
run jails g command Normalise

fixtures 01-normalise happy.json malformed.json ordering.json impossible-date.json

# ---- assertions ------------------------------------------------------------

section "g enum" "closed set of currencies"
has "$DOMAIN/Currency.java" 'public enum Currency' 'Currency is an enum'
has "$DOMAIN/Currency.java" 'GBP'                  'declares GBP'
has "$DOMAIN/Currency.java" 'USD'                  'declares USD'

section "capitalized types" "used verbatim, not degraded"
has   "$DOMAIN/CanonicalTransaction.java" 'Currency currency'  'Currency component'
has   "$DOMAIN/CanonicalTransaction.java" 'SourceRef source'   'SourceRef component'
lacks "$DOMAIN/CanonicalTransaction.java" 'String currency'    'Currency was not made a String'

section "field table" "lowercase types unchanged"
has   "$DOMAIN/CanonicalTransaction.java" 'long amountMinor' 'money is a primitive long'
lacks "$DOMAIN/CanonicalTransaction.java" 'Long amountMinor' 'money is not boxed'
has   "$DOMAIN/CanonicalTransaction.java" 'LocalDate date'   'date maps to LocalDate'

section "!/? suffixes" "per-field rules"
has   "$DOMAIN/CanonicalTransaction.java" 'requireNonNull\(id'          'id: null rejected'
has   "$DOMAIN/CanonicalTransaction.java" 'id.*(isEmpty|isBlank)'       'id: blank rejected (!)'
has   "$DOMAIN/CanonicalTransaction.java" 'requireNonNull\(description' 'description: null rejected'
lacks "$DOMAIN/CanonicalTransaction.java" 'description.*(isEmpty|isBlank)' \
                                                                        'description: blank ALLOWED (bare)'
has   "$DOMAIN/ValidationIssue.java" 'Optional<SourceRef> source'       'issue source is Optional (?)'

section wiring "command reachable"
has "$SRC/App.java" 'NormaliseCommand' 'App dispatches to NormaliseCommand'

build

# `!` means non-blank, and only a String can be blank. Silently ignoring it on
# other types would leave you believing you had added a constraint.
section errors "misapplied suffixes rejected"
rejects "date! rejected" jails g value Bad occurredOn:date!

verdict "workout 1 (normalise)"
