package com.example.demo.web;

import com.example.demo.domain.Article;
import java.util.UUID;

/**
 * What this application returns. Deliberately not Article itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record ArticleResponse(
        UUID id,
        String title,
        String body) {

    /** @return the response describing {@code article}. */
    public static ArticleResponse from(Article article) {
        return new ArticleResponse(
                article.id(),
                article.title(),
                article.body());
    }
}
