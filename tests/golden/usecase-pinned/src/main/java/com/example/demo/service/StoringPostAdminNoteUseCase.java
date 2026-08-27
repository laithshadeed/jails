package com.example.demo.service;

import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import com.example.demo.domain.SenderType;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * The implementation that stores the resource and does nothing else.
 *
 * <p>Named for what it does rather than for its position. `Default` is what
 * you call a class when you have not decided what it is, and it gave the
 * reader no way to tell this apart from {@code OutboxPostAdminNoteUseCase}, which
 * stores the resource <em>and</em> stages its event.
 */
@Component
public class StoringPostAdminNoteUseCase implements PostAdminNoteUseCase {

    private final NoteRepository repository;

    public StoringPostAdminNoteUseCase(NoteRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Note execute(PostAdminNoteCommand command) {
        Objects.requireNonNull(command, "command is required");
        Note note = new Note(
                0L,
                command.authorId(),
                command.body(),
                SenderType.ADMIN);
        return repository.save(note);
    }
}
