package com.example.webcrawler.api;

/**
 * A failure this application knows how to describe to a client.
 *
 * <p>Sealed on purpose. {@link ApiExceptionHandler} switches over these to
 * choose a status code, and that switch has no {@code default} branch -- so
 * adding a variant here stops the build until someone decides what it means
 * over HTTP. An open hierarchy would instead let a new failure quietly become
 * a 500.
 *
 * <p>Abstract as well as sealed: a sealed class that can itself be
 * instantiated is one more case the switch has to cover, and javac says so.
 *
 * <p>These carry no stack trace: they describe an expected outcome (the id was
 * not there, the version had moved on), not a bug, and collecting a trace for
 * every 404 is pure cost.
 */
public abstract sealed class ApiException extends RuntimeException {

    private ApiException(String message) {
        // No writable stack trace, no suppression: an expected outcome does
        // not need the cost of a fill-in.
        super(message, null, false, false);
    }

    /** Nothing with that identity exists. Becomes a 404. */
    public static final class NotFound extends ApiException {
        public NotFound(String message) {
            super(message);
        }
    }

    /** The request conflicts with the current state. Becomes a 409. */
    public static final class Conflict extends ApiException {
        public Conflict(String message) {
            super(message);
        }
    }

    /**
     * The request was well-formed but the domain rejected it. Becomes a 422 --
     * as opposed to a 400, which means jails could not read the request at all.
     */
    public static final class Rejected extends ApiException {
        public Rejected(String message) {
            super(message);
        }
    }
}
