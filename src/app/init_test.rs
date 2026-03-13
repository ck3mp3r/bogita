use crate::app::App;
use crate::domain::VaultBackend;
use crate::storage::config::AppConfig;
use serial_test::serial;
use tempfile::TempDir;

/// Redirect HOME and XDG dirs to `dir` for the duration of the test.
/// Returns the TempDir so it stays alive for the test body.
fn redirect_xdg(dir: &TempDir) {
    std::env::set_var("HOME", dir.path());
    std::env::set_var("XDG_DATA_HOME", dir.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
}

#[tokio::test]
#[serial]
async fn first_run_creates_config_identity_and_personal_vault() {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);

    let app = App::init().await.unwrap();

    assert!(
        AppConfig::default_path().exists(),
        "config.toml should exist"
    );
    assert!(
        app.config.identity_path.exists(),
        "identity.age should exist"
    );
    assert!(app.config.default_vault_id.is_some());

    let vaults = app.registry.list_vaults().await.unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "Personal");
    assert!(vaults[0].is_default);
}

#[tokio::test]
#[serial]
async fn second_init_loads_existing_identity_unchanged() {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);

    let app1 = App::init().await.unwrap();
    let recipient1 = app1.identity.to_recipient().to_string();

    let app2 = App::init().await.unwrap();
    let recipient2 = app2.identity.to_recipient().to_string();

    assert_eq!(
        recipient1, recipient2,
        "identity must not change on second init"
    );
}

#[tokio::test]
#[serial]
async fn personal_vault_is_default_and_sqlite_backed() {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);

    let app = App::init().await.unwrap();
    let vaults = app.registry.list_vaults().await.unwrap();
    let vault = &vaults[0];

    assert!(vault.is_default);
    assert!(
        matches!(&vault.backend, VaultBackend::Sqlite(_)),
        "expected Sqlite backend"
    );

    if let VaultBackend::Sqlite(cfg) = &vault.backend {
        assert!(
            cfg.path.contains(&vault.id.to_string()),
            "db path should contain vault id, got: {}",
            cfg.path
        );
    }
}
