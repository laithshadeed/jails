package {{pkg}};

import java.net.URI;
import java.util.Arrays;
import java.util.Objects;

/** A bounded outbound byte fetch; callers own parsing and link policy. */
@FunctionalInterface
public interface {{name}}Fetcher {

    FetchedResource fetch(URI uri);

    record FetchedResource(URI uri, int statusCode, String contentType, byte[] body) {

        public FetchedResource {
            Objects.requireNonNull(uri, "uri is required");
            Objects.requireNonNull(contentType, "contentType is required");
            body = Arrays.copyOf(Objects.requireNonNull(body, "body is required"), body.length);
        }

        @Override
        public byte[] body() {
            return Arrays.copyOf(body, body.length);
        }
    }

    /** Failure classification lets a durable caller avoid retrying policy and 4xx errors. */
    final class FetchException extends RuntimeException {

        private final boolean retryable;

        public FetchException(String message, boolean retryable) {
            super(message);
            this.retryable = retryable;
        }

        public FetchException(String message, boolean retryable, Throwable cause) {
            super(message, cause);
            this.retryable = retryable;
        }

        public boolean retryable() {
            return retryable;
        }
    }
}
