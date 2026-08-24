package {{pkg}};

{{extra}}{{guard_import}}{{disabled_import}}import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

class {{name}}ControllerTest {

    /**
     * The body {@code requests/{{route_file}}.http} documents, generated from
     * the same builder.
     *
     * <p>One source, two readers, for the reason every other pair in this tool
     * shares one: a collection describing a request the record refuses is a
     * request nobody can make, and it shipped. A timestamped scaffold asked the
     * caller for {@code createdAt} and {@code updatedAt}, so its own documented
     * POST answered 400 naming two columns the create path sets itself.
     */
    private static final String CREATE_REQUEST =
            """
{{create_body}}            """;

    private {{name}}Service service;
    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        service = mock({{name}}Service.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new {{name}}Controller(service, scopeAuthorizer));
    }

    {{disabled}}@Test
    void theDocumentedCreateRequestIsAccepted() {
        given(service.create(any())).willAnswer(invocation -> invocation.getArgument(0));

        assertThat(mvc.post()
                        .uri({{name}}Controller.PATH)
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(CREATE_REQUEST))
                .hasStatus(201);
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri({{name}}Controller.PATH)).hasStatus(405);
        assertThat(mvc.get().uri({{name}}Controller.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
