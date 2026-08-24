package com.example.demo.domain;

/**
 * An immutable Verification value.
 *
 * <p>There is nothing to validate: no instance of this record can be in an
 * invalid state.
 */
public record Verification(boolean success) {
}
