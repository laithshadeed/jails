package {{pkg}};

import java.util.List;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Repository;

@Repository
public final class {{class}} implements {{name}}Repository {

    private final JdbcClient jdbc;

    public {{class}}(JdbcClient jdbc) {
        this.jdbc = jdbc;
    }

    @Override
    public Optional<{{name}}> findById({{key_type}} id) {
        return jdbc.sql("select {{columns}} from {{table}} where {{key_column}} = :id")
                .param("id", id)
                .query({{name}}.class)
                .optional();
    }

    @Override
    public List<{{name}}> findAll() {
        return jdbc.sql("select {{columns}} from {{table}} order by {{key_column}}")
                .query({{name}}.class)
                .list();
    }

    @Override
    public {{name}} save({{name}} value) {
        return jdbc.sql("insert into {{table}} ({{insert_columns}}) values ({{insert_values}}){{conflict}} returning {{columns}}"){{bindings}}
                .query({{name}}.class)
                .single();
    }

    @Override
    public boolean deleteById({{key_type}} id) {
        return jdbc.sql("delete from {{table}} where {{key_column}} = :id")
                .param("id", id)
                .update() > 0;
    }
}
