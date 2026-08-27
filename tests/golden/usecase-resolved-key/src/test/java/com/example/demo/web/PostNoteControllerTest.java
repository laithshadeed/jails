package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Note;
import com.example.demo.domain.SenderType;
import com.example.demo.service.PostNoteCommand;
import com.example.demo.service.PostNoteUseCase;
import java.util.Optional;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class PostNoteControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new PostNoteController(
            command -> Optional.of(new Note(
                    1L,
                    1L,
                    "sample",
                    SenderType.CUSTOMER))));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(PostNoteController.PATH)
                .param("email", "sample")
                .param("body", "sample"))
                .hasStatus(201);
    }

}
