# Version Comparison and Ordering Specification

**Version:** 1.0
**Status:** Design Phase
**Last Updated:** 2026-01-07

**Audience:** Developers, system architects, integration engineers

---

## Overview

VBDP is a **Version-Aware** Binary Differential Update System. The core functionality depends on correctly comparing and ordering versions to:

1. **Determine if an update is available** (is `new_version` > `current_version`?)
2. **Select optimal diff path** (find shortest path from version A to B)
3. **Prevent rollback attacks** (reject `new_version` if `new_version` < `current_version`)
4. **Order versions chronologically** for display and analytics

This document specifies how VBDP compares versions across different versioning schemes.

---

## Table of Contents

- [Version Schemes](#version-schemes)
- [Semantic Versioning](#semantic-versioning)
- [Date-Based Versioning](#date-based-versioning)
- [Build Number Versioning](#build-number-versioning)
- [Custom Versioning](#custom-versioning)
- [Version Normalization](#version-normalization)
- [Comparison Algorithm](#comparison-algorithm)
- [Implementation](#implementation)
- [Edge Cases](#edge-cases)
- [Security Considerations](#security-considerations)

---

## Version Schemes

VBDP supports four primary versioning schemes:

### 1. Semantic Versioning (SemVer)

**Format:** `MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`

**Examples:**
- `1.0.0`
- `2.3.5`
- `1.0.0-alpha.1`
- `3.2.1-rc.2+build.1234`

**Specification:** https://semver.org/

**Detection:** Matches regex `^\d+\.\d+\.\d+(?:-[\w\.]+)?(?:\+[\w\.]+)?$`

---

### 2. Date-Based Versioning

**Format:** `YYYY.MM.DD[.BUILD]` or `YYYY-MM-DD[.BUILD]`

**Examples:**
- `2027.01.15`
- `2027-01-15`
- `2027.01.15.2` (second build on same day)

**Detection:** Matches regex `^\d{4}[.-]\d{2}[.-]\d{2}(?:\.\d+)?$`

**Use Case:** Daily builds, continuous deployment

---

### 3. Build Number Versioning

**Format:** `BUILD_NUMBER` (incrementing integer)

**Examples:**
- `1234`
- `5678`
- `10000`

**Detection:** Matches regex `^\d+$`

**Use Case:** CI/CD pipelines with incrementing build numbers

---

### 4. Custom Versioning

**Format:** Application-specific (requires custom comparator)

**Examples:**
- `v1.0.0` (prefix)
- `release-2023-11-15`
- `1.0.0.2023.11.15` (hybrid)

**Detection:** Anything not matching above patterns

**Comparison:** Uses lexicographic (string) comparison by default, or custom comparison function

---

## Semantic Versioning

### SemVer Format

```
MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]
```

**Components:**
- **MAJOR**: Incompatible API changes (e.g., 1.x.x → 2.x.x)
- **MINOR**: Backwards-compatible functionality (e.g., 1.1.x → 1.2.x)
- **PATCH**: Backwards-compatible bug fixes (e.g., 1.0.1 → 1.0.2)
- **PRERELEASE**: Optional pre-release identifier (e.g., `-alpha.1`, `-rc.2`)
- **BUILD**: Optional build metadata (e.g., `+build.1234`, `+sha.abc123`)

### Comparison Rules

**Precedence:**
1. Compare MAJOR (numerically)
2. If equal, compare MINOR (numerically)
3. If equal, compare PATCH (numerically)
4. If equal, compare PRERELEASE (see below)
5. BUILD metadata is **IGNORED** for comparison

**Examples:**
```
1.0.0 < 2.0.0        (MAJOR differs)
1.2.0 < 1.3.0        (MINOR differs)
1.0.1 < 1.0.2        (PATCH differs)
1.0.0-alpha < 1.0.0  (prerelease < release)
1.0.0+build.1 == 1.0.0+build.2  (build metadata ignored)
```

### Prerelease Comparison

**Rules:**
1. **Release > Prerelease**: `1.0.0` > `1.0.0-alpha`
2. **Prerelease comparison**: Split by `.`, compare each segment
3. **Numeric segments**: Compare numerically
4. **Alphanumeric segments**: Compare lexicographically (ASCII)
5. **Fewer segments < more segments** (if otherwise equal)

**Examples:**
```
1.0.0-alpha < 1.0.0-alpha.1 < 1.0.0-alpha.beta < 1.0.0-beta
1.0.0-beta.2 < 1.0.0-beta.11  (numeric comparison)
1.0.0-rc.1 < 1.0.0
```

### Normalization

**Canonical form:** `MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`

**Variations accepted:**
- `v1.0.0` → `1.0.0` (strip leading `v`)
- `1.0` → `1.0.0` (append missing `.0`)
- `1` → `1.0.0`

### Conversion to Integer

For database sorting, convert to 64-bit integer:

```
version_number = (MAJOR * 1,000,000,000) + (MINOR * 1,000,000) + PATCH
```

**Examples:**
- `1.0.0` → `1000000000`
- `2.3.5` → `2003005000`
- `10.15.20` → `10015020000`

**Limitations:**
- MAJOR < 1000
- MINOR < 1000
- PATCH < 1000
- Prerelease versions: Store as separate flag or use negative offset

**Database schema:**
```sql
-- In versions table
version_number BIGINT, -- For sorting
is_prerelease BOOLEAN,
prerelease_string TEXT
```

---

## Date-Based Versioning

### Format Options

**Dot-separated:** `YYYY.MM.DD[.BUILD]`
```
2027.01.15
2027.01.15.1
```

**Dash-separated:** `YYYY-MM-DD[.BUILD]`
```
2027-01-15
2027-01-15.2
```

### Comparison Rules

1. **Compare year** (numerically)
2. **Compare month** (numerically)
3. **Compare day** (numerically)
4. **Compare build number** (numerically, if present)

**Examples:**
```
2027.01.14 < 2027.01.15
2027.01.15 < 2027.02.01
2027.01.15.1 < 2027.01.15.2
```

### Normalization

**Canonical form:** `YYYY.MM.DD[.BUILD]`

**Variations accepted:**
- `2027-01-15` → `2027.01.15` (replace `-` with `.`)
- `2027.1.5` → `2027.01.05` (zero-pad month/day)

### Conversion to Integer

```
version_number = (YEAR * 100000000) + (MONTH * 1000000) + (DAY * 10000) + BUILD
```

**Examples:**
- `2027.01.15` → `202701150000`
- `2027.01.15.2` → `202701150002`

**Range:** Years 0-99999 supported

---

## Build Number Versioning

### Format

**Simple integer:** `1234`, `5678`, `10000`

### Comparison Rules

**Numeric comparison:** `1234 < 5678 < 10000`

**No special rules:** Direct integer comparison

### Conversion to Integer

**Direct storage:** `version_number = BUILD_NUMBER`

**Range:** 0 to 9,223,372,036,854,775,807 (64-bit signed integer)

### Use Case

**CI/CD pipelines:**
```bash
# Jenkins, GitLab CI, GitHub Actions
BUILD_NUMBER=$(git rev-list --count HEAD)
vbdp-register --version "$BUILD_NUMBER" --binary ./myapp
```

**Monotonicity:** Guaranteed if build system increments properly

---

## Custom Versioning

### When to Use

Use custom versioning when:
1. Application uses non-standard versioning scheme
2. Hybrid versioning (e.g., `v1.0.0-build.2023.11.15`)
3. Need special comparison logic

### Custom Comparator

**Publisher specifies comparison function:**

```toml
# .vbdp/config.toml
[version]
scheme = "custom"
comparator = "scripts/compare_versions.sh"
```

**Comparator interface:**
```bash
#!/bin/bash
# compare_versions.sh
# Exit codes:
#   0: version1 == version2
#   1: version1 < version2
#   2: version1 > version2

version1="$1"
version2="$2"

# Custom comparison logic here
```

**Example (Git SHA-based versioning):**
```bash
# Get commit date for each version
date1=$(git log -1 --format=%ct "$version1")
date2=$(git log -1 --format=%ct "$version2")

if [ "$date1" -eq "$date2" ]; then
    exit 0  # Equal
elif [ "$date1" -lt "$date2" ]; then
    exit 1  # version1 < version2
else
    exit 2  # version1 > version2
fi
```

### Lexicographic Comparison (Default)

If no custom comparator provided, use **lexicographic (string) comparison**:

```
"1.0.0" < "2.0.0"
"release-2023" < "release-2024"
"v1.0.0" < "v1.1.0"
```

**Warning:** May produce unexpected results:
```
"10.0.0" < "2.0.0"  # Lexicographic: "1" < "2"
```

---

## Version Normalization

### Normalization Process

Before comparison or storage, versions are normalized:

1. **Detect scheme** (SemVer, date, build number, custom)
2. **Strip prefix** (remove leading `v`, `V`, `release-`, etc.)
3. **Canonicalize format** (standardize separators)
4. **Compute version_number** (for database sorting)
5. **Store original string** (for display)

### Example

**Input:** `v1.2.3-rc.1+build.456`

**Normalized:**
```
scheme: "semver"
major: 1
minor: 2
patch: 3
prerelease: "rc.1"
build: "build.456"
version_number: 1002003000
is_prerelease: true
canonical: "1.2.3-rc.1+build.456"
```

### Auto-Detection Algorithm

```rust
fn detect_scheme(version_string: &str) -> VersionScheme {
    let v = version_string.trim_start_matches(['v', 'V', 'r']);

    // Check SemVer
    if SEMVER_REGEX.is_match(v) {
        return VersionScheme::SemVer;
    }

    // Check date-based
    if DATE_REGEX.is_match(v) {
        return VersionScheme::DateBased;
    }

    // Check build number
    if BUILD_NUMBER_REGEX.is_match(v) {
        return VersionScheme::BuildNumber;
    }

    // Default to custom
    VersionScheme::Custom
}
```

---

## Comparison Algorithm

### Version Comparison Function

**Signature:**
```rust
fn compare_versions(v1: &Version, v2: &Version) -> Ordering {
    // Returns: Less, Equal, or Greater
}
```

**Algorithm:**

```rust
impl Version {
    pub fn compare(&self, other: &Self) -> Ordering {
        // 1. If schemes differ, use version_number
        if self.scheme != other.scheme {
            return self.version_number.cmp(&other.version_number);
        }

        // 2. Scheme-specific comparison
        match self.scheme {
            VersionScheme::SemVer => self.compare_semver(other),
            VersionScheme::DateBased => self.compare_date(other),
            VersionScheme::BuildNumber => self.version_number.cmp(&other.version_number),
            VersionScheme::Custom => self.compare_custom(other),
        }
    }

    fn compare_semver(&self, other: &Self) -> Ordering {
        // Compare MAJOR.MINOR.PATCH
        match self.version_number.cmp(&other.version_number) {
            Ordering::Equal => {
                // If base versions equal, compare prerelease
                self.compare_prerelease(other)
            }
            ord => ord,
        }
    }

    fn compare_prerelease(&self, other: &Self) -> Ordering {
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,  // Prerelease < release
            (None, Some(_)) => Ordering::Greater,
            (Some(pre1), Some(pre2)) => {
                // Compare prerelease identifiers segment by segment
                compare_prerelease_segments(pre1, pre2)
            }
        }
    }
}
```

### Prerelease Segment Comparison

```rust
fn compare_prerelease_segments(pre1: &str, pre2: &str) -> Ordering {
    let segments1: Vec<&str> = pre1.split('.').collect();
    let segments2: Vec<&str> = pre2.split('.').collect();

    for (seg1, seg2) in segments1.iter().zip(segments2.iter()) {
        // Try to parse as numbers
        let ord = match (seg1.parse::<u64>(), seg2.parse::<u64>()) {
            (Ok(n1), Ok(n2)) => n1.cmp(&n2),  // Both numeric: compare numerically
            (Ok(_), Err(_)) => Ordering::Less,  // Numeric < alphanumeric
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => seg1.cmp(seg2),  // Both alphanumeric: lexicographic
        };

        if ord != Ordering::Equal {
            return ord;
        }
    }

    // All segments equal: fewer segments < more segments
    segments1.len().cmp(&segments2.len())
}
```

**Examples:**
```rust
compare("1.0.0-alpha.1", "1.0.0-alpha.2")    // Less (1 < 2)
compare("1.0.0-alpha", "1.0.0-beta")         // Less (alpha < beta)
compare("1.0.0-rc.1", "1.0.0")               // Less (prerelease < release)
compare("1.0.0+build.1", "1.0.0+build.999")  // Equal (build ignored)
```

---

## Implementation

### Rust Implementation

**Data structure:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Original version string as provided
    pub original: String,

    /// Detected versioning scheme
    pub scheme: VersionScheme,

    /// Normalized canonical representation
    pub canonical: String,

    /// Integer for sorting (scheme-dependent)
    pub version_number: i64,

    /// SemVer-specific fields
    pub major: Option<u32>,
    pub minor: Option<u32>,
    pub patch: Option<u32>,
    pub prerelease: Option<String>,
    pub build: Option<String>,

    /// Date-based specific fields
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub build_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionScheme {
    SemVer,
    DateBased,
    BuildNumber,
    Custom,
}

impl Version {
    /// Parse version string and detect scheme
    pub fn parse(version_str: &str) -> Result<Self, VersionError> {
        // Implementation
    }

    /// Compare this version to another
    pub fn compare(&self, other: &Self) -> Ordering {
        // Implementation (see above)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.compare(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.compare(other)
    }
}
```

**Usage:**
```rust
let v1 = Version::parse("1.2.3")?;
let v2 = Version::parse("1.3.0")?;

assert!(v1 < v2);

// Sorting
let mut versions = vec![
    Version::parse("2.0.0")?,
    Version::parse("1.0.0")?,
    Version::parse("1.5.0")?,
];
versions.sort();
// [1.0.0, 1.5.0, 2.0.0]
```

### API Endpoints

**Register version:**
```http
POST /api/v1/publishers/{publisher_id}/binaries/{binary_id}/versions
Content-Type: application/json

{
  "version_string": "1.2.3",
  "file_size_bytes": 52428800,
  "file_hash_blake3": "a3c5...",
  "signature_ed25519": "b7f2..."
}
```

**Server response:**
```json
{
  "version_id": "uuid-here",
  "version_string": "1.2.3",
  "canonical": "1.2.3",
  "scheme": "semver",
  "version_number": 1002003000,
  "is_prerelease": false
}
```

**Client update check:**
```http
GET /api/v1/updates?binary_id={id}&current_version=1.0.0
```

**Server logic:**
```rust
// Find latest version > current_version
let current = Version::parse(&current_version)?;
let latest = db.get_latest_published_version(binary_id).await?;

if latest > current {
    // Update available
    return UpdateResponse::Available {
        from_version: current,
        to_version: latest,
        diff_id: find_optimal_diff(current, latest),
    };
} else {
    // No update
    return UpdateResponse::UpToDate;
}
```

---

## Edge Cases

### Mixed Versioning Schemes

**Problem:** Binary switches from SemVer to date-based

**Example:**
```
1.0.0 → 1.1.0 → 2024.11.15 → 2024.11.16
```

**Solution:** Use `version_number` for cross-scheme comparison

**Recommendation:** Don't change versioning schemes mid-lifecycle

---

### Non-Monotonic Versions

**Problem:** Versions not strictly increasing

**Example:**
```
1.0.0 → 2.0.0 → 1.5.0 (rollback or parallel release)
```

**Solution:**
- **Prevent:** Reject registration if `new_version` <= `latest_version`
- **Allow:** Use timestamp to determine "latest" (not recommended)

**VBDP stance:** **Prevent** by default, configurable override for advanced use cases

---

### Version Collisions

**Problem:** Two different binaries with same version string

**Example:**
- Binary A: `1.0.0` (size: 50MB, hash: abc123)
- Binary B: `1.0.0` (size: 60MB, hash: def456)

**Solution:** Version strings unique per binary (database constraint)

```sql
CONSTRAINT unique_version_per_binary UNIQUE (binary_id, version_string)
```

---

### Prerelease Ordering

**Complex example:**
```
1.0.0-alpha
1.0.0-alpha.1
1.0.0-alpha.beta
1.0.0-beta
1.0.0-beta.2
1.0.0-beta.11
1.0.0-rc.1
1.0.0
```

**All should be ordered correctly** according to SemVer spec.

---

### Very Long Version Strings

**Problem:** Custom versions can be arbitrarily long

**Example:** `release-2023-11-15-bugfix-auth-retry-logic-v2`

**Solution:**
- Database: `version_string VARCHAR(255)` (limit to 255 chars)
- Display: Truncate with ellipsis if needed
- Comparison: Lexicographic (or custom comparator)

---

## Security Considerations

### Rollback Attack Prevention

**Attack:** Malicious server serves old signed version with known vulnerability

**Mitigation:**
1. **Client checks:** `new_version` > `current_version` (monotonicity)
2. **Reject downgrades:** Even if signature valid
3. **Exception:** Authorized rollback (signed rollback command)

**Implementation:**
```rust
fn is_update_allowed(current: &Version, new: &Version, rollback_policy: &RollbackPolicy) -> bool {
    if new > current {
        return true;  // Normal update
    }

    if new < current && rollback_policy.rollback_enabled {
        // Check if rollback is authorized
        return verify_rollback_authorization(current, new);
    }

    false  // Reject downgrade
}
```

### Version Confusion Attacks

**Attack:** Trick client into accepting malicious version through comparison bug

**Examples:**
- Overflow: `999999999.0.0` → integer overflow → negative number
- Parsing bug: `1.0.0\x00malicious` → null byte injection

**Mitigation:**
1. **Strict parsing:** Reject malformed versions
2. **Bounds checking:** Limit MAJOR/MINOR/PATCH to reasonable ranges
3. **Validation:** Regex + length limits
4. **Fuzzing:** Test parser with malicious inputs

**Validation:**
```rust
fn validate_version_string(v: &str) -> Result<(), VersionError> {
    // Length check
    if v.len() > 255 {
        return Err(VersionError::TooLong);
    }

    // No null bytes
    if v.contains('\0') {
        return Err(VersionError::NullByte);
    }

    // No control characters
    if v.chars().any(|c| c.is_control()) {
        return Err(VersionError::ControlCharacter);
    }

    // Scheme-specific validation
    // ...

    Ok(())
}
```

### Timestamp-Based Fallback

**Risk:** If version comparison fails, using timestamps for ordering

**Problem:** Timestamp can be manipulated by publisher

**Mitigation:**
- **Don't use timestamps** for version ordering
- **Only use version_number** or explicit comparison

---

## Summary

VBDP version comparison:

1. **Supports multiple schemes**: SemVer, date-based, build number, custom
2. **Auto-detects** versioning scheme from version string
3. **Normalizes** versions for consistent comparison
4. **Stores `version_number`** for efficient database sorting
5. **Implements SemVer spec** correctly (including prerelease)
6. **Prevents rollback attacks** through monotonicity checks
7. **Handles edge cases** (mixed schemes, collisions, long strings)

**Next Steps:**
1. Implement `Version` struct in Rust
2. Add comprehensive unit tests (especially SemVer prerelease)
3. Fuzz test parser with malicious inputs
4. Document publisher best practices for version selection
5. Add version scheme documentation to publisher toolkit

---

**References:**
- [Semantic Versioning 2.0.0](https://semver.org/)
- [Rust `semver` crate](https://crates.io/crates/semver)
- [Calendar Versioning](https://calver.org/)
- [OWASP: Input Validation](https://cheatsheetseries.owasp.org/cheatsheets/Input_Validation_Cheat_Sheet.html)
