package net.acme.billing;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

/**
 * At the package root, which is the Spring convention -- component scanning
 * starts here, so nothing needs `scanBasePackages`.
 *
 * <p>It is also what makes the base package unambiguous to a tool reading this
 * project: `base_package()` falls back to the shallowest `.java`, and a
 * project whose shallowest files are all one level down leaves that a guess
 * between siblings.
 */
@SpringBootApplication
public class BillingApplication {

    public static void main(String[] args) {
        SpringApplication.run(BillingApplication.class, args);
    }
}
