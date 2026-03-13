-- Initial schema for Bogita password manager
-- Field-based system with granular encryption

-- Vaults table
CREATE TABLE vaults (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    is_default BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    -- Backend configuration stored as JSON
    backend_type TEXT NOT NULL CHECK(backend_type IN ('git', 'aws', 'gcp', 'sqlite')),
    backend_config TEXT NOT NULL,
    -- Age recipients stored as JSON array
    recipients TEXT NOT NULL,
    lock_timeout INTEGER,
    auto_sync BOOLEAN NOT NULL DEFAULT 0
);

-- Entries table
CREATE TABLE entries (
    id TEXT PRIMARY KEY NOT NULL,
    vault_id TEXT NOT NULL,
    name TEXT NOT NULL,
    entry_type TEXT NOT NULL CHECK(entry_type IN ('password', 'otp', 'ssh_key', 'note')),
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    FOREIGN KEY (vault_id) REFERENCES vaults(id) ON DELETE CASCADE,
    UNIQUE(vault_id, name)
);

-- Entry fields table (field-based key-value system)
CREATE TABLE entry_fields (
    id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL,
    key TEXT NOT NULL,
    -- Value stores serialized FieldValue JSON (plaintext) or base64-encoded encrypted blob
    value TEXT NOT NULL,
    field_type TEXT NOT NULL,
    encrypted BOOLEAN NOT NULL,
    -- Display order
    idx INTEGER NOT NULL,
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

-- Indexes for entries
CREATE INDEX idx_entries_vault ON entries(vault_id);
CREATE INDEX idx_entries_name ON entries(vault_id, name);
CREATE INDEX idx_entries_type ON entries(entry_type);
CREATE INDEX idx_entries_modified ON entries(modified_at);

-- Indexes for entry_fields (optimized for search on plaintext fields)
CREATE INDEX idx_fields_entry ON entry_fields(entry_id);
CREATE INDEX idx_fields_key ON entry_fields(key) WHERE encrypted = 0;
CREATE INDEX idx_fields_type ON entry_fields(field_type);
-- Composite index for fast field search (username, url, etc.)
CREATE INDEX idx_fields_search ON entry_fields(key, value) WHERE encrypted = 0;
