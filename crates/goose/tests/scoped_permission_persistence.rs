use goose::config::permission::{PermissionLevel, PermissionManager};
use goose::config::scoped_permissions::{
    PermissionEffect, PermissionPrincipal, PermissionRequest, PermissionScope,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn function() -> PermissionPrincipal {
    PermissionPrincipal::Function {
        extension: "developer".into(),
        function: "shell".into(),
    }
}

fn request() -> PermissionRequest {
    PermissionRequest {
        extension: "developer".into(),
        function: "shell".into(),
        capabilities: BTreeSet::from(["execute".into()]),
    }
}

fn update(
    manager: &PermissionManager,
    scope: PermissionScope,
    project: Option<&Path>,
    session: Option<&str>,
    effect: PermissionEffect,
) {
    manager
        .update_scoped_permission(scope, project, session, function(), effect)
        .unwrap();
}

#[test]
fn user_and_project_rules_persist_and_reset_independently() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("user-config");
    let project = root.path().join("project");
    let manager = PermissionManager::new(config.clone());

    update(
        &manager,
        PermissionScope::User,
        None,
        None,
        PermissionEffect::Allow,
    );
    update(
        &manager,
        PermissionScope::ProjectLocal,
        Some(&project),
        None,
        PermissionEffect::Deny,
    );
    update(
        &manager,
        PermissionScope::ProjectShared,
        Some(&project),
        None,
        PermissionEffect::Ask,
    );

    let reloaded = PermissionManager::new(config);
    let rules = reloaded.scoped_rules(Some(&project), None).unwrap();
    assert_eq!(rules.len(), 3);
    assert_eq!(
        reloaded
            .resolve_scoped_permission(Some(&project), None, &request())
            .effect,
        PermissionEffect::Deny
    );

    reloaded
        .reset_scoped_permissions(PermissionScope::ProjectLocal, Some(&project), None)
        .unwrap();
    assert_eq!(
        reloaded
            .resolve_scoped_permission(Some(&project), None, &request())
            .effect,
        PermissionEffect::Ask
    );

    reloaded
        .reset_scoped_permissions(PermissionScope::ProjectShared, Some(&project), None)
        .unwrap();
    assert_eq!(
        reloaded
            .resolve_scoped_permission(Some(&project), None, &request())
            .effect,
        PermissionEffect::Allow
    );

    reloaded
        .reset_scoped_permissions(PermissionScope::User, None, None)
        .unwrap();
    assert_eq!(
        PermissionManager::new(root.path().join("user-config"))
            .resolve_scoped_permission(None, None, &request())
            .effect,
        PermissionEffect::Ask
    );
}

#[test]
fn sessions_are_isolated_in_memory_and_removable() {
    let root = tempfile::tempdir().unwrap();
    let manager = PermissionManager::new(root.path().to_path_buf());
    update(
        &manager,
        PermissionScope::Session,
        None,
        Some("first"),
        PermissionEffect::Deny,
    );
    update(
        &manager,
        PermissionScope::Session,
        None,
        Some("second"),
        PermissionEffect::Allow,
    );

    assert_eq!(
        manager
            .resolve_scoped_permission(None, Some("first"), &request())
            .effect,
        PermissionEffect::Deny
    );
    assert_eq!(
        manager
            .resolve_scoped_permission(None, Some("second"), &request())
            .effect,
        PermissionEffect::Allow
    );
    manager.remove_session_permissions("first");
    assert_eq!(
        manager
            .resolve_scoped_permission(None, Some("first"), &request())
            .effect,
        PermissionEffect::Ask
    );
    assert_eq!(
        PermissionManager::new(root.path().to_path_buf())
            .resolve_scoped_permission(None, Some("second"), &request())
            .effect,
        PermissionEffect::Ask
    );
}

#[test]
fn existing_permission_yaml_is_a_user_scope_source() {
    let config = tempfile::tempdir().unwrap();
    fs::write(
        config.path().join("permission.yaml"),
        "user:\n  always_allow: [developer__shell]\n  ask_before: []\n  never_allow: []\n",
    )
    .unwrap();
    let manager = PermissionManager::new(config.path().to_path_buf());

    assert_eq!(
        manager
            .resolve_scoped_permission(None, None, &request())
            .effect,
        PermissionEffect::Allow
    );
    assert_eq!(
        manager.get_user_permission("developer__shell"),
        Some(PermissionLevel::AlwaysAllow)
    );
}

