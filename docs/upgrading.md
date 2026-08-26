# Upgrading jails durable state

Jails fails closed on unknown durable formats. Upgrade the executable before
opening a project written by a newer schema; do not hand-edit `.jails` machine
state, transaction journals, receipts, or prepared plans.

The current compatibility boundaries are listed in
[`compatibility.tsv`](compatibility.tsv). Ledger payload v1 remains read-only:
the next successful mutation decodes it with an empty lifecycle registry and
publishes v2. Journals and receipts retain their existing authenticated v1
formats. Resource lifecycle adoption adds stable entity identity, explicit
table bindings, migration seals, and rolling-rename campaigns without changing
previous migration bytes.

Prepared plans use `jails.prepared-plan.v1`. They intentionally do not survive
a jails tool-version, protocol, canonical-root, generation, catalog-input, or
file-preimage change. Re-run the original command with `--pretend --plan-out`
instead of editing a rejected plan. Plans may contain complete postimage bytes,
so store them as private short-lived review artifacts and do not commit them.

Receipt inspection is additive. `history` and `show` authenticate the existing
receipt before rendering provenance, risk, evidence, and before/after digests.
`undo` never rewrites history or generates rollback migrations; an eligible
file-only undo is recorded as another forward transaction.
