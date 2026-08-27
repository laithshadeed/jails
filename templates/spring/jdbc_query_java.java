package {{pkg}};

{{target_import}}{{query_import}}{{port_import}}{{imports}}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible, named-parameter SQL generated from the target and filter field models. */
@Component
public final class Jdbc{{name}}Query implements {{name}}Query {

    /** Equality queries are deliberately bounded; use a keyset query for navigation. */
    private static final int MAX_RESULTS = 100;

    private static final String COLUMNS =
            """
{{select}}
            """;

    private final JdbcClient db;

    public Jdbc{{name}}Query(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<{{target}}> execute({{name}}Criteria criteria) {
        Objects.requireNonNull(criteria, "criteria is required");
        return db.sql("""
                        select %s
                        from {{from}}
                        where {{predicates}}
                        order by {{order}}
                        limit :max_results
                        """.formatted(COLUMNS))
{{bindings}}
                .param("max_results", MAX_RESULTS)
                .query(Jdbc{{name}}Query::map)
                .list();
    }

    private static {{target}} map(ResultSet rows, int rowNumber) throws SQLException {
        return new {{target}}(
{{map_args}});
    }
}
