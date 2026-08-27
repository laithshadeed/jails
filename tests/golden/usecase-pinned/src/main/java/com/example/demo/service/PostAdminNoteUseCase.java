package com.example.demo.service;

import com.example.demo.domain.Note;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface PostAdminNoteUseCase {

    Note execute(PostAdminNoteCommand command);
}
