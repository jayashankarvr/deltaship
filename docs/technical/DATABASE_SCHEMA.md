# Database Schema Specification

**Version:** 1.0
**Status:** Design Phase (PostgreSQL) / Implemented (SQLite)
**Last Updated:** 2026-01-13

**Audience:** Database administrators, backend developers, system architects

---

## ⚠️ IMPORTANT: Design Specification vs Current Implementation

**P0 Issue 16 Fix: Schema Documentation Clarity**

This document serves dual purposes:

1. **Design Specification (Future)**: The PostgreSQL schemas described here represent the **future design** for a multi-publisher update server that has not yet been implemented.

2. **Current Implementation**: The SQLite schemas for Publisher and Client databases are **currently implemented** in the codebase at:
   - `/crates/deltaship-db/src/schema.rs` - SQLite table definitions
   - `/crates/deltaship-db/src/client.rs` - Client database operations
   - `/crates/deltaship-db/src/publisher.rs` - Publisher database operations (if exists)

### Current Status by Component

| Component | Database Type | Status | Implementation File |
|-----------|--------------|--------|-------------------|
| **Update Server** | PostgreSQL | ❌ Not Implemented | N/A - Future design |
| **Publisher Toolkit** | SQLite | ✅ Implemented | `crates/deltaship-db/src/schema.rs` |
| **Client Patcher** | SQLite | ✅ Implemented | `crates/deltaship-db/src/schema.rs` |

**For current implementation details**, always refer to:
- **Source of Truth**: `/crates/deltaship-db/src/schema.rs`
- **API Usage**: `/crates/deltaship-db/src/client.rs` and `/crates/deltaship-db/src/publisher.rs`

**This documentation** provides design context for future PostgreSQL implementation.

---

## Overview

Deltaship is designed to use different database systems for different components:

- **Update Server** (Future): PostgreSQL (multi-user, high concurrency, relational integrity)
- **Publisher Toolkit** (Implemented): SQLite (local, single-user, embedded)
- **Client Patcher** (Implemented): SQLite (local, lightweight, embedded)

This document defines the complete schema for all three database systems.

---

## Table of Contents

