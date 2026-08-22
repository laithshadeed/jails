package {{pkg}};

{{target_import}}/** Atomic state change guarded by tenant scope and an optimistic version. */
@FunctionalInterface
public interface {{name}}UseCase {

    {{target}} execute({{name}}Command command);

    final class NotFoundException extends RuntimeException {
        public NotFoundException() { super("resource not found in the authorized scope"); }
    }

    final class StaleVersionException extends RuntimeException {
        public StaleVersionException() { super("resource version is stale"); }
    }
}
