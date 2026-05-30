-- Migration: 002_initial_publisher_schema
-- Description: Initial schema for VBDP Publisher database
-- Created: 2026-01-14
--
-- This migration creates the base schema for the publisher database including:
-- - schema_version table for tracking migrations
-- - publisher_config table for publisher configuration
-- - binaries table for binaries being published
-- - versions table for version tracking
-- - diff_jobs table for diff computation jobs

-- Schema version tracking table
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    description TEXT NOT NULL
);

-- Publisher configuration table
CREATE TABLE IF NOT EXISTS publisher_config (
    config_key TEXT PRIMARY KEY,
    config_value TEXT NOT NULL,
    is_secret INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Insert default publisher configuration
INSERT INTO publisher_config (config_key, config_value, is_secret) VALUES
    ('server_url', 'https://updates.example.com', 0),
    ('publisher_name', '', 0),
    ('api_key', '', 1),
    ('public_key_path', '.vbdp/keys/public.key', 0),
    ('private_key_path', '.vbdp/keys/private.key', 1)
ON CONFLICT(config_key) DO UPDATE SET
    updated_at = CURRENT_TIMESTAMP
WHERE 0; -- Never actually update, just ignore conflicts

-- Binaries table (publisher side)
CREATE TABLE IF NOT EXISTS binaries (
    binary_id TEXT PRIMARY KEY,
    binary_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    binary_path TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (binary_name, platform)
);

CREATE INDEX IF NOT EXISTS idx_publisher_binaries_name ON binaries(binary_name);

-- Versions table (publisher side)
CREATE TABLE IF NOT EXISTS versions (
    version_id TEXT PRIMARY KEY,
    binary_id TEXT NOT NULL,
    version_string TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    file_hash_blake3 BLOB NOT NULL,
    file_hash_sha256 BLOB NOT NULL,
    signature_ed25519 BLOB,
    signature_timestamp TEXT,
    registered_at TEXT,
    published_at TEXT,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (binary_id) REFERENCES binaries(binary_id) ON DELETE CASCADE,
    UNIQUE (binary_id, version_string),
    CHECK ((is_published = 0 AND published_at IS NULL) OR (is_published = 1 AND published_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_publisher_versions_binary ON versions(binary_id);
CREATE INDEX IF NOT EXISTS idx_publisher_versions_published ON versions(is_published);

-- Diff jobs table
CREATE TABLE IF NOT EXISTS diff_jobs (
    job_id TEXT PRIMARY KEY,
    from_version_id TEXT NOT NULL,
    to_version_id TEXT NOT NULL,
    diff_algorithm TEXT NOT NULL DEFAULT 'bsdiff',
    status TEXT NOT NULL DEFAULT 'pending',
    diff_path TEXT,
    diff_size_bytes INTEGER,
    diff_hash_blake3 BLOB,
    computation_time_ms INTEGER,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    FOREIGN KEY (from_version_id) REFERENCES versions(version_id) ON DELETE CASCADE,
    FOREIGN KEY (to_version_id) REFERENCES versions(version_id) ON DELETE CASCADE,
    UNIQUE (from_version_id, to_version_id, diff_algorithm)
);

CREATE INDEX IF NOT EXISTS idx_publisher_diff_jobs_status ON diff_jobs(status);
CREATE INDEX IF NOT EXISTS idx_publisher_diff_jobs_from ON diff_jobs(from_version_id);
CREATE INDEX IF NOT EXISTS idx_publisher_diff_jobs_to ON diff_jobs(to_version_id);

-- Record this migration
INSERT INTO schema_version (version, description) VALUES
    (1, 'Initial publisher schema with binaries, versions, and diff jobs')
ON CONFLICT(version) DO UPDATE SET description = excluded.description;