- [Server Database (PostgreSQL)](#server-database-postgresql)
- [Publisher Database (SQLite)](#publisher-database-sqlite)
- [Client Database (SQLite)](#client-database-sqlite)
- [Data Types and Conventions](#data-types-and-conventions)
- [Indexes and Performance](#indexes-and-performance)
- [Migrations](#migrations)
- [Security Considerations](#security-considerations)

---

## Server Database (PostgreSQL)

**❌ NOT IMPLEMENTED - FUTURE DESIGN ONLY**

This section describes the **planned architecture** for a future multi-publisher update server.
The current Deltaship implementation uses only SQLite databases (see Publisher and Client sections).

**Database Name:** `deltaship_server`
**PostgreSQL Version:** 12+
**Character Set:** UTF-8
**Timezone:** UTC

### Schema Creation

```sql
-- Create schema version tracking table first
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    description TEXT NOT NULL
);

INSERT INTO schema_version (version, description)
VALUES (1, 'Initial schema');
```

---

### Table: `publishers`

Stores registered publishers (software vendors using Deltaship).

```sql
CREATE TABLE publishers (
    publisher_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    publisher_name VARCHAR(255) NOT NULL UNIQUE,
    contact_email VARCHAR(255) NOT NULL,

    -- API authentication
    api_key_hash BYTEA NOT NULL, -- SHA-256 hash of API key
    api_key_created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    api_key_last_used TIMESTAMP WITH TIME ZONE,

    -- Public key for signature verification
    public_key_ed25519 BYTEA NOT NULL, -- 32 bytes Ed25519 public key
    public_key_fingerprint VARCHAR(64) NOT NULL UNIQUE, -- Hex-encoded Blake3 hash

    -- Metadata
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT true,

    -- Constraints
    CONSTRAINT valid_email CHECK (contact_email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$'),
    CONSTRAINT valid_api_key_hash CHECK (octet_length(api_key_hash) = 32),
    CONSTRAINT valid_public_key CHECK (octet_length(public_key_ed25519) = 32)
);

-- Indexes
CREATE INDEX idx_publishers_active ON publishers(is_active) WHERE is_active = true;
CREATE INDEX idx_publishers_fingerprint ON publishers(public_key_fingerprint);
CREATE INDEX idx_publishers_api_key_hash ON publishers(api_key_hash);

-- Updated timestamp trigger
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_publishers_updated_at BEFORE UPDATE ON publishers
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

---

### Table: `binaries`

Tracks different binaries (applications) published by each publisher.

```sql
CREATE TABLE binaries (
    binary_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    publisher_id UUID NOT NULL REFERENCES publishers(publisher_id) ON DELETE CASCADE,

    binary_name VARCHAR(255) NOT NULL, -- e.g., "myapp", "game-client"
    platform VARCHAR(50) NOT NULL, -- linux-x86_64, windows-x86_64, macos-aarch64
    binary_type VARCHAR(50), -- executable, library, bundle

    -- Metadata
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT true,

    -- Unique constraint: one binary name per publisher per platform
    CONSTRAINT unique_binary_per_publisher UNIQUE (publisher_id, binary_name, platform),
    CONSTRAINT valid_platform CHECK (platform ~ '^[a-z0-9]+-[a-z0-9_]+$')
);

-- Indexes
CREATE INDEX idx_binaries_publisher ON binaries(publisher_id);
CREATE INDEX idx_binaries_active ON binaries(is_active) WHERE is_active = true;
CREATE INDEX idx_binaries_platform ON binaries(platform);

CREATE TRIGGER update_binaries_updated_at BEFORE UPDATE ON binaries
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

---

### Table: `versions`

Stores all versions of binaries.

```sql
CREATE TABLE versions (
    version_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    binary_id UUID NOT NULL REFERENCES binaries(binary_id) ON DELETE CASCADE,

    -- Version identification
    version_string VARCHAR(255) NOT NULL, -- e.g., "1.0.0", "2023-11-15", "build-1234"
    version_number BIGINT, -- For sorting: 1000000 for 1.0.0, NULL if not semantic

    -- Binary file information
    file_size_bytes BIGINT NOT NULL,
    file_hash_blake3 BYTEA NOT NULL, -- 32 bytes
    file_hash_sha256 BYTEA NOT NULL, -- 32 bytes for compatibility

    -- Storage
    storage_url TEXT NOT NULL, -- S3 URL or file path
    storage_bucket VARCHAR(255), -- S3 bucket name
    storage_key TEXT, -- S3 object key

    -- Signature (from publisher)
    signature_ed25519 BYTEA NOT NULL, -- 64 bytes Ed25519 signature
    signature_timestamp TIMESTAMP WITH TIME ZONE NOT NULL,

    -- Metadata
    release_notes TEXT,
    changelog_url TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    published_at TIMESTAMP WITH TIME ZONE,
    is_published BOOLEAN NOT NULL DEFAULT false,

    -- Constraints
    CONSTRAINT unique_version_per_binary UNIQUE (binary_id, version_string),
    CONSTRAINT valid_file_size CHECK (file_size_bytes > 0),
    CONSTRAINT valid_blake3_hash CHECK (octet_length(file_hash_blake3) = 32),
    CONSTRAINT valid_sha256_hash CHECK (octet_length(file_hash_sha256) = 32),
    CONSTRAINT valid_signature CHECK (octet_length(signature_ed25519) = 64),
    CONSTRAINT valid_version_number CHECK (version_number IS NULL OR version_number >= 0)
);

-- Indexes
CREATE INDEX idx_versions_binary ON versions(binary_id);
CREATE INDEX idx_versions_published ON versions(is_published, published_at) WHERE is_published = true;
CREATE INDEX idx_versions_version_number ON versions(binary_id, version_number);
CREATE INDEX idx_versions_hash_blake3 ON versions(file_hash_blake3);
CREATE INDEX idx_versions_storage_key ON versions(storage_bucket, storage_key);

CREATE TRIGGER update_versions_updated_at BEFORE UPDATE ON versions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

---

### Table: `diffs`

Stores computed diffs between version pairs.

```sql
CREATE TABLE diffs (
    diff_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Version pair
    from_version_id UUID NOT NULL REFERENCES versions(version_id) ON DELETE CASCADE,
    to_version_id UUID NOT NULL REFERENCES versions(version_id) ON DELETE CASCADE,

    -- Diff file information
    diff_algorithm VARCHAR(50) NOT NULL, -- bsdiff, courgette, xdelta3
    diff_size_bytes BIGINT NOT NULL,
    diff_hash_blake3 BYTEA NOT NULL, -- 32 bytes
    compression_format VARCHAR(50), -- gzip, zstd, none
    compressed_size_bytes BIGINT,

    -- Storage
    storage_url TEXT NOT NULL,
    storage_bucket VARCHAR(255),
    storage_key TEXT,

    -- Performance metrics
    compression_ratio REAL, -- diff_size / original_size
    computation_time_ms INTEGER, -- Time taken to compute diff

    -- Metadata
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_accessed_at TIMESTAMP WITH TIME ZONE,
    access_count INTEGER NOT NULL DEFAULT 0,

    -- Constraints
    CONSTRAINT unique_diff_pair UNIQUE (from_version_id, to_version_id, diff_algorithm),
    CONSTRAINT different_versions CHECK (from_version_id != to_version_id),
    CONSTRAINT valid_diff_size CHECK (diff_size_bytes > 0),
    CONSTRAINT valid_blake3_hash CHECK (octet_length(diff_hash_blake3) = 32),
    CONSTRAINT valid_compression_ratio CHECK (compression_ratio > 0 AND compression_ratio <= 1),
    CONSTRAINT valid_algorithm CHECK (diff_algorithm IN ('bsdiff', 'courgette', 'xdelta3', 'fastcdc'))
);

-- Indexes
CREATE INDEX idx_diffs_from_version ON diffs(from_version_id);
CREATE INDEX idx_diffs_to_version ON diffs(to_version_id);
CREATE INDEX idx_diffs_algorithm ON diffs(diff_algorithm);
CREATE INDEX idx_diffs_accessed ON diffs(last_accessed_at);
CREATE INDEX idx_diffs_storage_key ON diffs(storage_bucket, storage_key);

-- Composite index for common query pattern
CREATE INDEX idx_diffs_version_pair ON diffs(from_version_id, to_version_id);
```

---

### Table: `update_requests`

Logs all update check requests from clients.

```sql
CREATE TABLE update_requests (
    request_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Request details
    binary_id UUID NOT NULL REFERENCES binaries(binary_id) ON DELETE CASCADE,
    current_version_id UUID REFERENCES versions(version_id) ON DELETE SET NULL,
    requested_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Client information
    client_ip INET,
    client_user_agent TEXT,
    client_id UUID, -- Anonymous client identifier (optional)

    -- Response
    update_available BOOLEAN NOT NULL,
    recommended_version_id UUID REFERENCES versions(version_id) ON DELETE SET NULL,
    diff_id UUID REFERENCES diffs(diff_id) ON DELETE SET NULL,
    response_time_ms INTEGER,

    -- Constraints
    CONSTRAINT valid_response_time CHECK (response_time_ms >= 0)
);

-- Indexes for analytics
CREATE INDEX idx_update_requests_binary ON update_requests(binary_id, requested_at DESC);
CREATE INDEX idx_update_requests_timestamp ON update_requests(requested_at DESC);
CREATE INDEX idx_update_requests_version ON update_requests(current_version_id);
CREATE INDEX idx_update_requests_client ON update_requests(client_id, requested_at DESC);

-- Partition by month for performance (optional, for high-volume systems)
-- CREATE TABLE update_requests_y2027m01 PARTITION OF update_requests
--     FOR VALUES FROM ('2027-01-01') TO ('2027-02-01');
```

---

### Table: `download_logs`

Tracks diff downloads for analytics and monitoring.

```sql
CREATE TABLE download_logs (
    download_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    diff_id UUID NOT NULL REFERENCES diffs(diff_id) ON DELETE CASCADE,
    request_id UUID REFERENCES update_requests(request_id) ON DELETE SET NULL,

    -- Download details
    started_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP WITH TIME ZONE,
    bytes_transferred BIGINT,
    success BOOLEAN,
    error_message TEXT,

    -- Client information
    client_ip INET,
    cdn_cache_status VARCHAR(50), -- HIT, MISS, BYPASS

    -- Constraints
    CONSTRAINT valid_bytes_transferred CHECK (bytes_transferred IS NULL OR bytes_transferred >= 0)
);

-- Indexes
CREATE INDEX idx_download_logs_diff ON download_logs(diff_id);
CREATE INDEX idx_download_logs_timestamp ON download_logs(started_at DESC);
CREATE INDEX idx_download_logs_success ON download_logs(success, started_at DESC);
CREATE INDEX idx_download_logs_cdn_cache ON download_logs(cdn_cache_status);
```

---

### Table: `rollback_policies`

Defines rollback policies for binaries.

```sql
CREATE TABLE rollback_policies (
    policy_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    binary_id UUID NOT NULL REFERENCES binaries(binary_id) ON DELETE CASCADE,

    -- Rollback settings
    rollback_enabled BOOLEAN NOT NULL DEFAULT true,
    max_rollback_versions INTEGER NOT NULL DEFAULT 3,
    rollback_method VARCHAR(50) NOT NULL DEFAULT 'backup', -- backup, reverse_diff
    auto_rollback_on_failure BOOLEAN NOT NULL DEFAULT false,

    -- Validation
    verify_after_rollback BOOLEAN NOT NULL DEFAULT true,
    rollback_timeout_seconds INTEGER NOT NULL DEFAULT 300,

    -- Metadata
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Constraints
    CONSTRAINT unique_policy_per_binary UNIQUE (binary_id),
    CONSTRAINT valid_rollback_method CHECK (rollback_method IN ('backup', 'reverse_diff')),
    CONSTRAINT valid_max_versions CHECK (max_rollback_versions >= 0 AND max_rollback_versions <= 10),
    CONSTRAINT valid_timeout CHECK (rollback_timeout_seconds > 0)
);

CREATE INDEX idx_rollback_policies_binary ON rollback_policies(binary_id);

CREATE TRIGGER update_rollback_policies_updated_at BEFORE UPDATE ON rollback_policies
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

---

### Table: `rollout_configs`

Manages phased rollout configurations.

```sql
CREATE TABLE rollout_configs (
    rollout_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version_id UUID NOT NULL REFERENCES versions(version_id) ON DELETE CASCADE,

    -- Rollout percentage (0-100)
    rollout_percentage INTEGER NOT NULL DEFAULT 0,

    -- Targeting criteria (JSONB for flexibility)
    targeting_rules JSONB, -- {"platform": "linux", "region": "us-east", "min_version": "1.0.0"}

    -- Timing
    start_time TIMESTAMP WITH TIME ZONE,
    end_time TIMESTAMP WITH TIME ZONE,

    -- Status
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Constraints
    CONSTRAINT valid_percentage CHECK (rollout_percentage >= 0 AND rollout_percentage <= 100),
    CONSTRAINT valid_time_range CHECK (end_time IS NULL OR end_time > start_time),
    CONSTRAINT unique_active_rollout UNIQUE (version_id, is_active)
);

CREATE INDEX idx_rollout_configs_version ON rollout_configs(version_id);
CREATE INDEX idx_rollout_configs_active ON rollout_configs(is_active) WHERE is_active = true;
CREATE INDEX idx_rollout_configs_targeting ON rollout_configs USING gin(targeting_rules);

CREATE TRIGGER update_rollout_configs_updated_at BEFORE UPDATE ON rollout_configs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

---

## Publisher Database (SQLite)

**✅ CURRENTLY IMPLEMENTED** - See `crates/deltaship-db/src/schema.rs` for actual schema

**Database File:** `.deltaship/publisher.db`
**SQLite Version:** 3.35+
**Journal Mode:** WAL (Write-Ahead Logging)
**Foreign Keys:** ENABLED

**Schema below matches the current implementation in the codebase.**

### Initialization

```sql
-- Enable foreign keys
PRAGMA foreign_keys = ON;

-- Use WAL mode for better concurrency
PRAGMA journal_mode = WAL;

-- Schema version
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    description TEXT NOT NULL
);

INSERT INTO schema_version (version, description)
VALUES (1, 'Initial schema');
```

---

### Table: `publisher_config`

Stores publisher configuration (API keys, server URL).

```sql
CREATE TABLE publisher_config (
    config_key TEXT PRIMARY KEY,
    config_value TEXT NOT NULL,
    is_secret BOOLEAN NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Default configuration
INSERT INTO publisher_config (config_key, config_value, is_secret) VALUES
('server_url', 'https://updates.example.com', 0),
('publisher_name', '', 0),
('api_key', '', 1), -- Encrypted or use system keychain
('public_key_path', '.deltaship/keys/public.key', 0),
('private_key_path', '.deltaship/keys/private.key', 1);
```

---

### Table: `binaries`

Tracks binaries being published.

```sql
CREATE TABLE binaries (
    binary_id TEXT PRIMARY KEY, -- UUID from server, or local UUID
    binary_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    binary_path TEXT NOT NULL, -- Local file path

    description TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE (binary_name, platform)
);

CREATE INDEX idx_publisher_binaries_name ON binaries(binary_name);
```

---

### Table: `versions`

Tracks registered versions.

```sql
CREATE TABLE versions (
    version_id TEXT PRIMARY KEY, -- UUID from server after registration
    binary_id TEXT NOT NULL,

    version_string TEXT NOT NULL,
    file_path TEXT NOT NULL, -- Local path to binary file
    file_size_bytes INTEGER NOT NULL,
    file_hash_blake3 BLOB NOT NULL,
    file_hash_sha256 BLOB NOT NULL,

    signature_ed25519 BLOB, -- Signature (64 bytes)
    signature_timestamp TEXT,

    registered_at TEXT,
    published_at TEXT,
    is_published BOOLEAN NOT NULL DEFAULT 0,

    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (binary_id) REFERENCES binaries(binary_id) ON DELETE CASCADE,
    UNIQUE (binary_id, version_string)
);

CREATE INDEX idx_publisher_versions_binary ON versions(binary_id);
CREATE INDEX idx_publisher_versions_published ON versions(is_published);
```

---

### Table: `diff_jobs`

Tracks diff computation jobs.

```sql
CREATE TABLE diff_jobs (
    job_id TEXT PRIMARY KEY,

    from_version_id TEXT NOT NULL,
    to_version_id TEXT NOT NULL,

    diff_algorithm TEXT NOT NULL DEFAULT 'bsdiff',
    status TEXT NOT NULL DEFAULT 'pending', -- pending, running, completed, failed

    diff_path TEXT, -- Local path to generated diff
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

CREATE INDEX idx_publisher_diff_jobs_status ON diff_jobs(status);
CREATE INDEX idx_publisher_diff_jobs_from ON diff_jobs(from_version_id);
CREATE INDEX idx_publisher_diff_jobs_to ON diff_jobs(to_version_id);
```

---

## Client Database (SQLite)

**✅ CURRENTLY IMPLEMENTED** - See `crates/deltaship-db/src/schema.rs` for actual schema

**Database File:** `/var/lib/deltaship/client.db` (Linux) or `%PROGRAMDATA%\Deltaship\client.db` (Windows)
**SQLite Version:** 3.35+
**Journal Mode:** WAL
**Foreign Keys:** ENABLED

**Schema below matches the current implementation in the codebase.**

### Initialization

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    description TEXT NOT NULL
);

INSERT INTO schema_version (version, description)
VALUES (1, 'Initial schema');
```

---

### Table: `client_config`

Client configuration.

```sql
CREATE TABLE client_config (
    config_key TEXT PRIMARY KEY,
    config_value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Default configuration
INSERT INTO client_config (config_key, config_value) VALUES
('server_url', 'https://updates.example.com'),
('check_interval_seconds', '3600'),
('auto_update_enabled', '1'),
('verify_signatures', '1'),
('rollback_enabled', '1'),
('max_rollback_backups', '3');
```

---

### Table: `managed_binaries`

Binaries managed by this client.

```sql
CREATE TABLE managed_binaries (
    binary_id TEXT PRIMARY KEY, -- UUID from server
    binary_name TEXT NOT NULL UNIQUE,
    platform TEXT NOT NULL,

    install_path TEXT NOT NULL, -- Path to the installed binary
    current_version_id TEXT,
    current_version_string TEXT,

    -- Trusted public key for this binary
    publisher_public_key BLOB NOT NULL, -- 32 bytes Ed25519 public key

    auto_update BOOLEAN NOT NULL DEFAULT 1,
    last_check_at TEXT,
    last_update_at TEXT,

    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_client_binaries_name ON managed_binaries(binary_name);
```

---

### Table: `installed_versions`

History of installed versions.

```sql
CREATE TABLE installed_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,

    version_id TEXT NOT NULL, -- UUID from server
    version_string TEXT NOT NULL,

    file_hash_blake3 BLOB NOT NULL,
    file_hash_sha256 BLOB NOT NULL,
    file_size_bytes INTEGER NOT NULL,

    installed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_current BOOLEAN NOT NULL DEFAULT 0,

    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);

CREATE INDEX idx_client_versions_binary ON installed_versions(binary_id);
CREATE INDEX idx_client_versions_current ON installed_versions(binary_id, is_current);
```

---

### Table: `update_history`

Tracks all update attempts.

```sql
CREATE TABLE update_history (
    update_id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,

    from_version_id TEXT,
    from_version_string TEXT,
    to_version_id TEXT NOT NULL,
    to_version_string TEXT NOT NULL,

    diff_id TEXT, -- UUID from server
    diff_algorithm TEXT,
    diff_size_bytes INTEGER,

    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,

    status TEXT NOT NULL, -- downloading, applying, verifying, completed, failed, rolled_back
    success BOOLEAN,
    error_message TEXT,

    download_time_ms INTEGER,
    apply_time_ms INTEGER,
    verify_time_ms INTEGER,

    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);

CREATE INDEX idx_client_update_history_binary ON update_history(binary_id, started_at DESC);
CREATE INDEX idx_client_update_history_status ON update_history(status);
```

---

### Table: `rollback_backups`

Stores backup information for rollbacks.

```sql
CREATE TABLE rollback_backups (
    backup_id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,

    version_id TEXT NOT NULL,
    version_string TEXT NOT NULL,

    backup_path TEXT NOT NULL, -- Path to backed-up binary
    backup_hash_blake3 BLOB NOT NULL,
    backup_size_bytes INTEGER NOT NULL,

    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TEXT, -- When this backup can be deleted

    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);

CREATE INDEX idx_client_backups_binary ON rollback_backups(binary_id);
CREATE INDEX idx_client_backups_expires ON rollback_backups(expires_at);
```

---

### Table: `signature_cache`

Caches verified signatures to avoid re-verification.

```sql
CREATE TABLE signature_cache (
    file_hash_blake3 BLOB PRIMARY KEY,

    signature_ed25519 BLOB NOT NULL,
    public_key_ed25519 BLOB NOT NULL,
    verified_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Cache expiration
    expires_at TEXT NOT NULL
);

CREATE INDEX idx_client_signature_cache_expires ON signature_cache(expires_at);
```

---

## Data Types and Conventions

### PostgreSQL Data Types

| Type | Usage | Notes |
|------|-------|-------|
| `UUID` | Primary keys, foreign keys | Uses `gen_random_uuid()` |
| `BYTEA` | Binary data (hashes, keys, signatures) | Fixed-length when possible |
| `VARCHAR(N)` | Bounded strings | With length limits |
| `TEXT` | Unbounded strings | URLs, descriptions, logs |
| `BIGINT` | File sizes, large numbers | Max ~9 exabytes |
| `INTEGER` | Counts, small numbers | Max ~2 billion |
| `REAL` | Ratios, percentages | Single precision |
| `BOOLEAN` | Flags | `true`/`false` |
| `TIMESTAMP WITH TIME ZONE` | All timestamps | Always use UTC |
| `INET` | IP addresses | IPv4 and IPv6 |
| `JSONB` | Flexible structured data | Indexed with GIN |

### SQLite Data Types

| Type | Usage | Notes |
|------|-------|-------|
| `TEXT` | Strings, UUIDs, timestamps | ISO 8601 for timestamps |
| `INTEGER` | Numbers, booleans | Boolean: 0 or 1 |
| `REAL` | Floating point | Ratios, percentages |
| `BLOB` | Binary data | Hashes, keys, signatures |

### Naming Conventions

- **Tables**: Plural nouns (`publishers`, `versions`, `diffs`)
- **Columns**: Snake case (`file_hash_blake3`, `created_at`)
- **Primary Keys**: `<table>_id` (e.g., `publisher_id`, `binary_id`)
- **Foreign Keys**: Same name as referenced column
- **Indexes**: `idx_<table>_<column(s)>` (e.g., `idx_versions_binary`)
- **Constraints**: `<type>_<description>` (e.g., `valid_email`, `unique_version_per_binary`)

### Hash Storage

- **Blake3**: 32 bytes (`BYTEA` in PostgreSQL, `BLOB` in SQLite)
- **SHA-256**: 32 bytes
- **Format**: Raw bytes, not hex-encoded (saves space)
- **Conversion**: Use `encode(hash, 'hex')` in PostgreSQL for display

### Timestamp Format

**PostgreSQL:**
```sql
TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
```

**SQLite:**
```sql
TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP  -- ISO 8601: YYYY-MM-DD HH:MM:SS
```

---

## Indexes and Performance

### Index Strategy

**Purpose-Based Indexing:**

1. **Primary key lookups**: Automatic (PRIMARY KEY)
2. **Foreign key joins**: Index all foreign keys
3. **Filter conditions**: Index columns in WHERE clauses
4. **Sort operations**: Index columns in ORDER BY
5. **Range queries**: B-tree indexes
6. **JSON queries**: GIN indexes (PostgreSQL)
7. **Full-text search**: Future (if needed)

### PostgreSQL-Specific Optimizations

**Partial Indexes** (filter on condition):
```sql
CREATE INDEX idx_versions_published
ON versions(published_at)
WHERE is_published = true;
```

**Composite Indexes** (multi-column queries):
```sql
CREATE INDEX idx_diffs_version_pair
ON diffs(from_version_id, to_version_id);
```

**GIN Indexes** (JSONB):
```sql
CREATE INDEX idx_rollout_configs_targeting
ON rollout_configs USING gin(targeting_rules);
```

### Query Performance Targets

- **Primary key lookup**: < 1ms
- **Simple foreign key join**: < 10ms
- **Complex multi-table query**: < 100ms
- **Analytics query**: < 1s (with proper indexes)

### Maintenance

**PostgreSQL:**
```sql
-- Vacuum and analyze regularly
VACUUM ANALYZE;

-- Reindex if needed
REINDEX TABLE versions;

-- Monitor index usage
SELECT * FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;
```

**SQLite:**
```sql
-- Vacuum
VACUUM;

-- Analyze
ANALYZE;

-- Integrity check
PRAGMA integrity_check;
```

---

## Migrations

### Migration Strategy

**Approach:** Forward-only migrations with version tracking

**Directory structure:**
```
migrations/
├── server/
│   ├── 001_initial_schema.sql
│   ├── 002_add_rollback_policies.sql
│   └── 003_add_rollout_configs.sql
├── publisher/
│   ├── 001_initial_schema.sql
│   └── 002_add_diff_jobs.sql
└── client/
    ├── 001_initial_schema.sql
    └── 002_add_signature_cache.sql
```

### Migration Template

```sql
-- Migration: 002_add_rollback_policies.sql
-- Description: Add rollback_policies table
-- Applied: 2027-01-15

BEGIN;

-- Check current schema version
DO $$
BEGIN
    IF (SELECT MAX(version) FROM schema_version) != 1 THEN
        RAISE EXCEPTION 'Wrong schema version, expected 1';
    END IF;
END $$;

-- Create new table
CREATE TABLE rollback_policies (
    -- ... table definition ...
);

-- Update schema version
INSERT INTO schema_version (version, description)
VALUES (2, 'Add rollback_policies table');

COMMIT;
```

### Migration Tool

**Recommended:** Use `sqlx` (Rust) or custom migration tool

```bash
# Apply migrations
deltaship-migrate --database server --up

# Rollback (if supported)
deltaship-migrate --database server --down

# Check status
deltaship-migrate --database server --status
```

---

## Security Considerations

### Sensitive Data Protection

**API Keys:**
- Store as SHA-256 hash in `publishers.api_key_hash`
- Never store plaintext API keys
- Use bcrypt or Argon2 for additional security (future enhancement)

**Private Keys:**
- NEVER store in database
- Publisher toolkit: Encrypted file or system keychain
- Client: Not needed (only public keys)

**Passwords/Secrets:**
- Not stored in Deltaship databases
- Use external secret management (HashiCorp Vault, AWS Secrets Manager)

### SQL Injection Prevention

**Use parameterized queries:**
```rust
// Good
sqlx::query!("SELECT * FROM versions WHERE version_id = $1", version_id)

// Bad
format!("SELECT * FROM versions WHERE version_id = '{}'", version_id)
```

### Access Control

**PostgreSQL:**
```sql
-- Create read-only user for analytics
CREATE USER deltaship_readonly WITH PASSWORD 'secure_password';
GRANT CONNECT ON DATABASE deltaship_server TO deltaship_readonly;
GRANT USAGE ON SCHEMA public TO deltaship_readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO deltaship_readonly;

-- Create application user with limited permissions
CREATE USER deltaship_app WITH PASSWORD 'secure_password';
GRANT CONNECT ON DATABASE deltaship_server TO deltaship_app;
GRANT USAGE ON SCHEMA public TO deltaship_app;
GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA public TO deltaship_app;
GRANT DELETE ON download_logs, update_requests TO deltaship_app;
```

### Encryption

**At Rest:**
- PostgreSQL: Use encrypted volumes (LUKS, AWS EBS encryption)
- SQLite: Use SQLCipher for encryption (optional)

**In Transit:**
- Always use TLS for PostgreSQL connections
- Set `sslmode=require` in connection string

---

## Summary

This schema provides:

1. **Complete server schema** (PostgreSQL) for multi-publisher update distribution
2. **Publisher schema** (SQLite) for local version management and diff computation
3. **Client schema** (SQLite) for update tracking and rollback support
4. **Performance optimizations** through strategic indexing
5. **Migration strategy** for schema evolution
6. **Security best practices** for sensitive data

**Next Steps:**
1. Review and validate schema design
2. Implement migration scripts
3. Create database initialization scripts
4. Build ORM/query layer (using `sqlx` in Rust)
5. Performance test with realistic data volumes

---

**References:**
- [PostgreSQL Documentation](https://www.postgresql.org/docs/)
- [SQLite Documentation](https://www.sqlite.org/docs.html)
- [sqlx Documentation](https://docs.rs/sqlx/)
