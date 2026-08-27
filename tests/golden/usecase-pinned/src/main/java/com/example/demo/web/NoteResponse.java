package com.example.demo.web;

import com.example.demo.domain.Note;
import com.example.demo.domain.SenderType;

/**
 * What this application returns. Deliberately not Note itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record NoteResponse(
        Long id,
        Long authorId,
        String body,
        SenderType senderType) {

    /** @return the response describing {@code note}. */
    public static NoteResponse from(Note note) {
        return new NoteResponse(
                note.id(),
                note.authorId(),
                note.body(),
                note.senderType());
    }
}
