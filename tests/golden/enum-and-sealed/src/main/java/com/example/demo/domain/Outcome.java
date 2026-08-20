package com.example.demo.domain;

/**
 * The outcomes a Outcome can have.
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
 *     case Accepted v -> "accepted";
 *     case Rejected v -> "rejected";
 * };
 * }
 */
public sealed interface Outcome permits Outcome.Accepted, Outcome.Rejected {

    /** TODO: give Accepted the components it carries. */
    record Accepted() implements Outcome {}

    /** TODO: give Rejected the components it carries. */
    record Rejected() implements Outcome {}
}
