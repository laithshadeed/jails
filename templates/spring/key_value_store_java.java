package {{pkg}};

import java.time.Duration;
import java.util.Objects;
import java.util.Optional;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;

/**
 * The application's view of Redis: get, put with a lifetime, and remove.
 *
 * <p>A named wrapper rather than {@link StringRedisTemplate} injected
 * everywhere. Two reasons, and the second is the real one:
 *
 * <ul>
 *   <li>Callers depend on three methods instead of Redis's whole surface,
 *       so the store can be replaced with an in-memory one in a test.
 *   <li>**Every write gets a TTL.** {@code opsForValue().set(k, v)} with no
 *       expiry stores a key forever, and a cache that never evicts is a
 *       memory leak that survives restarts and is discovered in production.
 *       Making the lifetime a required argument -- with a configured default
 *       -- means forgetting it is not possible.
 * </ul>
 *
 * <p>{@link StringRedisTemplate} rather than a serializing template: the
 * values are strings, so what is in Redis is readable with {@code redis-cli}
 * rather than being a Java serialization blob nothing else can inspect.
 */
@Component
public class KeyValueStore {

    private final StringRedisTemplate redis;
    private final Duration defaultTtl;

    public KeyValueStore(StringRedisTemplate redis, @Value("${app.redis.default-ttl:PT10M}") Duration defaultTtl) {
        this.redis = Objects.requireNonNull(redis, "redis is required");
        this.defaultTtl = Objects.requireNonNull(defaultTtl, "defaultTtl is required");
    }

    /** @return the value, or empty when the key is absent or has expired. */
    public Optional<String> get(String key) {
        return Optional.ofNullable(redis.opsForValue().get(key));
    }

    /** Stores with the configured default lifetime. */
    public void put(String key, String value) {
        put(key, value, defaultTtl);
    }

    public void put(String key, String value, Duration ttl) {
        redis.opsForValue().set(key, value, ttl);
    }

    /** @return true when a key was actually removed. */
    public boolean remove(String key) {
        return Boolean.TRUE.equals(redis.delete(key));
    }
}
