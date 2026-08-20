package com.example.demo.adapters;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class CsvReaderTest {

    @TempDir
    Path tmp;

    private Path csv(String contents) throws Exception {
        var path = tmp.resolve("rows.csv");
        Files.writeString(path, contents);
        return path;
    }

    @Test
    void readsRowsKeyedByHeader() throws Exception {
        var rows = CsvReader.read(csv("name,qty\nbolt,7\n"));

        assertEquals(1, rows.size());
        assertEquals("bolt", rows.getFirst().get("name"));
        assertEquals(7, rows.getFirst().getInt("qty"));
    }

    @Test
    void keepsCommasInsideQuotedFields() throws Exception {
        var rows = CsvReader.read(csv("name,qty\n\"widget, large\",3\n"));

        assertEquals("widget, large", rows.getFirst().get("name"));
    }

    @Test
    void readsAnEmptyFileAsNoRows() throws Exception {
        assertEquals(List.of(), CsvReader.read(csv("name,qty\n")));
    }

    @Test
    void namesTheColumnWhenItIsMissing() throws Exception {
        var rows = CsvReader.read(csv("name,qty\nbolt,7\n"));

        var error = assertThrows(IllegalArgumentException.class, () -> rows.getFirst().get("price"));
        assertEquals(true, error.getMessage().contains("price"));
    }
}
