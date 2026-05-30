# VBDP Database Migrations

This directory contains SQL migration files for VBDP databases.

## Current Migrations

| Migration | Description | Applies To |
|-----------|-------------|------------|
| 001_initial_client_schema.sql | Initial client database schema | Client DB |
| 002_initial_publisher_schema.sql | Initial publisher database schema | Publisher DB |

## Migration Files

### 001_initial_client_schema.sql

Creates the base schema for the VBDP Client database including:

- `schema_version` - Migration tracking
- `client_config` - Client configuration
- `managed_binaries` - Binaries managed by the client
- `installed_versions` - Version history
- `update_history` - Update attempt tracking
- `rollback_backups` - Rollback capability

**Schema Version**: 1

### 002_initial_publisher_schema.sql

Creates the base schema for the VBDP Publisher database including:

- `schema_version` - Migration tracking
- `publisher_config` - Publisher configuration
- `binaries` - Binaries being published
- `versions` - Version tracking
- `diff_jobs` - Diff computation jobs

**Schema Version**: 1

## Using Migration Files

### Automatic (Application)

The VBDP application automatically creates the schema when initializing databases:

```rust
use vbdp_db::ClientDb;

let db = ClientDb::open(&db_path).await?;
db.init().await?; // Creates schema automatically
```

### Manual (SQLite CLI)

To manually create a database using migration files:

```bash
# Client database
sqlite3 client.db < migrations/001_initial_client_schema.sql

# Publisher database
sqlite3 publisher.db < migrations/002_initial_publisher_schema.sql
```

## Migration Strategy

VBDP v0.1.0 uses **inline migrations** where the schema is created programmatically in the `init()` functions. These SQL files serve as:

1. **Documentation** of the schema structure
2. **Reference** for manual database creation
3. **Foundation** for future migration tooling

Future versions (v0.2.0+) will support automatic migrations using these files.

## Creating New Migrations

When adding schema changes:

1. Create new migration file: `00X_description.sql`
2. Update schema version constant in code
3. Update `init()` function to include changes
4. Document the migration in this README
5. Test on a database copy

See [docs/DATABASE_MIGRATIONS.md](../docs/DATABASE_MIGRATIONS.md) for detailed migration guidelines.

## Documentation

- [DATABASE_MIGRATIONS.md](../docs/DATABASE_MIGRATIONS.md) - Complete migration guide
- [MIGRATION_ROLLBACK.md](../docs/MIGRATION_ROLLBACK.md) - Rollback procedures
- [Schema documentation](../docs/technical/) - Detailed schema reference

## Schema Versioning

Both databases track their schema version:

```sql
SELECT version, description, applied_at FROM schema_version;
```

Current versions:
- Client DB: v1
- Publisher DB: v1
