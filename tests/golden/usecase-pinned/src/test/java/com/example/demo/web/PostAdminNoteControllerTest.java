package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Note;
import com.example.demo.domain.SenderType;
import com.example.demo.service.PostAdminNoteCommand;
import com.example.demo.service.PostAdminNoteUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class PostAdminNoteControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new PostAdminNoteController(
            command -> new Note(
                    1L,
                    1L,
                    "sample",
                    SenderType.CUSTOMER)));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(PostAdminNoteController.PATH)
                .param("authorId", "7")
                .param("body", "sample"))
                .hasStatus(201);
    }

}
