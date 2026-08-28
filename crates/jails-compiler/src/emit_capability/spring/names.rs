//! Conventional Java names used by Spring capability packs.

use super::*;

pub(super) fn root_package(model: &AppModel) -> String {
    model.project.base_package.clone()
}

macro_rules! fixed_name {
    ($function:ident, $name:literal) => {
        pub(super) fn $function(_: &Capability) -> String {
            $name.to_string()
        }
    };
}

fixed_name!(h2_test_class, "H2DatabaseTest");
fixed_name!(actuator_test_class, "ActuatorEndpointsTest");
fixed_name!(cache_config_class, "CacheConfig");
fixed_name!(cache_test_class, "CacheConfigTest");
fixed_name!(cors_config_class, "CorsConfig");
fixed_name!(cors_test_class, "CorsConfigTest");
fixed_name!(metrics_config_class, "MetricsConfig");
fixed_name!(app_metrics_class, "AppMetrics");
fixed_name!(app_metrics_test_class, "AppMetricsTest");
fixed_name!(prometheus_scrape_test_class, "PrometheusScrapeTest");
fixed_name!(security_config_class, "SecurityConfig");
fixed_name!(production_security_config_class, "ProductionSecurityConfig");
fixed_name!(scope_authorizer_class, "ScopeAuthorizer");
fixed_name!(security_config_test_class, "SecurityConfigTest");
fixed_name!(scope_authorizer_test_class, "ScopeAuthorizerTest");
fixed_name!(event_name, "Event");
fixed_name!(event_hub_class, "EventHub");
fixed_name!(scheduling_config_class, "SchedulingConfig");
fixed_name!(event_stream_controller_class, "EventStreamController");
fixed_name!(event_hub_test_class, "EventHubTest");
fixed_name!(key_value_store_class, "KeyValueStore");
fixed_name!(key_value_store_it_class, "KeyValueStoreIT");
