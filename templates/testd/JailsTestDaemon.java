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
    // The coordinator's outcome vocabulary, in the order readCases numbers them.
    private static final String[] OUTCOMES = {"passed", "failed", "skipped", "error"};

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
        if (request.kind.equals("stop")) return true;
        if (request.kind.equals("run")) {
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
        return switch (request.kind) {
            case "hello" -> hello(request);
            case "run" -> run(request);
            case "status" -> event(request.id, "ready", request.epoch, true, null);
            case "cancel" -> cancel(request);
            case "stop" -> event(request.id, "recycling", 0, false, "stop requested");
            default -> refused(request.id, "unknown-request",
                    "unknown request `" + request.kind + "`", "upgrade both testd protocol peers");
        };
    }

    private byte[] hello(Request request) throws Exception {
        if (request.protocolMin > PROTOCOL_MAX || request.protocolMax < PROTOCOL_MIN) {
            return refused(request.id, "protocol-mismatch",
                    "no mutually supported testd protocol version",
                    "restart the daemon with a compatible jails version");
        }
        return frame("{\"response\":\"hello\",\"request_id\":" + quote(toHex(request.id))
                + ",\"protocol\":" + PROTOCOL_MAX + "}");
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
        return event(request.id, "recycling", 0, false,
                "active request cancelled; daemon recycle required");
    }

    private byte[] runActive(Request request) throws Exception {
        if (!request.isolation.equals("isolated")) {
            return refused(request.id, "isolation-ineligible",
                    "fork-sensitive tests cannot run in the warm daemon",
                    "choose --engine auto so the build tool owns this partition");
        }
        String stale = verifyOutputs(request.outputs);
        if (stale != null) {
            return refused(request.id, "classes-stale", stale,
                    "compile through the selected owner, then retry the current epoch");
        }
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
        // A completed report carries the daemon's own output on its first
        // case, so a run that produced no cases has nowhere to put a
        // diagnostic and the coordinator exits non-zero having printed
        // nothing. Refuse instead: the refusal frame has fields of its own,
        // and no report is a refusal rather than a run of zero tests.
        if (result.cases.isEmpty()) {
            return refused(request.id, "no-case-report", summarize(result.output),
                    "run the same selection with --engine build to see JUnit's own output");
        }
        return concat(accepted(request.id, request.epoch), completed(request, result));
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
        return frame("{\"response\":\"accepted\",\"request_id\":" + quote(toHex(id))
                + ",\"epoch\":" + epoch + "}");
    }

    // What the daemon *observed*, and nothing else. Which engine ran these,
    // which scope the reader asked for and why each selector was chosen are
    // the coordinator's facts, and a daemon that restated them would be a
    // second copy of a Rust type -- wrong the first time a field moved.
    private static byte[] completed(Request request, Result result) throws Exception {
        var cases = new StringBuilder();
        for (int index = 0; index < result.cases.size(); index++) {
            Case testCase = result.cases.get(index);
            if (index > 0) cases.append(',');
            cases.append("{\"selector\":").append(quote(testCase.selector()))
                    .append(",\"outcome\":").append(quote(OUTCOMES[testCase.outcome()]))
                    .append(",\"duration_us\":").append(testCase.durationUs()).append('}');
        }
        return frame("{\"response\":\"completed\",\"request_id\":" + quote(toHex(request.id))
                + ",\"result\":{\"epoch\":" + request.epoch
                + ",\"passed\":" + (result.exitCode == 0)
                // JUnit's own output belongs to the run, not to a case: where
                // it is printed is the coordinator's decision.
                + ",\"output\":" + quote(result.output)
                + ",\"cases\":[" + cases + "]}}");
    }

    private static byte[] event(byte[] id, String kind, long epoch, boolean current, String reason)
            throws Exception {
        String detail = switch (kind) {
            case "ready" -> ",\"output_current\":" + current;
            case "classes_stale" -> ",\"path\":null";
            default -> ",\"reason\":" + quote(reason == null ? "" : reason);
        };
        return frame("{\"response\":\"event\",\"request_id\":" + quote(toHex(id))
                + ",\"event\":{\"kind\":" + quote(kind) + ",\"epoch\":" + epoch + detail + "}}");
    }

    // Why JUnit produced nothing, in the two places it says so: the head of
    // its output, where an unaccepted argument or a thrown exception lands,
    // and every `Caused by:` line, which is the half of a stack trace worth
    // reading. The frames between them are noise, and the tail is this
    // daemon's own note plus a page of usage text. Empty output still says
    // something rather than nothing.
    private static String summarize(String output) {
        List<String> kept = new ArrayList<>();
        List<String> causes = new ArrayList<>();
        for (String line : output.strip().split("\\R")) {
            String trimmed = line.strip();
            if (trimmed.isEmpty()) continue;
            if (trimmed.startsWith("Caused by:")) causes.add(trimmed);
            else if (kept.size() < 6) kept.add(trimmed);
        }
        kept.addAll(causes);
        if (kept.isEmpty()) return "JUnit produced no case report and said nothing";
        String joined = String.join(" | ", kept);
        return joined.length() > 1200 ? joined.substring(0, 1200) + "..." : joined;
    }

    private static byte[] refused(byte[] id, String code, String message, String fix) throws Exception {
        return frame("{\"response\":\"refused\",\"request_id\":" + quote(toHex(id))
                + ",\"diagnostic\":{\"code\":" + quote(code)
                + ",\"message\":" + quote(message)
                + ",\"fix\":" + (fix == null ? "null" : quote(fix)) + "}}");
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

    private static byte[] frame(String json) throws Exception {
        byte[] payload = json.getBytes(StandardCharsets.UTF_8);
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
    private record Output(String path, long modifiedNs, byte[] digest) {}
    private record Cached(byte[] request, byte[] response) {}

    private record Request(String kind, byte[] id, int protocolMin, int protocolMax, byte[] project,
            byte[] cookie, long epoch, List<String> selectors, byte[] classpath,
            List<Output> outputs, String isolation) {
        static Request decode(byte[] payload) {
            Map<String, Object> frame = Json.object(
                    Json.parse(new String(payload, StandardCharsets.UTF_8)), "request frame");
            String kind = Json.string(frame, "request");
            byte[] id = fromHex(Json.string(frame, "request_id"));
            int min = 0;
            int max = 0;
            long epoch = 0;
            List<String> selectors = List.of();
            byte[] classpath = new byte[32];
            List<Output> outputs = List.of();
            String isolation = "isolated";
            if (kind.equals("hello")) {
                min = (int) Json.number(frame, "protocol_min");
                max = (int) Json.number(frame, "protocol_max");
            } else if (kind.equals("run")) {
                epoch = Json.number(frame, "epoch");
                var names = new ArrayList<String>();
                for (Object selector : Json.array(frame, "selectors")) {
                    names.add(Json.text(selector, "selector"));
                }
                selectors = List.copyOf(names);
                classpath = fromHex(Json.string(frame, "classpath"));
                outputs = outputs(Json.object(frame.get("outputs"), "outputs"));
                isolation = Json.string(frame, "isolation");
            }
            return new Request(kind, id, min, max, fromHex(Json.string(frame, "project")),
                    fromHex(Json.string(frame, "cookie")), epoch, selectors, classpath,
                    outputs, isolation);
        }

        // Sorted and distinct is checked here, not assumed: this list is
        // compared against the tree on disk, and two spellings of one set
        // would make that comparison depend on iteration order.
        private static List<Output> outputs(Map<String, Object> snapshot) {
            var entries = new ArrayList<Output>();
            String previous = null;
            for (Object element : Json.array(snapshot, "entries")) {
                Map<String, Object> entry = Json.object(element, "output entry");
                String path = Json.string(entry, "path");
                if (previous != null && previous.compareTo(path) >= 0) {
                    throw new IllegalArgumentException("output paths are not canonical");
                }
                previous = path;
                entries.add(new Output(path, Json.number(entry, "modified_ns"),
                        fromHex(Json.string(entry, "digest"))));
            }
            return List.copyOf(entries);
        }
    }

    // The smallest JSON reader that reads these frames, and no more: objects,
    // arrays, strings, integers, booleans and null. Not a JSON library, for
    // the same reason the daemon is a single source file -- it runs with
    // JUnit on its classpath and nothing else.
    private static final class Json {
        private final String text;
        private int at;

        private Json(String text) {
            this.text = text;
        }

        static Object parse(String text) {
            Json reader = new Json(text);
            Object value = reader.value();
            reader.skip();
            if (reader.at != text.length()) throw new IllegalArgumentException("trailing JSON");
            return value;
        }

        @SuppressWarnings("unchecked")
        static Map<String, Object> object(Object value, String what) {
            if (value instanceof Map) return (Map<String, Object>) value;
            throw new IllegalArgumentException(what + " is not a JSON object");
        }

        @SuppressWarnings("unchecked")
        static List<Object> array(Map<String, Object> owner, String key) {
            Object value = owner.get(key);
            if (value instanceof List) return (List<Object>) value;
            throw new IllegalArgumentException("`" + key + "` is not a JSON array");
        }

        static String string(Map<String, Object> owner, String key) {
            return text(owner.get(key), key);
        }

        static String text(Object value, String what) {
            if (value instanceof String string) return string;
            throw new IllegalArgumentException("`" + what + "` is not a JSON string");
        }

        static long number(Map<String, Object> owner, String key) {
            if (owner.get(key) instanceof Long value && value >= 0) return value;
            throw new IllegalArgumentException("`" + key + "` is not a non-negative integer");
        }

        private Object value() {
            skip();
            if (at >= text.length()) throw new IllegalArgumentException("truncated JSON");
            return switch (text.charAt(at)) {
                case '{' -> readObject();
                case '[' -> readArray();
                case '"' -> readString();
                case 't' -> literal("true", Boolean.TRUE);
                case 'f' -> literal("false", Boolean.FALSE);
                case 'n' -> literal("null", null);
                default -> readNumber();
            };
        }

        private Map<String, Object> readObject() {
            var members = new LinkedHashMap<String, Object>();
            at++;
            skip();
            if (peek() == '}') {
                at++;
                return members;
            }
            while (true) {
                skip();
                String key = readString();
                skip();
                expect(':');
                members.put(key, value());
                skip();
                char next = peek();
                at++;
                if (next == '}') return members;
                if (next != ',') throw new IllegalArgumentException("expected `,` or `}`");
            }
        }

        private List<Object> readArray() {
            var elements = new ArrayList<Object>();
            at++;
            skip();
            if (peek() == ']') {
                at++;
                return elements;
            }
            while (true) {
                elements.add(value());
                skip();
                char next = peek();
                at++;
                if (next == ']') return elements;
                if (next != ',') throw new IllegalArgumentException("expected `,` or `]`");
            }
        }

        private String readString() {
            expect('"');
            var out = new StringBuilder();
            while (true) {
                if (at >= text.length()) throw new IllegalArgumentException("unterminated string");
                char c = text.charAt(at++);
                if (c == '"') return out.toString();
                if (c != '\\') {
                    if (c < 0x20) throw new IllegalArgumentException("raw control character");
                    out.append(c);
                    continue;
                }
                if (at >= text.length()) throw new IllegalArgumentException("truncated escape");
                char escape = text.charAt(at++);
                switch (escape) {
                    case '"', '\\', '/' -> out.append(escape);
                    case 'b' -> out.append('\b');
                    case 'f' -> out.append('\f');
                    case 'n' -> out.append('\n');
                    case 'r' -> out.append('\r');
                    case 't' -> out.append('\t');
                    case 'u' -> {
                        if (at + 4 > text.length()) throw new IllegalArgumentException("truncated \\u escape");
                        out.append((char) Integer.parseInt(text.substring(at, at + 4), 16));
                        at += 4;
                    }
                    default -> throw new IllegalArgumentException("unknown escape `" + escape + "`");
                }
            }
        }

        private Long readNumber() {
            int start = at;
            if (peek() == '-') at++;
            while (at < text.length() && Character.isDigit(text.charAt(at))) at++;
            if (at == start) throw new IllegalArgumentException("expected a JSON value");
            return Long.parseLong(text.substring(start, at));
        }

        private Object literal(String word, Object value) {
            if (!text.startsWith(word, at)) throw new IllegalArgumentException("expected " + word);
            at += word.length();
            return value;
        }

        private char peek() {
            if (at >= text.length()) throw new IllegalArgumentException("truncated JSON");
            return text.charAt(at);
        }

        private void expect(char expected) {
            if (peek() != expected) throw new IllegalArgumentException("expected `" + expected + "`");
            at++;
        }

        private void skip() {
            while (at < text.length() && Character.isWhitespace(text.charAt(at))) at++;
        }
    }

    // One JSON string, escaped. JUnit's output goes through here, so control
    // characters are escaped rather than passed through: a raw newline in a
    // string is what turns a report into a decode error on the far side.
    private static String quote(String value) {
        var out = new StringBuilder(value.length() + 2).append('"');
        for (int index = 0; index < value.length(); index++) {
            char c = value.charAt(index);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\b' -> out.append("\\b");
                case '\f' -> out.append("\\f");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (c < 0x20) out.append(String.format("\\u%04x", (int) c));
                    else out.append(c);
                }
            }
        }
        return out.append('"').toString();
    }
}
