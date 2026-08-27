package {{pkg}};

{{target_import}}{{command_import}}{{port_import}}{{imports}}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * Get-or-create keyed on {@code {{conflict_component}}}, as one statement.
 *
 * <p>Not the repository: a port with a {@code save(T)} cannot express
 * {@code on conflict do nothing returning}, and read-then-insert leaves the
 * window where two callers both see nothing and both proceed. The insert wins
 * and returns its row, or does nothing and the second statement reads the row
 * that was already there -- one transaction, so never half of either.
 *
 * <p>The conflict column must carry a unique index. jails cannot check that
 * from here, because a record read off disk carries no constraints; the
 * generated {@code IT} checks it against a real database instead.
 */
@Component
public class Ensuring{{name}}UseCase implements {{name}}UseCase {

    private final JdbcClient db;

    public Ensuring{{name}}UseCase(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public {{target}} execute({{name}}Command command) {
        Objects.requireNonNull(command, "command is required");
{{preamble}}        {{target}} candidate = new {{target}}(
{{args}});
        var created = db.sql("""
                        insert into {{table}} ({{columns}})
                        values ({{placeholders}})
                        on conflict ({{conflict_target}}) do nothing
                        returning {{select}}
                        """)
{{bindings}}
                .query(Ensuring{{name}}UseCase::map)
                .optional();
        if (created.isPresent()) {
            return created.orElseThrow();
        }

        // Somebody else has this key. Read their row rather than reporting a
        // conflict: that is the whole difference between this and an insert.
        return db.sql("""
                        select {{select}}
                        from {{table}}
                        where {{conflict_predicate}}
                        """)
                .param("{{conflict_column}}", {{conflict_write}})
                .query(Ensuring{{name}}UseCase::map)
                .optional()
                .orElseThrow(() -> new IllegalStateException(
                        "{{target}} with this {{conflict_component}} was claimed by a transaction "
                                + "that then rolled back; retry"));
    }

    private static {{target}} map(ResultSet rows, int rowNumber) throws SQLException {
        return new {{target}}(
{{map_args}});
    }
}
