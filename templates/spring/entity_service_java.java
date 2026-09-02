package {{pkg}};

import java.util.List;
import java.util.Objects;
import java.util.Optional;

/**
 * What the application can do with {@link {{name}}}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
{{component}}public class {{class}} {

    private final {{name}}Repository repository;

    public {{class}}({{name}}Repository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public Optional<{{name}}> byId({{key_type}} id) {
        return repository.findById(id);
    }

    public List<{{name}}> all() {
        return repository.findAll();
    }

    public {{name}} save({{name}} {{variable}}) {
        return repository.save({{variable}});
    }

    public boolean delete({{key_type}} id) {
        return repository.deleteById(id);
    }

    // Reader-owned application methods belong below this stable boundary.
}
