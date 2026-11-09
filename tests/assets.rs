//! 同梱 JSON がすべて解釈・検証を通ることを確かめる。

use llmsearch::core::config::filter::CompiledFilters;
use llmsearch::core::config::{FilterSet, NetworkProfile, QuotaPolicy, Settings, quota};

fn base() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

#[test]
fn engines_load_and_validate() {
    let loaded = llmsearch::adapters::engine::load_registry(&[base().join("engines")]).unwrap();
    assert!(loaded.registry.len() >= 3, "{:?}", loaded.warnings);
    for engine in loaded.registry.all() {
        assert!(
            !engine.has_fail(),
            "{}: {:?}",
            engine.selector(),
            engine.validate()
        );
    }
}

#[test]
fn network_profiles_are_valid() {
    for id in ["default", "self-hosted"] {
        let text =
            std::fs::read_to_string(base().join("network").join(format!("{id}.json"))).unwrap();
        let profile: NetworkProfile = serde_json::from_str(&text).unwrap();
        assert!(!profile.has_fail(), "{id}: {:?}", profile.validate());
        assert_eq!(profile.id, id);
    }
}

#[test]
fn filters_and_quotas_are_valid() {
    let text = std::fs::read_to_string(base().join("filters/default.json")).unwrap();
    let set: FilterSet = serde_json::from_str(&text).unwrap();
    assert!(CompiledFilters::compile(set).is_ok());

    let text = std::fs::read_to_string(base().join("quotas/default.json")).unwrap();
    let policy: QuotaPolicy = serde_json::from_str(&text).unwrap();
    assert!(!quota::validate(&policy).iter().any(|c| c.is_fail()));
    assert_eq!(policy.session.searches, Some(50));
    assert_eq!(policy.day.searches, Some(500));
}

#[test]
fn settings_template_is_valid() {
    let text = std::fs::read_to_string(base().join("config/settings.json")).unwrap();
    let settings: Settings = serde_json::from_str(&text).unwrap();
    assert!(!settings.has_fail(), "{:?}", settings.validate());
    assert_eq!(settings.search.limit, 7);
}
