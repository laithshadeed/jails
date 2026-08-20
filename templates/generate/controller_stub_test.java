package {{pkg}};

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import {{mockmvc_import}};
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

@SpringBootTest
@AutoConfigureMockMvc
class {{name}}ControllerTest {

    @Autowired
    private MockMvcTester mvc;

    @Test
    void getReturnsOk() {
        assertThat(mvc.get().uri("/{{route}}"))
                .hasStatusOk()
                .bodyText()
                .isEqualTo("{{name}}");
    }
}
