package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.Mockito.mock;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.ConversationService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class ConversationControllerTest {

    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        var service = mock(ConversationService.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new ConversationController(service, scopeAuthorizer));
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri(ConversationController.PATH)).hasStatus(405);
        assertThat(mvc.get().uri(ConversationController.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
