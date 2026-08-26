package {{pkg}};

{{target_import}}{{query_import}}{{port_import}}{{repository_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;

{{annotation}}@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Jdbc{{name}}QueryIT {

    @Autowired
    private {{target}}Repository repository;

    @Autowired
    private {{name}}Query query;

    @Test
    void filtersInTheRealDatabase() {
        // The stored row, not the argument: with a database-assigned key the
        // two differ by exactly the component the query filters on.
        {{target}} stored = repository.save(new {{target}}(
                {{target_args}}));

        var found = query.execute(new {{name}}Criteria(
                {{query_args}}));

        assertThat(found).contains(stored);
    }
}
