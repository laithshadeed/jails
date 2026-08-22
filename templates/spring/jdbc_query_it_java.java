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
    private {{name}}QueryPort queryPort;

    @Test
    void filtersInTheRealDatabase() {
        {{target}} stored = new {{target}}(
                {{target_args}});
        repository.save(stored);

        var found = queryPort.execute(new {{name}}Query(
                {{query_args}}));

        assertThat(found).contains(stored);
    }
}
