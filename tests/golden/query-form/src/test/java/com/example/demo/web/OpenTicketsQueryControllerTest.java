package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Ticket;
import com.example.demo.service.OpenTicketsQuery;
import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class OpenTicketsQueryControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new OpenTicketsQueryController(
            criteria -> List.of(new Ticket(
                    1L,
                    "sample",
                    Optional.empty()))));

    @Test
    void getExecutesTheDatabaseQuery() {
        assertThat(mvc.get()
                .uri(OpenTicketsQueryController.PATH)
                .param("status", "sample"))
                .hasStatusOk()
                .bodyJson();
    }

}
