package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.Mockito.mock;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.ContactService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class ContactControllerTest {

    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        var service = mock(ContactService.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new ContactController(service, scopeAuthorizer));
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri(ContactController.PATH)).hasStatus(405);
        assertThat(mvc.get().uri(ContactController.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
