package com.example.demo.service;

import com.example.demo.domain.Note;
import java.util.Optional;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface PostNoteUseCase {

    /** Empty when no parent matched the component the caller sent. */
    Optional<Note> execute(PostNoteCommand command);
}
