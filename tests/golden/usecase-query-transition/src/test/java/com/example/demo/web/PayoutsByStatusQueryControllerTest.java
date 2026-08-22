package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import com.example.demo.service.PayoutsByStatusQueryPort;
import java.time.Instant;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

@WebMvcTest(PayoutsByStatusQueryController.class)
@Import(PayoutsByStatusQueryControllerTest.Config.class)
class PayoutsByStatusQueryControllerTest {

    @Autowired
    private MockMvcTester mvc;

    @Test
    void postExecutesTheDatabaseQueryPort() {
        assertThat(mvc.post()
                .uri(PayoutsByStatusQueryController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "status": "PENDING"
}
"""))
                .hasStatusOk()
                .bodyJson();
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Config {

        @Bean
        PayoutsByStatusQueryPort queryPort() {
            return query -> List.of(new Payout(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    1L,
                    PayoutStatus.values()[0],
                    1L,
                    Instant.parse("2024-01-01T00:00:00Z")));
        }

    }
}
