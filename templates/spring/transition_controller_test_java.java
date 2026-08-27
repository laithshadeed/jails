package {{web}};

{{command_import}}{{usecase_import}}{{target_import}}{{scope_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.http.HttpHeaders;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

class {{name}}ControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new {{name}}Controller(
            ({{id_component}}, command, expectedVersion) -> new {{name}}UseCase.Result.Applied(new {{target}}(
                    {{target_args}})){{scope_argument}}));

{{annotation}}    @Test
    void {{verb}}ExecutesTheTransitionAndReturnsTheNewVersionAsAnETag() {
        assertThat(mvc.{{verb}}().uri({{name}}Controller.PATH{{path_arguments}})
                .header(HttpHeaders.IF_MATCH, "\"{{sample_version}}\"")
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
{{json}}
}
"""))
                .hasStatusOk()
                .hasHeader(HttpHeaders.ETAG, "\"{{sample_version}}\"");
    }

{{annotation}}    @Test
    void aRequestWithNoIfMatchIsRefusedRatherThanAppliedBlind() {
        assertThat(mvc.{{verb}}().uri({{name}}Controller.PATH{{path_arguments}})
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
{{json}}
}
"""))
                .hasStatus(400);
    }

}
