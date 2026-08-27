package com.example.demo.adapters;

import com.example.demo.domain.Topic;
import com.example.demo.service.SetTopicSubjectCommand;
import com.example.demo.service.SetTopicSubjectUseCase;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap. */
@Component
public class JdbcSetTopicSubjectTransition implements SetTopicSubjectUseCase {

    private final JdbcClient db;

    public JdbcSetTopicSubjectTransition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public SetTopicSubjectUseCase.Result execute(
            Long userId, SetTopicSubjectCommand command, long expectedVersion) {
        Objects.requireNonNull(userId, "userId is required");
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update topics
                        set subject = :subject,
                            version = version + 1
                        where user_id = :user_id
                          and version = :version
                        returning id, user_id, subject, version
                        """)
                .param("user_id", userId)
                .param("subject", command.subject())
                .param("version", expectedVersion)
                .query(JdbcSetTopicSubjectTransition::map)
                .optional();
        if (updated.isPresent()) {
            return new SetTopicSubjectUseCase.Result.Applied(updated.orElseThrow());
        }

        // Nothing moved, and the two reasons are different facts: the row is
        // at another version -- in which case the caller wants to see which,
        // and gets it -- or there is no such row at all.
        return db.sql("""
                        select id, user_id, subject, version
                        from topics
                        where user_id = :user_id
                        """)
                .param("user_id", userId)
                .query(JdbcSetTopicSubjectTransition::map)
                .optional()
                .<SetTopicSubjectUseCase.Result>map(SetTopicSubjectUseCase.Result.StaleVersion::new)
                .orElseGet(() -> new SetTopicSubjectUseCase.Result.NotFound(userId));
    }

    private static Topic map(ResultSet rows, int rowNumber) throws SQLException {
        return new Topic(
                rows.getLong("id"),
                rows.getLong("user_id"),
                rows.getString("subject"),
                rows.getLong("version"));
    }
}
