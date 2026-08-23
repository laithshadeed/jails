package {{pkg}};

{{extra}}{{guard_import}}import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.Mockito.mock;

class {{name}}ControllerTest {

    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        var service = mock({{name}}Service.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new {{name}}Controller(service, scopeAuthorizer));
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri({{name}}Controller.PATH)).hasStatus(405);
        assertThat(mvc.get().uri({{name}}Controller.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
