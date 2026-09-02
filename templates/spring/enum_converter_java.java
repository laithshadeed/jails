package {{pkg}};

import org.springframework.core.convert.converter.Converter;
import org.springframework.stereotype.Component;

@Component
public final class {{class}} implements Converter<String, {{name}}> {

    @Override
    public {{name}} convert(String source) {
        return {{name}}.fromWire(source);
    }
}
