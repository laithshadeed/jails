package {{pkg}};

{{imports}}
/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link Jdbc{{name}}Repository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
{{annotations}}class Jdbc{{name}}RepositoryIT {

{{repository_field}}
    @Test
    void roundTripsThroughTheRealDatabase() {
{{body}}
    }
}
