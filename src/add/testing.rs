//! The capabilities that exist to make tests possible: `testkit`, `fake`
//! and `toxiproxy`.
//!
//! All three write into the test tree only. `toxiproxy` is the odd one --
//! it puts a proxy in front of a dependency so a test can cut the
//! connection or add latency, which is the failure everything else assumes
//! away.

use super::*;

// ---------------------------------------------------------------------------
// testkit
// ---------------------------------------------------------------------------

/// The four things every testable CLI needs and nobody enjoys writing twice.
/// No dependency: JUnit and AssertJ are already there, and everything here is
/// plain JDK.
///
/// These helpers also apply pressure in the right direction. `Clocks` and
/// `Ids` are only usable by code that *takes* a `Clock` and a
/// `Supplier<String>` instead of calling `Instant.now()` and
/// `UUID.randomUUID()` -- so generating them nudges the design toward the one
/// that can be tested deterministically at all.
pub(super) fn testkit_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        files: vec![
            NewFile {
                kind: "capability file",
                path: dir.join("Clocks.java"),
                contents: clocks_java(testkit),
            },
            NewFile {
                kind: "capability file",
                path: dir.join("Ids.java"),
                contents: ids_java(testkit),
            },
            NewFile {
                kind: "capability file",
                path: dir.join("Fixtures.java"),
                contents: fixtures_java(testkit),
            },
            NewFile {
                kind: "capability file",
                path: dir.join("Cli.java"),
                contents: testkit_cli_java(testkit),
            },
            NewFile {
                kind: "capability file",
                path: dir.join("TestkitTest.java"),
                contents: testkit_test_java(testkit),
            },
            NewFile {
                kind: "capability file",
                path: root.join("src/test/resources/fixtures/example.json"),
                contents: EXAMPLE_FIXTURE.to_string(),
            },
        ],
        ..Plan::default()
    })
}

pub(super) const EXAMPLE_FIXTURE: &str = "{\n  \"name\": \"bolt\",\n  \"qty\": 7\n}\n";

pub(super) fn clocks_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/clocks_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn ids_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/ids_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn fixtures_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/fixtures_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn testkit_cli_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/testkit_cli_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn testkit_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/testkit_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// fake
// ---------------------------------------------------------------------------

/// A scripted test double. Generic by construction: jails has no Java parser
/// and no business acquiring one, so rather than generating a fake *of* some
/// interface, this generates the replay engine and you attach it to any
/// interface with a lambda. One class covers every collaborator in the project.
pub(super) fn fake_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        files: vec![
            NewFile {
                kind: "capability file",
                path: dir.join("Fake.java"),
                contents: scripted_java(testkit),
            },
            NewFile {
                kind: "capability file",
                path: dir.join("FakeTest.java"),
                contents: scripted_test_java(testkit),
            },
        ],
        ..Plan::default()
    })
}

// ---------------------------------------------------------------------------
// toxiproxy -- network failure as something a test can switch on
// ---------------------------------------------------------------------------

pub(super) const TESTCONTAINERS_TOXIPROXY: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-toxiproxy",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
/// The client the container speaks to. Testcontainers 2.x ships the container
/// and nothing else -- `getProxy` lived on the 1.x class and is gone -- so the
/// control API has to be driven directly.
pub(super) const TOXIPROXY_JAVA: Dependency = Dependency {
    group_id: "eu.rekawek.toxiproxy",
    artifact_id: "toxiproxy-java",
    version: Some("2.1.11"),
    scope: Some("test"),
    optional: false,
};

pub(super) fn toxiproxy_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        // Deliberately not TESTCONTAINERS_JUNIT: the generated test drives the
        // container itself, and claiming a dependency another capability also
        // owns means `remove toxiproxy` takes it away from `db` too.
        deps: vec![TESTCONTAINERS_TOXIPROXY, TOXIPROXY_JAVA],
        files: vec![
            NewFile {
                kind: "capability file",
                path: dir.join("Faults.java"),
                contents: faults_java(testkit),
            },
            NewFile {
                kind: "capability file",
                path: dir.join("FaultsTest.java"),
                contents: faults_test_java(testkit),
            },
        ],
        ..Plan::default()
    })
}

