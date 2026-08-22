package {{pkg}};

{{target_import}}/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface {{name}}UseCase {

    {{target}} execute({{name}}Command command);
}
