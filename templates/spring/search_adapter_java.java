package {{adapters}};

{{extra}}import java.util.List;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Repository;

/**
 * PostgreSQL full-text search against the generated {@code {{column}}} column.
 *
 * <p>Two details that decide whether this works at all.
 *
 * <p><b>{@code websearch_to_tsquery}, not {@code to_tsquery}.</b> The latter
 * demands operator syntax and throws a syntax error on anything a person would
 * actually type -- a bare two-word phrase included. The former accepts what a
 * search box produces, quotes and {@code OR} and {@code -} and all, and never
 * throws on malformed input. A search endpoint that 500s on an apostrophe is
 * the failure this avoids.
 *
 * <p><b>The query is a bind parameter.</b> It is text PostgreSQL parses, not
 * SQL it executes, so there is no injection surface -- and no need for the
 * escaping that a concatenated query would have to get right every time.
 */
@Repository
public class Jdbc{{name}}Search implements {{name}}Search {

    private static final String SQL =
            """
            select {{columns}}
              from {{table}}
             where {{column}} @@ websearch_to_tsquery('{{configuration}}', :query)
             order by ts_rank({{column}}, websearch_to_tsquery('{{configuration}}', :query)) desc
             limit :limit
            """;

    private final JdbcClient jdbc;

    public Jdbc{{name}}Search(JdbcClient jdbc) {
        this.jdbc = jdbc;
    }

    @Override
    public List<{{name}}> matching(String query, int limit) {
        return jdbc.sql(SQL)
                .param("query", query)
                .param("limit", limit)
                // The row mapper is derived from the same column list as the
                // select above, so the two cannot name different columns --
                // which is the drift `sql.rs` exists to remove.
                .query((rows, rowNumber) -> new {{name}}({{mapper}}))
                .list();
    }
}
