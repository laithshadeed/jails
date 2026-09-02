package {{pkg}};

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.List;
import org.springframework.boot.ApplicationArguments;
import org.springframework.boot.ApplicationRunner;
import org.springframework.context.annotation.Profile;
import org.springframework.stereotype.Component;

/**
 * Development data for {@link {{name}}}, from
 * {@code src/main/resources/{{resource}}}.
 *
 * <p>Through the port rather than SQL, so a row the record rejects fails at
 * start-up rather than sitting in the table. Under the {@code seed} profile
 * only ({@code SPRING_PROFILES_ACTIVE=seed jails run}), and only into an empty
 * table: an edited seed row cannot be told from a change made in the database.
 */
@Component
@Profile("seed")
public class {{name}}Seeder implements ApplicationRunner {

    private static final String RESOURCE = "/{{resource}}";

    private final {{name}}Repository repository;

    public {{name}}Seeder({{name}}Repository repository) {
        this.repository = repository;
    }

    @Override
    public void run(ApplicationArguments arguments) {
        if (!repository.findAll().isEmpty()) {
            return;
        }
        for (var row : read()) {
            repository.save(row);
        }
    }

    /** The file as records; package-private so its test can read it without a database. */
    static List<{{name}}> read() {
        try (var in = {{name}}Seeder.class.getResourceAsStream(RESOURCE)) {
            if (in == null) {
                throw new IllegalStateException("no seed data at src/main/resources" + RESOURCE);
            }
            return {{json}}.readList(in, {{name}}.class);
        } catch (IOException error) {
            throw new UncheckedIOException("unreadable seed data: " + RESOURCE, error);
        }
    }
}
