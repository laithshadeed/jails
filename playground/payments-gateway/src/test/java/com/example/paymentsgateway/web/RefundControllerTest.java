package com.example.paymentsgateway.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.Mockito.mock;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.RefundService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class RefundControllerTest {

    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        var service = mock(RefundService.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new RefundController(service, scopeAuthorizer));
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri(RefundController.PATH)).hasStatus(405);
        assertThat(mvc.get().uri(RefundController.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
