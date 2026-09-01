package {{pkg}};

{{imports}}/**
 * The fake is held to the same contract as the real adapter.
 *
 * <p>No database and no application context: the in-memory adapter is an
 * ordinary object with a constructor, which is the whole reason it exists.
 * The assertions live in {@code {{record}}RepositoryContract} so this and the
 * JDBC adapter's integration test cannot come to disagree about them.
 */
class InMemory{{record}}RepositoryTest {

    @Test
    void satisfiesThe{{record}}RepositoryContract() {
        {{record}}RepositoryContract.savesReadsAndDeletes(
                new InMemory{{record}}Repository(), new {{record}}({{arguments}}));
    }

    // Reader-owned cases belong below this stable boundary.
}
