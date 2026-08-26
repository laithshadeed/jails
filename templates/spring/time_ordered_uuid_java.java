package {{pkg}};

import java.security.SecureRandom;
import java.util.UUID;

/**
 * A time-ordered identifier, for use as a primary key.
 *
 * <p>{@link UUID#randomUUID()} produces a version 4 UUID: 122 random bits and
 * no order. As a primary key on a growing table that is the wrong shape -- a
 * b-tree index over random values touches a different page for every insert,
 * so the working set is the whole index rather than its right-hand edge. This
 * produces a version 7 UUID instead (RFC 9562): 48 bits of Unix milliseconds
 * first, so successive values sort in the order they were created and inserts
 * stay local.
 *
 * <p>Not in the JDK, which is why this class exists rather than a call. It is
 * also not a general-purpose utility: the one thing it does is name a new row,
 * and {@code TimeOrderedUuidTest} pins the bits that make it a valid v7.
 *
 * <p><b>Ordering is to the millisecond, not within one.</b> Two identifiers
 * minted in the same millisecond order randomly with respect to each other,
 * which RFC 9562 permits. Anything that needs a total order needs a column
 * that carries one.
 */
public final class TimeOrderedUuid {

    private static final SecureRandom RANDOM = new SecureRandom();

    private TimeOrderedUuid() {}

    /** @return a fresh version 7 UUID. */
    public static UUID next() {
        byte[] value = new byte[16];
        RANDOM.nextBytes(value);
        long milliseconds = System.currentTimeMillis();
        value[0] = (byte) (milliseconds >>> 40);
        value[1] = (byte) (milliseconds >>> 32);
        value[2] = (byte) (milliseconds >>> 24);
        value[3] = (byte) (milliseconds >>> 16);
        value[4] = (byte) (milliseconds >>> 8);
        value[5] = (byte) milliseconds;
        // Version 7 in the high nibble of byte 6, and the RFC 9562 variant in
        // the top two bits of byte 8. Both overwrite random bits rather than
        // adding any, so the remaining 74 stay random.
        value[6] = (byte) ((value[6] & 0x0f) | 0x70);
        value[8] = (byte) ((value[8] & 0x3f) | 0x80);
        long high = 0;
        long low = 0;
        for (int index = 0; index < 8; index++) {
            high = (high << 8) | (value[index] & 0xffL);
        }
        for (int index = 8; index < 16; index++) {
            low = (low << 8) | (value[index] & 0xffL);
        }
        return new UUID(high, low);
    }
}
