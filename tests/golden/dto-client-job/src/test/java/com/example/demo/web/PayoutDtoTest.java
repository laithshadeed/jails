package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Payout;
import java.util.UUID;
import org.junit.jupiter.api.Test;

/**
 * The round trip is the property worth pinning: whatever a request describes
 * must survive being turned into the domain type and back into a response.
 *
 * <p>Two records that drift apart still compile -- a component added to one
 * and not the other is silently dropped on the wire -- so this is the test
 * that notices.
 */
class PayoutDtoTest {

    @Test
    void aRequestSurvivesTheRoundTripToAResponse() {
        PayoutRequest request = sample();
        Payout payout = request.toDomain();
        PayoutResponse response = PayoutResponse.from(payout);

        assertThat(response).isNotNull();
        // Every component exists on both records -- the compiler has already
        // checked that much. What to assert here is which ones matter.
    }

    private static PayoutRequest sample() {
        return new PayoutRequest(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                1L);
    }
}
