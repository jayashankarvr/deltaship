# Performance Targets

**Document:** Measurable performance goals and benchmarks for Deltaship
**Audience:** Engineers, architects, performance testers
**Last Updated:** 2026-01-07

---

## Overview

This document defines specific, measurable performance targets for the Deltaship system. All targets must be met for v1.0 release.

**Measurement Methodology:**
- Hardware: Standard cloud instance (4 vCPU, 8GB RAM, SSD)
- Network: Simulated 10 Mbps connection with 50ms latency
- Test suite: Reproducible benchmarks in `benchmarks/` directory

---

## Bandwidth Savings Targets

### Primary Metric: Compression Ratio

**Target:** ≥95% reduction for typical incremental updates

**Measurement:**
```
compression_ratio = 1 - (diff_size / full_binary_size)
```

**Breakdown by Update Type:**

| Update Type | Minimum | Target | Stretch Goal |
|-------------|---------|--------|--------------|
| **Bug fix** (code only, <1% changed) | 98% | 99% | 99.5% |
| **Minor version** (small features, <5% changed) | 92% | 95% | 97% |
| **Major version** (large features, <20% changed) | 75% | 85% | 90% |
| **Major refactor** (>50% changed) | 40% | 50% | 60% |

**Fallback Threshold:**
- If diff size >50% of binary, automatically fall back to full download
- Prevents sending 40MB diff when binary is 100MB (wasteful)

### Bandwidth Savings by Binary Type

**Executables (ELF, PE, Mach-O):**
- Target: 97% reduction (using Courgette for exe-aware diffing)
- Minimum: 90%

**Shared Libraries:**
- Target: 95% reduction
- Minimum: 85%

**Static Binaries:**
- Target: 90% reduction (less optimal due to no relocations)
- Minimum: 80%

**Compressed Archives (.tar.gz, .zip):**
- Target: 85% reduction (decompress → diff → recompress overhead)
- Minimum: 70%

---

## Update Speed Targets

### End-to-End Update Time

**For 100MB binary, 1MB diff:**

| Phase | Target | Maximum |
|-------|--------|---------|
| Check for update | <500ms | 1s |
| Download diff (10 Mbps) | <5s | 10s |
| Verify signature | <100ms | 500ms |
| Apply patch | <10s | 20s |
| Verify result | <500ms | 1s |
| **Total** | **<16s** | **32s** |

**Compared to full download:**
- Full binary download (10 Mbps): 80 seconds
- Deltaship update: 16 seconds
- **Speedup: 5x**

### Diff Generation Time (Server-Side)

**Target:** <5 seconds for 100MB binary pair

**Breakdown by Algorithm:**

| Algorithm | 10MB | 100MB | 1GB | 10GB |
|-----------|------|-------|-----|------|
| **bsdiff** | <1s | <5s | <60s | Not recommended |
| **Courgette** | <2s | <10s | <120s | Not recommended |
| **xdelta3** | <0.5s | <2s | <20s | <200s |

**Note:** Courgette slower but produces smaller diffs for executables

### Patch Application Time (Client-Side)

**Target:** <10 seconds for 100MB binary

**Factors affecting speed:**
- Algorithm (bspatch is slower than xdelta3)
- CPU performance (single-threaded)
- Disk I/O (SSD vs HDD)

**Breakdown:**

| Binary Size | Target (SSD) | Maximum (HDD) |
|-------------|--------------|---------------|
| 10MB | <1s | <3s |
| 100MB | <10s | <30s |
| 1GB | <120s | <300s |
| 10GB | Not supported | Not supported |

**Maximum Binary Size:** 2GB (above this, use chunking or full download)

---

## Resource Usage Targets

### Client Patcher

**CPU Usage:**
- Idle: <1%
- Checking for updates: <5%
- Downloading: <5%
- Applying patch: <50% of one core (single-threaded)

**Memory Usage:**
- Idle: <20MB
- Downloading: <50MB
- Applying patch: <binary_size + diff_size + 50MB overhead
- Maximum: 500MB (for 2GB binary update)

**Disk I/O:**
- Download write speed: Limited to network speed (10-100 MB/s)
- Patch read/write: ~50MB/s on SSD, ~10MB/s on HDD

**Network Usage:**
- Update check: <5KB per request
- Diff download: Variable (1KB to 100MB typical)
- Status report: <1KB per request

### Update Server

