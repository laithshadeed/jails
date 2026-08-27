package {{pkg}};

{{target_import}}{{command_import}}{{port_import}}{{container_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

import static org.assertj.core.api.Assertions.assertThat;

{{annotation}}@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Ensuring{{name}}UseCaseIT {

    @Autowired
    private {{name}}UseCase useCase;

    /**
     * The behaviour the whole kind exists for, and the one an in-memory fake
     * could not prove: {@code on conflict} is the database's decision, so only
     * the database can be asked whether it was made.
     */
    @Test
    void twoCallsWithTheSameKeyAreOneRow() {
        {{name}}Command command = new {{name}}Command(
                {{args}});

        {{target}} first = useCase.execute(command);
        {{target}} second = useCase.execute(command);

        assertThat(second).isEqualTo(first);
        assertThat(second.{{conflict_component}}()).isEqualTo(first.{{conflict_component}}());
    }
}
