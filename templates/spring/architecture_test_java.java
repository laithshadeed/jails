package {{pkg}};

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
@AnalyzeClasses(packages = "{{pkg}}")
final class ArchitectureTest {

    private static ArchRule reviewed(ArchRule rule) {
        return Files.exists(Path.of(".jails/architecture-baseline"))
                ? FreezingArchRule.freeze(rule)
                : rule;
    }

    @ArchTest
    static final ArchRule DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES = reviewed(noClasses()
            .that().resideInAPackage("{{domain}}..")
            .should().dependOnClassesThat().resideInAnyPackage(
                    "org.springframework..",
                    "jakarta.persistence..",
                    "{{app}}..",
                    "{{service}}..",
                    "{{web}}..",
                    "{{adapters}}.."));

    @ArchTest
    static final ArchRule APPLICATION_PORTS_DEPEND_INWARD = reviewed(noClasses()
            .that().resideInAPackage("{{app}}..")
            .should().dependOnClassesThat().resideInAnyPackage(
                    "org.springframework..",
                    "{{service}}..",
                    "{{web}}..",
                    "{{adapters}}.."));

    @ArchTest
    static final ArchRule ADAPTERS_DO_NOT_DEPEND_ON_WEB = reviewed(noClasses()
            .that().resideInAnyPackage("{{adapters}}..", "{{messaging}}..", "{{clients}}..")
            .should().dependOnClassesThat().resideInAPackage("{{web}}.."));

    @ArchTest
    static final ArchRule RAW_JDBC_STAYS_IN_ADAPTERS = reviewed(noClasses()
            .that().resideOutsideOfPackages("{{adapters}}..", "{{jobs}}..")
            .should().dependOnClassesThat().resideInAPackage("java.sql.."));

    @ArchTest
    static final ArchRule CONTROLLERS_DO_NOT_EXPOSE_PERSISTENCE = reviewed(noClasses()
            .that().haveSimpleNameEndingWith("Controller")
            .should().dependOnClassesThat().resideInAnyPackage("{{app}}..", "{{adapters}}.."));

    @ArchTest
    static final ArchRule TOP_LEVEL_SLICES_ARE_ACYCLIC = reviewed(slices()
            .matching("{{domain}}.(*)..")
            .should().beFreeOfCycles()
            .allowEmptyShould(true));
}
