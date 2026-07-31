-- Update backend_type CHECK constraint to allow 'none' for local-only vaults
-- This is a breaking change: old 'sqlite' values are treated as None by the mapper

-- SQLite doesn't support ALTER TABLE ... ALTER CONSTRAINT, so we need to
-- recreate the table with the new constraint.

PRAGMA foreign_keys = OFF;

-- Step 1: Create the new table with updated CHECK constraint
CREATE TABLE vaults_new (
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

-- Step 2: Copy data from old table
INSERT INTO vaults_new (id, name, is_default, created_at, backend_type, backend_config, recipients, lock_timeout, auto_sync)
SELECT id, name, is_default, created_at, backend_type, backend_config, recipients, lock_timeout, auto_sync
FROM vaults;

-- Step 3: Drop old table
DROP TABLE vaults;

-- Step 4: Rename new table
ALTER TABLE vaults_new RENAME TO vaults;

PRAGMA foreign_keys = ON;
