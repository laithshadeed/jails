//! The security pack: who may call what, and the scope a claim proves.
//!
//! Every other pack in `spring.rs` declares files and dependencies; this one
//! is the reader's authorization surface -- the filter chain, the production
//! profile that refuses an unconfigured deployment, and the `ScopeAuthorizer`
//! a scoped operation is refused without -- and the Boot floor `requestMatchers`
//! puts on the whole project.

use super::*;

const SECURITY_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "config",
        template: crate::template!("spring/security_config_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("SecurityConfig"),
        template_class: Naming::Fixed("SecurityConfig"),
    },
    JavaFile {
        role: "production_config",
        template: crate::template!("spring/production_security_config_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("ProductionSecurityConfig"),
        template_class: Naming::Fixed("ProductionSecurityConfig"),
    },
    JavaFile {
        role: "scope_authorizer",
        template: crate::template!("spring/scope_authorizer_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("ScopeAuthorizer"),
        template_class: Naming::Fixed("ScopeAuthorizer"),
    },
    JavaFile {
        role: "config_test",
        template: crate::template!("spring/security_test_java.java"),
        before_boot: None,
        imports: &[Import::Moved(WEBMVC_TEST)],
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("SecurityConfigTest"),
        template_class: Naming::Fixed("SecurityConfigTest"),
    },
    JavaFile {
        role: "scope_authorizer_test",
        template: crate::template!("spring/scope_authorizer_test_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("ScopeAuthorizerTest"),
        template_class: Naming::Fixed("ScopeAuthorizerTest"),
    },
];

const SECURITY_DEPENDENCIES: &[DependencySpec] = &[
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-security",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-oauth2-resource-server",
        version: None,
        scope: DependencyScope::Compile,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.security",
        artifact: "spring-security-test",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    },
    DependencySpec {
        group: "org.springframework.boot",
        artifact: "spring-boot-starter-webmvc-test",
        version: None,
        scope: DependencyScope::Test,
        spring_managed_version: true,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::AtLeast(4),
    },
];

pub(in crate::emit_capability) const SECURITY_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: SECURITY_FILES,
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: SECURITY_DEPENDENCIES,
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: NO_BUILD_FEATURES,
    default_package: root_package,
    minimum_boot: Some((3, "requestMatchers")),
};
