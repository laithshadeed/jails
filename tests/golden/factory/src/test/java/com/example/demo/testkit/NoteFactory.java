package com.example.demo.testkit;

import com.example.demo.domain.Note;
import java.time.Instant;
import java.util.UUID;

/** Mutable test-data builder for {@link Note}. */
public final class NoteFactory {
    private UUID id = UUID.fromString("00000000-0000-0000-0000-000000000001");
    private String title = "sample";
    private Instant createdAt = Instant.parse("2024-01-01T00:00:00Z");

    public static NoteFactory aNote() {
return new NoteFactory();
}

    public NoteFactory withId(UUID value) {
this.id = value;
return this;
}

    public NoteFactory withTitle(String value) {
this.title = value;
return this;
}

    public NoteFactory withCreatedAt(Instant value) {
this.createdAt = value;
return this;
}

    public Note build() {
        return new Note(
                id,
                title,
                createdAt);
    }
}
