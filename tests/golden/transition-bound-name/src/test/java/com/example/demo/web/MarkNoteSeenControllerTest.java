package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Note;
import com.example.demo.service.MarkNoteSeenCommand;
import com.example.demo.service.MarkNoteSeenUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.http.HttpHeaders;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class MarkNoteSeenControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new MarkNoteSeenController(
            (id, command, expectedVersion) -> new MarkNoteSeenUseCase.Result.Applied(new Note(
                    1L,
                    "sample",
                    true,
                    1L))));

    @Test
    void postExecutesTheTransitionAndReturnsTheNewVersionAsAnETag() {
        assertThat(mvc.post().uri(MarkNoteSeenController.PATH)
                .header(HttpHeaders.IF_MATCH, "\"1\"")
                .param("note_id", "7"))
                .hasStatusOk()
                .hasHeader(HttpHeaders.ETAG, "\"1\"");
    }

    @Test
    void aRequestWithNoIfMatchIsAppliedUnconditionally() {
        assertThat(mvc.post().uri(MarkNoteSeenController.PATH)
                .param("note_id", "7"))
                .hasStatus(200);
    }

}
