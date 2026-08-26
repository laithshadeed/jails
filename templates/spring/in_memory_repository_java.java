package {{pkg}};

{{extra}}import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
{{key_import}}{{repository_import}}
/**
 * {@link {{name}}Repository} in memory, so the application runs before it has
 * a database.
 *
{{note}} *
 * <p>{@link ConcurrentHashMap} rather than {@link java.util.HashMap}: a web
 * application serves requests on many threads at once, and an unsynchronised
 * map corrupts silently under exactly the load that makes it hard to
 * reproduce.
 *
{{role_note}} */
{{repository_annotation}}public class InMemory{{name}}Repository implements {{name}}Repository {

    private final Map<{{key}}, {{name}}> items = new ConcurrentHashMap<>();

    @Override
    public Optional<{{name}}> findById({{key}} id) {
{{find_by_id}}
    }

    @Override
    public List<{{name}}> findAll() {
        return List.copyOf(items.values());
    }

    @Override
    public void save({{name}} {{var}}) {
{{save_body}}
    }

    @Override
    public boolean deleteById({{key}} id) {
{{delete_by_id}}
    }
}
