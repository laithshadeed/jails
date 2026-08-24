package com.example.ledgercli.domain;

/**
 * The outcomes a LedgerError can have.
 *
 * <p>Sealed rather than an enum because each case carries its own data --
 * give a variant the components it needs and no other case has to pretend
 * to have them.
 *
 * <p>A switch over this is checked for exhaustiveness, so leave the
 * {@code default} off: adding a variant should make the compiler point at
 * every place that has to handle it.
 *
 * {@snippet :
 * var summary = switch (result) {
 *     case MalformedRow v -> "malformedrow";
 *     case UnknownCurrency v -> "unknowncurrency";
 *     case DuplicateReference v -> "duplicatereference";
 * };
 * }
 */
public sealed interface LedgerError
        permits LedgerError.MalformedRow, LedgerError.UnknownCurrency, LedgerError.DuplicateReference {

    /** TODO: give MalformedRow the components it carries. */
    record MalformedRow() implements LedgerError {}

    /** TODO: give UnknownCurrency the components it carries. */
    record UnknownCurrency() implements LedgerError {}

    /** TODO: give DuplicateReference the components it carries. */
    record DuplicateReference() implements LedgerError {}
}
