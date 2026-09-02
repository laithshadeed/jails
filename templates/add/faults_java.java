package {{pkg}};

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
 * <p>A dependency reached through {@link Faults} is reached through a proxy
 * you control, so "the database went away mid-transaction" and "the broker
 * answers, slowly" stop being things you reason about and become things you
 * assert. Stopping the dependency's container proves much less: it takes
 * seconds, it cannot be undone, and it never reproduces the case that actually
 * pages you -- a connection that is up, accepted, and then silent.
 *
 * <p>Point the application at {@link Fault#host()} and {@link Fault#port()}
 * rather than at the dependency's own address. Traffic sent to the real address
 * bypasses the proxy, and the test then passes for no reason:
 *
 * {@snippet :
 * try (var faults = Faults.start()) {
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
 * }
 * }
 */
public final class Faults implements AutoCloseable {

    private static final String IMAGE = "ghcr.io/shopify/toxiproxy:2.12.0";

    /**
     * Toxiproxy listens on a port per proxy, and a container's ports have to be
     * declared before it starts -- so a fixed handful are opened up front and
     * handed out as proxies are created.
     */
    private static final int FIRST_LISTEN_PORT = 8666;

    /** The proxy's own alias on {@link #network()}, and its control port. */
    public static final String ALIAS = "toxiproxy";

    public static final int CONTROL_PORT = 8474;

    private static final int LISTEN_PORTS = 8;

    private final Network network;
    private final ToxiproxyContainer container;
    private final ToxiproxyClient client;
    private final AtomicInteger nextPort = new AtomicInteger(FIRST_LISTEN_PORT);

    private Faults(Network network, ToxiproxyContainer container) {
        this.network = network;
        this.container = container;
        // getControlPort() is already the mapped port, not 8474 -- mapping it
        // again asks for a port that was never exposed.
        this.client = new ToxiproxyClient(container.getHost(), container.getControlPort());
    }

    /** Starts the proxy. Put every container you want to disturb on {@link #network()}. */
    public static Faults start() {
        var network = Network.newNetwork();
        var ports = new Integer[LISTEN_PORTS + 1];
        ports[0] = CONTROL_PORT;
        for (int i = 0; i < LISTEN_PORTS; i++) {
            ports[i + 1] = FIRST_LISTEN_PORT + i;
        }
        var container = new ToxiproxyContainer(IMAGE)
                .withNetwork(network)
                .withNetworkAliases(ALIAS)
                .withExposedPorts(ports);
        container.start();
        return new Faults(network, container);
    }

    /** The network the proxy is on. A container is only reachable if it shares this. */
    public Network network() {
        return network;
    }

    /**
     * A proxy in front of {@code alias:port}, where {@code alias} is the
     * network alias of another container on {@link #network()}.
     */
    public Fault inFrontOf(String alias, int port) {
        return proxy(alias + "-" + port, alias + ":" + port);
    }

    /**
     * A proxy in front of a server running in this JVM -- a stub HTTP server, an
     * embedded broker -- rather than in a container.
     */
    public Fault inFrontOfHost(int port) {
        Testcontainers.exposeHostPorts(port);
        return proxy("host-" + port, "host.testcontainers.internal:" + port);
    }

    private Fault proxy(String name, String upstream) {
        var listen = nextPort.getAndIncrement();
        if (listen >= FIRST_LISTEN_PORT + LISTEN_PORTS) {
            throw new IllegalStateException("no listen port left: Faults opens " + LISTEN_PORTS);
        }
        try {
            var proxy = client.createProxy(name, "0.0.0.0:" + listen, upstream);
            return new Fault(proxy, container.getHost(), container.getMappedPort(listen));
        } catch (IOException error) {
            throw new UncheckedIOException("could not proxy " + upstream, error);
        }
    }

    @Override
    public void close() {
        container.stop();
        network.close();
    }

    /** One proxied dependency, and the ways it is allowed to misbehave. */
    public record Fault(Proxy proxy, String host, int port) {

        /**
         * Refuses every connection, and drops the ones already open. What a
         * process being killed looks like from the other side.
         */
        public void cut() {
            run(proxy::disable);
        }

        public void restore() {
            run(proxy::enable);
        }

        /** Delays every packet, in both directions. Use to prove a timeout exists. */
        public void latency(Duration delay) {
            run(() -> proxy.toxics().latency("latency", ToxicDirection.DOWNSTREAM, delay.toMillis()));
        }

        /**
         * Accepts the connection and then never answers, until {@code after}
         * bytes have gone by. The failure a missing read timeout hangs on
         * forever -- and the one that a "is the port open" health check misses.
         */
        public void blackhole() {
            run(() -> proxy.toxics().timeout("timeout", ToxicDirection.DOWNSTREAM, 0));
        }

        /**
         * Undoes everything: removes every toxic *and* re-enables a cut proxy.
         *
         * <p>Both, deliberately. A {@code heal} that only dropped the toxics
         * would leave a {@link #cut} in place, and the next test would fail
         * against a dependency it never touched -- with an error that points at
         * the wrong test.
         */
        public void heal() {
            run(() -> {
                for (var toxic : proxy.toxics().getAll()) {
                    toxic.remove();
                }
                proxy.enable();
            });
        }

        private static void run(Failing action) {
            try {
                action.run();
            } catch (IOException error) {
                throw new UncheckedIOException("toxiproxy refused the change", error);
            }
        }

        @FunctionalInterface
        private interface Failing {
            void run() throws IOException;
        }
    }
}
