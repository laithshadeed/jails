package {{pkg}};

{{target_import}}import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface {{name}}Query {

    List<{{target}}> execute({{name}}Criteria criteria);
}
