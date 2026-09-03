package com.example.demo.adapters;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import tools.jackson.databind.JsonNode;
import tools.jackson.databind.json.JsonMapper;

/**
 * JSON reading and writing over one shared, thread-safe {@link JsonMapper}.
 *
 * <p>Jackson 3 (`tools.jackson`), not the 2.x `com.fasterxml.jackson` line.
 * java.time support is built in, so {@code LocalDate} round-trips as an ISO
 * string with no module to register, and dates are written as strings by
 * default rather than as numeric timestamps.
 *
 * <p>Records map to JSON objects without any annotations.
 *
 * <p>Two ways in, for two situations. {@link #read} binds the whole document
 * to a type -- right for input you control, wrong for input you do not, since
 * one bad element fails the entire parse. For untrusted input use
 * {@link #readTree} and {@link #convert} to validate element by element,
 * keeping the good records and reporting the bad ones.
 */
public final class Json {

    private static final JsonMapper MAPPER = JsonMapper.builder().build();

    private Json() {}

    public static <T> T read(Path path, Class<T> type) throws IOException {
        try (var in = Files.newInputStream(path)) {
            return MAPPER.readValue(in, type);
        }
    }

    /**
     * Reads the whole document as a tree, without binding it to any type.
     *
     * <p>Use this when the shape cannot be trusted: walk the tree, check each
     * node with {@code isObject()} and friends, and {@link #convert} the ones
     * that look right. Nothing is lost to a single malformed element.
     */
    public static JsonNode readTree(Path path) throws IOException {
        try (var in = Files.newInputStream(path)) {
            return MAPPER.readTree(in);
        }
    }

    /** Binds one already-parsed tree node to {@code type}. */
    public static <T> T convert(JsonNode node, Class<T> type) {
        return MAPPER.convertValue(node, type);
    }

    /**
     * Reads a JSON Lines file: one JSON value per line, blank lines skipped.
     *
     * <p>The format event logs and streaming exports use, because appending a
     * line is cheap where appending to an array is not. Returned as trees
     * rather than bound values for the same reason {@link #readTree} exists --
     * one malformed line should not cost you the whole file.
     */
    public static List<JsonNode> readJsonl(Path path) throws IOException {
        try (var lines = Files.lines(path)) {
            var nodes = new ArrayList<JsonNode>();
            for (var line : lines.filter(text -> !text.isBlank()).toList()) {
                nodes.add(MAPPER.readTree(line));
            }
            return List.copyOf(nodes);
        }
    }

    /** Reads a top-level JSON array into a list of {@code element}. */
    public static <T> List<T> readList(Path path, Class<T> element) throws IOException {
        var listType = MAPPER.getTypeFactory().constructCollectionType(List.class, element);
        try (var in = Files.newInputStream(path)) {
            return MAPPER.readValue(in, listType);
        }
    }

    /**
     * The same, from an already-open stream.
     *
     * <p>A classpath resource is not a {@link Path} once the application is a
     * jar, so anything shipped inside the build -- seed data, a fixture --
     * has to come in this way.
     */
    public static <T> List<T> readList(java.io.InputStream in, Class<T> element) {
        var listType = MAPPER.getTypeFactory().constructCollectionType(List.class, element);
        return MAPPER.readValue(in, listType);
    }

    /** Writes {@code value} as indented JSON, replacing any existing file. */
    public static void write(Path path, Object value) throws IOException {
        try (var out = Files.newOutputStream(path)) {
            MAPPER.writerWithDefaultPrettyPrinter().writeValue(out, value);
        }
    }

    /**
     * No {@code throws}: {@code JacksonException} extends
     * {@link RuntimeException} in Jackson 3, where its 2.x counterpart was
     * checked.
     */
    public static String toJson(Object value) {
        return MAPPER.writeValueAsString(value);
    }

    public static <T> T parse(String json, Class<T> type) {
        return MAPPER.readValue(json, type);
    }
}
