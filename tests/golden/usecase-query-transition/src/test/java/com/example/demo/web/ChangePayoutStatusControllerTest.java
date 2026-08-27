package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Payout;
import com.example.demo.domain.PayoutStatus;
import com.example.demo.service.ChangePayoutStatusCommand;
import com.example.demo.service.ChangePayoutStatusUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.HttpHeaders;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class ChangePayoutStatusControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new ChangePayoutStatusController(
            (id, command, expectedVersion) -> new ChangePayoutStatusUseCase.Result.Applied(new Payout(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    1L,
                    PayoutStatus.PENDING,
                    1L,
                    Instant.parse("2024-01-01T00:00:00Z")))));

    @Test
    void putExecutesTheTransitionAndReturnsTheNewVersionAsAnETag() {
        assertThat(mvc.put().uri(ChangePayoutStatusController.PATH)
                .header(HttpHeaders.IF_MATCH, "\"1\"")
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "status": "PENDING"
}
"""))
                .hasStatusOk()
                .hasHeader(HttpHeaders.ETAG, "\"1\"");
    }

    @Test
    void aRequestWithNoIfMatchIsRefusedRatherThanAppliedBlind() {
        assertThat(mvc.put().uri(ChangePayoutStatusController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "id": "00000000-0000-0000-0000-000000000001",
  "status": "PENDING"
}
"""))
                .hasStatus(400);
    }

}
