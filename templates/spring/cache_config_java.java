package {{pkg}};

import org.springframework.cache.annotation.EnableCaching;
import org.springframework.context.annotation.Configuration;

/**
 * Turns on {@code @Cacheable} and friends.
 *
 * <p>Spring Boot auto-configures a {@code CacheManager} from
 * {@code spring.cache.*}, but caching itself stays off until something
 * enables it -- which is why a freshly added {@code @Cacheable} so often
 * appears to do nothing at all.
 *
 * <p>The bound in {@code spring.cache.caffeine.spec} is not decoration: an
 * unbounded cache is a memory leak that reports itself as a performance
 * feature.
 */
@Configuration(proxyBeanMethods = false)
@EnableCaching
public class CacheConfig {}
