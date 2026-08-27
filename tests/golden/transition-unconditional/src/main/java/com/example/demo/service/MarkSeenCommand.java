package com.example.demo.service;

/**
 * Validated input for the MarkSeen use case.
 *
 * <p>There is nothing to validate: no instance of this record can be in an
 * invalid state.
 */
public record MarkSeenCommand(long id) {
}
