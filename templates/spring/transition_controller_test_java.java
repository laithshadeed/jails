package {{web}};

{{command_import}}{{usecase_import}}{{target_import}}{{scope_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

class {{name}}ControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new {{name}}Controller(
            command -> new {{target}}(
                    {{target_args}}){{scope_argument}}));

{{annotation}}    @Test
    void putExecutesTheTransition() {
        assertThat(mvc.put().uri({{name}}Controller.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
{{json}}
}
"""))
                .hasStatusOk();
    }

}
