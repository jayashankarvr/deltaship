# Migration Rollback Procedures

This document describes how to rollback database migrations in Deltaship when issues occur.

## Overview

Database migrations can sometimes fail or cause issues. This guide provides step-by-step procedures for rolling back to a previous schema version.

## Important Notes

⚠️ **Current Status**: Deltaship v0.1.0 does not support automated rollbacks. All rollbacks must be performed manually.

⚠️ **Data Loss**: Rolling back migrations may result in data loss if new tables or columns were added.

⚠️ **Backups Required**: Always maintain backups before running migrations.

## Pre-Migration Checklist

Before applying any migration, ensure you have:

- [ ] **Full database backup** with verified integrity
- [ ] **Application backup** of the current version
- [ ] **Migration script** reviewed and tested on a copy
- [ ] **Rollback plan** documented and understood
- [ ] **Downtime window** scheduled if needed
- [ ] **Monitoring** in place to detect issues

## Rollback Methods

### Method 1: Restore from Backup (Recommended)

The safest and fastest rollback method is to restore from a pre-migration backup.

#### Client Database Rollback

```bash
# 1. Stop the client application
systemctl stop deltaship-client  # or your service manager

# 2. Verify backup integrity
sqlite3 client.db.backup "PRAGMA integrity_check"

# 3. Create a backup of the failed migration (for debugging)
cp client.db client.db.failed
cp client.db-wal client.db-wal.failed 2>/dev/null || true
cp client.db-shm client.db-shm.failed 2>/dev/null || true

# 4. Restore from backup
cp client.db.backup client.db

# 5. Verify restored database
sqlite3 client.db "PRAGMA integrity_check"
sqlite3 client.db "SELECT version FROM schema_version"

# 6. Start the application
systemctl start deltaship-client
```

#### Publisher Database Rollback

```bash
# 1. Stop the publisher application
systemctl stop deltaship-publisher  # or your service manager

# 2. Verify backup integrity
sqlite3 publisher.db.backup "PRAGMA integrity_check"

# 3. Create a backup of the failed migration (for debugging)
cp publisher.db publisher.db.failed
cp publisher.db-wal publisher.db-wal.failed 2>/dev/null || true
cp publisher.db-shm publisher.db-shm.failed 2>/dev/null || true

# 4. Restore from backup
cp publisher.db.backup publisher.db

# 5. Verify restored database
sqlite3 publisher.db "PRAGMA integrity_check"
sqlite3 publisher.db "SELECT version FROM schema_version"

# 6. Start the application
systemctl start deltaship-publisher
```

### Method 2: Manual Rollback SQL

If you don't have a backup, you can manually reverse the migration using SQL.

#### General Rollback Steps

1. **Identify changes** made by the migration
2. **Write reverse SQL** to undo each change
3. **Test on a copy** of the database
4. **Apply to production** database
5. **Update schema version** table

#### Example: Rolling Back a Column Addition

If migration 003 added a column:

```sql
-- Migration 003 (forward)
ALTER TABLE binaries ADD COLUMN metadata TEXT;

-- Rollback (reverse)
-- SQLite doesn't support DROP COLUMN before version 3.35.0
-- You must recreate the table without the column

BEGIN TRANSACTION;

-- Create new table without the column
CREATE TABLE binaries_new (
    binary_id TEXT PRIMARY KEY,
    binary_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    binary_path TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (binary_name, platform)
);

-- Copy data
INSERT INTO binaries_new SELECT
    binary_id, binary_name, platform, binary_path,
    description, created_at, updated_at
FROM binaries;

-- Drop old table
DROP TABLE binaries;

-- Rename new table
ALTER TABLE binaries_new RENAME TO binaries;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_publisher_binaries_name ON binaries(binary_name);

-- Update schema version
DELETE FROM schema_version WHERE version = 3;

COMMIT;
```

### Method 3: Point-in-Time Recovery

If you have WAL backups, you can perform point-in-time recovery.

```bash
# 1. Copy database and WAL files
cp publisher.db publisher.db.recovery
cp publisher.db-wal publisher.db-wal.recovery

# 2. Open in recovery mode
sqlite3 publisher.db.recovery

# In SQLite:
PRAGMA journal_mode=WAL;
PRAGMA wal_checkpoint(TRUNCATE);

# Verify
SELECT version FROM schema_version;
.quit
```

## Rollback Scenarios

### Scenario 1: Migration Failed Midway

**Symptoms:**
- Migration SQL threw an error
- Application won't start
- Schema version is inconsistent

**Solution:**
1. Check if migration ran in a transaction
2. If yes, database auto-rolled back - verify and restart
3. If no, restore from backup (Method 1)

### Scenario 2: Migration Succeeded but Application Fails

**Symptoms:**
- Migration completed successfully
- Schema version updated
- Application crashes or behaves incorrectly

