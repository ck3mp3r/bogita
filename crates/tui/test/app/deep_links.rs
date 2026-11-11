type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;

use super::support::*;

#[tokio::test]
async fn edit_entry_context_opens_edit_form_for_existing_entry() -> Result<()> {
    let tui = Tui::new(
        make_app_with_entry().await?,
        TuiContext::EditEntry {
            name: "GitHub".to_string(),
            vault: None,
        },
    )
    .await?;

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "expected the edit form on the name slot"
    );
    assert!(tui.error_message.is_none());
    Ok(())
}

#[tokio::test]
async fn edit_entry_context_reports_missing_entry() -> Result<()> {
    let tui = Tui::new(
        make_app_with_entry().await?,
        TuiContext::EditEntry {
            name: "NoSuchEntry".to_string(),
            vault: None,
        },
    )
    .await?;

    let message = tui.error_message.unwrap_or_default();
    assert!(
        message.contains("not found"),
        "expected a not-found error, got: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn delete_entry_context_opens_confirm_modal_for_existing_entry() -> Result<()> {
    let tui = Tui::new(
        make_app_with_entry().await?,
        TuiContext::DeleteEntry {
            name: "GitHub".to_string(),
            vault: Some("Personal".to_string()),
        },
    )
    .await?;

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ConfirmDelete,
        "expected the delete confirmation"
    );
    assert!(tui.error_message.is_none());
    Ok(())
}

#[tokio::test]
async fn delete_entry_context_reports_missing_entry() -> Result<()> {
    let tui = Tui::new(
        make_app_with_entry().await?,
        TuiContext::DeleteEntry {
            name: "NoSuchEntry".to_string(),
            vault: None,
        },
    )
    .await?;

    let message = tui.error_message.unwrap_or_default();
    assert!(
        message.contains("not found"),
        "expected a not-found error, got: {message}"
    );
    Ok(())
}
