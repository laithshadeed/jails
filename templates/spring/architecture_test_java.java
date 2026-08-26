package {{pkg}};

import static com.tngtech.archunit.lang.syntax.ArchRuleDefinition.noClasses;
import static com.tngtech.archunit.lang.syntax.ArchRuleDefinition.classes;
import static com.tngtech.archunit.core.domain.JavaClass.Predicates.resideInAPackage;
import static com.tngtech.archunit.core.domain.JavaClass.Predicates.resideInAnyPackage;
import static com.tngtech.archunit.library.dependencies.SlicesRuleDefinition.slices;

import com.tngtech.archunit.core.domain.Dependency;
import com.tngtech.archunit.core.domain.JavaClass;
import com.tngtech.archunit.junit.AnalyzeClasses;
import com.tngtech.archunit.junit.ArchTest;
import com.tngtech.archunit.lang.ArchCondition;
import com.tngtech.archunit.lang.ArchRule;
import com.tngtech.archunit.lang.ConditionEvents;
import com.tngtech.archunit.lang.SimpleConditionEvent;
import com.tngtech.archunit.library.dependencies.SliceRule;
import com.tngtech.archunit.library.freeze.FreezingArchRule;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDate;
import java.time.ZoneOffset;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Project-level ports-and-adapters fitness rules.
 *
 * <p>New projects run every rule strictly. An adopted project may commit the
 * explicit {@code .jails/architecture-baseline} store; only then are recorded
 * pre-existing violations frozen while new violations continue to fail.
 */
@AnalyzeClasses(packages = "{{pkg}}")
final class ArchitectureTest {
    private static final String SLICE_ROOT = "{{domain}}";
    private static final List<Allowance> ALLOWANCES = loadAllowances();

    private static ArchRule reviewed(ArchRule rule) {
        return Files.exists(Path.of(".jails/architecture-baseline"))
                ? FreezingArchRule.freeze(rule)
                : rule;
    }

    private static ArchRule sliceCycles() {
        SliceRule rule = slices().matching("{{domain}}.(*)..").should().beFreeOfCycles();
        for (Allowance allowance : ALLOWANCES) {
            rule = rule.ignoreDependency(
                    resideInAPackage(SLICE_ROOT + "." + allowance.from() + ".."),
                    resideInAnyPackage(allowance.packages().toArray(String[]::new)));
        }
        return rule.allowEmptyShould(true);
    }

    private static ArchRule allowancesAreUsed() {
        return classes().should(new ArchCondition<JavaClass>("use every architecture allowance") {
            private boolean[] used;

            @Override
            public void init(Collection<JavaClass> classes) {
                used = new boolean[ALLOWANCES.size()];
            }

            @Override
            public void check(JavaClass item, ConditionEvents events) {
                for (Dependency dependency : item.getDirectDependenciesFromSelf()) {
                    for (int index = 0; index < ALLOWANCES.size(); index++) {
                        if (ALLOWANCES.get(index).permits(dependency)) {
                            used[index] = true;
                        }
                    }
                }
            }

            @Override
            public void finish(ConditionEvents events) {
                for (int index = 0; index < ALLOWANCES.size(); index++) {
                    if (!used[index]) {
                        Allowance allowance = ALLOWANCES.get(index);
                        events.add(SimpleConditionEvent.violated(
                                allowance,
                                "unused architecture allowance " + allowance.identity()
                                        + "; delete it or narrow it to a real dependency"));
                    }
                }
            }
        }).allowEmptyShould(true);
    }

    private static List<Allowance> loadAllowances() {
        Path policy = Path.of(".jails/architecture.toml");
        if (!Files.exists(policy)) {
            return List.of();
        }
        try {
            List<Allowance> allowances = new ArrayList<>();
            Map<String, String> fields = null;
            int lineNumber = 0;
            for (String raw : Files.readAllLines(policy, StandardCharsets.UTF_8)) {
                lineNumber++;
                String line = raw.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                if (line.equals("[[architecture.allow]]")) {
                    addAllowance(allowances, fields);
                    fields = new LinkedHashMap<>();
                    continue;
                }
                if (fields == null) {
                    throw invalid(lineNumber, "expected [[architecture.allow]]");
                }
                int equals = line.indexOf('=');
                if (equals < 1) {
                    throw invalid(lineNumber, "expected key = value");
                }
                String key = line.substring(0, equals).trim();
                String previous = fields.putIfAbsent(key, line.substring(equals + 1).trim());
                if (previous != null) {
                    throw invalid(lineNumber, "duplicate key " + key);
                }
            }
            addAllowance(allowances, fields);
            Set<String> identities = new HashSet<>();
            for (Allowance allowance : allowances) {
                if (!identities.add(allowance.identity())) {
                    throw invalid(0, "duplicate allowance " + allowance.identity());
                }
            }
            return List.copyOf(allowances);
        } catch (IOException error) {
            throw new IllegalArgumentException("cannot read .jails/architecture.toml", error);
        }
    }

