package com.example.demo.web;

import com.example.demo.domain.IssuePriority;
import org.springframework.core.convert.converter.Converter;
import org.springframework.stereotype.Component;

/**
 * Reads a IssuePriority from the value a client sends.
 *
 * <p>{@code @JsonValue} covers a JSON body and nothing else: a form field, a
 * path variable and a query parameter all go through Spring's conversion
 * service, whose enum converter calls {@code valueOf} and therefore knows
 * only the Java names. Without this bean, a request carrying a wire value is a
 * 400 whose message is about binding rather than about the value.
 */
@Component
public final class IssuePriorityConverter implements Converter<String, IssuePriority> {

    @Override
    public IssuePriority convert(String source) {
        return IssuePriority.fromWire(source);
    }
}
