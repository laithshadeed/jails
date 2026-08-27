package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

import com.example.demo.service.PersonService;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class PersonControllerTest {

    /**
     * The body {@code requests/person.http} documents, generated from
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
            {
              "email": "sample-email",
              "createdAt": "2026-01-01T00:00:00Z"
            }
            """;

    private PersonService service;
    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        service = mock(PersonService.class);
        mvc = MockMvcTester.of(new PersonController(service));
    }

    @Test
    void theDocumentedCreateRequestIsAccepted() {
        given(service.create(any())).willAnswer(invocation -> invocation.getArgument(0));

        assertThat(mvc.post()
                        .uri(PersonController.PATH)
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(CREATE_REQUEST))
                .hasStatus(201);
    }

    @Test
    void anEmptyCollectionIsAnEmptyArray() {
        given(service.findAll()).willReturn(List.of());

        assertThat(mvc.get().uri(PersonController.PATH))
                .hasStatusOk()
                .bodyJson()
                .isEqualTo("[]");
    }

    @Test
    void aMissingItemIs404() {
        UUID missing = UUID.fromString("00000000-0000-0000-0000-000000000002");
        given(service.findById(missing)).willReturn(Optional.empty());

        assertThat(mvc.get().uri(PersonController.PATH + "/" + missing)).hasStatus(404);
    }

    @Test
    void aDeleteThatRemovedNothingIs404() {
        UUID missing = UUID.fromString("00000000-0000-0000-0000-000000000002");
        given(service.deleteById(missing)).willReturn(false);

        assertThat(mvc.delete().uri(PersonController.PATH + "/" + missing)).hasStatus(404);
    }
}