#[test]
fn restrictive_effect_wins_across_persisted_and_session_scopes() {
    let root = tempfile::tempdir().unwrap();
    let manager = PermissionManager::new(root.path().join("config"));
    update(
        &manager,
        PermissionScope::User,
        None,
        None,
        PermissionEffect::Deny,
    );
    update(
        &manager,
        PermissionScope::Session,
        None,
        Some("session"),
        PermissionEffect::Allow,
    );

    let resolution = manager.resolve_scoped_permission(None, Some("session"), &request());
    assert_eq!(resolution.effect, PermissionEffect::Deny);
    assert_eq!(
        resolution.matched_rule.unwrap().origin.scope,
        PermissionScope::User
    );
}

#[test]
fn corrupt_project_file_fails_closed_to_ask() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let project = root.path().join("project");
    let manager = PermissionManager::new(config);
    update(
        &manager,
        PermissionScope::User,
        None,
        None,
        PermissionEffect::Allow,
    );
    let path = PermissionManager::project_permission_path(&project, PermissionScope::ProjectLocal)
        .unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "not: [valid").unwrap();

    assert_eq!(
        manager
            .resolve_scoped_permission(Some(&project), None, &request())
            .effect,
        PermissionEffect::Ask
    );
}

#[test]
fn shared_project_rules_cannot_expand_permissions_without_trust() {
    let root = tempfile::tempdir().unwrap();
    let manager = PermissionManager::new(root.path().join("config"));
    let error = manager
        .update_scoped_permission(
            PermissionScope::ProjectShared,
            Some(root.path()),
            None,
            function(),
            PermissionEffect::Allow,
        )
        .unwrap_err();
    assert!(error.to_string().contains("workspace trust"));
}

#[cfg(unix)]
#[test]
fn project_updates_preserve_symlink_and_replace_target_atomically() {
    use std::io::Read;
    use std::os::unix::fs::{symlink, MetadataExt};

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let settings = project.join(".config/goose");
    let target_dir = root.path().join("policy");
    fs::create_dir_all(&settings).unwrap();
    fs::create_dir_all(&target_dir).unwrap();
    let link = settings.join("permission.local.yaml");
    let target = target_dir.join("permission.yaml");
    fs::write(&target, "[]\n").unwrap();
    symlink("../../../policy/permission.yaml", &link).unwrap();
    let mut old_file = fs::File::open(&target).unwrap();
    let old_inode = old_file.metadata().unwrap().ino();

    let manager = PermissionManager::new(root.path().join("config"));
    update(
        &manager,
        PermissionScope::ProjectLocal,
        Some(&project),
        None,
        PermissionEffect::Deny,
    );

    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_ne!(fs::metadata(&target).unwrap().ino(), old_inode);
    let mut old_contents = String::new();
    old_file.read_to_string(&mut old_contents).unwrap();
    assert_eq!(old_contents, "[]\n");
    assert_eq!(
        manager
            .resolve_scoped_permission(Some(&project), None, &request())
            .effect,
        PermissionEffect::Deny
    );
}

#[test]
fn concurrent_project_writers_merge_under_the_storage_lock() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let project = root.path().join("project");
    let barrier = Arc::new(Barrier::new(3));
    let mut threads = Vec::new();

    for function_name in ["shell", "text_editor"] {
        let manager = PermissionManager::new(config.clone());
        let project = project.clone();
        let barrier = Arc::clone(&barrier);
        threads.push(thread::spawn(move || {
            barrier.wait();
            manager
                .update_scoped_permission(
                    PermissionScope::ProjectLocal,
                    Some(&project),
                    None,
                    PermissionPrincipal::Function {
                        extension: "developer".into(),
                        function: function_name.into(),
                    },
                    PermissionEffect::Deny,
                )
                .unwrap();
        }));
    }
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }

    let manager = PermissionManager::new(config);
    let rules = manager.scoped_rules(Some(&project), None).unwrap();
    assert_eq!(rules.len(), 2);
}