    private static void addAllowance(
            List<Allowance> allowances, Map<String, String> fields) {
        if (fields == null) {
            return;
        }
        Set<String> known = Set.of("from", "to", "packages", "reason", "expires");
        for (String key : fields.keySet()) {
            if (!known.contains(key)) {
                throw invalid(0, "unknown allowance key " + key);
            }
        }
        String from = stringField(fields, "from");
        String to = stringField(fields, "to");
        String reason = stringField(fields, "reason");
        String expires = stringField(fields, "expires");
        List<String> packages = listField(fields, "packages");
        if (!identifier(from) || !identifier(to) || from.equals(to)) {
            throw invalid(0, "from/to must be distinct slice names");
        }
        if (reason.isBlank()) {
            throw invalid(0, "reason must not be blank");
        }
        LocalDate expiry;
        try {
            expiry = LocalDate.parse(expires);
        } catch (RuntimeException error) {
            throw invalid(0, "expires must be an ISO-8601 date");
        }
        if (expiry.isBefore(LocalDate.now(ZoneOffset.UTC))) {
            throw invalid(0, "allowance expired on " + expiry);
        }
        if (packages.isEmpty()) {
            throw invalid(0, "packages must name at least one bounded package");
        }
        String targetPrefix = SLICE_ROOT + "." + to + ".";
        for (String pattern : packages) {
            if (!bounded(pattern, targetPrefix)) {
                throw invalid(0, "blanket or out-of-slice package pattern " + pattern);
            }
        }
        allowances.add(new Allowance(from, to, List.copyOf(packages), reason, expiry));
    }

    private static String stringField(Map<String, String> fields, String key) {
        String value = fields.get(key);
        if (value == null || value.length() < 2 || value.charAt(0) != '"'
                || value.charAt(value.length() - 1) != '"') {
            throw invalid(0, key + " must be a quoted string");
        }
        return value.substring(1, value.length() - 1)
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
    }

    private static List<String> listField(Map<String, String> fields, String key) {
        String value = fields.get(key);
        if (value == null || !value.startsWith("[") || !value.endsWith("]")) {
            throw invalid(0, key + " must be an array of quoted strings");
        }
        String body = value.substring(1, value.length() - 1).trim();
        if (body.isEmpty()) {
            return List.of();
        }
        List<String> result = new ArrayList<>();
        for (String part : body.split(",")) {
            Map<String, String> one = Map.of(key, part.trim());
            result.add(stringField(one, key));
        }
        return result;
    }

    private static boolean identifier(String value) {
        if (value.isEmpty() || !Character.isJavaIdentifierStart(value.charAt(0))) {
            return false;
        }
        return value.chars().skip(1).allMatch(Character::isJavaIdentifierPart);
    }

    private static boolean bounded(String pattern, String targetPrefix) {
        if (!pattern.startsWith(targetPrefix) || !pattern.endsWith("..")) {
            return false;
        }
        String exact = pattern.substring(0, pattern.length() - 2);
        String suffix = exact.substring(targetPrefix.length());
        if (suffix.isEmpty() || suffix.contains("*") || suffix.contains("(")) {
            return false;
        }
        return List.of(suffix.split("\\.")).stream().allMatch(ArchitectureTest::identifier);
    }

    private static IllegalArgumentException invalid(int line, String detail) {
        String at = line == 0 ? "" : " at line " + line;
        return new IllegalArgumentException("invalid .jails/architecture.toml" + at + ": " + detail);
    }

    private record Allowance(
            String from, String to, List<String> packages, String reason, LocalDate expires) {
        boolean permits(Dependency dependency) {
            return resideInAPackage(SLICE_ROOT + "." + from + "..")
                            .test(dependency.getOriginClass())
                    && resideInAnyPackage(packages.toArray(String[]::new))
                            .test(dependency.getTargetClass());
        }

        String identity() {
            return from + " -> " + to + " " + packages + " (" + reason + ", expires " + expires + ")";
        }
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
    static final ArchRule TOP_LEVEL_SLICES_ARE_ACYCLIC = reviewed(sliceCycles());

    @ArchTest
    static final ArchRule ARCHITECTURE_ALLOWANCES_ARE_USED = allowancesAreUsed();
}
