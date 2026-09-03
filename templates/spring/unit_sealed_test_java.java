package {{pkg}};

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class {{class}}Test {

    private String describe({{class}} value) {
        return switch (value) {
{{arms}}
        };
    }

{{tests}}
}
