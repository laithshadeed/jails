package com.example.demo.service;

import org.springframework.web.bind.annotation.BindParam;

/**
 * Validated input for the MarkNoteSeen use case.
 *
 * <p>There is nothing to validate: no instance of this record can be in an
 * invalid state.
 */
public record MarkNoteSeenCommand(@BindParam("note_id") long id) {
}