pub(super) fn faults_java(pkg: &str) -> String {
    format!(
        r##"package {pkg};

import eu.rekawek.toxiproxy.Proxy;
import eu.rekawek.toxiproxy.ToxiproxyClient;
import eu.rekawek.toxiproxy.model.ToxicDirection;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.time.Duration;
import java.util.concurrent.atomic.AtomicInteger;
import org.testcontainers.Testcontainers;
import org.testcontainers.containers.Network;
import org.testcontainers.toxiproxy.ToxiproxyContainer;

/**
 * Network failure you can switch on and off inside a test.
 *
 * <p>A dependency reached through {{@link Faults}} is reached through a proxy
 * you control, so "the database went away mid-transaction" and "the broker
 * answers, slowly" stop being things you reason about and become things you
 * assert. Stopping the dependency's container proves much less: it takes
 * seconds, it cannot be undone, and it never reproduces the case that actually
 * pages you -- a connection that is up, accepted, and then silent.
 *
 * <p>Point the application at {{@link Fault#host()}} and {{@link Fault#port()}}
 * rather than at the dependency's own address. Traffic sent to the real address
 * bypasses the proxy, and the test then passes for no reason:
 *
 * {{@snippet :
 * try (var faults = Faults.start()) {{
 *     var postgres = new PostgreSQLContainer("postgres:17-alpine")
 *             .withNetwork(faults.network())
 *             .withNetworkAliases("postgres");
 *     postgres.start();
 *     var db = faults.inFrontOf("postgres", 5432);
 *
 *     // ... point the datasource at db.host():db.port() ...
 *     db.cut();
 *     assertThatThrownBy(() -> repository.findAll()).isInstanceOf(DataAccessException.class);
 *     db.restore();
 * }}
 * }}
 */
public final class Faults implements AutoCloseable {{

    private static final String IMAGE = "ghcr.io/shopify/toxiproxy:2.12.0";

    /**
     * Toxiproxy listens on a port per proxy, and a container's ports have to be
     * declared before it starts -- so a fixed handful are opened up front and
     * handed out as proxies are created.
     */
    private static final int FIRST_LISTEN_PORT = 8666;

    /** The proxy's own alias on {{@link #network()}}, and its control port. */
    public static final String ALIAS = "toxiproxy";

    public static final int CONTROL_PORT = 8474;

    private static final int LISTEN_PORTS = 8;

    private final Network network;
    private final ToxiproxyContainer container;
    private final ToxiproxyClient client;
    private final AtomicInteger nextPort = new AtomicInteger(FIRST_LISTEN_PORT);

    private Faults(Network network, ToxiproxyContainer container) {{
        this.network = network;
        this.container = container;
        // getControlPort() is already the mapped port, not 8474 -- mapping it
        // again asks for a port that was never exposed.
        this.client = new ToxiproxyClient(container.getHost(), container.getControlPort());
    }}

    /** Starts the proxy. Put every container you want to disturb on {{@link #network()}}. */
    public static Faults start() {{
        var network = Network.newNetwork();
        var ports = new Integer[LISTEN_PORTS + 1];
        ports[0] = CONTROL_PORT;
        for (int i = 0; i < LISTEN_PORTS; i++) {{
            ports[i + 1] = FIRST_LISTEN_PORT + i;
        }}
        var container = new ToxiproxyContainer(IMAGE)
                .withNetwork(network)
                .withNetworkAliases(ALIAS)
                .withExposedPorts(ports);
        container.start();
        return new Faults(network, container);
    }}

    /** The network the proxy is on. A container is only reachable if it shares this. */
    public Network network() {{
        return network;
    }}

    /**
     * A proxy in front of {{@code alias:port}}, where {{@code alias}} is the
     * network alias of another container on {{@link #network()}}.
     */
    public Fault inFrontOf(String alias, int port) {{
        return proxy(alias + "-" + port, alias + ":" + port);
    }}

    /**
     * A proxy in front of a server running in this JVM -- a stub HTTP server, an
     * embedded broker -- rather than in a container.
     */
    public Fault inFrontOfHost(int port) {{
        Testcontainers.exposeHostPorts(port);
        return proxy("host-" + port, "host.testcontainers.internal:" + port);
    }}

    private Fault proxy(String name, String upstream) {{
        var listen = nextPort.getAndIncrement();
        if (listen >= FIRST_LISTEN_PORT + LISTEN_PORTS) {{
            throw new IllegalStateException("no listen port left: Faults opens " + LISTEN_PORTS);
        }}
        try {{
            var proxy = client.createProxy(name, "0.0.0.0:" + listen, upstream);
            return new Fault(proxy, container.getHost(), container.getMappedPort(listen));
        }} catch (IOException error) {{
            throw new UncheckedIOException("could not proxy " + upstream, error);
        }}
    }}

    @Override
    public void close() {{
        container.stop();
        network.close();
    }}

    /** One proxied dependency, and the ways it is allowed to misbehave. */
    public record Fault(Proxy proxy, String host, int port) {{

        /**
         * Refuses every connection, and drops the ones already open. What a
         * process being killed looks like from the other side.
         */
        public void cut() {{
            run(proxy::disable);
        }}

        public void restore() {{
            run(proxy::enable);
        }}

        /** Delays every packet, in both directions. Use to prove a timeout exists. */
        public void latency(Duration delay) {{
            run(() -> proxy.toxics().latency("latency", ToxicDirection.DOWNSTREAM, delay.toMillis()));
        }}

        /**
         * Accepts the connection and then never answers, until {{@code after}}
         * bytes have gone by. The failure a missing read timeout hangs on
         * forever -- and the one that a "is the port open" health check misses.
         */
        public void blackhole() {{
            run(() -> proxy.toxics().timeout("timeout", ToxicDirection.DOWNSTREAM, 0));
        }}

        /**
         * Undoes everything: removes every toxic *and* re-enables a cut proxy.
         *
         * <p>Both, deliberately. A {{@code heal}} that only dropped the toxics
         * would leave a {{@link #cut}} in place, and the next test would fail
         * against a dependency it never touched -- with an error that points at
         * the wrong test.
         */
        public void heal() {{
            run(() -> {{
                for (var toxic : proxy.toxics().getAll()) {{
                    toxic.remove();
                }}
                proxy.enable();
            }});
        }}

        private static void run(Failing action) {{
            try {{
                action.run();
            }} catch (IOException error) {{
                throw new UncheckedIOException("toxiproxy refused the change", error);
            }}
        }}

        @FunctionalInterface
        private interface Failing {{
            void run() throws IOException;
        }}
    }}
}}
"##
    )
}

