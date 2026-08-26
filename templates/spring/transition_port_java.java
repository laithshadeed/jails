package {{pkg}};

{{target_import}}/**
 * One atomic state change{{scope_clause}}, guarded by an optimistic version.
 *
 * <p>The three things this can conclude are a <b>return type</b>, not
 * exceptions. A caller that forgets a {@code catch} finds out in production; a
 * caller that forgets a {@code switch} arm does not compile. Both outcomes
 * below are expected -- a stale version is what optimistic locking is
 * <em>for</em> -- and an expected outcome is not a fault.
 */
@FunctionalInterface
public interface {{name}}UseCase {

    /**
     * @param command what to change
     * @param expectedVersion the version the caller believes the row is at.
     *     It arrives as an {@code If-Match} header rather than in the body:
     *     HTTP already has a word for "only if it is still what I read".
     */
    Result execute({{name}}Command command, {{version_type}} expectedVersion);

    /**
     * What the transition concluded.
     *
     * <p>Sealed, and every {@code switch} over it is written without a
     * {@code default}, so a fourth outcome is a compile error at every site
     * that has to decide what it means.
     */
    sealed interface Result {

        /** The row moved, and this is what it moved to. */
        record Applied({{target}} resource) implements Result {}

        /**
         * The row exists at a different version, and {@code current} is what
         * is actually stored -- so a caller can decide what to do without a
         * second request.
         */
        record StaleVersion({{target}} current) implements Result {}

        /** No row{{scope_clause}} has this id. */
        record NotFound({{key_type}} id) implements Result {}
    }
}
