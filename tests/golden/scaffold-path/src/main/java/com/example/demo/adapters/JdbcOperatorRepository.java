package com.example.demo.adapters;

import com.example.demo.app.OperatorRepository;
import com.example.demo.domain.Operator;
import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.Optional;

/**
 * {@link OperatorRepository} over plain JDBC. No ORM: the queries are visible,
 * and a {@code PreparedStatement} is the whole abstraction.
 *
 * <p>The caller owns the {@link Connection} -- this class neither opens nor
 * closes it, so one transaction can span several repositories.
 *
 * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
 */
public final class JdbcOperatorRepository implements OperatorRepository {

    private static final String FIND_BY_ID =
            """
            select
                id,
                email
            from operators
            where id = ?
            """;
    private static final String FIND_ALL =
            """
            select
                id,
                email
            from operators
            order by id
            """;
    private static final String INSERT =
            """
            insert into operators (email)
            values (?)
            """;
    private static final String DELETE_BY_ID =
            """
            delete from operators
            where id = ?
            """;

    private final Connection connection;

    public JdbcOperatorRepository(Connection connection) {
        this.connection = Objects.requireNonNull(connection, "connection is required");
    }

    @Override
    public Optional<Operator> findById(Long id) {
        Objects.requireNonNull(id, "id is required");
        try (var query = connection.prepareStatement(FIND_BY_ID)) {
            query.setObject(1, id);
            try (var rows = query.executeQuery()) {
                return rows.next() ? Optional.of(map(rows)) : Optional.empty();
            }
        } catch (SQLException error) {
            throw new IllegalStateException("could not read operators " + id, error);
        }
    }

    @Override
    public List<Operator> findAll() {
        // Ordered explicitly: SQL does not otherwise promise row order, and
        // this table has no timestamp to order by.
        try (var query = connection.prepareStatement(FIND_ALL);
                var rows = query.executeQuery()) {
            var all = new ArrayList<Operator>();
            while (rows.next()) {
                all.add(map(rows));
            }
            return List.copyOf(all);
        } catch (SQLException error) {
            throw new IllegalStateException("could not read operators", error);
        }
    }

    @Override
    public Operator save(Operator operator) {
        Objects.requireNonNull(operator, "operator is required");
        try (var insert = connection.prepareStatement(INSERT, Statement.RETURN_GENERATED_KEYS)) {
            bind(insert, operator);
            insert.executeUpdate();
            try (var keys = insert.getGeneratedKeys()) {
                if (!keys.next()) {
                    throw new IllegalStateException("operators assigned no key");
                }
                return new Operator(
                        keys.getObject(1, Long.class),
                        operator.email());
            }
        } catch (SQLException error) {
            throw new IllegalStateException("could not save to operators", error);
        }
    }

    @Override
    public boolean deleteById(Long id) {
        Objects.requireNonNull(id, "id is required");
        try (var delete = connection.prepareStatement(DELETE_BY_ID)) {
            delete.setObject(1, id);
            return delete.executeUpdate() > 0;
        } catch (SQLException error) {
            throw new IllegalStateException("could not delete from operators " + id, error);
        }
    }

    /** Builds a Operator from the current row. */
    private Operator map(ResultSet rows) throws SQLException {
        return new Operator(
                rows.getLong("id"),
                rows.getString("email"));
    }

    /** Sets every column the insert above declares, in that order. */
    private void bind(java.sql.PreparedStatement insert, Operator operator) throws SQLException {
        insert.setObject(1, operator.email());
    }
}
