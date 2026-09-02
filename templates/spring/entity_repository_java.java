package {{pkg}};

import java.util.List;
import java.util.Optional;

public interface {{class}} {

    Optional<{{name}}> findById({{key_type}} id);

    List<{{name}}> findAll();

    {{name}} save({{name}} {{variable}});

    boolean deleteById({{key_type}} id);

    // Reader extensions belong below this stable boundary.
}
