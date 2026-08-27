package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Note;
import com.example.demo.service.MarkSeenCommand;
import com.example.demo.service.MarkSeenUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.http.HttpHeaders;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class MarkSeenControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new MarkSeenController(
            (id, command, expectedVersion) -> new MarkSeenUseCase.Result.Applied(new Note(
                    1L,
                    "sample",
                    true,
                    1L))));

    @Test
    void putExecutesTheTransitionAndReturnsTheNewVersionAsAnETag() {
        assertThat(mvc.put().uri(MarkSeenController.PATH)
                .header(HttpHeaders.IF_MATCH, "\"1\"")
                .param("id", "7"))
                .hasStatusOk()
                .hasHeader(HttpHeaders.ETAG, "\"1\"");
    }

    @Test
    void aRequestWithNoIfMatchIsAppliedUnconditionally() {
        assertThat(mvc.put().uri(MarkSeenController.PATH)
                .param("id", "7"))
                .hasStatus(200);
    }

}
