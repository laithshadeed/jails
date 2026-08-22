package {{web}};

{{command_import}}{{usecase_import}}{{target_import}}{{scope_import}}{{imports}}{{disabled_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {{webmvc_test_import}};

import static org.assertj.core.api.Assertions.assertThat;

@WebMvcTest({{name}}Controller.class)
@Import({{name}}ControllerTest.Config.class)
class {{name}}ControllerTest {

    @Autowired private MockMvcTester mvc;

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

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {
        @Bean
        {{name}}UseCase useCase() {
            return command -> new {{target}}(
                    {{target_args}});
        }
{{scope_bean}}    }
}