pub(super) fn faults_test_java(pkg: &str) -> String {
    format!(
        r##"package {pkg};

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
class FaultsTest {{

    private static final Duration PATIENCE = Duration.ofSeconds(5);

    /** Long enough to rule out slowness, short enough that a hang fails fast. */
    private static final Duration IMPATIENCE = Duration.ofSeconds(2);

    @Test
    void aProxiedDependencyAnswersUntilItIsCutAndThenAgainAfterItIsRestored() throws Exception {{
        try (var faults = Faults.start()) {{
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);

            assertThat(status(fault, PATIENCE)).as("the proxy passes traffic through").isEqualTo(200);

            fault.cut();
            assertThatThrownBy(() -> status(fault, PATIENCE))
                    .as("a cut dependency refuses the connection")
                    .isInstanceOf(IOException.class);

            fault.restore();
            assertThat(status(fault, PATIENCE)).as("the dependency came back").isEqualTo(200);
        }}
    }}

    @Test
    void aBlackholedDependencyAcceptsTheConnectionAndThenSaysNothing() throws Exception {{
        try (var faults = Faults.start()) {{
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);
            fault.blackhole();

            // The failure a missing read timeout hangs on forever: the socket
            // is open, so anything that checks only "did it connect" believes
            // the dependency is healthy.
            assertThatThrownBy(() -> status(fault, IMPATIENCE))
                    .isInstanceOf(HttpTimeoutException.class);

            fault.heal();
            assertThat(status(fault, PATIENCE)).isEqualTo(200);
        }}
    }}

    @Test
    void latencyIsAddedToAnOtherwiseHealthyDependency() throws Exception {{
        try (var faults = Faults.start()) {{
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);
            fault.latency(Duration.ofSeconds(3));

            assertThatThrownBy(() -> status(fault, IMPATIENCE))
                    .as("a caller more impatient than the delay gives up")
                    .isInstanceOf(HttpTimeoutException.class);

            fault.heal();
            assertThat(status(fault, PATIENCE)).isEqualTo(200);
        }}
    }}

    private static int status(Faults.Fault fault, Duration timeout) throws Exception {{
        try (var http = HttpClient.newHttpClient()) {{
            var request = HttpRequest.newBuilder(URI.create("http://%s:%d/version".formatted(fault.host(), fault.port())))
                    .timeout(timeout)
                    .build();
            return http.send(request, HttpResponse.BodyHandlers.discarding()).statusCode();
        }}
    }}
}}
"##
    )
}

pub(super) fn scripted_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/scripted_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn scripted_test_java(pkg: &str) -> String {
    crate::template::render(
        include_str!("../../templates/add/scripted_test_java.java"),
        &[("pkg", pkg)],
    )
}
