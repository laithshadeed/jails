package com.example.demo.adapters;

import com.example.demo.domain.Note;
import com.example.demo.service.MarkSeenCommand;
import com.example.demo.service.MarkSeenUseCase;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap. */
@Component
public class JdbcMarkSeenTransition implements MarkSeenUseCase {

    private final JdbcClient db;

    public JdbcMarkSeenTransition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public MarkSeenUseCase.Result execute(
            Long id, MarkSeenCommand command, Long expectedVersion) {
        Objects.requireNonNull(id, "id is required");
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update notes
                        set seen = :seen,
                            version = version + 1
                        where id = :id
                          and version = coalesce(:version, version)
                        returning id, body, seen, version
                        """)
                .param("id", id)
                .param("seen", true)
                .param("version", expectedVersion)
                .query(JdbcMarkSeenTransition::map)
                .optional();
        if (updated.isPresent()) {
            return new MarkSeenUseCase.Result.Applied(updated.orElseThrow());
        }

        // Nothing moved, and the two reasons are different facts: the row is
        // at another version -- in which case the caller wants to see which,
        // and gets it -- or there is no such row at all.
        return db.sql("""
                        select id, body, seen, version
                        from notes
                        where id = :id
                        """)
                .param("id", id)
                .query(JdbcMarkSeenTransition::map)
                .optional()
                .<MarkSeenUseCase.Result>map(MarkSeenUseCase.Result.StaleVersion::new)
                .orElseGet(() -> new MarkSeenUseCase.Result.NotFound(id));
    }

    private static Note map(ResultSet rows, int rowNumber) throws SQLException {
        return new Note(
                rows.getLong("id"),
                rows.getString("body"),
                rows.getBoolean("seen"),
                rows.getLong("version"));
    }
}
