package {{pkg}};

{{target_import}}{{adapter_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

{{disabled}}class {{name}}UseCaseTest {

    private final InMemory{{target}}Repository repository = new InMemory{{target}}Repository();
    private final {{name}}UseCase useCase = new Storing{{name}}UseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        {{name}}Command command = new {{name}}Command(
                {{args}});

        {{target}} created = useCase.execute(command);

{{id_assertion}}
{{copied}}
        assertThat(repository.findById({{key_argument}})).contains(created);
    }
{{two_creates_test}}}
