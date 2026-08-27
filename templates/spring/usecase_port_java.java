package {{pkg}};

{{target_import}}{{optional_import}}/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface {{name}}UseCase {

{{returns_doc}}    {{returns}} execute({{name}}Command command);
}