**Per Request:**
- check-update: <10ms CPU, <1MB RAM
- download-diff: <5ms CPU (served from CDN)
- publish/init: <50ms CPU, <10MB RAM

**Throughput:**
- check-update: 10,000 requests/second per server instance
- download-diff: 1,000 concurrent downloads per instance
- publish: 100 publishes/hour per instance

**Database Queries:**
- check-update query: <5ms (with proper indexes)
- publish query: <20ms (transactional)
- Analytics query: <100ms (read replica)

### Publisher Toolkit

**Diff Generation:**
- CPU: 100% of all cores (parallelized where possible)
- Memory: ~2x binary size (loads old and new binary)
- Time: See "Diff Generation Time" above

**Signing:**
- CPU: <1% (Ed25519 is very fast)
- Time: <100ms for signature generation

**Upload:**
- Limited by network bandwidth
- 100MB binary on 10 Mbps upload: ~80 seconds

---

## Storage Efficiency Targets

### Server Storage

**Per Version:**
- Binary: 1x size (original)
- Diffs: 0.5x size (average, assuming 5 recent versions)
- Signatures: <1MB (negligible)
- **Total per version: ~1.5x binary size**

**Storage Growth:**
- For 100 versions of 100MB binary:
  - Binaries: 100 × 100MB = 10GB
  - Diffs: 100 × 5 × 1MB = 500MB
  - **Total: ~10.5GB**

**With Retention Policy (keep last 10 versions):**
- Binaries: 10 × 100MB = 1GB
- Diffs: 10 × 5 × 1MB = 50MB
- **Total: ~1GB**

### Client Storage

**Cache:**
- Downloaded diffs: Deleted after successful application
- Backup binary: 1x size (kept for rollback, deleted after verification)
- Maximum: 2x binary size during update

**Version Database (SQLite):**
- Size: <1MB for 100 registered applications
- Growth: ~10KB per app

---

## Scalability Targets

### Small Deployment (<10,000 users)

**Infrastructure:**
- 1 server (4 vCPU, 8GB RAM)
- Local storage or single S3 bucket
- No CDN (direct serving)

**Performance:**
- Concurrent updates: 100
- Check-update requests/second: 500
- Update success rate: >99%

### Medium Deployment (10,000 - 100,000 users)

**Infrastructure:**
- 2-3 API servers (load-balanced)
- PostgreSQL database
- S3 + CDN (CloudFlare or CloudFront)

**Performance:**
- Concurrent updates: 1,000
- Check-update requests/second: 5,000
- Update success rate: >99.5%

### Large Deployment (100,000 - 1,000,000 users)

**Infrastructure:**
- 10+ API servers (auto-scaling)
- PostgreSQL cluster (primary + 2 replicas)
- Multi-region S3 + global CDN
- Redis caching layer

**Performance:**
- Concurrent updates: 10,000
- Check-update requests/second: 50,000
- Update success rate: >99.9%

### Massive Deployment (>1,000,000 users)

**Infrastructure:**
- 50+ API servers (multi-region auto-scaling)
- Distributed database (Cassandra or CockroachDB)
- Global CDN with edge caching
- Separate analytics cluster

**Performance:**
- Concurrent updates: 100,000
- Check-update requests/second: 500,000
- Update success rate: >99.95%

---

## Reliability Targets

### Update Success Rate

**Target:** >99% successful updates

**Failure Budget:**
- 1% of updates may fail
- For 100,000 updates/day: <1,000 failures allowed

**Failure Causes (acceptable):**
- Network interruption: <0.5%
- Disk space insufficient: <0.3%
- Corrupted download: <0.1% (retry succeeds)
- Other: <0.1%

### Rollback Success Rate

**Target:** 100% successful rollbacks

**Mechanism:**
- Binary backup before update
- Atomic file replacement
- Verification before deleting backup

**Maximum Rollback Time:** <5 seconds

### Uptime

**Target:** 99.9% uptime (43 minutes downtime per month)

**Availability by Component:**
- API servers: 99.95% (multi-instance redundancy)
- Database: 99.99% (replication + failover)
- CDN: 99.99% (CloudFlare/CloudFront SLA)
- **Overall: 99.9%** (weakest link: API server maintenance)

---

## Latency Targets

### API Response Times

**Percentiles:**

