package {{pkg}};

{{target_import}}{{command_import}}{{port_import}}{{imports}}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap{{scope_clause}}. */
@Component
public class Jdbc{{name}}Transition implements {{name}}UseCase {

    private final JdbcClient db;

    public Jdbc{{name}}Transition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public {{name}}UseCase.Result execute(
            {{key_type}} {{id_component}}, {{name}}Command command, {{version_type}} expectedVersion) {
        Objects.requireNonNull({{id_component}}, "{{id_component}} is required");
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update {{table}}
                        set {{assignments}}
                        where {{optimistic_predicates}}
                        returning {{select}}
                        """)
{{update_bindings}}
                .param("version", expectedVersion)
                .query(Jdbc{{name}}Transition::map)
                .optional();
        if (updated.isPresent()) {
            return new {{name}}UseCase.Result.Applied(updated.orElseThrow());
        }

        // Nothing moved, and the two reasons are different facts: the row is
        // at another version -- in which case the caller wants to see which,
        // and gets it -- or there is no such row at all.
        return db.sql("""
                        select {{select}}
                        from {{table}}
                        where {{existence_predicates}}
                        """)
{{existence_bindings}}
                .query(Jdbc{{name}}Transition::map)
                .optional()
                .<{{name}}UseCase.Result>map({{name}}UseCase.Result.StaleVersion::new)
                .orElseGet(() -> new {{name}}UseCase.Result.NotFound({{id_component}}));
    }

    private static {{target}} map(ResultSet rows, int rowNumber) throws SQLException {
        return new {{target}}(
{{map_args}});
    }
}
