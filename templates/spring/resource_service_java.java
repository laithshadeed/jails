package {{pkg}};

{{extra}}{{key_import}}{{uuid_import}}import java.util.List;
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

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public {{name}} create({{name}} {{var}}) {
        return repository.save({{created}});
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById({{key}} id) {
        return repository.deleteById(id);
    }
}
