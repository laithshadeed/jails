package com.example.demo.adapters;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import org.apache.commons.csv.CSVFormat;

/**
 * Reads a CSV file with a header row into {@link Row} values.
 *
 * <p>Parsing is delegated to Commons CSV so quoted fields, embedded commas
 * and embedded newlines are handled correctly.
 */
public final class CsvReader {

    private CsvReader() {}

    /** One CSV record: column name to value. */
    public record Row(Map<String, String> values) {

        public Row {
            values = Map.copyOf(values);
        }

        /** Value of {@code column}, or a clear failure if it is not in the header. */
        public String get(String column) {
            var value = values.get(column);
            if (value == null) {
                throw new IllegalArgumentException("no column named '" + column + "' in " + values.keySet());
            }
            return value;
        }

        public int getInt(String column) {
            return Integer.parseInt(get(column));
        }
    }

    /** Reads every row of {@code path}, treating the first line as the header. */
    public static List<Row> read(Path path) throws IOException {
        var format = CSVFormat.DEFAULT.builder()
                .setHeader()
                .setSkipHeaderRecord(true)
                .setTrim(true)
                .get();
        try (var reader = Files.newBufferedReader(path);
                var parser = format.parse(reader)) {
            return parser.stream().map(record -> new Row(record.toMap())).toList();
        } catch (UncheckedIOException e) {
            throw e.getCause();
        }
    }
}
