package com.example.demo.testkit;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.net.URISyntaxException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/**
 * Loads sample files from {@code src/test/resources/fixtures}.
 *
 * <p>Off the classpath, not by walking relative paths from the working
 * directory: {@code Path.of("../fixtures")} works until something runs the
 * suite from elsewhere, and then fails in a way that looks like a test bug.
 *
 * <p>A missing fixture fails immediately, naming what it looked for. Silently
 * returning empty input turns a typo into a passing test.
 */
public final class Fixtures {

    private static final String ROOT = "/fixtures/";

    private Fixtures() {}

    /** Raw bytes of a fixture, e.g. {@code bytes("example.json")}. */
    public static byte[] bytes(String name) {
        try (var in = Fixtures.class.getResourceAsStream(ROOT + name)) {
            if (in == null) {
                throw new IllegalArgumentException("no fixture named '" + name + "' under src/test/resources" + ROOT);
            }
            return in.readAllBytes();
        } catch (IOException error) {
            throw new UncheckedIOException("unreadable fixture: " + name, error);
        }
    }

    public static String text(String name) {
        return new String(bytes(name), StandardCharsets.UTF_8);
    }

    /** Non-blank lines, for line-oriented formats like CSV and JSONL. */
    public static List<String> lines(String name) {
        return text(name).lines().filter(line -> !line.isBlank()).toList();
    }

    /** Real filesystem path, for code under test that insists on a {@link Path}. */
    public static Path path(String name) {
        var url = Fixtures.class.getResource(ROOT + name);
        if (url == null) {
            throw new IllegalArgumentException("no fixture named '" + name + "' under src/test/resources" + ROOT);
        }
        try {
            return Path.of(url.toURI());
        } catch (URISyntaxException error) {
            throw new IllegalStateException("fixture path is not a file: " + name, error);
        }
    }

    /** Copies a fixture into {@code directory}, for tests that mutate their input. */
    public static Path copyTo(String name, Path directory) {
        try {
            Files.createDirectories(directory);
            var target = directory.resolve(Path.of(name).getFileName().toString());
            Files.write(target, bytes(name));
            return target;
        } catch (IOException error) {
            throw new UncheckedIOException("could not copy fixture " + name, error);
        }
    }
}
