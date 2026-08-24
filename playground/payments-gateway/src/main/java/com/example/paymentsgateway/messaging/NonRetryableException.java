package com.example.paymentsgateway.messaging;

/**
 * A failure that will fail identically on every redelivery.
 *
 * <p>This is the only classification the Kafka error handler cannot work out
 * for itself. Spring already knows that a record which will not deserialize is
 * unprocessable; it cannot know that a value which parsed perfectly names a
 * currency, region or status this service has no constant for. That is a
 * property of the domain, so the domain declares it -- throw this and the
 * record goes to the dead-letter topic on the first attempt instead of costing
 * the partition three retries first.
 *
 * <p>The distinction that matters is not "expected vs unexpected", it is
 * "would a retry change the outcome". A database that is briefly unavailable is
 * unexpected and worth retrying. A {@code NullPointerException} is a bug in
 * this service: do <em>not</em> wrap it in this, or the bug is committed past
 * and buried in the dead-letter topic along with genuinely bad records.
 *
 * <p>Keeps its stack trace, unlike an expected-outcome exception: something
 * has to be readable when the dead-lettered record is investigated.
 */
public class NonRetryableException extends RuntimeException {

    public NonRetryableException(String message) {
        super(message);
    }

    /**
     * @param cause the failure that proves the record unprocessable -- an enum
     *     lookup that found nothing, a value out of range. Kept, because the
     *     dead-letter headers carry it.
     */
    public NonRetryableException(String message, Throwable cause) {
        super(message, cause);
    }
}
