package {{pkg}};

import java.util.List;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Repository;

@Repository
public final class {{class}} implements {{name}}Search {

    private static final String SQL =
            """
            select {{columns}}
              from {{table}}
             where {{search_column}} @@ websearch_to_tsquery('{{search_configuration}}', :query)
             order by ts_rank({{search_column}}, websearch_to_tsquery('{{search_configuration}}', :query)) desc
             limit :limit
            """;

    private final JdbcClient jdbc;

    public {{class}}(JdbcClient jdbc) {
        this.jdbc = jdbc;
    }

    @Override
    public List<{{name}}> matching(String query, int limit) {
        return jdbc.sql(SQL)
                .param("query", query)
                .param("limit", limit)
                .query({{name}}.class)
                .list();
    }
}
