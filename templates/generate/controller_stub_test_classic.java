package {{pkg}};

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.{{handler}};
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import org.junit.jupiter.api.Test;
{{disabled_import}}import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import {{mockmvc_import}};
import org.springframework.test.web.servlet.MockMvc;

@SpringBootTest
@AutoConfigureMockMvc
class {{name}}ControllerTest {

    @Autowired private MockMvc mvc;

    @Test
{{disabled}}    void {{handler}}Answers() throws Exception {
        mvc.perform({{handler}}("{{path}}")){{assertion}};
    }
}
