package com.example.demo.clients;

import com.example.demo.domain.ChatReply;
import com.example.demo.domain.ChatRequest;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.service.annotation.PostExchange;

/**
 * The one call this application makes to the OpenAiChat service.
 *
 * <p>An interface and nothing else: Spring builds the implementation, and the
 * base URL is configuration (see {@link OpenAiChatClientConfig}), so pointing it
 * at a stub, staging or production is not a code change. It returns a domain
 * type rather than {@code ResponseEntity} because a non-2xx response is
 * already an exception.
 */
public interface OpenAiChatClient {

    @PostExchange("/v1/chat/completions")
    ChatReply call(@RequestBody ChatRequest request);
}
