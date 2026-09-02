# `jails-codec-derive`

The `#[derive(Codec)]` proc macro: one canonical wire encoding per type,
derived from the declaration so the encode and decode halves cannot disagree.

It writes absolute `jails_support::codec::...` paths into every impl, which is
why `jails-support` names itself with `extern crate self as jails_support`.

Scheduled for review with the transaction kernel: `docs/51-kernel.md` S51.4
decides whether the daemon's wire protocol keeps it.
