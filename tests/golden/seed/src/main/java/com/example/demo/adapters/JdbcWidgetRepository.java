package com.example.demo.adapters;

import com.example.demo.app.WidgetRepository;
import com.example.demo.domain.Widget;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/**
 * {@link WidgetRepository} over {@link JdbcClient}. No ORM: the queries are
 * visible, and the only abstraction is a named parameter.
 *
 * <p>Parameters are named rather than positional on purpose. A {@code ?} list
 * is a silent-swap bug waiting for a schema change -- reorder two columns of
 * the same type and nothing fails to compile and nothing throws.
 *
 * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
 */
@Component
public final class JdbcWidgetRepository implements WidgetRepository {

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {@code amount} in the insert against
     * {@code amount_minor} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
            id,
            name
            """;

    private final JdbcClient db;

    public JdbcWidgetRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Widget> findById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from widgets
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcWidgetRepository::map)
                .optional();
    }

    @Override
    public List<Widget> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order, and
        // this table has no timestamp to order by.
        return db.sql("""
                        select %s
                        from widgets
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcWidgetRepository::map)
                .list();
    }

    @Override
    public Widget save(Widget widget) {
        Objects.requireNonNull(widget, "widget is required");
        db.sql("""
                        insert into widgets (id, name)
                        values (:id, :name)
                        """)
                .param("id", widget.id())
                .param("name", widget.name())
                .update();
        return widget;
    }

    @Override
    public boolean deleteById(UUID id) {
        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from widgets
                        where id = :id
                        """)
                .param("id", id)
                .update()
                > 0;
    }

    /** Builds a Widget from the current row. */
    private static Widget map(ResultSet rows, int rowNumber) throws SQLException {
        return new Widget(
                rows.getObject("id", UUID.class),
                rows.getString("name"));
    }
}
