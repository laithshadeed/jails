package {{pkg}};

{{target_import}}{{command_import}}{{port_import}}{{repository_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

import static org.assertj.core.api.Assertions.assertThat;

{{annotation}}@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Jdbc{{name}}TransitionIT {

    @Autowired private {{target}}Repository repository;
    @Autowired private {{name}}UseCase useCase;

    @Test
    void appliesOnceAndReportsTheStaleVersionWithoutAnotherMutation() {
        var stored = repository.save(new {{target}}(
                {{target_args}}));
        var command = new {{name}}Command(
                {{command_args}});

        var applied = useCase.execute({{key_expression}}, command, {{expected_version}});

        assertThat(applied).isInstanceOf({{name}}UseCase.Result.Applied.class);
        var resource = (({{name}}UseCase.Result.Applied) applied).resource();
        assertThat(resource.version()).isEqualTo({{expected_version}} + 1);

        // The same expectation a second time is stale, and the outcome
        // carries the row as it now stands rather than a message about it.
        var again = useCase.execute({{key_expression}}, command, {{expected_version}});
        assertThat(again).isInstanceOf({{name}}UseCase.Result.StaleVersion.class);
        assertThat((({{name}}UseCase.Result.StaleVersion) again).current()).isEqualTo(resource);
        assertThat(repository.findById({{key_argument}})).contains(resource);
    }
{{unconditional_test}}{{wrong_scope_test}}}
