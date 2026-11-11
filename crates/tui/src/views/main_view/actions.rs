//! Actions and column focus produced by key handling.

// ── MainViewAction ────────────────────────────────────────────────────────────

/// Actions returned by [`MainView::handle_key`](crate::views::main_view::MainView::handle_key)
/// that require the parent `Tui` to take async action (open a form, persist a
/// delete, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainViewAction {
    None,
    OpenAddForm {
        vault_id: uuid::Uuid,
    },
    OpenEditForm {
        entry_id: uuid::Uuid,
    },
    DeleteEntry {
        entry_id: uuid::Uuid,
    },
    CopyField {
        entry_id: uuid::Uuid,
        field_idx: usize,
    },
    /// Entry selection changed — caller should fetch and decrypt this entry.
    SelectEntry {
        entry_id: uuid::Uuid,
        vault_id: uuid::Uuid,
    },
    /// Open the vault creation form.
    OpenAddVault,
    /// Delete the selected vault.
    DeleteVault {
        vault_id: uuid::Uuid,
    },
}

// ── Column focus ──────────────────────────────────────────────────────────────

/// Which column currently has keyboard focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Column {
    Entries,
    Detail,
}

impl Column {
    /// Two-column cycle. `BackTab` uses the same cycle because two columns make
    /// next and previous the same edge.
    pub(super) fn next(&self) -> Self {
        match self {
            Column::Entries => Column::Detail,
            Column::Detail => Column::Entries,
        }
    }

    /// The other pane — what `h` and `l` step to.
    pub(super) fn prev(&self) -> Self {
        self.next()
    }
}
