package {{web}};

{{port_import}}{{target_import}}{{scope_import}}{{imports}}{{disabled_import}}import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

class {{name}}QueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new {{name}}QueryController(
            criteria -> List.of(new {{target}}(
                    {{target_args}})){{scope_argument}}));

{{annotation}}    @Test
    void getExecutesTheDatabaseQuery() {
        assertThat(mvc.get()
                .uri({{name}}QueryController.PATH)
{{params}})
                .hasStatusOk()
                .bodyJson();
    }

}
