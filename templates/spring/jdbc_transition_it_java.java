package {{pkg}};

{{target_import}}{{command_import}}{{port_import}}{{repository_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

{{annotation}}@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Jdbc{{name}}TransitionIT {

    @Autowired private {{target}}Repository repository;
    @Autowired private {{name}}UseCase useCase;

    @Test
    void updatesOnceAndRejectsTheStaleVersionWithoutAnotherMutation() {
        repository.save(new {{target}}(
                {{target_args}}));
        var command = new {{name}}Command(
                {{command_args}});

        var updated = useCase.execute(command);

        assertThat(updated.version()).isEqualTo(command.version() + 1);
        assertThatThrownBy(() -> useCase.execute(command))
                .isInstanceOf({{name}}UseCase.StaleVersionException.class);
        assertThat(repository.findById({{key_argument}}))
                .get().extracting({{target}}::version)
                .isEqualTo(updated.version());
    }
{{wrong_scope_test}}
}
