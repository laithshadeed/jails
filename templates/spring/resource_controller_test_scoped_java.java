package {{pkg}};

{{extra}}{{guard_import}}import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {{webmvc_test_import}};

import static org.assertj.core.api.Assertions.assertThat;

@WebMvcTest({{name}}Controller.class)
class {{name}}ControllerTest {

    @Autowired private MockMvcTester mvc;
    @MockitoBean private {{name}}Service service;
    @MockitoBean private ScopeAuthorizer scopeAuthorizer;

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri({{name}}Controller.PATH)).hasStatus(405);
        assertThat(mvc.get().uri({{name}}Controller.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
