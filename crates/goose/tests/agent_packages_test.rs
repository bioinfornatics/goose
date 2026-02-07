use goose::agent_packages::{AgentPackageDraft, AgentPackageStore, ModeDraft};
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn create_and_load_agent_package_and_mode() {
    let _guard = env_lock().lock().unwrap();

    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("GOOSE_PATH_ROOT", tmp.path());

    let store = AgentPackageStore::new();

    store
        .init_agent_package(
            "my_agent",
            AgentPackageDraft {
                display_name: "My Agent".to_string(),
                description: Some("desc".to_string()),
                default_mode: None,
            },
        )
        .unwrap();

    let created = store
        .create_mode(
            "my_agent",
            ModeDraft {
                display_name: "DBA".to_string(),
                description: Some("database".to_string()),
                instructions_md: "You are the DBA".to_string(),
                extensions_allow: vec!["developer".to_string()],
                extensions_deny: vec![],
            },
        )
        .unwrap();

    assert!(created.mode_manifest_path.is_file());
    assert!(created.instructions_path.is_file());

    let manifest = store.load_manifest("my_agent").unwrap();
    assert_eq!(manifest.agent_id, "my_agent");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.modes.len(), 1);
    assert_eq!(manifest.modes[0].mode_id, "dba");

    let listed = store.list().unwrap();
    assert_eq!(listed, vec!["my_agent".to_string()]);
}

#[test]
fn rejects_non_canonical_agent_id() {
    let _guard = env_lock().lock().unwrap();

    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("GOOSE_PATH_ROOT", tmp.path());

    let store = AgentPackageStore::new();
    let err = store
        .init_agent_package(
            "My Agent",
            AgentPackageDraft {
                display_name: "My Agent".to_string(),
                description: None,
                default_mode: None,
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains("Invalid agent_id"));
}
