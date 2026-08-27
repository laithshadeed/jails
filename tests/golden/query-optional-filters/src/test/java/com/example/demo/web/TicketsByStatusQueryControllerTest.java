package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Ticket;
import com.example.demo.service.TicketsByStatusQuery;
import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class TicketsByStatusQueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new TicketsByStatusQueryController(
            criteria -> List.of(new Ticket(
                    1L,
                    "sample",
                    Optional.empty()))));

    @Test
    void postExecutesTheDatabaseQuery() {
        assertThat(mvc.post()
                .uri(TicketsByStatusQueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "status": "sample",
  "category": null
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

}
