import java.io.ByteArrayOutputStream;
import java.io.PrintWriter;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * The resident JVM behind `jails testd`.
 *
 * <p>It exists for one measured reason. plan.md 19.1 found that both
 * {@code mvnd} and {@code jails test --fast} sit at ~0.6 s for a single test
 * method, and that what is left in both is a cold {@code java} process. 19.2
 * then measured where that time actually goes: the first JUnit session in a
 * fresh JVM costs 464 ms where the warm ones cost 20 ms. This process pays
 * that once.
 *
 * <p><b>It does not compile.</b> The design 10.2 sketched had the daemon hold
 * {@code ToolProvider.getSystemJavaCompiler()} and compile in-process, and
 *19.5's measurement removed the need: the editor's language server already
 * writes {@code target/classes} on every save. So the daemon runs what is on
 * disk and the Rust side refuses when a source is newer, which is the same
 * staleness gate {@code --fast} uses. One less thing to be subtly wrong about
 * -- 10.2 itself notes that compiling only the changed file is unsound.
 *
 * <p><b>Freshness comes from JUnit, not from here.</b> Each run goes through
 * {@code ConsoleLauncher} with {@code --class-path} naming the project's
 * output directories, and JUnit builds a child loader for them and closes it
 * afterwards. That is why the output directories must NOT be on this process's
 * own classpath: the child delegates to its parent first, so a copy up there
 * would serve the stale class on every run and the daemon would report on code
 * that no longer exists -- silently, which is the failure mode the whole
 * staleness gate exists to prevent.
 *
 * <p>Started only by `jails testd`, which owns the socket path, the classpath
 * and the protocol on the other side.
 */
public final class JailsTestDaemon {

    /** Ends a response; cannot occur in JUnit's output, which is text. */
    private static final byte END = 4;

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("usage: JailsTestDaemon <socket> <idle-seconds> <output-classpath>");
            System.exit(2);
        }
        Path socket = Path.of(args[0]);
        long idleMillis = Long.parseLong(args[1]) * 1000L;
        String outputs = args[2];

        Files.deleteIfExists(socket);
        if (socket.getParent() != null) {
            Files.createDirectories(socket.getParent());
        }
        try (ServerSocketChannel server = ServerSocketChannel.open(StandardProtocolFamily.UNIX)) {
            server.bind(UnixDomainSocketAddress.of(socket));
            // Announce readiness only once the socket is bound, so the client
            // never races a socket that exists but is not listening yet.
            System.out.println("ready");
            System.out.flush();
            warmUp(outputs);
            serve(server, socket, outputs, idleMillis);
        } finally {
            Files.deleteIfExists(socket);
        }
    }

    /**
     * Pay the 464 ms first-session cost before anyone is waiting on it.
     *
     * <p>Discovery alone loads the engine, the launcher's ServiceLoader graph
     * and most of what a run touches. {@code --dry-run} is deliberate: warming
     * up by *executing* the suite would run the project's tests as a side
     * effect of starting a daemon, which is the sort of surprise that makes a
     * tool untrustworthy.
     */
    private static void warmUp(String outputs) {
        try {
            run(List.of("--class-path", outputs, "--scan-class-path", "--dry-run", "--details=none"));
        } catch (Throwable ignored) {
            // A warm-up that fails costs the first real run its 464 ms and
            // nothing else. It must never stop the daemon starting.
        }
    }

    private static void serve(ServerSocketChannel server, Path socket, String outputs, long idleMillis)
            throws Exception {
        while (true) {
            server.configureBlocking(true);
            SocketChannel channel = acceptWithin(server, idleMillis);
            if (channel == null) {
                return; // idle timeout: exit rather than linger for a session that ended
            }
            try (SocketChannel client = channel) {
                List<String> request = readRequest(client);
                if (request.isEmpty() || request.get(0).equals("STOP")) {
                    reply(client, "", 0);
                    return;
                }
                if (request.get(0).equals("PING")) {
                    reply(client, "ok\n", 0);
                    continue;
                }
                List<String> arguments = new ArrayList<>();
                arguments.add("--class-path");
                arguments.add(outputs);
                arguments.addAll(request.subList(1, request.size()));
                Result result = run(arguments);
                reply(client, result.output, result.exitCode);
            } catch (Exception failure) {
                // One malformed request must not take the daemon down; the
                // client falls back to a cold run and says why.
                System.err.println("jails testd: " + failure);
            }
        }
    }

    /** Accept, or return null if nothing arrives within the idle window. */
    private static SocketChannel acceptWithin(ServerSocketChannel server, long idleMillis) throws Exception {
        server.configureBlocking(false);
        long deadline = System.currentTimeMillis() + idleMillis;
        while (System.currentTimeMillis() < deadline) {
            SocketChannel client = server.accept();
            if (client != null) {
                return client;
            }
            Thread.sleep(25);
        }
        return null;
    }

    private record Result(String output, int exitCode) {}

    private static Result run(List<String> arguments) {
        List<String> full = new ArrayList<>();
        full.add("execute");
        full.addAll(arguments);
        var buffer = new ByteArrayOutputStream();
        var writer = new PrintWriter(buffer, true, StandardCharsets.UTF_8);
        int exitCode;
        try {
            // INTERNAL in JUnit's own @API terms, and the only entry point that
            // returns rather than calling System.exit -- which a resident JVM
            // obviously cannot have. `jails testd` pins the console artifact to
            // the project's own JUnit version (see launcher.rs), so this moves
            // only when the project's JUnit does, and a break surfaces as a
            // daemon that refuses to start rather than as a wrong result.
            exitCode = org.junit.platform.console.ConsoleLauncher
                    .run(writer, writer, full.toArray(String[]::new))
                    .getExitCode();
        } catch (Throwable failure) {
            writer.println("jails testd: " + failure);
            exitCode = 2;
        }
        writer.flush();
        return new Result(buffer.toString(StandardCharsets.UTF_8), exitCode);
    }

    /** Lines until a blank one. */
    private static List<String> readRequest(SocketChannel client) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var buffer = ByteBuffer.allocate(4096);
        while (true) {
            buffer.clear();
            int read = client.read(buffer);
            if (read < 0) {
                break;
            }
            buffer.flip();
            byte[] chunk = new byte[buffer.remaining()];
            buffer.get(chunk);
            bytes.write(chunk);
            String seen = bytes.toString(StandardCharsets.UTF_8);
            if (seen.contains("\n\n")) {
                break;
            }
        }
        List<String> lines = new ArrayList<>();
        for (String line : bytes.toString(StandardCharsets.UTF_8).split("\n", -1)) {
            if (line.isEmpty()) {
                break;
            }
            lines.add(line);
        }
        return lines;
    }

    private static void reply(SocketChannel client, String output, int exitCode) throws Exception {
        var payload = new ByteArrayOutputStream();
        payload.write(output.getBytes(StandardCharsets.UTF_8));
        payload.write(END);
        payload.write(Integer.toString(exitCode).getBytes(StandardCharsets.UTF_8));
        payload.write('\n');
        var buffer = ByteBuffer.wrap(payload.toByteArray());
        while (buffer.hasRemaining()) {
            client.write(buffer);
        }
    }

    private JailsTestDaemon() {}
}