| Endpoint | p50 | p95 | p99 | p99.9 |
|----------|-----|-----|-----|-------|
| check-update | 50ms | 200ms | 500ms | 1s |
| download-diff | N/A | N/A | N/A | N/A (streaming) |
| report-status | 20ms | 100ms | 300ms | 500ms |
| publish/init | 100ms | 300ms | 1s | 2s |
| publish/finalize | 200ms | 500ms | 2s | 5s |

**Geographic Latency:**
- Same region: <50ms
- Cross-region (same continent): <100ms
- Cross-continent: <300ms

### CDN Cache Hit Rate

**Target:** >95% cache hit rate for diffs and binaries

**Measurement:**
```
cache_hit_rate = cache_hits / (cache_hits + cache_misses)
```

**Cold Start:** First download is cache miss (unavoidable)
**Subsequent:** All downloads should be cache hits

---

## Cost Efficiency Targets

### Bandwidth Cost per User

**Scenario:** 100MB application, monthly updates

**Traditional (full download):**
- Bandwidth per update: 100MB
- Cost (at $0.10/GB): $0.01 per user per month
- 100,000 users: $1,000/month

**Deltaship (95% reduction):**
- Bandwidth per update: 5MB
- Cost: $0.0005 per user per month
- 100,000 users: $50/month
- **Savings: $950/month (95%)**

### Compute Cost per Update

**Diff Generation (Server-Side):**
- Average time: 5 seconds on 4 vCPU instance
- Cost (AWS c5.xlarge: $0.17/hour): $0.0002 per diff
- Amortized over 100,000 users: negligible

**Patch Application (Client-Side):**
- Free (runs on user's device)

### Storage Cost per Application

**100MB application, 10 versions retained:**
- Binaries: 1GB ($0.023/month on S3)
- Diffs: 50MB ($0.001/month)
- **Total: $0.024/month per application**

**For 100 applications:** $2.40/month

---

## Performance Testing Methodology

### Benchmark Suite

**Location:** `benchmarks/` directory in repository

**Test Cases:**

1. **diff-generation-benchmark:**
   - Binary pairs of various sizes (10MB, 100MB, 1GB)
   - Different change percentages (1%, 5%, 20%, 50%)
   - Measure time and resulting diff size

2. **patch-application-benchmark:**
   - Apply diffs of various sizes
   - Measure time and memory usage
   - Test on different hardware (SSD vs HDD)

3. **api-latency-benchmark:**
   - Simulate concurrent check-update requests
   - Measure p50, p95, p99 latency
   - Test under load (100, 1000, 10000 concurrent)

4. **end-to-end-benchmark:**
   - Full update flow from check to completion
   - Measure total time
   - Test on various network speeds

### Continuous Performance Testing

**Automated:**
- Run benchmark suite on every commit
- Track performance regression
- Alert if targets not met

**Scheduled:**
- Weekly full benchmark run
- Monthly performance report
- Quarterly review and adjust targets

---

## Performance Optimization Guidelines

### For Publisher:
- Minimize changes between versions (smaller diffs)
- Avoid rearranging large blocks of data
- Use consistent build configurations

### For Server:
- Pre-compute diffs for popular version transitions
- Use CDN for all static content
- Enable HTTP/2 or HTTP/3 for faster transfers
- Cache frequently accessed data (Redis)

### For Client:
- Download diffs in background (low priority)
- Apply patches during idle time (low CPU usage)
- Verify incrementally (streaming hash computation)
- Use memory-mapped files for large binaries

---

## Performance Monitoring

### Key Metrics to Track

**Real-Time:**
- API response time (p99)
- Active concurrent updates
- Error rate
- Bandwidth usage

**Historical:**
- Average diff size over time
- Bandwidth savings trend
- Update success rate
- Storage growth

**Alerts:**
- p99 latency >1s for 5 minutes
- Error rate >2% for 10 minutes
- Bandwidth savings <80% for 1 hour
- Storage >90% of allocated capacity

---

## Next Steps

**For Developers:**
- Implement performance testing framework
- Create benchmark suite
- Set up continuous performance monitoring

**For Operations:**
- Configure performance dashboards
- Set up alerts for target violations
- Monitor trends and capacity planning

**Related Documents:**
- [Feasibility Analysis](FEASIBILITY_ANALYSIS.md) - Technical feasibility
- [System Design](../architecture/SYSTEM_DESIGN.md) - Architecture for performance
- [Operations Guide](../operations/MAINTENANCE.md) - Performance tuning

---

**End of Performance Targets**
