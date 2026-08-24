package com.example.paymentsgateway.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.domain.PaymentMethod;
import com.example.paymentsgateway.domain.PaymentStatus;
import com.example.paymentsgateway.service.AuthorisePaymentCommand;
import com.example.paymentsgateway.service.AuthorisePaymentUseCase;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class AuthorisePaymentControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new AuthorisePaymentController(
            command -> new Payment(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    1L,
                    "sample",
                    PaymentMethod.values()[0],
                    PaymentStatus.values()[0],
                    1L,
                    Optional.empty(),
                    Optional.empty(),
                    Instant.parse("2024-01-01T00:00:00Z"),
                    Instant.parse("2024-01-01T00:00:00Z")),
            new ScopeAuthorizer(new MockEnvironment())));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(AuthorisePaymentController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "merchantId": "00000000-0000-0000-0000-000000000001",
  "idempotencyKey": "sample",
  "amountMinor": 7,
  "currency": "sample",
  "method": "CARD"
}
"""))
                .hasStatus(201);
    }

}
