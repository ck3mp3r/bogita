type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::views::entry_form::{EntryForm, FormAction};
use bogita_core::agent::VaultService;
use bogita_core::domain::{agent_servable_field, FieldValue, AGENT_SERVABLE_KEY};
use bogita_core::test_support::test_entry;
use ratatui::crossterm::event::KeyCode;

use super::support::make_app;

/// The stored row must survive a wholesale TUI save as a Boolean.
#[tokio::test]
async fn entry_form_roundtrip_keeps_the_servable_boolean_row() -> Result<()> {
    // -- Setup & Fixtures
    let service = make_app().await?;
    let vaults = service.list_vaults().await?;
    let vault = vaults.first().ok_or("should have a vault")?;
    let entry = test_entry(vault.id, "Deploy key", vec![agent_servable_field(true, 0)]);
    service.save_entry(&entry).await?;
    let stored = service
        .get_entry(entry.id)
        .await?
        .ok_or("should have stored the entry")?;

    // -- Exec: the wholesale TUI save path
    let mut form = EntryForm::new_edit(&stored);
    let FormAction::Confirm(confirmed) = form.handle_key(KeyCode::Enter) else {
        return Err("the form should confirm an untouched entry".into());
    };
    service.save_entry(&confirmed).await?;
    let reloaded = service
        .get_entry(entry.id)
        .await?
        .ok_or("should have reloaded the entry")?;

    // -- Check
    let gate = reloaded
        .fields
        .iter()
        .find(|field| field.key == AGENT_SERVABLE_KEY)
        .ok_or("the reloaded entry should keep the servable gate")?;
    assert_eq!(
        gate.value,
        FieldValue::Boolean(true),
        "the TUI save path must not turn the gate into Text"
    );
    Ok(())
}
