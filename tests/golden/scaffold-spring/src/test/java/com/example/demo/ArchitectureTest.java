package com.example.demo;

import static com.tngtech.archunit.lang.syntax.ArchRuleDefinition.noClasses;
import static com.tngtech.archunit.library.dependencies.SlicesRuleDefinition.slices;

import com.tngtech.archunit.junit.AnalyzeClasses;
import com.tngtech.archunit.junit.ArchTest;
import com.tngtech.archunit.lang.ArchRule;
import com.tngtech.archunit.library.freeze.FreezingArchRule;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Project-level ports-and-adapters fitness rules.
 *
 * <p>New projects run every rule strictly. An adopted project may commit the
 * explicit {@code .jails/architecture-baseline} store; only then are recorded
 * pre-existing violations frozen while new violations continue to fail.
 */
@AnalyzeClasses(packages = "com.example.demo")
final class ArchitectureTest {

    private static ArchRule reviewed(ArchRule rule) {
        return Files.exists(Path.of(".jails/architecture-baseline"))
                ? FreezingArchRule.freeze(rule)
                : rule;
    }

    @ArchTest
    static final ArchRule DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES = reviewed(noClasses()
            .that().resideInAPackage("com.example.demo.domain..")
            .should().dependOnClassesThat().resideInAnyPackage(
                    "org.springframework..",
                    "jakarta.persistence..",
                    "com.example.demo.app..",
                    "com.example.demo.service..",
                    "com.example.demo.web..",
                    "com.example.demo.adapters.."));

    @ArchTest
    static final ArchRule APPLICATION_PORTS_DEPEND_INWARD = reviewed(noClasses()
            .that().resideInAPackage("com.example.demo.app..")
            .should().dependOnClassesThat().resideInAnyPackage(
                    "org.springframework..",
                    "com.example.demo.service..",
                    "com.example.demo.web..",
                    "com.example.demo.adapters.."));

    @ArchTest
    static final ArchRule ADAPTERS_DO_NOT_DEPEND_ON_WEB = reviewed(noClasses()
            .that().resideInAnyPackage("com.example.demo.adapters..", "com.example.demo.messaging..", "com.example.demo.clients..")
            .should().dependOnClassesThat().resideInAPackage("com.example.demo.web.."));

    @ArchTest
    static final ArchRule RAW_JDBC_STAYS_IN_ADAPTERS = reviewed(noClasses()
            .that().resideOutsideOfPackages("com.example.demo.adapters..", "com.example.demo.jobs..")
            .should().dependOnClassesThat().resideInAPackage("java.sql.."));

    @ArchTest
    static final ArchRule CONTROLLERS_DO_NOT_EXPOSE_PERSISTENCE = reviewed(noClasses()
            .that().haveSimpleNameEndingWith("Controller")
            .should().dependOnClassesThat().resideInAnyPackage("com.example.demo.app..", "com.example.demo.adapters.."));

    @ArchTest
    static final ArchRule TOP_LEVEL_SLICES_ARE_ACYCLIC = reviewed(slices()
            .matching("com.example.demo.domain.(*)..")
            .should().beFreeOfCycles()
            .allowEmptyShould(true));
}
