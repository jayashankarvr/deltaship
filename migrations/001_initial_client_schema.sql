-- Migration: 001_initial_client_schema
-- Description: Initial schema for VBDP Client database
-- Created: 2026-01-14
--
-- This migration creates the base schema for the client database including:
-- - schema_version table for tracking migrations
-- - client_config table for client configuration
-- - managed_binaries table for binaries managed by the client
-- - installed_versions table for version history
-- - update_history table for tracking update attempts
-- - rollback_backups table for rollback capability

-- Schema version tracking table
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    description TEXT NOT NULL
);

-- Client configuration table
CREATE TABLE IF NOT EXISTS client_config (
    config_key TEXT PRIMARY KEY,
    config_value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Insert default client configuration
INSERT INTO client_config (config_key, config_value) VALUES
    ('server_url', 'https://updates.example.com'),
    ('check_interval_seconds', '3600'),
    ('auto_update_enabled', '1'),
    ('verify_signatures', '1'),
    ('rollback_enabled', '1'),
    ('max_rollback_backups', '3')
ON CONFLICT(config_key) DO UPDATE SET
    updated_at = CURRENT_TIMESTAMP
WHERE 0; -- Never actually update, just ignore conflicts

-- Managed binaries table
CREATE TABLE IF NOT EXISTS managed_binaries (
    binary_id TEXT PRIMARY KEY,
    binary_name TEXT NOT NULL UNIQUE,
    platform TEXT NOT NULL,
    install_path TEXT NOT NULL,
    current_version_id TEXT,
    current_version_string TEXT,
    publisher_public_key BLOB NOT NULL,
    auto_update INTEGER NOT NULL DEFAULT 1,
    last_check_at TEXT,
    last_update_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_client_binaries_name ON managed_binaries(binary_name);

-- Installed versions table
CREATE TABLE IF NOT EXISTS installed_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    version_string TEXT NOT NULL,
    file_hash_blake3 BLOB NOT NULL,
    file_hash_sha256 BLOB NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    installed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_current INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_client_versions_binary ON installed_versions(binary_id);
CREATE INDEX IF NOT EXISTS idx_client_versions_current ON installed_versions(binary_id, is_current);
CREATE UNIQUE INDEX IF NOT EXISTS idx_client_versions_one_current
    ON installed_versions(binary_id) WHERE is_current = 1;

-- Update history table
CREATE TABLE IF NOT EXISTS update_history (
    update_id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,
    from_version_id TEXT,
    from_version_string TEXT,
    to_version_id TEXT NOT NULL,
    to_version_string TEXT NOT NULL,
    diff_id TEXT,
    diff_algorithm TEXT,
    diff_size_bytes INTEGER,
    full_size_bytes INTEGER,
    actual_downloaded_bytes INTEGER,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    status TEXT NOT NULL,
    success INTEGER,
    error_message TEXT,
    download_time_ms INTEGER,
    apply_time_ms INTEGER,
    verify_time_ms INTEGER,
    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_client_update_history_binary ON update_history(binary_id, started_at);
CREATE INDEX IF NOT EXISTS idx_client_update_history_status ON update_history(status);

-- Rollback backups table
CREATE TABLE IF NOT EXISTS rollback_backups (
    backup_id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    version_string TEXT NOT NULL,
    backup_path TEXT NOT NULL,
    backup_hash_blake3 BLOB NOT NULL,
    backup_size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TEXT,
    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_client_backups_binary ON rollback_backups(binary_id);
CREATE INDEX IF NOT EXISTS idx_client_backups_expires ON rollback_backups(expires_at);

-- Record this migration
INSERT INTO schema_version (version, description) VALUES
    (1, 'Initial client schema with managed binaries, versions, update history, and rollback support')
ON CONFLICT(version) DO UPDATE SET description = excluded.description;
