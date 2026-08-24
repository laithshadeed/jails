package com.example.ledgercli.adapters;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class DatabaseTest {

    @TempDir
    Path tmp;

    private Path migrationDir() throws Exception {
        var dir = tmp.resolve("migration");
        Files.createDirectories(dir);
        Files.writeString(
                dir.resolve("001_init.sql"), "create table item (id integer primary key, name text not null);");
        return dir;
    }

    @Test
    void appliesEachMigrationExactlyOnce() throws Exception {
        var database = new Database(tmp.resolve("test.db"));
        var dir = migrationDir();

        try (var connection = database.open()) {
            assertEquals(List.of("001_init.sql"), Migrations.applyAll(connection, dir));
            assertEquals(List.of(), Migrations.applyAll(connection, dir), "second run should be a no-op");
        }
    }

    @Test
    void storesAndReadsRows() throws Exception {
        var database = new Database(tmp.resolve("test.db"));
        var dir = migrationDir();

        try (var connection = database.open()) {
            Migrations.applyAll(connection, dir);

            try (var insert = connection.prepareStatement("insert into item(name) values (?)")) {
                insert.setString(1, "bolt");
                insert.executeUpdate();
            }
            try (var query = connection.prepareStatement("select name from item");
                    var rows = query.executeQuery()) {
                assertTrue(rows.next());
                assertEquals("bolt", rows.getString("name"));
            }
        }
    }

    @Test
    void treatsAMissingMigrationDirectoryAsNoMigrations() throws Exception {
        try (var connection = Database.inMemory().open()) {
            assertEquals(List.of(), Migrations.applyAll(connection, tmp.resolve("nope")));
        }
    }
}
