# Database Migrations

This document describes the database migration strategy for VBDP.

## Overview

VBDP uses SQLite for both the publisher and client databases. To manage schema changes over time, we use a migration-based approach using sqlx migrations.

## Migration Files

Migration files are stored in the `migrations/` directory at the workspace root:

```
migrations/
├── 001_initial_client_schema.sql
└── 002_initial_publisher_schema.sql
```

### Migration File Naming Convention

Migration files follow the pattern: `{version}_{description}.sql`

- **version**: Zero-padded 3-digit number (001, 002, etc.)
- **description**: Snake_case description of the migration

Examples:
- `001_initial_client_schema.sql`
- `002_initial_publisher_schema.sql`
- `003_add_metadata_table.sql`

## Migration Structure

Each migration file contains:

1. **Header comment** - Description and metadata
2. **SQL statements** - DDL statements to create or modify schema
3. **Migration record** - Insert into schema_version table

Example:

```sql
-- Migration: 001_initial_client_schema
-- Description: Initial schema for VBDP Client database
-- Created: 2026-01-14

-- Create tables
CREATE TABLE IF NOT EXISTS managed_binaries (
    binary_id TEXT PRIMARY KEY,
    -- ... columns
);

-- Record this migration
INSERT INTO schema_version (version, description) VALUES
    (1, 'Initial client schema')
ON CONFLICT(version) DO UPDATE SET description = excluded.description;
```

## Current Migration System

VBDP currently uses an **inline migration system** where schema is created programmatically in the `init()` functions of `ClientDb` and `PublisherDb`. The migration SQL files serve as:

1. **Documentation** of the schema structure
2. **Reference** for manual migrations if needed
3. **Foundation** for future migration tooling

### Why Inline Migrations?

The current approach using inline schema creation has several advantages for v0.1.0:

- **Simplicity**: No external tooling required
- **Embedded**: Schema is part of the compiled binary
- **Atomicity**: Schema creation happens in the application code
- **Testing**: Easy to test with in-memory databases

### Future Migration Support

In future versions (v0.2.0+), VBDP will support:

- **sqlx-cli integration** for running migrations
- **Migration verification** against embedded schema
- **Automatic migration on upgrade** using the migration files

## Using Migration Files (Manual)

If you need to manually create a database using the migration files:

### Client Database

```bash
# Create database and run migration
sqlite3 client.db < migrations/001_initial_client_schema.sql
```

### Publisher Database

```bash
# Create database and run migration
sqlite3 publisher.db < migrations/002_initial_publisher_schema.sql
```

## Schema Versioning

Both databases track their schema version in the `schema_version` table:

```sql
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    description TEXT NOT NULL
);
```

### Checking Current Schema Version

**Client database:**
```sql
SELECT version, description, applied_at FROM schema_version;
```

**Publisher database:**
```sql
SELECT version, description, applied_at FROM schema_version;
```

### Current Schema Versions

| Database  | Current Version | Description                    |
|-----------|-----------------|--------------------------------|
| Client    | 1               | Initial client schema          |
| Publisher | 1               | Initial publisher schema       |

## Creating New Migrations

When adding new features that require schema changes:

1. **Create migration file** in `migrations/` with next sequential number
2. **Write SQL statements** to modify schema
3. **Add migration record** to schema_version table
4. **Update constants** in `src/schema.rs`:
   - `CLIENT_SCHEMA_VERSION` or `PUBLISHER_SCHEMA_VERSION`
5. **Update init() function** to include new schema changes
6. **Test migration** on a copy of production database
7. **Document changes** in this file

### Migration File Template

```sql
-- Migration: {NNN}_{description}
-- Description: {Detailed description of changes}
-- Created: {YYYY-MM-DD}
--
-- This migration adds/modifies:
-- - {List of changes}

-- Add your DDL statements here
ALTER TABLE foo ADD COLUMN bar TEXT;

-- Record this migration
INSERT INTO schema_version (version, description) VALUES
    ({N}, '{Description}')
ON CONFLICT(version) DO UPDATE SET description = excluded.description;
```

## Migration Best Practices

### DO:
- ✅ Use `IF NOT EXISTS` for CREATE statements
- ✅ Use `ON CONFLICT` for INSERT statements
- ✅ Include comments explaining the purpose
- ✅ Test migrations on a database copy first
- ✅ Keep migrations small and focused
- ✅ Use transactions for multi-step migrations
- ✅ Update schema version constants in code

### DON'T:
- ❌ Modify existing migration files after release
- ❌ Delete old migration files
- ❌ Skip version numbers
- ❌ Mix DDL and DML in same migration (except schema_version)
- ❌ Use destructive operations without backups
- ❌ Rely on data being in a specific state

## Schema Compatibility

### Version Compatibility Matrix

| Client Version | Min DB Schema | Max DB Schema |
|----------------|---------------|---------------|
| 0.1.0          | 1             | 1             |

| Publisher Version | Min DB Schema | Max DB Schema |
|-------------------|---------------|---------------|
| 0.1.0             | 1             | 1             |

### Schema Mismatch Handling

When the application detects a schema version mismatch:

**Newer schema than expected:**
```
Error: Database schema version 2 is newer than expected version 1.
Please upgrade vbdp-db or use a compatible database.
Migrations are not yet supported.
```

**Older schema than expected:**
```
Error: Database schema version 0 is older than expected version 1.
Please backup and recreate the database.
Migrations are not yet supported.
```

## Backup and Restore

Before running any migrations:

1. **Backup the database:**
   ```bash
   cp publisher.db publisher.db.backup
   cp client.db client.db.backup
   ```

2. **Verify backup:**
   ```bash
   sqlite3 publisher.db.backup "PRAGMA integrity_check"
   ```

3. **Run migration** (when supported)

4. **Verify migration:**
   ```bash
   sqlite3 publisher.db "SELECT * FROM schema_version"
   ```

5. **Test application** with new schema

6. **Remove backup** after confirming success (or keep for auditing)

## Rollback Procedures

See [MIGRATION_ROLLBACK.md](./MIGRATION_ROLLBACK.md) for detailed rollback procedures.

## Future Enhancements

Planned improvements for migration system:

1. **Automatic migrations** on application startup (v0.2.0)
2. **sqlx-cli integration** for offline migration checking
3. **Migration dry-run** mode to preview changes
4. **Migration rollback** support for reversible migrations
5. **Migration hooks** for data transformations
6. **Multi-database migrations** for server deployments

## References

- [SQLite Migration Best Practices](https://www.sqlite.org/lang_altertable.html)
- [sqlx Migration Documentation](https://docs.rs/sqlx/latest/sqlx/migrate/)
- [Schema Versioning Patterns](https://martinfowler.com/articles/evodb.html)

## Support

For migration-related issues:

1. Check the [FAQ](./FAQ.md)
2. Review [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)
3. File an issue on GitHub with:
   - Current schema version
   - Expected schema version
   - Full error message
   - Steps to reproduce
