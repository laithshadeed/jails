package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import com.example.demo.service.RequestPayoutCommand;
import com.example.demo.service.RequestPayoutUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

@WebMvcTest(RequestPayoutController.class)
@Import(RequestPayoutControllerTest.Config.class)
class RequestPayoutControllerTest {

    @Autowired
    private MockMvcTester mvc;

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(RequestPayoutController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "amount": 7
}
"""))
                .hasStatus(201);
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {

        @Bean
        RequestPayoutUseCase useCase() {
            return command -> new Payout(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    1L,
                    PayoutStatus.values()[0],
                    1L,
                    Instant.parse("2024-01-01T00:00:00Z"));
        }

    }
}
