# Storage Cost Model and Retention Policies

**Version:** 1.0
**Status:** Design Phase
**Last Updated:** 2026-01-07

**Audience:** System architects, financial planners, infrastructure engineers

---

## Overview

This document provides a **comprehensive storage cost model** for VBDP deployments, addressing the critical issue identified in the original analysis: **Storage costs grow significantly over time and can exceed initial estimates by 10-100x**.

**Key insight:** Naive "store all diffs forever" approach is economically unsustainable for active projects.

---

## Table of Contents

- [Problem Statement](#problem-statement)
- [Storage Components](#storage-components)
- [Cost Breakdown](#cost-breakdown)
- [Growth Projections](#growth-projections)
- [Retention Policies](#retention-policies)
- [Cost Optimization Strategies](#cost-optimization-strategies)
- [Real-World Examples](#real-world-examples)
- [Implementation](#implementation)

---

## Problem Statement

### Naive Storage Model (Unsustainable)

**Assumptions:**
- Binary size: 100MB
- Release frequency: Weekly (52 releases/year)
- Users: 10,000
- Naive policy: Store ALL diffs forever

**Year 1:**
```
Versions: 52
Diffs (n-1 to n): 51 diffs × 5MB = 255MB
Total forward diffs: 255MB
Reverse diffs (optional): 255MB
Full binaries: 52 × 100MB = 5.2GB
Total: ~5.5GB
```

**Year 5:**
```
Versions: 260
Diffs: 259 diffs × 5MB = 1.3GB (forward only)
But wait... this is wrong!
```

### The Real Problem: O(n²) Growth

**Reality:** Need diffs between **MANY version pairs**, not just sequential:

```
Version 1.0 → All future versions (1.1, 1.2, ..., 5.0): 260 diffs
Version 1.1 → All future versions (1.2, ..., 5.0): 259 diffs
Version 1.2 → All future versions (1.3, ..., 5.0): 258 diffs
...
Total diffs needed: 260 + 259 + 258 + ... + 1 = 33,670 diffs
```

**If each diff is 5MB:**
```
33,670 diffs × 5MB = 168GB (forward only)
× 2 (reverse diffs) = 336GB
Plus binaries: 260 × 100MB = 26GB
Total: ~362GB for one binary!
```

**At $0.023/GB/month (S3 Standard):**
```
Year 5 cost: 362GB × $0.023 = $8.33/month for one binary
For 100 binaries: $833/month = $10,000/year
```

**This grows quadratically!**

---

## Storage Components

### 1. Full Binary Versions

**What:** Complete binary files for each version

**Size:** `binary_size × number_of_versions`

**Example:** 100MB binary, 260 versions = 26GB

**Growth:** Linear O(n)

**Purpose:**
- Source for diff computation
- Fallback for full downloads
- Verification reference

---

### 2. Forward Diffs

**What:** Diffs from version A → B (A < B)

**Size:** Depends on changes, typically 1-10% of binary size

**Growth:** Quadratic O(n²) if store all pairs, Linear O(n) if store only sequential

**Purpose:**
- Enable incremental updates
- Bandwidth savings

---

### 3. Reverse Diffs (Optional)

**What:** Diffs from version B → A (B > A, for rollbacks)

**Size:** Similar to forward diffs

**Growth:** Quadratic O(n²) or Linear O(n)

**Purpose:**
- Enable rollbacks without full download
- Publisher-initiated downgrades

---

### 4. Database Metadata

**What:** PostgreSQL/SQLite data (versions, diffs, logs)

**Size:** Relatively small (MB-GB range)

**Growth:** Linear O(n), slow

**Purpose:**
- Track versions, diffs, downloads
- Analytics and monitoring

---

### 5. Logs and Analytics

**What:** Request logs, download logs, error logs

**Size:** Grows with user base and activity

**Growth:** Linear with users × requests

**Purpose:**
- Debugging
- Analytics
- Security auditing

---

## Cost Breakdown

### AWS S3 Pricing (2026 estimates)

| Storage Class | Price (GB/month) | Use Case |
|---------------|------------------|----------|
| **S3 Standard** | $0.023 | Frequently accessed (recent versions) |
| **S3 Intelligent-Tiering** | $0.023 + $0.0025 (monitoring) | Auto-optimize access patterns |
| **S3 Infrequent Access (IA)** | $0.0125 | Older versions (monthly access) |
| **S3 Glacier Instant Retrieval** | $0.004 | Archive (quarterly access) |
| **S3 Glacier Flexible Retrieval** | $0.0036 | Long-term archive (retrieval: 1-12h) |
| **S3 Glacier Deep Archive** | $0.00099 | Compliance/audit (retrieval: 12-48h) |

**Retrieval costs (additional):**
- Standard/IA: Free retrieval
- Glacier Instant: $0.03/GB
- Glacier Flexible: $0.01/GB (standard retrieval, 3-5h)
- Glacier Deep: $0.0025/GB (standard retrieval, 12h)

**Transfer costs:**
- Inbound: Free
- Outbound (to internet): $0.09/GB (first 10TB/month), then $0.085/GB

### CloudFlare R2 Pricing (alternative, cheaper)

| Feature | Price |
|---------|-------|
| **Storage** | $0.015/GB/month |
| **Class A operations** (write) | $4.50 per million requests |
| **Class B operations** (read) | $0.36 per million requests |
| **Egress** | **FREE** (major advantage) |

**For VBDP:** R2 may be more cost-effective due to free egress (downloads)

### CDN Costs (CloudFlare, Fastly, CloudFront)

**CloudFlare:**
- Free tier: 100GB/month bandwidth
- Pro: $20/month + $0.04/GB overage
- Business: $200/month + $0.02/GB overage

**AWS CloudFront:**
- $0.085/GB (first 10TB/month)
- $0.080/GB (next 40TB)
- Decreases to $0.020/GB at 5PB+

### Database Costs

**PostgreSQL (managed - AWS RDS):**
- db.t3.micro (1vCPU, 1GB RAM): $13/month
- db.t3.small (2vCPU, 2GB RAM): $26/month
- db.m5.large (2vCPU, 8GB RAM): $138/month
- Storage: $0.115/GB/month (SSD)

**PostgreSQL (self-hosted):**
- EC2 instance: $10-100/month depending on size
- EBS storage: $0.08-0.10/GB/month

---

## Growth Projections

### Scenario 1: Small Project

**Assumptions:**
- Binary size: 50MB
- Release frequency: Monthly (12/year)
- Active support: 2 years back
- Users: 1,000
- Strategy: Store sequential diffs only + last 24 versions

**Year 1:**
```
Versions: 12
Binaries: 12 × 50MB = 600MB
Forward diffs (11 sequential): 11 × 2.5MB = 28MB
Total: 628MB × $0.023 = $0.014/month = $0.17/year
```

**Year 5:**
```
Versions retained: 24 (last 2 years)
Binaries: 24 × 50MB = 1.2GB
Diffs: 23 × 2.5MB = 58MB
Total: 1.26GB × $0.023 = $0.029/month = $0.35/year
```

**Conclusion:** Negligible cost, sustainable

---

### Scenario 2: Medium Project

**Assumptions:**
- Binary size: 200MB
- Release frequency: Bi-weekly (26/year)
- Active support: 1 year back
- Users: 50,000
- Strategy: Store multi-hop diffs (1→2, 1→3, 2→3, etc.) for last year

**Year 1:**
```
Versions: 26
Binaries: 26 × 200MB = 5.2GB
Diffs (sequential): 25 × 10MB = 250MB
Diffs (multi-hop, optimized): ~2GB
Total: ~7.5GB × $0.023 = $0.17/month = $2.04/year
```

**Year 5:**
```
Versions retained: 26 (last year)
Binaries: 26 × 200MB = 5.2GB
Diffs: ~2GB
Total: ~7.5GB (steady state) × $0.023 = $0.17/month = $2.04/year
```

**Conclusion:** Moderate cost, sustainable with retention policy

---

### Scenario 3: Large Project (Naive "Store Everything")

**Assumptions:**
- Binary size: 500MB
- Release frequency: Weekly (52/year)
- Active support: ALL versions forever (BAD IDEA)
- Users: 1,000,000
- Strategy: Store all version pairs (O(n²))

**Year 1:**
```
Versions: 52
Binaries: 52 × 500MB = 26GB
All-pairs diffs: 52×51/2 = 1,326 diffs × 25MB = 33GB
Total: 59GB × $0.023 = $1.36/month = $16.32/year
```

**Year 5:**
```
Versions: 260
Binaries: 260 × 500MB = 130GB
All-pairs diffs: 260×259/2 = 33,670 diffs × 25MB = 842GB
Total: 972GB × $0.023 = $22.36/month = $268/year
```

**Year 10:**
```
Versions: 520
Binaries: 520 × 500MB = 260GB
All-pairs diffs: 520×519/2 = 134,940 diffs × 25MB = 3,374GB
Total: 3,634GB × $0.023 = $83.58/month = $1,003/year
```

**Conclusion:** UNSUSTAINABLE! Costs grow quadratically.

---

### Scenario 4: Large Project (Smart Retention)

**Assumptions:**
- Binary size: 500MB
- Release frequency: Weekly (52/year)
- Active support: Last 26 versions (6 months)
- Archive: Older versions in Glacier
- Users: 1,000,000
- Strategy: Sequential diffs + selective multi-hop for active versions

**Year 1:**
```
Active versions: 26 (last 6 months)
  Binaries: 26 × 500MB = 13GB (S3 Standard)
  Diffs: 25 sequential + 20 multi-hop = 45 × 25MB = 1.1GB
  Subtotal: 14.1GB × $0.023 = $0.32/month

Archived versions: 26 (older than 6 months)
  Binaries: 26 × 500MB = 13GB (Glacier Deep Archive)
  No diffs (recompute on-demand if needed)
  Subtotal: 13GB × $0.00099 = $0.013/month

Total: $0.33/month = $4.00/year
```

**Year 5:**
```
Active versions: 26 (last 6 months)
  14.1GB × $0.023 = $0.32/month

Archived versions: 234 (older)
  234 × 500MB = 117GB × $0.00099 = $0.116/month

Total: $0.44/month = $5.28/year
```

**Year 10:**
```
Active versions: 26
  $0.32/month

Archived versions: 494
  247GB × $0.00099 = $0.24/month

Total: $0.56/month = $6.72/year
```

**Conclusion:** Sustainable! Linear growth with smart retention.

---

## Retention Policies

### Policy 1: Time-Based Retention (Recommended)

**Keep:**
- **Recent versions** (last 6-12 months): Full binaries + diffs (S3 Standard)
- **Older versions** (1-3 years): Binaries only, archive diffs (S3 IA)
- **Ancient versions** (>3 years): Binaries only (Glacier Deep Archive)

**Configuration:**
```toml
[storage.retention]
policy = "time_based"

[storage.retention.recent]
duration_months = 6
storage_class = "S3_STANDARD"
keep_binaries = true
keep_diffs = true
diff_strategy = "sequential_and_multi_hop"

[storage.retention.older]
duration_months = 36  # 1-3 years
storage_class = "S3_IA"
keep_binaries = true
keep_diffs = false  # Recompute on-demand

[storage.retention.archive]
min_age_months = 36  # >3 years
storage_class = "GLACIER_DEEP_ARCHIVE"
keep_binaries = true
keep_diffs = false
```

---

### Policy 2: Version-Count Retention

**Keep:**
- **Last N versions**: Full support (binaries + diffs)
- **Older versions**: Binaries only, no diffs

**Configuration:**
```toml
[storage.retention]
policy = "version_count"
keep_last_n_versions = 50
keep_binaries_beyond_n = true  # Archive older binaries
keep_diffs_beyond_n = false
```

---

### Policy 3: User-Activity Based Retention

**Keep:**
- **Actively used versions** (>1% of users): Full support
- **Rarely used versions** (<0.1% of users): Archive

**Configuration:**
```toml
[storage.retention]
policy = "usage_based"
active_threshold_percent = 1.0  # >1% of users
archive_threshold_percent = 0.1  # <0.1%
check_interval_days = 7
```

**Implementation:**
```sql
-- Find versions used by <0.1% of users
SELECT v.version_id, v.version_string, COUNT(DISTINCT u.client_id) as user_count
FROM versions v
LEFT JOIN update_requests u ON u.current_version_id = v.version_id
WHERE v.created_at < NOW() - INTERVAL '6 months'
GROUP BY v.version_id
HAVING COUNT(DISTINCT u.client_id) < (SELECT COUNT(DISTINCT client_id) * 0.001 FROM update_requests);
```

---

### Policy 4: Hybrid (Recommended for Production)

**Combines time-based + usage-based:**

```toml
[storage.retention]
policy = "hybrid"

# Always keep recent versions
always_keep_months = 6

# After 6 months, decision based on usage
archive_if_users_below = "1%"
delete_diffs_if_users_below = "0.1%"

# Hard limits
max_versions_with_diffs = 100
max_total_storage_gb = 1000
```

---

## Cost Optimization Strategies

### Strategy 1: Tiered Storage

**Move data through lifecycle:**

```
Recent (0-6 months):     S3 Standard        $0.023/GB/month
Moderate (6-12 months):  S3 IA              $0.0125/GB/month
Old (1-3 years):         Glacier Instant    $0.004/GB/month
Archive (>3 years):      Glacier Deep       $0.00099/GB/month
```

**S3 Lifecycle Policy:**
```json
{
  "Rules": [{
    "Id": "VBDP-Lifecycle",
    "Status": "Enabled",
    "Transitions": [
      {
        "Days": 180,
        "StorageClass": "STANDARD_IA"
      },
      {
        "Days": 365,
        "StorageClass": "GLACIER_INSTANT_RETRIEVAL"
      },
      {
        "Days": 1095,
        "StorageClass": "DEEP_ARCHIVE"
      }
    ]
  }]
}
```

**Savings:** 60-95% for older data

---

### Strategy 2: Selective Diff Computation

**Don't pre-compute all diffs:**

**Compute eagerly:**
- Sequential diffs (v1→v2, v2→v3): Always
- Recent multi-hop (v1→v3, v2→v4): Yes
- Long-range recent (v1→v10): Yes

**Compute on-demand:**
- Old-to-new diffs (v1.0.0 → v5.0.0): Rare, compute when requested
- Reverse diffs: Only if requested

**Implementation:**
```rust
fn should_precompute_diff(from: &Version, to: &Version) -> bool {
    let age_from = from.age_days();
    let age_to = to.age_days();

    // Always: Sequential diffs
    if to.version_number == from.version_number + 1 {
        return true;
    }

    // Recent multi-hop (both versions <6 months old)
    if age_from < 180 && age_to < 180 {
        return true;
    }

    // Don't precompute old→new or old→old
    false
}
```

**Savings:** 50-90% on diff storage

---

### Strategy 3: Compression and Deduplication

**Compress diffs:**
- Use zstd compression (3-5x size reduction)
- Store compressed diffs

**Deduplicate chunks:**
- If using FastCDC, deduplicate chunk storage
- Shared chunks referenced by multiple diffs

**Example:**
```
Diff A→B: [chunk1, chunk2, chunk3] = 10MB
Diff A→C: [chunk1, chunk2, chunk4] = 10MB

With deduplication:
chunk1: 3MB
chunk2: 3MB
chunk3: 2MB
chunk4: 2MB
Total: 10MB (instead of 20MB)
```

**Savings:** 20-50%

---

### Strategy 4: CDN Offloading

**Use CDN for downloads:**
- Origin: S3
- CDN: CloudFlare (free egress) or CloudFront (cached)

**Cost comparison (1TB downloads/month):**

**Without CDN:**
```
S3 egress: 1,000GB × $0.09 = $90/month
```

**With CloudFlare CDN:**
```
S3 → CloudFlare: $0 (intra-region) or $0.02/GB
CloudFlare → users: $0 (free)
Cache hit rate: 95%
Actual S3 egress: 50GB × $0.09 = $4.50/month
Total: $4.50/month
```

**Savings:** 95% on bandwidth costs

---

### Strategy 5: CloudFlare R2 (Zero Egress)

**Switch from S3 to R2:**

**S3 costs (1TB downloads/month, 100GB storage):**
```
Storage: 100GB × $0.023 = $2.30/month
Egress: 1,000GB × $0.09 = $90/month
Total: $92.30/month
```

**R2 costs:**
```
Storage: 100GB × $0.015 = $1.50/month
Egress: $0 (FREE!)
Total: $1.50/month
```

**Savings:** 98% total cost reduction for high-bandwidth scenarios

---

### Strategy 6: Database Optimization

**Partition large tables:**
```sql
-- Partition update_requests by month
CREATE TABLE update_requests_2027_01 PARTITION OF update_requests
    FOR VALUES FROM ('2027-01-01') TO ('2027-02-01');
```

**Archive old logs:**
- Move logs older than 90 days to S3
- Compress as Parquet for analytics

**Savings:** 50-80% on database costs

---

## Real-World Examples

### Example 1: Electron App (VSCode-like)

**Profile:**
- Binary size: 150MB (macOS), 120MB (Windows), 140MB (Linux)
- Release frequency: Monthly stable, weekly insiders
- Users: 500,000
- Retention: Last 12 stable versions, last 4 insiders

**Cost estimate (with smart retention):**

```
Stable versions: 12 × 410MB (all platforms) = 4.9GB
  Diffs: 11 sequential × 20MB = 220MB
Insiders: 4 × 410MB = 1.6GB
  Diffs: 3 × 20MB = 60MB

Total active: 6.8GB × $0.023 = $0.16/month

Archived (old stable): Assume 100 versions archived
  100 × 410MB = 41GB × $0.00099 = $0.041/month

Total: ~$0.20/month = $2.40/year
```

**With 500k users downloading monthly:**
```
Avg download per user: 20MB (diff, not full 150MB)
Total bandwidth: 500,000 × 20MB = 10TB
With CloudFlare CDN (95% cache hit): 0.5TB from S3
Cost: 500GB × $0.09 = $45/month = $540/year

Total cost: $540 (bandwidth) + $2.40 (storage) = $542/year
```

**Without VBDP (full downloads):**
```
500,000 × 150MB = 75TB
With CDN (80% cache): 15TB from S3
Cost: 15,000GB × $0.09 = $1,350/month = $16,200/year
```

**Savings:** $15,658/year (96.7% reduction)

---

### Example 2: Game Client (100MB monthly updates)

**Profile:**
- Binary size: 5GB
- Monthly major updates (12/year) + weekly hotfixes (52/year)
- Users: 2 million
- Typical update: 100MB changed (2%)

**Cost estimate:**

```
Active versions: 12 major (last year) = 12 × 5GB = 60GB
Diffs: ~100MB average per update = 1.2GB
Total: ~61GB × $0.023 = $1.40/month

Archived: Older versions in Glacier
Cost estimate: ~$0.50/month

Total storage: $1.90/month = $23/year
```

**Bandwidth (2M users, monthly):**
```
Avg diff download: 100MB
Total: 2,000,000 × 100MB = 200TB/month
With R2 (zero egress):
  Storage: 61GB × $0.015 = $0.92/month
  Egress: $0
Total cost: ~$11/month = $132/year
```

**Without VBDP:**
```
2M × 5GB = 10PB/month
Even with CDN, impractical cost (>$100k/month)
```

**Savings:** >99.9%

---

## Implementation

### Server Configuration

```toml
# /etc/vbdp/server.toml

[storage]
provider = "s3"  # or "r2", "gcs", "azure"
bucket = "vbdp-binaries"
region = "us-east-1"

[storage.lifecycle]
enabled = true
policy = "hybrid"

# Recent versions (full support)
[storage.lifecycle.recent]
duration_days = 180
storage_class = "STANDARD"
keep_diffs = true
diff_strategies = ["sequential", "multi_hop_limited"]
max_multi_hop_distance = 10

# Older versions (reduced support)
[storage.lifecycle.older]
min_age_days = 180
max_age_days = 1095
storage_class = "STANDARD_IA"
keep_binaries = true
keep_diffs = false
on_demand_diff_computation = true

# Archive (long-term retention)
[storage.lifecycle.archive]
min_age_days = 1095
storage_class = "GLACIER_DEEP_ARCHIVE"
keep_binaries = true
keep_diffs = false

[storage.costs]
# Cost monitoring and alerting
max_monthly_cost_usd = 1000
alert_threshold_percent = 80

[storage.optimization]
compress_diffs = true
compress_level = 3  # zstd
enable_deduplication = true
```

### Monitoring and Alerts

```yaml
# Prometheus alerts for storage costs

- alert: StorageCostHigh
  expr: vbdp_storage_cost_usd > 800
  for: 24h
  labels:
    severity: warning
  annotations:
    summary: "VBDP storage costs approaching limit"
    description: "Current: ${{ $value }}, Limit: $1000"

- alert: StorageGrowthRapid
  expr: rate(vbdp_storage_total_gb[7d]) > 10
  for: 24h
  labels:
    severity: warning
  annotations:
    summary: "Storage growing >10GB/day"
```

---

## Summary

**Key Takeaways:**

1. **Naive "store everything" is unsustainable**: O(n²) growth bankrupts project
2. **Smart retention policies are critical**: Linear growth with time/version limits
3. **Tiered storage saves 60-95%**: Move old data to Glacier
4. **CDN + R2 saves 95-99% on bandwidth**: Free egress is game-changer
5. **Selective diff computation saves 50-90%**: Don't precompute rare diffs
6. **Monitoring prevents cost explosions**: Set alerts and limits

**Recommended approach:**
- Recent versions (6 months): Full support, S3 Standard
- Older versions (1-3 years): Binaries only, S3 IA
- Archive (>3 years): Binaries only, Glacier Deep Archive
- On-demand diff computation for old versions
- CloudFlare R2 for storage (zero egress cost)
- CDN for global distribution

**Result:** Sustainable costs even for large-scale deployments

---

**Next Steps:**
1. Implement lifecycle policies in server
2. Add cost monitoring and alerting
3. Test storage migration (S3 Standard → IA → Glacier)
4. Benchmark on-demand diff computation performance
5. Document publisher cost optimization best practices

---

**References:**
- [AWS S3 Pricing](https://aws.amazon.com/s3/pricing/)
- [CloudFlare R2 Pricing](https://developers.cloudflare.com/r2/pricing/)
- [S3 Lifecycle Management](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lifecycle-mgmt.html)
