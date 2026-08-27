package com.example.demo.domain;

import java.util.Objects;

/**
 * An immutable ChatRequest value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ChatRequest(String model, String prompt) {

    public ChatRequest {
        Objects.requireNonNull(model, "model");
        Objects.requireNonNull(prompt, "prompt");
        model = model.trim();
        if (model.isEmpty()) {
            throw new IllegalArgumentException("model must not be blank");
        }
        prompt = prompt.trim();
        if (prompt.isEmpty()) {
            throw new IllegalArgumentException("prompt must not be blank");
        }
    }
}
