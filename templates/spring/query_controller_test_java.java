package {{web}};

{{port_import}}{{target_import}}{{scope_import}}{{imports}}{{disabled_import}}import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {{webmvc_test_import}};

import static org.assertj.core.api.Assertions.assertThat;

@WebMvcTest({{name}}QueryController.class)
@Import({{name}}QueryControllerTest.Config.class)
class {{name}}QueryControllerTest {

    @Autowired
    private MockMvcTester mvc;

{{annotation}}    @Test
    void postExecutesTheDatabaseQueryPort() {
        assertThat(mvc.post()
                .uri({{name}}QueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
{{json}}
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {

        @Bean
        {{name}}QueryPort queryPort() {
            return query -> List.of(new {{target}}(
                    {{target_args}}));
        }
{{scope_bean}}
    }
}
