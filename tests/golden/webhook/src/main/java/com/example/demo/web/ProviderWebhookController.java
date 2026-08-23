package com.example.demo.web;

import com.example.demo.ProviderVerifier;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RestController;

/**
 * The inbound endpoint.
 *
 * <p>{@code byte[]} rather than a typed body, and that is not laziness: the
 * signature is over the bytes that arrived. Binding to a record first throws
 * those bytes away, and re-serialising the record produces different ones
 * whenever the sender's formatting differs from Jackson's — which is most
 * senders, most of the time.
 *
 * <p>Returns 200 as soon as the delivery is verified, before doing the work.
 * Senders retry on anything else and time out in seconds; a handler that does
 * the processing inline gets retried while it is still running, and the same
 * event arrives twice. Hand it to a queue — `jails g durable-job` is that — or
 * make the handler idempotent with `jails g idempotency`.
 */
@RestController
public class ProviderWebhookController {

    private final ProviderVerifier verifier;

    public ProviderWebhookController(ProviderVerifier verifier) {
        this.verifier = verifier;
    }

    @PostMapping("/webhooks/provider")
    public ResponseEntity<Void> receive(
            @RequestBody byte[] body,
            @RequestHeader("X-Provider-Timestamp") String timestamp,
            @RequestHeader("X-Provider-Signature") String signature) {
        try {
            verifier.verify(body, timestamp, signature);
        } catch (ProviderVerifier.InvalidSignatureException rejected) {
            // 400, not 401: there is no credential to re-present and no
            // authentication scheme to name in a WWW-Authenticate header. A
            // sender that gets 401 will look for one.
            return ResponseEntity.status(HttpStatus.BAD_REQUEST).build();
        }

        // Accept first, work later. See the class Javadoc.
        return ResponseEntity.ok().build();
    }
}
