package {{pkg}};

{{extra}}{{key_import}}import static org.mockito.ArgumentMatchers.any;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.delete;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;
import static org.springframework.test.web.servlet.setup.MockMvcBuilders.standaloneSetup;

import java.util.List;
import java.util.Optional;
{{disabled_import}}import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

/**
 * Written against plain {@code MockMvc} rather than {@code MockMvcTester},
 * because this project's Spring Framework predates the AssertJ entry point.
 */
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
    private MockMvc mvc;

    @BeforeEach
    void setUp() {
        service = mock({{name}}Service.class);
        mvc = standaloneSetup(new {{name}}Controller(service)).build();
    }

    {{disabled}}@Test
    void theDocumentedCreateRequestIsAccepted() throws Exception {
        given(service.create(any())).willAnswer(invocation -> invocation.getArgument(0));

        mvc.perform(post({{name}}Controller.PATH)
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(CREATE_REQUEST))
                .andExpect(status().isCreated());
    }

    @Test
    void anEmptyCollectionIsAnEmptyArray() throws Exception {
        given(service.findAll()).willReturn(List.of());

        mvc.perform(get({{name}}Controller.PATH))
                .andExpect(status().isOk())
                .andExpect(content().json("[]"));
    }

    @Test
    void aMissingItemIs404() throws Exception {
        {{key}} missing = {{absent}};
        given(service.findById(missing)).willReturn(Optional.empty());

        mvc.perform(get({{name}}Controller.PATH + "/" + missing)).andExpect(status().isNotFound());
    }

    @Test
    void aDeleteThatRemovedNothingIs404() throws Exception {
        {{key}} missing = {{absent}};
        given(service.deleteById(missing)).willReturn(false);

        mvc.perform(delete({{name}}Controller.PATH + "/" + missing)).andExpect(status().isNotFound());
    }
}
