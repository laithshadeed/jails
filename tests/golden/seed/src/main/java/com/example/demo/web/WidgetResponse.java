package com.example.demo.web;

import com.example.demo.domain.Widget;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Widget itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record WidgetResponse(
        UUID id,
        String name) {

    /** @return the response describing {@code widget}. */
    public static WidgetResponse from(Widget widget) {
        return new WidgetResponse(
                widget.id(),
                widget.name());
    }
}