**Solution:**
1. Restore from backup (Method 1)
2. Downgrade application to previous version
3. Investigate compatibility issue
4. Fix application code or migration script

### Scenario 3: Data Corruption After Migration

**Symptoms:**
- `PRAGMA integrity_check` fails
- Queries return corrupted data
- Application reports database errors

**Solution:**
1. Stop application immediately
2. Backup corrupted database for forensics
3. Restore from pre-migration backup
4. Investigate root cause (disk issue, bug in migration)

### Scenario 4: Performance Degradation After Migration

**Symptoms:**
- Queries are slower than before
- Missing indexes
- Query plans changed

**Solution:**
1. Check if migration dropped indexes
2. Run `ANALYZE` to update statistics
3. Verify indexes exist: `.indexes` in sqlite3
4. Recreate missing indexes
5. If unresolved, rollback using Method 1

## Verification After Rollback

After rolling back, verify:

### 1. Schema Version
```sql
SELECT version, description, applied_at FROM schema_version ORDER BY version DESC LIMIT 1;
```

Expected: Previous version number

### 2. Database Integrity
```bash
sqlite3 client.db "PRAGMA integrity_check"
```

Expected: `ok`

### 3. Foreign Keys
```bash
sqlite3 client.db "PRAGMA foreign_key_check"
```

Expected: No output (no violations)

### 4. Table Structure
```sql
.schema managed_binaries
```

Expected: Schema matches previous version

### 5. Data Presence
```sql
SELECT COUNT(*) FROM managed_binaries;
SELECT COUNT(*) FROM installed_versions;
```

Expected: Same row counts as before migration

### 6. Application Startup
```bash
# Test application startup
deltaship-client --help

# Check logs
journalctl -u deltaship-client -n 50
```

Expected: No errors

## Recovery from Failed Rollback

If the rollback itself fails:

1. **Stay calm** - Don't make rushed changes
2. **Stop the application** to prevent further damage
3. **Make a copy** of the current state
4. **Consult backups** - Do you have multiple backup points?
5. **Check disk space** - Ensure sufficient space for operations
6. **Review SQLite integrity** using `PRAGMA integrity_check`
7. **Attempt recovery** using `.recover` command in sqlite3:

```bash
sqlite3 client.db.failed ".recover" | sqlite3 client.db.recovered
```

8. **Verify recovered database** thoroughly
9. **Contact support** if data is critical and recovery fails

## Preventing Rollback Needs

### Best Practices

1. **Test migrations thoroughly** on copies of production data
2. **Use transactions** for all migration steps when possible
3. **Implement migration validation** before applying
4. **Monitor during migration** - watch logs and metrics
5. **Backup before every migration** - automated and verified
6. **Have rollback plan ready** before starting migration
7. **Schedule migrations** during low-traffic windows
8. **Version control** all migration scripts
9. **Document dependencies** between migrations
10. **Test rollback procedures** in staging environment

### Testing Checklist

Before applying migration to production:

- [ ] Migration tested on development database
- [ ] Migration tested on staging database (copy of production)
- [ ] Rollback procedure tested
- [ ] Application tested with new schema
- [ ] Performance tested with new schema
- [ ] Backup strategy verified
- [ ] Monitoring alerts configured
- [ ] Documentation updated
- [ ] Team notified of migration window

## Emergency Contacts

If you encounter issues during rollback:

1. Check `docs/FAQ.md` for common issues
2. Review `docs/TROUBLESHOOTING.md`
3. Search GitHub issues
4. File a new issue with:
   - Schema version before migration
   - Schema version after failed migration
   - Migration script used
   - Error messages
   - Database integrity check results
   - Steps already attempted

## Rollback History Template

Keep a log of rollbacks for auditing:

```
Date: 2026-01-14
Database: publisher.db
From Version: 3
To Version: 2
Reason: Application compatibility issue with new column
Method: Restore from backup
Duration: 5 minutes
Data Loss: None
Performed By: admin@example.com
Verified By: admin@example.com
Notes: Column added in v3 broke compatibility with v0.1.0 client
```

## Future Improvements

Planned enhancements to rollback capability:

1. **Automated rollback** - Application detects incompatibility and auto-rolls back
2. **Reversible migrations** - Each migration includes explicit down migration
3. **Migration checkpoints** - Save state before each migration step
4. **Backup automation** - Automatic verified backups before migrations
5. **Canary deployments** - Test migration on subset of data first
6. **Migration simulation** - Dry-run mode to preview changes

## Related Documents

- [DATABASE_MIGRATIONS.md](./DATABASE_MIGRATIONS.md) - Migration overview
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - General troubleshooting
- [BACKUP_RESTORE.md](./operations/BACKUP_RESTORE.md) - Backup procedures
