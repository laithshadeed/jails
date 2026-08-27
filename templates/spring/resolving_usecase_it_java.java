package {{pkg}};

{{parent_import}}{{command_import}}{{port_import}}{{parent_repository_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;

{{annotation}}@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Resolving{{name}}UseCaseIT {

    @Autowired private {{parent}}Repository parents;
    @Autowired private {{name}}UseCase useCase;

    @Test
    void resolvesTheKeyFromTheParentAndReportsWhenThereIsNoParent() {
        var command = new {{name}}Command(
                {{command_args}});

        // Nothing to resolve against yet: the empty result is the answer.
        assertThat(useCase.execute(command)).isEmpty();

        var parent = parents.save(new {{parent}}(
                {{parent_args}}));

        var created = useCase.execute(command);
        assertThat(created).isPresent();
        assertThat(created.orElseThrow().{{child_component}}()).isEqualTo(parent.{{parent_component}}());
    }
}
