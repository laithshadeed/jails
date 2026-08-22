package {{pkg}};

{{target_import}}{{command_import}}{{port_import}}{{imports}}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.Objects;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** One SQL compare-and-swap: scoped matches cannot mutate another tenant's row. */
@Component
public class Jdbc{{name}}Transition implements {{name}}UseCase {

    private final JdbcClient db;

    public Jdbc{{name}}Transition(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    @Transactional
    public {{target}} execute({{name}}Command command) {
        Objects.requireNonNull(command, "command is required");
        var updated = db.sql("""
                        update {{table}}
                        set {{assignments}}
                        where {{optimistic_predicates}}
                        returning {{select}}
                        """)
{{update_bindings}}
                .query(Jdbc{{name}}Transition::map)
                .optional();
        if (updated.isPresent()) return updated.orElseThrow();

        boolean existsInScope = db.sql("""
                        select exists(
                            select 1 from {{table}}
                            where {{existence_predicates}}
                        )
                        """)
{{existence_bindings}}
                .query(Boolean.class)
                .single();
        if (existsInScope) throw new {{name}}UseCase.StaleVersionException();
        throw new {{name}}UseCase.NotFoundException();
    }

    private static {{target}} map(ResultSet rows, int rowNumber) throws SQLException {
        return new {{target}}(
{{map_args}});
    }
}
