-- no-transaction
-- Migration 002: Update backend_type CHECK constraint to include 'none'
--
-- SQLite cannot ALTER a CHECK constraint. We must rebuild the vaults table.
-- This migration runs outside a sqlx transaction (via -- no-transaction)
-- so that PRAGMA foreign_keys = OFF actually takes effect. Without this,
-- DROP TABLE vaults would cascade-delete all entries via ON DELETE CASCADE.
--
-- Steps:
-- 1. Disable foreign keys
-- 2. Backup entries and entry_fields
-- 3. Drop entry tables (they reference vaults)
-- 4. Drop and recreate vaults with new CHECK constraint
-- 5. Recreate entry tables with original schema
-- 6. Restore entries and entry_fields from backups
-- 7. Drop backup tables
-- 8. Recreate indexes
-- 9. Re-enable foreign keys

PRAGMA foreign_keys = OFF;

BEGIN;

-- Step 1: Backup entries and entry_fields
CREATE TABLE entries_backup AS SELECT * FROM entries;
CREATE TABLE entry_fields_backup AS SELECT * FROM entry_fields;

-- Step 2: Drop entry tables (FK references vaults)
DROP TABLE entry_fields;
DROP TABLE entries;

-- Step 3: Drop and recreate vaults with new CHECK constraint
DROP TABLE vaults;

CREATE TABLE vaults (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    is_default BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    backend_type TEXT NOT NULL CHECK(backend_type IN ('git', 'aws', 'gcp', 'sqlite', 'none')),
    backend_config TEXT NOT NULL,
    recipients TEXT NOT NULL,
    lock_timeout INTEGER,
    auto_sync BOOLEAN NOT NULL DEFAULT 0
);

-- Step 4: Recreate entries table
CREATE TABLE entries (
    id TEXT PRIMARY KEY NOT NULL,
    vault_id TEXT NOT NULL,
    name TEXT NOT NULL,
    entry_type TEXT NOT NULL CHECK(entry_type IN ('token', 'otp', 'ssh_key', 'note')),
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    FOREIGN KEY (vault_id) REFERENCES vaults(id) ON DELETE CASCADE,
    UNIQUE(vault_id, name)
);

-- Step 5: Recreate entry_fields table
CREATE TABLE entry_fields (
    id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    field_type TEXT NOT NULL,
    encrypted BOOLEAN NOT NULL,
    idx INTEGER NOT NULL,
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

-- Step 6: Restore entries and entry_fields
INSERT INTO entries SELECT * FROM entries_backup;
INSERT INTO entry_fields SELECT * FROM entry_fields_backup;

-- Step 7: Drop backup tables
DROP TABLE entry_fields_backup;
DROP TABLE entries_backup;

-- Step 8: Recreate indexes
CREATE INDEX idx_entries_vault ON entries(vault_id);
CREATE INDEX idx_entries_name ON entries(vault_id, name);
CREATE INDEX idx_entries_type ON entries(entry_type);
CREATE INDEX idx_entries_modified ON entries(modified_at);
CREATE INDEX idx_fields_entry ON entry_fields(entry_id);
CREATE INDEX idx_fields_key ON entry_fields(key) WHERE encrypted = 0;
CREATE INDEX idx_fields_type ON entry_fields(field_type);
CREATE INDEX idx_fields_search ON entry_fields(key, value) WHERE encrypted = 0;

COMMIT;

PRAGMA foreign_keys = ON;
