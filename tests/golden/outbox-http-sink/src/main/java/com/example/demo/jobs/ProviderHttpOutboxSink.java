package com.example.demo.jobs;

import com.example.demo.adapters.Json;
import com.example.demo.messaging.MessageReceivedEvent;
import io.micrometer.core.instrument.MeterRegistry;
import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.Objects;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.core.annotation.Order;
import org.springframework.stereotype.Component;

/**
 * Bounded provider delivery for a typed transactional outbox event.
 *
 * <p>The stable event id is always sent as {@code Idempotency-Key}. Redirects
 * are never followed, secrets are never logged, and only a 2xx response is an
 * acknowledgement. The owning outbox supplies durable leases and bounded
 * retries, so a process crash cannot silently discard this delivery.
 */
@Component
@Order(100)
@ConditionalOnProperty(name = "outbox.receive-message.http.provider.url")
public final class ProviderHttpOutboxSink implements ReceiveMessageOutboxSink {

    private final URI endpoint;
    private final String bearerToken;
    private final Duration requestTimeout;
    private final HttpClient client;
    private final MeterRegistry meters;

    public ProviderHttpOutboxSink(
            @Value("${outbox.receive-message.http.provider.url}") String url,
            @Value("${outbox.receive-message.http.provider.bearer-token:}") String bearerToken,
            @Value("${outbox.receive-message.http.provider.connect-timeout-ms:2000}") int connectTimeoutMillis,
            @Value("${outbox.receive-message.http.provider.request-timeout-ms:5000}") int requestTimeoutMillis,
            MeterRegistry meters) {
        this.endpoint = validateEndpoint(url);
        this.bearerToken = Objects.requireNonNull(bearerToken, "bearer token is required");
        if (connectTimeoutMillis < 1 || requestTimeoutMillis < 1) {
            throw new IllegalArgumentException("HTTP delivery timeouts must be positive");
        }
        this.requestTimeout = Duration.ofMillis(requestTimeoutMillis);
        this.client = HttpClient.newBuilder()
                .connectTimeout(Duration.ofMillis(connectTimeoutMillis))
                .followRedirects(HttpClient.Redirect.NEVER)
                .build();
        this.meters = Objects.requireNonNull(meters, "meter registry is required");
    }

    @Override public String name() { return "Provider"; }

    @Override
    public void deliver(MessageReceivedEvent event) {
        Objects.requireNonNull(event, "event is required");
        var request = HttpRequest.newBuilder(endpoint)
                .timeout(requestTimeout)
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .header("Idempotency-Key", String.valueOf(event.id()))
                .POST(HttpRequest.BodyPublishers.ofString(Json.toJson(event)));
        if (!bearerToken.isBlank()) request.header("Authorization", "Bearer " + bearerToken);
        try {
            var response = client.send(request.build(), HttpResponse.BodyHandlers.discarding());
            if (response.statusCode() < 200 || response.statusCode() >= 300) {
                meters.counter("jails.outbox.http", "sink", name(), "outcome", "rejected").increment();
                throw new DeliveryException("provider returned HTTP " + response.statusCode());
            }
            meters.counter("jails.outbox.http", "sink", name(), "outcome", "accepted").increment();
        } catch (InterruptedException failure) {
            Thread.currentThread().interrupt();
            throw new DeliveryException("provider delivery interrupted", failure);
        } catch (IOException failure) {
            meters.counter("jails.outbox.http", "sink", name(), "outcome", "io-error").increment();
            throw new DeliveryException("provider delivery failed", failure);
        }
    }

    private static URI validateEndpoint(String value) {
        URI uri = URI.create(Objects.requireNonNull(value, "provider URL is required"));
        if (!uri.isAbsolute() || uri.getHost() == null || uri.getUserInfo() != null
                || !(uri.getScheme().equalsIgnoreCase("http") || uri.getScheme().equalsIgnoreCase("https"))) {
            throw new IllegalArgumentException("provider URL must be an absolute HTTP(S) URL without user-info");
        }
        return uri;
    }

    public static final class DeliveryException extends RuntimeException {
        public DeliveryException(String message) { super(message); }
        public DeliveryException(String message, Throwable cause) { super(message, cause); }
    }
}
