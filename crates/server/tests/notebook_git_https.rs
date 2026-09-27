use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use aster_core::{Cell, Notebook, NotebookStore};
use aster_server::{GitNotebookStore, NotebookWorkspaceBinding};

#[tokio::test]
#[ignore = "requires tests/notebook-git-https.py disposable HTTPS Git fixture"]
async fn notebook_git_https_z_clone_pins_moved_default_without_persisting_token() {
    let checkout = PathBuf::from(std::env::var("ASTER_TEST_HTTPS_CHECKOUT").unwrap());
    let bare = PathBuf::from(std::env::var("ASTER_TEST_HTTPS_BARE").unwrap());
    let url = std::env::var("ASTER_TEST_HTTPS_URL").unwrap();
    let ca = PathBuf::from(std::env::var("ASTER_TEST_HTTPS_CA").unwrap());
    let token = std::env::var("ASTER_TEST_HTTPS_TOKEN").unwrap();
    let pinned = std::env::var("ASTER_TEST_HTTPS_SEED_OID").unwrap();
    assert_ne!(
        remote_head(&bare),
        pinned,
        "default move control did not run"
    );
    let clone_dir = checkout.parent().unwrap().join("cloned-session");
    let binding = NotebookWorkspaceBinding {
        team: "alpha".into(),
        branch: "sessions/alice/opaque".into(),
        installation_id: 17,
        repository_id: 19,
        default_branch: "main".into(),
        default_commit: pinned.clone(),
        target_version: 1,
        remote_url: url.clone(),
    };
    GitNotebookStore::clone_https_no_checkout(&clone_dir, &binding, &token, Some(&ca))
        .await
        .unwrap();
    let stored: NotebookWorkspaceBinding = serde_json::from_slice(
        &std::fs::read(clone_dir.join(".git/aster.workspace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stored, binding);
    let mismatch = NotebookWorkspaceBinding {
        repository_id: 20,
        ..binding.clone()
    };
    assert!(GitNotebookStore::open_bound(&clone_dir, &binding.branch, &mismatch).is_err());
    let reopened = GitNotebookStore::open_bound(&clone_dir, &binding.branch, &binding).unwrap();
    drop(reopened);
    let commit = Command::new("git")
        .args(["cat-file", "-e", &format!("{pinned}^{{commit}}")])
        .current_dir(&clone_dir)
        .status()
        .unwrap();
    assert!(commit.success());
    assert!(clone_dir.join("a.aster").exists());
    let local_head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&clone_dir)
        .output()
        .unwrap();
    assert!(local_head.status.success());
    assert_eq!(String::from_utf8_lossy(&local_head.stdout).trim(), pinned);
    assert!(!std::fs::read_to_string(clone_dir.join(".git/config"))
        .unwrap()
        .contains(&token));

    let invalid_dir = checkout.parent().unwrap().join("invalid-branch");
    let invalid = NotebookWorkspaceBinding {
        default_branch: "--upload-pack=unsafe".into(),
        ..binding.clone()
    };
    assert!(
        GitNotebookStore::clone_https_no_checkout(&invalid_dir, &invalid, &token, Some(&ca),)
            .await
            .is_err()
    );
    assert!(!invalid_dir.exists());

    let link = checkout.parent().unwrap().join("linked-checkout");
    std::os::unix::fs::symlink(&checkout, &link).unwrap();
    assert!(
        GitNotebookStore::clone_https_no_checkout(&link, &binding, &token, Some(&ca),)
            .await
            .is_err()
    );

    let absent_dir = checkout.parent().unwrap().join("missing-commit");
    let missing = NotebookWorkspaceBinding {
        default_commit: "0".repeat(40),
        ..binding.clone()
    };
    assert!(
        GitNotebookStore::clone_https_no_checkout(&absent_dir, &missing, &token, Some(&ca),)
            .await
            .is_err()
    );
    assert!(
        !absent_dir.exists(),
        "failed HTTPS clone exposed an unbound checkout"
    );

    let partial_dir = checkout.parent().unwrap().join("unbound-partial");
    let partial = Command::new("git")
        .args([
            "clone",
            "--no-checkout",
            "--branch",
            "main",
            bare.to_str().unwrap(),
            partial_dir.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(partial.success());
    assert!(GitNotebookStore::open_bound(&partial_dir, &binding.branch, &binding).is_err());
    assert!(!partial_dir.join("a.aster").exists());
}

#[tokio::test]
#[ignore = "requires tests/notebook-git-https.py disposable HTTPS Git fixture"]
async fn notebook_git_https_sync_pushes_exact_branch_without_persisting_token() {
    let checkout = PathBuf::from(std::env::var("ASTER_TEST_HTTPS_CHECKOUT").unwrap());
    let bare = PathBuf::from(std::env::var("ASTER_TEST_HTTPS_BARE").unwrap());
    let url = std::env::var("ASTER_TEST_HTTPS_URL").unwrap();
    let ca = PathBuf::from(std::env::var("ASTER_TEST_HTTPS_CA").unwrap());
    let token = std::env::var("ASTER_TEST_HTTPS_TOKEN").unwrap();
    let store = GitNotebookStore::open(&checkout, "main").unwrap();
    let local = store
        .save(
            &Notebook {
                id: "sales".into(),
                title: "Sales".into(),
                cells: vec![Cell {
                    id: "c1".into(),
                    sql: "SELECT 1".into(),
                    engine: None,
                }],
            },
            "alice",
        )
        .await
        .unwrap();
    let synced = store.sync_https(&url, &token, Some(&ca)).await.unwrap();
    assert_eq!(synced, local);
    let remote = Command::new("git")
        .args(["rev-parse", "refs/heads/main"])
        .current_dir(&bare)
        .output()
        .unwrap();
    assert!(remote.status.success());
    assert_eq!(String::from_utf8_lossy(&remote.stdout).trim(), local);
    let config = std::fs::read_to_string(checkout.join(".git/config")).unwrap();
    assert!(!config.contains(&token));
    assert!(!url.contains(&token));

    store
        .save(
            &Notebook {
                id: "sales".into(),
                title: "Sales".into(),
                cells: vec![Cell {
                    id: "c1".into(),
                    sql: "SELECT 2".into(),
                    engine: None,
                }],
            },
            "alice",
        )
        .await
        .unwrap();
    let config = |key: &str, value: &str| {
        let status = Command::new("git")
            .args(["config", "--local", key, value])
            .current_dir(&checkout)
            .status()
            .unwrap();
        assert!(status.success());
    };
    config("extensions.worktreeConfig", "true");
    let status = Command::new("git")
        .args([
            "config",
            "--worktree",
            "http.extraHeader",
            "Authorization: fixture-other",
        ])
        .current_dir(&checkout)
        .status()
        .unwrap();
    assert!(status.success());
    let error = store.sync_https(&url, &token, Some(&ca)).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsafe notebook Git configuration"),
        "credentialed Git must refuse hidden worktree HTTP headers: {error}"
    );
    let status = Command::new("git")
        .args(["config", "--local", "--unset", "extensions.worktreeConfig"])
        .current_dir(&checkout)
        .status()
        .unwrap();
    assert!(status.success());
    let marker = checkout.join(".git/gpg-invoked");
    let signer = checkout.join(".git/gpg-fixture.sh");
    std::fs::write(
        &signer,
        format!("#!/bin/sh\ntouch {}\nexit 1\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&signer, std::fs::Permissions::from_mode(0o700)).unwrap();
    config("push.gpgSign", "true");
    config("gpg.program", signer.to_str().unwrap());
    let error = store.sync_https(&url, &token, Some(&ca)).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsafe notebook Git configuration"),
        "credentialed Git must refuse executable push-signing configuration: {error}"
    );
    assert!(
        !marker.exists(),
        "Git executed local GPG program while token was present"
    );

    for key in ["push.gpgSign", "gpg.program"] {
        assert!(Command::new("git")
            .args(["config", "--local", "--unset", key])
            .current_dir(&checkout)
            .status()
            .unwrap()
            .success());
    }
    let pending = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&checkout)
        .output()
        .unwrap();
    assert!(pending.status.success());
    let pending = String::from_utf8_lossy(&pending.stdout).trim().to_owned();
    let unavailable = url.replace("/repo.git", "/missing.git");
    assert!(store
        .sync_https(&unavailable, &token, Some(&ca))
        .await
        .is_err());
    assert_eq!(remote_head(&bare), local);
    assert_eq!(local_head(&checkout), pending);

    let deny_push = PathBuf::from(std::env::var("ASTER_TEST_HTTPS_DENY_PUSH_FILE").unwrap());
    std::fs::write(&deny_push, b"deny").unwrap();
    assert!(store.sync_https(&url, &token, Some(&ca)).await.is_err());
    assert_eq!(
        remote_head(&bare),
        local,
        "denied push changed remote branch"
    );
    assert_eq!(
        local_head(&checkout),
        pending,
        "denied push lost local commit"
    );
    std::fs::remove_file(&deny_push).unwrap();
    let retry = store.sync_https(&url, &token, Some(&ca)).await.unwrap_err();
    assert!(retry.to_string().contains("Sync intent needs recovery"));
    assert_eq!(remote_head(&bare), local);
    assert_eq!(local_head(&checkout), pending);
}

fn local_head(checkout: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(checkout)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn remote_head(bare: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "refs/heads/main"])
        .current_dir(bare)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
