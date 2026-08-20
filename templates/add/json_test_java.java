package {{pkg}};

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {{class}}Test {

    /** Records need no annotations to round-trip. */
    record Item(String name, int qty) {}

    record Dated(String name, LocalDate on) {}

    @TempDir
    Path tmp;

    @Test
    void roundTripsARecordThroughAFile() throws Exception {
        var path = tmp.resolve("item.json");
        {{class}}.write(path, new Item("bolt", 7));

        assertEquals(new Item("bolt", 7), {{class}}.read(path, Item.class));
    }

    @Test
    void readsAJsonArrayAsAList() throws Exception {
        var path = tmp.resolve("items.json");
        Files.writeString(path, "[{\"name\":\"bolt\",\"qty\":7},{\"name\":\"nut\",\"qty\":3}]");

        assertEquals(List.of(new Item("bolt", 7), new Item("nut", 3)), {{class}}.readList(path, Item.class));
    }

    @Test
    void roundTripsThroughAString() throws Exception {
        assertEquals(new Item("bolt", 7), {{class}}.parse({{class}}.toJson(new Item("bolt", 7)), Item.class));
    }

    /**
     * Without the java.time module on the classpath this writes
     * {@code {"year":2026,...}} instead of an ISO string, and reading it back
     * fails outright.
     */
    @Test
    void writesDatesAsIsoStringsNotObjects() throws Exception {
        var json = {{class}}.toJson(new Dated("invoice", LocalDate.of(2026, 8, 1)));

        assertTrue(json.contains("\"2026-08-01\""), "expected an ISO date in " + json);
        assertEquals(new Dated("invoice", LocalDate.of(2026, 8, 1)), {{class}}.parse(json, Dated.class));
    }

    @Test
    void readsOneJsonValuePerLine() throws Exception {
        var path = tmp.resolve("events.jsonl");
        Files.writeString(path, "{\"id\":1}\n\n{\"id\":2}\n");

        var events = {{class}}.readJsonl(path);

        assertEquals(2, events.size(), "blank lines should be skipped");
        assertEquals(1, events.getFirst().get("id").asInt());
        assertEquals(2, events.getLast().get("id").asInt());
    }

    @Test
    void readsAnEmptyJsonlFileAsNoEvents() throws Exception {
        var path = tmp.resolve("empty.jsonl");
        Files.writeString(path, "");

        assertEquals(List.of(), {{class}}.readJsonl(path));
    }

    @Test
    void readsATreeWithoutBindingItToAType() throws Exception {
        var path = tmp.resolve("tree.json");
        Files.writeString(path, "{\"items\":[{\"name\":\"bolt\",\"qty\":7}]}");

        var root = {{class}}.readTree(path);

        assertTrue(root.isObject());
        assertEquals("bolt", root.get("items").get(0).get("name").asText());
    }

    /**
     * The reason the tree API exists: a document with junk mixed into an array
     * still yields every well-formed element, rather than failing as a whole.
     */
    @Test
    void keepsGoodElementsWhenSiblingsAreMalformed() throws Exception {
        var path = tmp.resolve("mixed.json");
        Files.writeString(path, "[{\"name\":\"bolt\",\"qty\":7},\"not-an-object\",{\"name\":\"nut\",\"qty\":3}]");

        var good = new ArrayList<Item>();
        var skipped = 0;
        for (var node : {{class}}.readTree(path)) {
            if (node.isObject()) {
                good.add({{class}}.convert(node, Item.class));
            } else {
                skipped++;
            }
        }

        assertEquals(List.of(new Item("bolt", 7), new Item("nut", 3)), good);
        assertEquals(1, skipped);
    }
}
