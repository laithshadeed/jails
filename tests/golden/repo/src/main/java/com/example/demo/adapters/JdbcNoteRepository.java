package com.example.demo.adapters;

import com.example.demo.app.NoteRepository;
import com.example.demo.domain.Note;
import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;

/**
 * {@link NoteRepository} over plain JDBC. No ORM: the queries are visible,
 * and a {@code PreparedStatement} is the whole abstraction.
 *
 * <p>The caller owns the {@link Connection} -- this class neither opens nor
 * closes it, so one transaction can span several repositories.
 *
 * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
 */
public final class JdbcNoteRepository implements NoteRepository {

    private static final String FIND_BY_ID =
            """
            select
                id,
                title
            from notes
            where id = ?
            """;
    private static final String FIND_ALL =
            """
            select
                id,
                title
            from notes
            order by id
            """;
    private static final String INSERT =
            """
            insert into notes (id, title)
            values (?, ?)
            """;
    private static final String DELETE_BY_ID =
            """
            delete from notes
            where id = ?
            """;

    private final Connection connection;

    public JdbcNoteRepository(Connection connection) {
        this.connection = Objects.requireNonNull(connection, "connection is required");
    }

    @Override
    public Optional<Note> findById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        try (var query = connection.prepareStatement(FIND_BY_ID)) {
            query.setObject(1, id);
            try (var rows = query.executeQuery()) {
                return rows.next() ? Optional.of(map(rows)) : Optional.empty();
            }
        } catch (SQLException error) {
            throw new IllegalStateException("could not read notes " + id, error);
        }
    }

    @Override
    public List<Note> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order.
        try (var query = connection.prepareStatement(FIND_ALL);
                var rows = query.executeQuery()) {
            var all = new ArrayList<Note>();
            while (rows.next()) {
                all.add(map(rows));
            }
            return List.copyOf(all);
        } catch (SQLException error) {
            throw new IllegalStateException("could not read notes", error);
        }
    }

    @Override
    public Note save(Note note) {
        Objects.requireNonNull(note, "note is required");
        try (var insert = connection.prepareStatement(INSERT)) {
            bind(insert, note);
            insert.executeUpdate();
            return note;
        } catch (SQLException error) {
            throw new IllegalStateException("could not save to notes", error);
        }
    }

    @Override
    public boolean deleteById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        try (var delete = connection.prepareStatement(DELETE_BY_ID)) {
            delete.setObject(1, id);
            return delete.executeUpdate() > 0;
        } catch (SQLException error) {
            throw new IllegalStateException("could not delete from notes " + id, error);
        }
    }

    /** Builds a Note from the current row. */
    private Note map(ResultSet rows) throws SQLException {
        return new Note(
                rows.getObject("id", UUID.class),
                rows.getString("title"));
    }

    /** Sets every column the insert above declares, in that order. */
    private void bind(java.sql.PreparedStatement insert, Note note) throws SQLException {
        insert.setObject(1, note.id());
        insert.setObject(2, note.title());
    }
}
