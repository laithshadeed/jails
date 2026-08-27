package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Ticket;
import com.example.demo.service.OpenTicketCommand;
import com.example.demo.service.OpenTicketUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class OpenTicketControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new OpenTicketController(
            command -> new Ticket(
                    1L,
                    "sample")));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(OpenTicketController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "subject": "sample"
}
"""))
                .hasStatus(201);
    }

}
