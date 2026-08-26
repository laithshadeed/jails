package {{pkg}};

{{extra}}{{key_import}}import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link {{name}}}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class {{name}}Service {

    private final {{name}}Repository repository;

    public {{name}}Service({{name}}Repository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<{{name}}> findAll() {
        return repository.findAll();
    }

    public Optional<{{name}}> findById({{key}} id) {
        return repository.findById(id);
    }

    public {{name}} create({{name}} {{var}}) {
        return repository.save({{var}});
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById({{key}} id) {
        return repository.deleteById(id);
    }
}
