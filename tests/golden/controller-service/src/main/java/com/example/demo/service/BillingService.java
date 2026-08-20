package com.example.demo.service;

import org.springframework.stereotype.Component;

/**
 * Package-private: Spring injects this by type, and nothing outside this
 * package should be compiling against it. Widen it when something genuinely
 * outside needs it, not before.
 */
@Component
class BillingService {
}
