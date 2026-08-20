package {{pkg}};

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link Jdbc{{name}}Repository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Disabled("todo: configure the test database and finish the repository SQL mapping")
class Jdbc{{name}}RepositoryIT {

    @Test
    void roundTripsThroughTheRealDatabase() {
        throw new UnsupportedOperationException("todo");
    }
}
