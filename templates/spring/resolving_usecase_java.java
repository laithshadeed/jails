package {{pkg}};

{{target_import}}{{command_import}}{{port_import}}{{imports}}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * Creates a {@link {{target}}} for the {@link {{parent}}} identified by
 * {@code {{lookup}}}, as one statement.
 *
 * <p>The insert selects {@code {{child_column}}} from {@code {{parent_table}}}
 * rather than trusting one the caller supplied, so a caller cannot name a row
 * that is not theirs and there is no window between the read and the write.
 * An empty result means no {@link {{parent}}} matched.
 */
@Component
public class Resolving{{name}}UseCase implements {{name}}UseCase {

    private final JdbcClient db;

    public Resolving{{name}}UseCase(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public Optional<{{target}}> execute({{name}}Command command) {
        Objects.requireNonNull(command, "command is required");
{{preamble}}        return db.sql("""
                        insert into {{table}} ({{columns}})
                        select {{selected}}
                        from {{parent_table}}
                        where {{parent_table}}.{{lookup_column}} = :{{lookup_column}}
                        returning {{select}}
                        """)
{{bindings}}
                .query(Resolving{{name}}UseCase::map)
                .optional();
    }

    private static {{target}} map(ResultSet rows, int rowNumber) throws SQLException {
        return new {{target}}(
{{map_args}});
    }
}
