package com.example.demo.domain;

import java.util.Map;
import java.util.Objects;

/**
 * A validated ApiError value.
 *
 * <p>All validation lives in the compact constructor, which runs before the
 * components are assigned -- so there is no way to reach an instance that
 * skipped it, not even through deserialisation or a copy.
 *
 * <p>Text marked {@code !} is trimmed and then required to be non-blank: a
 * present-but-empty value passes every null check downstream, which is
 * exactly why it is worth rejecting here instead.
 */
public record ApiError(String code, String message, Map<String, String> details) {

    public ApiError {
        Objects.requireNonNull(code, "code is required");
        Objects.requireNonNull(message, "message is required");
        details = details == null ? Map.of() : Map.copyOf(details);
        code = code.trim();
        if (code.isEmpty()) {
            throw new IllegalArgumentException("code must not be blank");
        }
        message = message.trim();
        if (message.isEmpty()) {
            throw new IllegalArgumentException("message must not be blank");
        }
    }

    /**
     * Builds a ApiError. Identical to the constructor today; it exists so that
     * parsing, defaulting or a cache can be added later without changing a
     * single call site.
     */
    public static ApiError of(String code, String message, Map<String, String> details) {
        return new ApiError(code, message, details);
    }
}
