package {{pkg}};

import org.springframework.context.annotation.Configuration;
import org.springframework.web.service.registry.ImportHttpServices;

/**
 * Registers this package's {@code @HttpExchange} interfaces as beans.
 *
 * <p>Scanned by package rather than listed by type, so a new client interface
 * dropped in here is wired up with no edit to this file.
 *
 * <p>The group name is what links the clients to their configuration:
 * {@code spring.http.serviceclient.{{group}}.base-url} sets where they point,
 * and the same prefix carries timeouts, default headers and SSL bundles.
 */
@Configuration(proxyBeanMethods = false)
@ImportHttpServices(group = "{{group}}", basePackages = "{{pkg}}")
public class HttpClientsConfig {}
