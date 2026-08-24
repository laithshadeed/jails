package com.example.paymentsgateway.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.RefundService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class RefundControllerTest {

    /**
     * The body {@code requests/refund.http} documents, generated from
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
              "id": "00000000-0000-0000-0000-000000000001",
              "merchantId": "00000000-0000-0000-0000-000000000001",
              "paymentId": "00000000-0000-0000-0000-000000000001",
              "amountMinor": 1,
              "reason": null
            }
            """;

    private RefundService service;
    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        service = mock(RefundService.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new RefundController(service, scopeAuthorizer));
    }

    @Test
    void theDocumentedCreateRequestIsAccepted() {
        given(service.create(any())).willAnswer(invocation -> invocation.getArgument(0));

        assertThat(mvc.post()
                        .uri(RefundController.PATH)
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(CREATE_REQUEST))
                .hasStatus(201);
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri(RefundController.PATH)).hasStatus(405);
        assertThat(mvc.get().uri(RefundController.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
