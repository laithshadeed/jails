package {{pkg}};

import java.util.List;

public interface {{class}} {

    /**
     * @param query what the reader typed. It is parsed by PostgreSQL, not
     *     concatenated into SQL -- see the adapter.
     * @param limit how many rows at most.
     */
    List<{{name}}> matching(String query, int limit);
}
