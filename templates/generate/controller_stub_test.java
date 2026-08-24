package {{pkg}};

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;
{{disabled_import}}import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import {{mockmvc_import}};
import org.springframework.test.web.servlet.assertj.MockMvcTester;

@SpringBootTest
@AutoConfigureMockMvc
class {{name}}ControllerTest {

    @Autowired private MockMvcTester mvc;

    @Test
{{disabled}}    void {{handler}}Answers() {
        assertThat(mvc.{{handler}}().uri("/{{route}}"))
{{assertion}};
    }
}
