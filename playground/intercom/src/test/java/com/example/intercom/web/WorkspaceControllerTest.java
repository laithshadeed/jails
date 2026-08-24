package com.example.intercom.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.Mockito.mock;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.WorkspaceService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class WorkspaceControllerTest {

    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        var service = mock(WorkspaceService.class);
        var scopeAuthorizer = mock(ScopeAuthorizer.class);
        mvc = MockMvcTester.of(new WorkspaceController(service, scopeAuthorizer));
    }

    @Test
    void broadUnscopedReadsAreNotExposed() {
        assertThat(mvc.get().uri(WorkspaceController.PATH)).hasStatus(405);
        assertThat(mvc.get().uri(WorkspaceController.PATH + "/other-tenant-id")).hasStatus(404);
    }
}
