package {{web}};

{{command_import}}{{usecase_import}}{{target_import}}{{scope_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
{{media_type_import}}import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

class {{name}}ControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new {{name}}Controller(
            command -> {{fake_result}}{{scope_argument}}));

{{disabled}}    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri({{name}}Controller.PATH)
{{request}})
                .hasStatus(201);
    }

}
