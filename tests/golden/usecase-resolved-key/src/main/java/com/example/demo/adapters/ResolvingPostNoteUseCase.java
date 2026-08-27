package com.example.demo.adapters;

import com.example.demo.domain.Note;
import com.example.demo.domain.SenderType;
import com.example.demo.service.PostNoteCommand;
import com.example.demo.service.PostNoteUseCase;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * Creates a {@link Note} for the {@link Author} identified by
 * {@code email}, as one statement.
 *
 * <p>The insert selects {@code author_id} from {@code authors}
 * rather than trusting one the caller supplied, so a caller cannot name a row
 * that is not theirs and there is no window between the read and the write.
 * An empty result means no {@link Author} matched.
 */
@Component
public class ResolvingPostNoteUseCase implements PostNoteUseCase {

    private final JdbcClient db;

    public ResolvingPostNoteUseCase(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public Optional<Note> execute(PostNoteCommand command) {
        Objects.requireNonNull(command, "command is required");
        return db.sql("""
                        insert into notes (author_id, body, sender_type)
                        select authors.id, :body, :sender_type
                        from authors
                        where authors.email = :email
                        returning id, author_id, body, sender_type
                        """)
                .param("body", command.body())
                .param("sender_type", SenderType.CUSTOMER.name())
                .param("email", command.email())
                .query(ResolvingPostNoteUseCase::map)
                .optional();
    }

    private static Note map(ResultSet rows, int rowNumber) throws SQLException {
        return new Note(
                rows.getLong("id"),
                rows.getLong("author_id"),
                rows.getString("body"),
                SenderType.valueOf(rows.getString("sender_type")));
    }
}
