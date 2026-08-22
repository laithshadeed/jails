package com.example.demo;

import org.springframework.context.annotation.Configuration;
import org.springframework.scheduling.annotation.EnableScheduling;

/**
 * Turns on {@code @Scheduled}.
 *
 * <p>Without this, every {@code @Scheduled} method in the application is
 * inert and nothing says so -- the same silent-no-op failure mode as
 * {@code @EnableCaching}.
 */
@Configuration(proxyBeanMethods = false)
@EnableScheduling
public class SchedulingConfig {}
