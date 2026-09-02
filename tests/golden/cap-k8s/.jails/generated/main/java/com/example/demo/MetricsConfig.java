package com.example.demo;

import io.micrometer.core.instrument.MeterRegistry;
import org.springframework.boot.micrometer.metrics.autoconfigure.MeterRegistryCustomizer;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

/**
 * Tags every meter with the application it came from.
 *
 * <p>Without this, two services reporting to the same Prometheus publish the
 * same series names and their values are summed together -- graphs that are
 * quietly wrong rather than visibly missing, which is the worse failure.
 *
 * <p>A customizer rather than a property: {@code management.observations.
 * key-values.*} tags observations, and a {@link io.micrometer.core.instrument
 * .Counter} registered straight on the registry is not an observation, so
 * half the meters would go untagged. {@code config().commonTags(...)} covers
 * both, and Spring Boot guarantees customizers run before any meter is
 * registered.
 */
@Configuration(proxyBeanMethods = false)
class MetricsConfig {

    @Bean
    MeterRegistryCustomizer<MeterRegistry> commonTags(@Value("${spring.application.name:unnamed}") String application) {
        return registry -> registry.config().commonTags("application", application);
    }
}
