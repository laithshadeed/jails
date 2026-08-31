# `jails-codec-derive`

The `#[derive(Codec)]` proc macro: one canonical wire encoding per type, derived from the type rather than written twice.

---

## Purpose & Overview

`jails-support`'s `codec` states the rule — one canonical encoding per type, and every wire decoder calls the same constructor, so a value rejected at the CLI cannot arrive through a recovered journal instead. A hand-written codec is two chances to state that encoding and only one of them is checked.

This crate derives the pair, so the encode and decode halves cannot disagree.

---

## The trap it sets, recorded so it is not walked into twice

`#[derive(Codec)]` writes **absolute paths** (`jails_support::codec::...`) into every impl it generates, and those do not resolve inside `jails-support` itself. That crate names itself with

```rust
extern crate self as jails_support;
```

so the macro's output compiles there exactly as it does in a dependent.

---

## What is left to convert

133 types still have a hand-written wire format, and `tests/architecture/`'s *types whose wire format is hand-written* row counts them. **Convert by reading, one at a time.** A regex sweep found its ceiling: `PreparedIdentityV1` passed all 62 golden trees and still changed the wire.
