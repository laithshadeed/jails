import java.io.ByteArrayOutputStream;
import java.io.PrintWriter;
import java.lang.management.ManagementFactory;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/** Authenticated, project-local warm engine for {@code jails test}. */
final class JailsTestDaemon {
    private static final int PROTOCOL_MIN = @JAILS_TESTD_PROTOCOL_MIN@;
    private static final int PROTOCOL_MAX = @JAILS_TESTD_PROTOCOL_MAX@;
    private static final int MAX_PAYLOAD = 8 * 1024 * 1024;
    private static final int MAX_GENERATIONS = 50;
    private static final long MAX_METASPACE_GROWTH = 128L * 1024L * 1024L;

    private final Path root;
    private final byte[] project;
    private final byte[] cookie;
    private final String outputs;
    private final long baselineMetaspace;
    private final AtomicBoolean stopping = new AtomicBoolean();
    private final AtomicReference<Thread> activeRun = new AtomicReference<>();
    private final Map<String, Cached> completed = new LinkedHashMap<>() {
        @Override
        protected boolean removeEldestEntry(Map.Entry<String, Cached> eldest) {
            return size() > 1024;
        }
    };
    private int generations;

    private JailsTestDaemon(Path root, byte[] project, byte[] cookie, String outputs) {
        this.root = root;
        this.project = project;
        this.cookie = cookie;
        this.outputs = outputs;
        this.baselineMetaspace = metaspace();
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 5) {
            System.err.println("usage: JailsTestDaemon <socket> <idle-seconds> <outputs> <project> <cookie>");
            System.exit(2);
        }
        Path socket = Path.of(args[0]);
        long idleMillis = Long.parseLong(args[1]) * 1000L;
        var daemon = new JailsTestDaemon(
                Path.of("").toAbsolutePath().normalize(), fromHex(args[3]), fromHex(args[4]), args[2]);
        Files.deleteIfExists(socket);
        Files.createDirectories(socket.getParent());
        try (ServerSocketChannel server = ServerSocketChannel.open(StandardProtocolFamily.UNIX)) {
            server.bind(UnixDomainSocketAddress.of(socket));
            System.out.println("ready");
            System.out.flush();
            daemon.warmUp();
            daemon.serve(server, idleMillis);
        } finally {
            Files.deleteIfExists(socket);
        }
    }

    private void warmUp() {
        try {
            runJUnit(List.of("--class-path", outputs, "--scan-class-path", "--dry-run", "--details=none"));
        } catch (Throwable ignored) {
            // Warming is an optimization. A real request still gets the full diagnostic.
        }
    }

    private void serve(ServerSocketChannel server, long idleMillis) throws Exception {
        while (!stopping.get()) {
            SocketChannel channel = acceptWithin(server, idleMillis);
            if (channel == null) return;
            Thread.ofVirtual().name("jails-testd-client").start(() -> {
                boolean stop = false;
                try (SocketChannel client = channel) {
                    stop = handle(client);
                } catch (Exception failure) {
                    System.err.println("jails testd: " + failure);
                }
                if (stop) {
                    stopping.set(true);
                    System.exit(0);
                }
            });
        }
    }

    private boolean handle(SocketChannel client) throws Exception {
        byte[] payload = readFrame(client);
        Request request;
        try {
            request = Request.decode(payload);
        } catch (Exception invalid) {
            System.err.println("jails testd: invalid frame: " + invalid.getMessage());
            return false;
        }
        String id = toHex(request.id);
        Cached cached;
        synchronized (completed) {
            cached = completed.get(id);
        }
        if (cached != null) {
            if (MessageDigest.isEqual(cached.request, payload)) {
                writeAll(client, cached.response);
            } else {
                writeAll(client, refused(request.id, "request-id-reused",
                        "request ID was reused for different bytes", "retry with a new request ID"));
            }
            return false;
        }
        byte[] response = execute(request);
        synchronized (completed) {
            completed.put(id, new Cached(payload, response));
        }
        writeAll(client, response);
        if (request.tag == 4) return true;
        if (request.tag == 1) {
            generations++;
            return generations >= MAX_GENERATIONS
                    || metaspace() - baselineMetaspace >= MAX_METASPACE_GROWTH
                    || leakedThread();
        }
        return false;
    }

    private byte[] execute(Request request) throws Exception {
        if (!MessageDigest.isEqual(project, request.project)
                || !MessageDigest.isEqual(cookie, request.cookie)) {
            return refused(request.id, "authentication-failed",
                    "project digest or daemon cookie does not match",
                    "remove .jails/run/testd-v2.* and retry from this project");
        }
        return switch (request.tag) {
            case 0 -> hello(request);
            case 1 -> run(request);
            case 2 -> event(request.id, 0, request.epoch, true, null);
            case 3 -> cancel(request);
            case 4 -> event(request.id, 3, 0, false, "stop requested");
            default -> refused(request.id, "unknown-request", "unknown request tag " + request.tag,
                    "upgrade both testd protocol peers");
        };
    }

    private byte[] hello(Request request) throws Exception {
        if (request.protocolMin > PROTOCOL_MAX || request.protocolMax < PROTOCOL_MIN) {
            return refused(request.id, "protocol-mismatch",
                    "no mutually supported testd protocol version",
                    "restart the daemon with a compatible jails version");
        }
        var body = new Wire();
        body.tag(0);
        body.bytes(request.id);
        body.u32(PROTOCOL_MAX);
        return frame(body.done());
    }

    private byte[] run(Request request) throws Exception {
        Thread thread = Thread.currentThread();
        if (!activeRun.compareAndSet(null, thread)) {
            return refused(request.id, "daemon-busy", "the daemon already has an active test request",
                    "wait for that request or retry through --engine build");
        }
        try {
            return runActive(request);
        } finally {
            activeRun.compareAndSet(thread, null);
        }
    }

    private byte[] cancel(Request request) throws Exception {
        Thread thread = activeRun.get();
        if (thread == null) {
            return refused(request.id, "nothing-to-cancel",
                    "the daemon has no active request", "retry the test command if work is still needed");
        }
        thread.interrupt();
        return event(request.id, 3, 0, false, "active request cancelled; daemon recycle required");
    }

    private byte[] runActive(Request request) throws Exception {
        if (request.isolation != 0) {
            return refused(request.id, "isolation-ineligible",
                    "fork-sensitive tests cannot run in the warm daemon",
                    "choose --engine auto so the build tool owns this partition");
        }
        String stale = verifyOutputs(request.outputs);
        if (stale != null) {
            return refused(request.id, "classes-stale", stale,
                    "compile through the selected owner, then retry the current epoch");
        }
        long started = System.nanoTime();
        List<String> arguments = new ArrayList<>();
        arguments.add("--class-path");
        arguments.add(outputs);
        if (request.selectors.isEmpty()) {
            arguments.add("--scan-class-path");
            arguments.add("--fail-if-no-tests");
        } else {
            for (String selector : request.selectors) {
                arguments.add(selector.contains("#")
                        ? "--select-method=" + selector
                        : "--select-class=" + selector);
            }
        }
        arguments.add("--details=testfeed");
        Path reportDirectory = root.resolve(".jails/run/testd-reports").resolve(toHex(request.id));
        deleteTree(reportDirectory);
        Files.createDirectories(reportDirectory);
        arguments.add("--reports-dir=" + reportDirectory);
        Result result;
        try {
            result = runJUnit(arguments, reportDirectory);
        } finally {
            deleteTree(reportDirectory);
        }
        long durationUs = (System.nanoTime() - started) / 1_000L;
        byte[] accepted = accepted(request.id, request.epoch);
        byte[] completed = completed(request, result, durationUs);
        return concat(accepted, completed);
    }

    private String verifyOutputs(List<Output> expected) throws Exception {
        Map<String, byte[]> actual = new LinkedHashMap<>();
        for (Path output : splitPaths(outputs)) {
            if (!Files.isDirectory(output)) continue;
            try (var paths = Files.walk(output)) {
                for (Path path : paths.filter(Files::isRegularFile).sorted().toList()) {
                    String relative = root.relativize(path.toAbsolutePath().normalize()).toString().replace('\\', '/');
                    actual.put(relative, sha256(Files.readAllBytes(path)));
                }
            }
        }
        if (actual.size() != expected.size()) {
            return "output snapshot changed from " + expected.size() + " to " + actual.size() + " files";
        }
        long newestClass = 0;
        for (Output entry : expected) {
            byte[] digest = actual.remove(entry.path);
            if (digest == null || !MessageDigest.isEqual(digest, entry.digest)) {
                return entry.path + " changed after the coordinator snapshot";
            }
            if (entry.path.endsWith(".class")) newestClass = Math.max(newestClass, entry.modifiedNs);
        }
        if (!actual.isEmpty()) return actual.keySet().iterator().next() + " appeared after the snapshot";
        long newestSource = newestJava(root.resolve("src"));
        if (newestClass == 0 || newestSource > newestClass) return "a Java source is newer than its class output";
        return null;
    }

    private static long newestJava(Path sourceRoot) throws Exception {
        if (!Files.isDirectory(sourceRoot)) return 0;
        long newest = 0;
        try (var paths = Files.walk(sourceRoot)) {
            for (Path path : paths.filter(path -> path.toString().endsWith(".java")).toList()) {
                newest = Math.max(newest, Files.getLastModifiedTime(path).toMillis() * 1_000_000L);
            }
        }
        return newest;
    }

    private static List<Path> splitPaths(String joined) {
        List<Path> paths = new ArrayList<>();
        for (String path : joined.split(java.io.File.pathSeparator, -1)) {
            if (!path.isEmpty()) paths.add(Path.of(path));
        }
        return paths;
    }

    private static Result runJUnit(List<String> arguments) {
        return runJUnit(arguments, null);
    }

    private static Result runJUnit(List<String> arguments, Path reportDirectory) {
        var buffer = new ByteArrayOutputStream();
        var writer = new PrintWriter(buffer, true, StandardCharsets.UTF_8);
        int exitCode;
        try {
            List<String> full = new ArrayList<>();
            full.add("execute");
            full.addAll(arguments);
            exitCode = org.junit.platform.console.ConsoleLauncher
                    .run(writer, writer, full.toArray(String[]::new)).getExitCode();
        } catch (Throwable failure) {
            writer.println("jails testd: " + failure);
            exitCode = 2;
        }
        writer.flush();
        String output = buffer.toString(StandardCharsets.UTF_8);
        if (output.length() > 65_536) output = output.substring(0, 65_536) + "\n[testd output truncated]\n";
        List<Case> cases = List.of();
        if (reportDirectory != null) {
            try {
                cases = readCases(reportDirectory);
                if (cases.isEmpty()) {
                    output += "\njails testd: JUnit produced no readable case report\n";
                    exitCode = 2;
                }
            } catch (Exception failure) {
                output += "\njails testd: could not normalize JUnit reports: " + failure + "\n";
                exitCode = 2;
            }
        }
        return new Result(output, exitCode, cases);
    }

    private static List<Case> readCases(Path directory) throws Exception {
        var factory = javax.xml.parsers.DocumentBuilderFactory.newInstance();
        factory.setFeature("http://apache.org/xml/features/disallow-doctype-decl", true);
        factory.setAttribute(javax.xml.XMLConstants.ACCESS_EXTERNAL_DTD, "");
        factory.setAttribute(javax.xml.XMLConstants.ACCESS_EXTERNAL_SCHEMA, "");
        var builder = factory.newDocumentBuilder();
        var cases = new ArrayList<Case>();
        try (var paths = Files.walk(directory)) {
            for (Path report : paths.filter(path -> path.toString().endsWith(".xml")).sorted().toList()) {
                var document = builder.parse(report.toFile());
                var nodes = document.getElementsByTagName("testcase");
                for (int index = 0; index < nodes.getLength(); index++) {
                    var element = (org.w3c.dom.Element) nodes.item(index);
                    String className = element.getAttribute("classname");
                    String method = element.getAttribute("name");
                    if (method.endsWith("()")) method = method.substring(0, method.length() - 2);
                    if (className.isBlank() || method.isBlank()) continue;
                    int outcome = 0;
                    var children = element.getChildNodes();
                    for (int child = 0; child < children.getLength(); child++) {
                        String name = children.item(child).getNodeName();
                        if (name.equals("error")) outcome = 3;
                        else if (name.equals("failure") && outcome != 3) outcome = 1;
                        else if (name.equals("skipped") && outcome == 0) outcome = 2;
                    }
                    long durationUs = 0;
                    try {
                        durationUs = Math.max(0L,
                                Math.round(Double.parseDouble(element.getAttribute("time")) * 1_000_000.0));
                    } catch (NumberFormatException ignored) {
                        // A malformed duration is unknown, not a reason to lose the case.
                    }
                    cases.add(new Case(className + "#" + method, outcome, durationUs));
                }
            }
        }
        cases.sort(java.util.Comparator.comparing(Case::selector));
        return List.copyOf(cases);
    }

    private static void deleteTree(Path directory) throws Exception {
        if (!Files.exists(directory)) return;
        try (var paths = Files.walk(directory)) {
            for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) {
                Files.deleteIfExists(path);
            }
        }
    }

    private static byte[] accepted(byte[] id, long epoch) throws Exception {
        var body = new Wire();
        body.tag(1);
        body.bytes(id);
        body.u64(epoch);
        return frame(body.done());
    }

    private static byte[] completed(Request request, Result result, long durationUs) throws Exception {
        var body = new Wire();
        body.tag(3);
        body.bytes(request.id);
        body.u64(request.epoch);
        body.bool(result.exitCode == 0);
        body.tag(0); // unit scope
        body.strings(request.selectors);
        body.u32(result.cases.size());
        for (int index = 0; index < result.cases.size(); index++) {
            Case testCase = result.cases.get(index);
            body.tag(2); // TestdV2
            body.tag(3); // compile owner: none (the coordinator compiled)
            body.string(testCase.selector);
            body.tag(0); // source absent
            body.tag(testCase.outcome);
            body.u64(testCase.durationUs);
            body.string(index == 0 ? result.output : "");
            body.string("");
            body.u32(1);
            if (request.selectors.isEmpty()) {
                body.tag(1); // scope
                body.tag(0); // unit
            } else {
                body.tag(0); // requested
            }
            body.tag(0); // fallback absent
        }
        body.u32(0); // fallback reasons
        return frame(body.done());
    }

    private static byte[] event(byte[] id, int kind, long epoch, boolean current, String reason)
            throws Exception {
        var body = new Wire();
        body.tag(2);
        body.bytes(id);
        body.tag(kind);
        body.u64(epoch);
        if (kind == 0) body.bool(current);
        else if (kind == 1) body.tag(0);
        else body.string(reason == null ? "" : reason);
        return frame(body.done());
    }

    private static byte[] refused(byte[] id, String code, String message, String fix) throws Exception {
        var body = new Wire();
        body.tag(4);
        body.bytes(id);
        body.string(code);
        body.string(message);
        if (fix == null) body.tag(0);
        else {
            body.tag(1);
            body.string(fix);
        }
        return frame(body.done());
    }

    private static SocketChannel acceptWithin(ServerSocketChannel server, long idleMillis) throws Exception {
        server.configureBlocking(false);
        long deadline = System.currentTimeMillis() + idleMillis;
        while (System.currentTimeMillis() < deadline) {
            SocketChannel client = server.accept();
            if (client != null) return client;
            Thread.sleep(25);
        }
        return null;
    }

    private static byte[] readFrame(SocketChannel client) throws Exception {
        ByteBuffer header = ByteBuffer.allocate(4).order(ByteOrder.BIG_ENDIAN);
        readFully(client, header);
        header.flip();
        int length = header.getInt();
        if (length < 0 || length > MAX_PAYLOAD) throw new IllegalArgumentException("payload length " + length);
        ByteBuffer payload = ByteBuffer.allocate(length);
        readFully(client, payload);
        return payload.array();
    }

    private static void readFully(SocketChannel client, ByteBuffer buffer) throws Exception {
        while (buffer.hasRemaining()) {
            if (client.read(buffer) < 0) throw new IllegalArgumentException("truncated frame");
        }
    }

    private static byte[] frame(byte[] payload) throws Exception {
        if (payload.length > MAX_PAYLOAD) throw new IllegalArgumentException("response is too large");
        var framed = new ByteArrayOutputStream(payload.length + 4);
        framed.write(ByteBuffer.allocate(4).putInt(payload.length).array());
        framed.write(payload);
        return framed.toByteArray();
    }

    private static void writeAll(SocketChannel client, byte[] bytes) throws Exception {
        ByteBuffer buffer = ByteBuffer.wrap(bytes);
        while (buffer.hasRemaining()) client.write(buffer);
    }

    private static byte[] concat(byte[] left, byte[] right) {
        byte[] joined = Arrays.copyOf(left, left.length + right.length);
        System.arraycopy(right, 0, joined, left.length, right.length);
        return joined;
    }

    private static byte[] sha256(byte[] bytes) throws Exception {
        return MessageDigest.getInstance("SHA-256").digest(bytes);
    }

    private static long metaspace() {
        return ManagementFactory.getMemoryPoolMXBeans().stream()
                .filter(pool -> pool.getName().contains("Metaspace"))
                .mapToLong(pool -> pool.getUsage().getUsed()).sum();
    }

    private static boolean leakedThread() {
        return Thread.getAllStackTraces().keySet().stream()
                .anyMatch(thread -> thread.isAlive() && !thread.isDaemon()
                        && thread != Thread.currentThread()
                        && !thread.getName().equals("main")
                        && !thread.getName().equals("DestroyJavaVM"));
    }

    private static byte[] fromHex(String text) {
        if (text.length() != 64) throw new IllegalArgumentException("digest must be 64 hex characters");
        byte[] bytes = new byte[32];
        for (int index = 0; index < bytes.length; index++) {
            int high = Character.digit(text.charAt(index * 2), 16);
            int low = Character.digit(text.charAt(index * 2 + 1), 16);
            if (high < 0 || low < 0) throw new IllegalArgumentException("digest is not hex");
            bytes[index] = (byte) ((high << 4) | low);
        }
        return bytes;
    }

    private static String toHex(byte[] bytes) {
        StringBuilder text = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) text.append(String.format("%02x", value & 0xff));
        return text.toString();
    }

    private record Result(String output, int exitCode, List<Case> cases) {}
    private record Case(String selector, int outcome, long durationUs) {}
    private record Output(String path, long size, long modifiedNs, byte[] digest) {}
    private record Cached(byte[] request, byte[] response) {}

    private record Request(int tag, byte[] id, int protocolMin, int protocolMax, byte[] project,
            byte[] cookie, long epoch, List<String> selectors, byte[] classpath,
            List<Output> outputs, int isolation) {
        static Request decode(byte[] payload) {
            Cursor cursor = new Cursor(payload);
            int tag = cursor.tag();
            byte[] id = cursor.bytes(32);
            int min = 0, max = 0;
            byte[] project;
            byte[] cookie;
            long epoch = 0;
            List<String> selectors = List.of();
            byte[] classpath = new byte[32];
            List<Output> outputs = List.of();
            int isolation = 0;
            if (tag == 0) {
                min = cursor.u32();
                max = cursor.u32();
                project = cursor.bytes(32);
                cookie = cursor.bytes(32);
            } else {
                project = cursor.bytes(32);
                cookie = cursor.bytes(32);
                if (tag == 1) {
                    epoch = cursor.u64();
                    selectors = cursor.strings();
                    classpath = cursor.bytes(32);
                    int count = cursor.u32();
                    var entries = new ArrayList<Output>(count);
                    String previous = null;
                    for (int index = 0; index < count; index++) {
                        String path = cursor.string();
                        if (previous != null && previous.compareTo(path) >= 0) {
                            throw new IllegalArgumentException("output paths are not canonical");
                        }
                        previous = path;
                        entries.add(new Output(path, cursor.u64(), cursor.u64(), cursor.bytes(32)));
                    }
                    outputs = List.copyOf(entries);
                    isolation = cursor.tag();
                } else if (tag < 2 || tag > 4) {
                    throw new IllegalArgumentException("unknown request tag " + tag);
                }
            }
            cursor.finish();
            return new Request(tag, id, min, max, project, cookie, epoch, selectors,
                    classpath, outputs, isolation);
        }
    }

    private static final class Cursor {
        private final ByteBuffer bytes;

        Cursor(byte[] payload) {
            this.bytes = ByteBuffer.wrap(payload).order(ByteOrder.BIG_ENDIAN);
        }

        int tag() {
            require(1);
            return bytes.get() & 0xff;
        }

        int u32() {
            require(4);
            int value = bytes.getInt();
            if (value < 0) throw new IllegalArgumentException("u32 exceeds Java protocol limit");
            return value;
        }

        long u64() {
            require(8);
            long value = bytes.getLong();
            if (value < 0) throw new IllegalArgumentException("u64 exceeds Java protocol limit");
            return value;
        }

        byte[] bytes(int count) {
            require(count);
            byte[] value = new byte[count];
            bytes.get(value);
            return value;
        }

        String string() {
            int count = u32();
            return new String(bytes(count), StandardCharsets.UTF_8);
        }

        List<String> strings() {
            int count = u32();
            var values = new ArrayList<String>(count);
            for (int index = 0; index < count; index++) values.add(string());
            return List.copyOf(values);
        }

        void finish() {
            if (bytes.hasRemaining()) throw new IllegalArgumentException("trailing request bytes");
        }

        private void require(int count) {
            if (count < 0 || bytes.remaining() < count) throw new IllegalArgumentException("truncated request");
        }
    }

    private static final class Wire {
        private final ByteArrayOutputStream bytes = new ByteArrayOutputStream();

        void tag(int value) { bytes.write(value); }
        void bool(boolean value) { tag(value ? 1 : 0); }
        void bytes(byte[] value) { bytes.writeBytes(value); }
        void u32(long value) { bytes.writeBytes(ByteBuffer.allocate(4).putInt((int) value).array()); }
        void u64(long value) { bytes.writeBytes(ByteBuffer.allocate(8).putLong(value).array()); }
        void string(String value) {
            byte[] encoded = value.getBytes(StandardCharsets.UTF_8);
            u32(encoded.length);
            bytes(encoded);
        }
        void strings(List<String> values) {
            u32(values.size());
            for (String value : values) string(value);
        }
        byte[] done() { return bytes.toByteArray(); }
    }
}
