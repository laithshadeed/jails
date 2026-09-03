package {{pkg}};

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

{{component}}public final class {{class}} implements {{name}}Repository {

    private final Map<{{boxed_key_type}}, {{name}}> rows = new LinkedHashMap<>();

    @Override
    public Optional<{{name}}> findById({{key_type}} id) {
        return Optional.ofNullable(rows.get(id));
    }

    @Override
    public List<{{name}}> findAll() {
        return List.copyOf(rows.values());
    }

    @Override
    public {{name}} save({{name}} value) {
        rows.put(value.{{key}}(), value);
        return value;
    }

    @Override
    public boolean deleteById({{key_type}} id) {
        return rows.remove(id) != null;
    }
}
