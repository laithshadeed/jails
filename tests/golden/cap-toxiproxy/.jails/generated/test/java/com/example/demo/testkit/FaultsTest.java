package com.example.demo.testkit;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.net.http.HttpTimeoutException;
import java.time.Duration;
import org.junit.jupiter.api.Test;

/**
 * Proves the fault injector itself works, so a failure elsewhere is never its
 * fault.
 *
 * <p>The upstream is Toxiproxy's own control API, reached through a proxy that
 * Toxiproxy is running. That sounds cute but it is the most honest option
 * available: it needs no second image and no bridge back to a port on the test
 * JVM, so a failure here is the proxy misbehaving and cannot be anything else.
 */
class FaultsTest {

    private static final Duration PATIENCE = Duration.ofSeconds(5);

    /** Long enough to rule out slowness, short enough that a hang fails fast. */
    private static final Duration IMPATIENCE = Duration.ofSeconds(2);

    @Test
    void aProxiedDependencyAnswersUntilItIsCutAndThenAgainAfterItIsRestored() throws Exception {
        try (var faults = Faults.start()) {
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);

            assertThat(status(fault, PATIENCE)).as("the proxy passes traffic through").isEqualTo(200);

            fault.cut();
            assertThatThrownBy(() -> status(fault, PATIENCE))
                    .as("a cut dependency refuses the connection")
                    .isInstanceOf(IOException.class);

            fault.restore();
            assertThat(status(fault, PATIENCE)).as("the dependency came back").isEqualTo(200);
        }
    }

    @Test
    void aBlackholedDependencyAcceptsTheConnectionAndThenSaysNothing() throws Exception {
        try (var faults = Faults.start()) {
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);
            fault.blackhole();

            // The failure a missing read timeout hangs on forever: the socket
            // is open, so anything that checks only "did it connect" believes
            // the dependency is healthy.
            assertThatThrownBy(() -> status(fault, IMPATIENCE))
                    .isInstanceOf(HttpTimeoutException.class);

            fault.heal();
            assertThat(status(fault, PATIENCE)).isEqualTo(200);
        }
    }

    @Test
    void latencyIsAddedToAnOtherwiseHealthyDependency() throws Exception {
        try (var faults = Faults.start()) {
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);
            fault.latency(Duration.ofSeconds(3));

            assertThatThrownBy(() -> status(fault, IMPATIENCE))
                    .as("a caller more impatient than the delay gives up")
                    .isInstanceOf(HttpTimeoutException.class);

            fault.heal();
            assertThat(status(fault, PATIENCE)).isEqualTo(200);
        }
    }

    private static int status(Faults.Fault fault, Duration timeout) throws Exception {
        try (var http = HttpClient.newHttpClient()) {
            var request = HttpRequest.newBuilder(URI.create("http://%s:%d/version".formatted(fault.host(), fault.port())))
                    .timeout(timeout)
                    .build();
            return http.send(request, HttpResponse.BodyHandlers.discarding()).statusCode();
        }
    }
}
