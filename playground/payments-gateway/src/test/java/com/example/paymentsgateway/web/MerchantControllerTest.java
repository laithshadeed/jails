package com.example.paymentsgateway.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.Mockito.mock;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.MerchantService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class MerchantControllerTest {

    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        var service = mock(MerchantService.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new MerchantController(service, scopeAuthorizer));
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri(MerchantController.PATH)).hasStatus(405);
        assertThat(mvc.get().uri(MerchantController.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
