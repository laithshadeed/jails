package com.example.paymentsgateway.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.domain.Refund;
import com.example.paymentsgateway.service.RefundPaymentRequestCommand;
import com.example.paymentsgateway.service.RefundPaymentRequestUseCase;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class RefundPaymentRequestControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new RefundPaymentRequestController(
            command -> new Refund(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    1L,
                    Optional.empty(),
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(RefundPaymentRequestController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "merchantId": "00000000-0000-0000-0000-000000000001",
  "paymentId": "00000000-0000-0000-0000-000000000001",
  "amountMinor": 7
}
"""))
                .hasStatus(201);
    }

}
