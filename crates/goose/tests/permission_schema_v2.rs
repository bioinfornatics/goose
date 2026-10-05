use goose::config::permission::{PermissionLevel, PermissionManager};
use goose::config::scoped_permissions::{PermissionEffect, PermissionPrincipal, PermissionScope};
use std::fs;

#[test]
fn legacy_load_is_non_mutating_and_explicit_mutation_writes_v2() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("permission.yaml");
    let legacy = "user:
  always_allow: [orphan__missing]
  ask_before: []
  never_allow: []
";
    fs::write(&path, legacy).unwrap();
    let manager = PermissionManager::new(dir.path().into());
    assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
    assert_eq!(
        manager.get_user_permission("orphan__missing"),
        Some(PermissionLevel::AlwaysAllow)
    );

    manager.update_user_permission("other__tool", PermissionLevel::AskBefore);
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.starts_with(
        "version: 2
user:"
    ));
    assert!(written.contains("orphan:"));
    assert!(written.contains("- missing"));
    let reloaded = PermissionManager::new(dir.path().into());
    assert_eq!(
        reloaded.get_user_permission("orphan__missing"),
        Some(PermissionLevel::AlwaysAllow)
    );
}

#[test]
fn v2_grouped_orphans_load_and_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("permission.yaml"),
        "version: 2
user:
  always_allow:
    vanished: [unknown]
  ask_before: {}
  never_allow: {}
",
    )
    .unwrap();
    let manager = PermissionManager::new(dir.path().into());
    assert_eq!(
        manager.get_user_permission("vanished__unknown"),
        Some(PermissionLevel::AlwaysAllow)
    );
    manager.update_user_permission("x__y", PermissionLevel::NeverAllow);
    let manager = PermissionManager::new(dir.path().into());
    assert_eq!(
        manager.get_user_permission("vanished__unknown"),
        Some(PermissionLevel::AlwaysAllow)
    );
    assert_eq!(
        manager.get_user_permission("x__y"),
        Some(PermissionLevel::NeverAllow)
    );
}

#[test]
fn v2_conflicting_effects_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("permission.yaml"),
        "version: 2
user:
  always_allow:
    ext: [tool]
  ask_before:
    ext: [tool]
  never_allow: {}
",
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| PermissionManager::new(dir.path().into())).is_err());
}

#[test]
fn project_v2_loads_and_shared_allow_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join(".config/goose");
    fs::create_dir_all(&config).unwrap();
    fs::write(
        config.join("permission.local.yaml"),
        "version: 2
permissions:
  always_allow:
    orphan: [call]
  ask_before: {}
  never_allow: {}
",
    )
    .unwrap();
    let user = tempfile::tempdir().unwrap();
    let manager = PermissionManager::new(user.path().into());
    let rules = manager.scoped_rules(Some(dir.path()), None).unwrap();
    assert!(rules
        .iter()
        .any(|rule| rule.effect == PermissionEffect::Allow
            && rule.principal
                == PermissionPrincipal::Function {
                    extension: "orphan".into(),
                    function: "call".into()
                }));

    fs::write(
        config.join("permission.yaml"),
        "version: 2
permissions:
  always_allow:
    orphan: [call]
  ask_before: {}
  never_allow: {}
",
    )
    .unwrap();
    assert!(manager.scoped_rules(Some(dir.path()), None).is_err());

    assert!(manager
        .update_scoped_permission(
            PermissionScope::ProjectShared,
            Some(dir.path()),
            None,
            PermissionPrincipal::Extension {
                extension: "x".into()
            },
            PermissionEffect::Allow
        )
        .is_err());
}

#[cfg(unix)]
#[test]
fn explicit_mutation_atomically_replaces_v2_file() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().unwrap();
    let manager = PermissionManager::new(dir.path().into());
    manager.update_user_permission("one__call", PermissionLevel::AlwaysAllow);
    let path = manager.get_config_path();
    let old = fs::File::open(path).unwrap();
    let inode = old.metadata().unwrap().ino();

    manager.update_user_permission("two__call", PermissionLevel::AskBefore);

    assert_ne!(fs::metadata(path).unwrap().ino(), inode);
    let old_contents = std::io::read_to_string(old).unwrap();
    assert!(old_contents.contains("one:"));
    assert!(!old_contents.contains("two:"));
}
