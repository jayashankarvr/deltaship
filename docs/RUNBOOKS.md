# Incident Response Runbooks

**Document:** Operational runbooks for VBDP incident response
**Audience:** On-call engineers, SREs, operations teams
**Last Updated:** 2026-01-14

> **NOTE:** Email addresses on this page use the RFC 2606 `example.com` domain and are placeholders. Replace with real contacts before publishing.

---

## Overview

This document provides step-by-step procedures for responding to common VBDP incidents. Each runbook includes diagnosis steps, resolution procedures, and prevention strategies.

**How to Use These Runbooks:**
1. Identify the alert or symptom
2. Follow the diagnosis steps to confirm the issue
3. Execute the resolution procedure
4. Verify the fix
5. Document the incident
6. Implement prevention measures

**Critical Contacts:**
- **On-Call Engineer:** PagerDuty rotation
- **Technical Lead:** maintainers@vbdp.example.com
- **Security Team:** security@vbdp.example.com
- **Infrastructure:** infrastructure@vbdp.example.com

---

## Table of Contents

### Critical Incidents
1. [Server Down](#runbook-1-server-down)
2. [Database Down](#runbook-2-database-down)
3. [High Error Rate](#runbook-3-high-error-rate)
4. [Out of Memory](#runbook-4-out-of-memory)
5. [Disk Full](#runbook-5-disk-full)

### High Priority Incidents
6. [High API Latency](#runbook-6-high-api-latency)
7. [Low Update Success Rate](#runbook-7-low-update-success-rate)
8. [Database Connection Pool Exhausted](#runbook-8-database-connection-pool-exhausted)
9. [Storage Failures](#runbook-9-storage-failures)
10. [High CPU Usage](#runbook-10-high-cpu-usage)

### Medium Priority Incidents
11. [Disk Space Warning](#runbook-11-disk-space-warning)
12. [Low Bandwidth Savings](#runbook-12-low-bandwidth-savings)
13. [Slow Database Queries](#runbook-13-slow-database-queries)
14. [Slow Diff Generation](#runbook-14-slow-diff-generation)

### Security Incidents
15. [High Signature Verification Failures](#runbook-15-high-signature-verification-failures)
16. [Unusual API Key Usage](#runbook-16-unusual-api-key-usage)

### Operational Procedures
17. [Certificate Renewal](#runbook-17-certificate-renewal)
18. [Emergency Rollback](#runbook-18-emergency-rollback)
19. [Scaling Operations](#runbook-19-scaling-operations)

---

## Runbook 1: Server Down

**Alert:** `VBDPServerDown`
**Severity:** Critical
**Impact:** All update operations fail, clients cannot download updates
**Expected Response Time:** Immediate (< 5 minutes)

### Symptoms
- Health check endpoint unreachable
- Prometheus shows `up{job="vbdp-server"} == 0`
- Client update checks fail with connection errors

### Diagnosis

1. **Check if server process is running:**
   ```bash
   systemctl status vbdp-server
   ```
   Expected: "active (running)"

2. **Check recent logs for crash:**
   ```bash
   journalctl -u vbdp-server -n 100 --no-pager
   ```
   Look for: panic, segfault, OOM killer, fatal errors

3. **Check system resources:**
   ```bash
   free -h          # Memory
   df -h            # Disk space
   uptime           # Load average
   ```

4. **Check network connectivity:**
   ```bash
   curl http://localhost:8080/health
   ```

### Resolution

**If process crashed:**

1. **Start the service:**
   ```bash
   sudo systemctl start vbdp-server
   ```

2. **Verify startup:**
   ```bash
   systemctl status vbdp-server
   journalctl -u vbdp-server -f
   ```

3. **Test health endpoint:**
   ```bash
   curl http://localhost:8080/health
   curl https://updates.example.com/health
   ```

**If process is running but not responding:**

1. **Check for deadlock (get stack trace):**
   ```bash
   # Send SIGQUIT to get stack trace
   sudo kill -QUIT $(pgrep vbdp-server)
   journalctl -u vbdp-server -n 200
   ```

2. **If unresponsive, force restart:**
   ```bash
   sudo systemctl restart vbdp-server
   ```

**If process won't start:**

1. **Check configuration:**
   ```bash
   vbdp-server --config /etc/vbdp-server/config.toml --check-config
   ```

2. **Check file permissions:**
   ```bash
   ls -la /etc/vbdp-server/config.toml
   ls -la /var/lib/vbdp/
   ```

3. **Fix permissions if needed:**
   ```bash
   sudo chown -R vbdp:vbdp /var/lib/vbdp/
   sudo chmod 600 /etc/vbdp-server/config.toml
   ```

4. **Try starting manually for debugging:**
   ```bash
   sudo -u vbdp /usr/local/bin/vbdp-server --config /etc/vbdp-server/config.toml
   ```

### Verification

1. **Health check returns 200 OK:**
   ```bash
   curl -I https://updates.example.com/health
   ```

2. **Metrics endpoint accessible:**
   ```bash
   curl https://updates.example.com/metrics | head -20
   ```

3. **Test update check:**
   ```bash
   curl "https://updates.example.com/api/check-update?app=test&version=1.0.0"
   ```

4. **Monitor error rate for 5 minutes:**
   Check Grafana dashboard - error rate should return to normal (<1%)

### Prevention

1. **Enable automatic restart on failure:**
   ```ini
   # In /etc/systemd/system/vbdp-server.service
   [Service]
   Restart=on-failure
   RestartSec=5s
   ```

2. **Set up resource limits:**
   ```ini
   [Service]
   MemoryMax=2G
   TasksMax=512
   ```

3. **Configure watchdog:**
   Enable health check monitoring with automatic restart

4. **Review logs for recurring issues:**
   Set up log aggregation and analysis

### Escalation

- **After 15 minutes:** Escalate to technical lead
- **After 30 minutes:** Consider emergency failover to backup server
- **If data corruption suspected:** Contact database team before restart

---

## Runbook 2: Database Down

**Alert:** `VBDPDatabaseDown`
**Severity:** Critical
**Impact:** Cannot serve updates, read-only mode may be possible
**Expected Response Time:** Immediate (< 5 minutes)

### Symptoms
- Database connection errors in logs
- `pg_up{job="postgres"} == 0`
- API returns 500 errors for database operations

### Diagnosis

1. **Check database process:**
   ```bash
   systemctl status postgresql
   # or for managed database:
   # Check cloud provider console
   ```

2. **Test connection from VBDP server:**
   ```bash
   psql -h db-host -U vbdp_server -d vbdp -c "SELECT 1;"
   ```

3. **Check database logs:**
   ```bash
   sudo tail -100 /var/log/postgresql/postgresql-14-main.log
   ```

4. **Check network connectivity:**
   ```bash
   ping db-host
   telnet db-host 5432
   ```

### Resolution

**If database process is down:**

1. **Start PostgreSQL:**
   ```bash
   sudo systemctl start postgresql
   ```

2. **Check status:**
   ```bash
   systemctl status postgresql
   ```

3. **Verify database integrity:**
   ```bash
   psql -U postgres -c "SELECT datname, pg_database_size(datname) FROM pg_database;"
   ```

**If database is out of connections:**

1. **Check active connections:**
   ```sql
   SELECT count(*) FROM pg_stat_activity;
   SELECT state, count(*) FROM pg_stat_activity GROUP BY state;
   ```

2. **Kill idle connections if needed:**
   ```sql
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE datname = 'vbdp'
     AND state = 'idle'
     AND state_change < NOW() - INTERVAL '30 minutes';
   ```

3. **Increase max_connections (temporary):**
   ```sql
   ALTER SYSTEM SET max_connections = 200;
   SELECT pg_reload_conf();
   ```

**If database credentials invalid:**

1. **Verify credentials:**
   ```bash
   cat /etc/vbdp-server/config.toml | grep -A 5 "\[database\]"
   ```

2. **Test credentials manually:**
   ```bash
   psql -h DB_HOST -U DB_USER -d DB_NAME
   ```

3. **Reset password if needed:**
   ```sql
   ALTER USER vbdp_server WITH PASSWORD 'new_password';
   ```

4. **Update VBDP configuration:**
   ```bash
   sudo nano /etc/vbdp-server/config.toml
   sudo systemctl restart vbdp-server
   ```

### Verification

1. **Database accepting connections:**
   ```bash
   psql -h db-host -U vbdp_server -d vbdp -c "SELECT version();"
   ```

2. **VBDP server connected:**
   ```bash
   journalctl -u vbdp-server -n 50 | grep -i database
   ```
   Should see: "Database connected successfully"

3. **Test API operations:**
   ```bash
   curl "https://updates.example.com/api/check-update?app=test&version=1.0.0"
   ```

### Prevention

1. **Enable database high availability:**
   - Set up read replicas
   - Configure automatic failover
   - Use managed database with built-in HA

2. **Monitor connection pool:**
   - Alert on connection pool >80% full
   - Tune connection pool size based on load

3. **Regular backups:**
   - Automated daily backups
   - Test restore procedure monthly

4. **Database maintenance:**
   - Regular VACUUM operations
   - Index maintenance
   - Statistics updates

---

## Runbook 3: High Error Rate

**Alert:** `VBDPHighErrorRate`
**Severity:** Critical
**Impact:** Users experiencing frequent failures
**Expected Response Time:** < 10 minutes

### Symptoms
- Error rate > 5% for 10 minutes
- Increased 5xx status codes
- Client complaints about failed updates

### Diagnosis

1. **Check error breakdown by endpoint:**
   ```promql
   sum by (endpoint, status) (rate(vbdp_api_requests_total{status=~"5.."}[5m]))
   ```

2. **Review recent logs:**
   ```bash
   journalctl -u vbdp-server --since "10 minutes ago" | grep -i error
   ```

3. **Check recent deployments:**
   ```bash
   # Check last deployment time
   systemctl show vbdp-server --property=ActiveEnterTimestamp
   ```

4. **Check dependencies:**
   ```bash
   # Database
   psql -h db-host -U vbdp_server -d vbdp -c "SELECT 1;"

   # Storage
   aws s3 ls s3://vbdp-updates-prod/
   ```

### Resolution

**If caused by database issues:**

1. Follow [Runbook 2: Database Down](#runbook-2-database-down)

**If caused by storage issues:**

1. Follow [Runbook 9: Storage Failures](#runbook-9-storage-failures)

**If caused by recent deployment:**

1. **Rollback to previous version:**
   ```bash
   # Stop current version
   sudo systemctl stop vbdp-server

   # Restore previous binary
   sudo cp /usr/local/bin/vbdp-server.backup /usr/local/bin/vbdp-server

   # Start service
   sudo systemctl start vbdp-server
   ```

2. **Notify team:**
   Post in incident channel about rollback

**If caused by specific endpoint:**

1. **Identify problematic endpoint:**
   Check metrics and logs

2. **Temporary mitigation:**
   - Rate limit the endpoint
   - Add circuit breaker
   - Return cached response if possible

3. **Fix and redeploy:**
   - Create hotfix
   - Deploy with gradual rollout

### Verification

1. **Error rate returns to normal (<1%):**
   Check Grafana dashboard

2. **Test affected endpoints:**
   ```bash
   curl -v "https://updates.example.com/api/check-update?app=test&version=1.0.0"
   ```

3. **Monitor for 15 minutes:**
   Ensure error rate stays low

### Post-Incident

1. **Root cause analysis:**
   - What caused the errors?
   - Why didn't we catch it in testing?
   - What monitoring could detect this earlier?

2. **Write incident report:**
   Document timeline, impact, resolution

3. **Implement improvements:**
   - Add better error handling
   - Improve testing coverage
   - Add specific monitoring

---

## Runbook 4: Out of Memory

**Alert:** `VBDPServerOutOfMemory`
**Severity:** Critical
**Impact:** Server will be killed by OOM killer, causing downtime
**Expected Response Time:** < 5 minutes

### Symptoms
- Memory usage > 95%
- OOM killer messages in logs
- Server becoming unresponsive

### Diagnosis

1. **Check current memory usage:**
   ```bash
   free -h
   ```

2. **Identify memory-heavy processes:**
   ```bash
   ps aux --sort=-%mem | head -20
   ```

3. **Check VBDP server memory usage:**
   ```bash
   ps -o pid,user,%mem,rss,cmd -C vbdp-server
   ```

4. **Look for memory leaks in logs:**
   ```bash
   journalctl -u vbdp-server | grep -i "memory\|allocation\|leak"
   ```

### Resolution

**Immediate mitigation:**

1. **Restart VBDP server to free memory:**
   ```bash
   sudo systemctl restart vbdp-server
   ```

2. **Monitor memory after restart:**
   ```bash
   watch -n 1 'free -h'
   ```

**If memory keeps growing:**

1. **Reduce worker count:**
   ```toml
   # In /etc/vbdp-server/config.toml
   [server]
   workers = 2  # Reduce from 4 to 2
   ```

2. **Restart with new config:**
   ```bash
   sudo systemctl restart vbdp-server
   ```

3. **Enable swap (temporary):**
   ```bash
   # Only if not already enabled
   sudo fallocate -l 4G /swapfile
   sudo chmod 600 /swapfile
   sudo mkswap /swapfile
   sudo swapon /swapfile
   ```

**If problem persists:**

1. **Scale vertically:**
   - Increase server memory (2x current)
   - Update instance type if cloud

2. **Scale horizontally:**
   - Add more servers
   - Distribute load via load balancer

### Verification

1. **Memory usage stabilized:**
   ```bash
   free -h
   # Should show <80% usage
   ```

2. **Server responding normally:**
   ```bash
   curl https://updates.example.com/health
   ```

3. **Monitor for memory leaks:**
   Watch memory usage over next few hours

### Prevention

1. **Set memory limits:**
   ```ini
   # In /etc/systemd/system/vbdp-server.service
   [Service]
   MemoryMax=1.5G
   MemoryHigh=1.2G
   ```

2. **Enable memory profiling:**
   - Profile application in staging
   - Identify memory-heavy operations
   - Optimize or limit them

3. **Regular restarts (if leak exists):**
   - Schedule weekly restart during low traffic
   - This is a band-aid, not a fix

4. **Fix memory leaks:**
   - Profile in development
   - Add memory regression tests
   - Review connection handling

---

## Runbook 5: Disk Full

**Alert:** `VBDPDiskCriticallyFull`
**Severity:** Critical
**Impact:** Cannot store new updates, server may crash
**Expected Response Time:** < 5 minutes

### Symptoms
- Disk usage > 95%
- Write operations failing
- "No space left on device" errors

### Diagnosis

1. **Check disk usage:**
   ```bash
   df -h /var/lib/vbdp
   ```

2. **Find largest directories:**
   ```bash
   du -sh /var/lib/vbdp/* | sort -h | tail -10
   ```

3. **Find large files:**
   ```bash
   find /var/lib/vbdp -type f -size +100M -exec ls -lh {} \; | sort -k5 -h
   ```

4. **Check for deleted but open files:**
   ```bash
   lsof +L1 | grep deleted
   ```

### Resolution

**Immediate cleanup:**

1. **Remove old logs:**
   ```bash
   # Find old logs
   find /var/log/vbdp -name "*.log.*" -mtime +7

   # Delete them
   find /var/log/vbdp -name "*.log.*" -mtime +7 -delete
   ```

2. **Clean temporary files:**
   ```bash
   rm -rf /var/lib/vbdp/tmp/*
   ```

3. **Remove old version diffs (be careful!):**
   ```bash
   # List versions to identify old ones
   ls -lht /var/lib/vbdp/storage/diffs/

   # Remove diffs older than 90 days
   find /var/lib/vbdp/storage/diffs/ -mtime +90 -type f -delete
   ```

**If using local storage:**

1. **Archive old binaries to S3:**
   ```bash
   # Move old versions to archive storage
   aws s3 sync /var/lib/vbdp/storage/binaries/ s3://vbdp-archive/ \
     --exclude "*" --include "*/v1.*"

   # Delete local copies after verification
   rm -rf /var/lib/vbdp/storage/binaries/v1.*
   ```

2. **Verify space freed:**
   ```bash
   df -h /var/lib/vbdp
   ```

**If still low on space:**

1. **Expand disk volume:**
   ```bash
   # Cloud provider specific
   # AWS: Modify EBS volume size in console
   # Then resize filesystem
   sudo resize2fs /dev/xvdf
   ```

2. **Or add additional volume:**
   ```bash
   # Mount new volume
   sudo mount /dev/xvdg /var/lib/vbdp/storage-new

   # Update configuration
   sudo nano /etc/vbdp-server/config.toml
   # Change storage.root_path

   # Migrate data
   sudo rsync -av /var/lib/vbdp/storage/ /var/lib/vbdp/storage-new/

   # Restart server
   sudo systemctl restart vbdp-server
   ```

### Verification

1. **Disk usage below 80%:**
   ```bash
   df -h /var/lib/vbdp
   ```

2. **Write operations work:**
   ```bash
   touch /var/lib/vbdp/test && rm /var/lib/vbdp/test
   ```

3. **Server functioning:**
   ```bash
   curl https://updates.example.com/health
   ```

### Prevention

1. **Implement storage lifecycle policy:**
   - Automatically archive old versions
   - Delete diffs after 90 days
   - Move to cheaper storage tier

2. **Set up monitoring:**
   - Alert at 80% disk usage
   - Track disk growth rate
   - Project when disk will be full

3. **Regular cleanup automation:**
   ```bash
   # Add to cron: /etc/cron.daily/vbdp-cleanup
   #!/bin/bash
   find /var/log/vbdp -name "*.log.*" -mtime +7 -delete
   find /var/lib/vbdp/tmp -mtime +1 -delete
   ```

4. **Use object storage:**
   - Migrate to S3/Azure Blob
   - Unlimited capacity
   - Pay-per-use pricing

---

## Runbook 6: High API Latency

**Alert:** `VBDPHighAPILatency`
**Severity:** High
**Impact:** Slow update downloads, poor user experience
**Expected Response Time:** < 15 minutes

### Symptoms
- API p99 latency > 5 seconds
- Clients timing out
- Slow response times in Grafana

### Diagnosis

1. **Identify slow endpoints:**
   ```promql
   histogram_quantile(0.99,
     sum by (endpoint, le) (rate(vbdp_api_request_duration_seconds_bucket[5m]))
   )
   ```

2. **Check database query performance:**
   ```sql
   SELECT query, mean_exec_time, calls
   FROM pg_stat_statements
   ORDER BY mean_exec_time DESC
   LIMIT 10;
   ```

3. **Check system load:**
   ```bash
   uptime
   top
   iostat -x 1 5
   ```

4. **Check network latency:**
   ```bash
   # To database
   ping -c 10 db-host

   # To storage
   time aws s3 ls s3://vbdp-updates-prod/ | head -10
   ```

### Resolution

**If database is slow:**

1. **Check for long-running queries:**
   ```sql
   SELECT pid, now() - query_start AS duration, query
   FROM pg_stat_activity
   WHERE state = 'active'
     AND now() - query_start > interval '1 minute';
   ```

2. **Kill slow queries if needed:**
   ```sql
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE state = 'active'
     AND now() - query_start > interval '5 minutes';
   ```

3. **Add missing indexes:**
   ```sql
   -- Analyze query plans
   EXPLAIN ANALYZE <slow_query>;

   -- Add indexes as needed
   CREATE INDEX CONCURRENTLY idx_name ON table(column);
   ```

**If storage is slow:**

1. **Check S3/storage status:**
   - Visit AWS Service Health Dashboard
   - Check for regional issues

2. **Enable CDN caching:**
   - Ensure CDN is caching downloads
   - Purge and refresh CDN cache

3. **Switch to replica/backup storage:**
   - Temporarily use different region
   - Or fallback storage location

**If server is overloaded:**

1. **Scale horizontally:**
   - Add more server instances
   - Update load balancer

2. **Enable caching:**
   ```toml
   # In /etc/vbdp-server/config.toml
   [cache]
   type = "redis"
   enabled = true
   ttl_seconds = 3600
   ```

3. **Increase worker count:**
   ```toml
   [server]
   workers = 8  # Match CPU cores
   ```

### Verification

1. **Latency improved:**
   ```promql
   histogram_quantile(0.99,
     rate(vbdp_api_request_duration_seconds_bucket[5m])
   ) < 1
   ```

2. **Test API response time:**
   ```bash
   time curl "https://updates.example.com/api/check-update?app=test&version=1.0.0"
   ```

3. **Monitor for 30 minutes:**
   Ensure latency stays low

### Prevention

1. **Database optimization:**
   - Regular VACUUM and ANALYZE
   - Connection pooling
   - Read replicas for analytics

2. **Implement caching:**
   - Redis for metadata
   - CDN for downloads
   - Edge caching

3. **Load testing:**
   - Regular performance testing
   - Identify bottlenecks proactively
   - Capacity planning

---

## Runbook 7: Low Update Success Rate

**Alert:** `VBDPLowUpdateSuccessRate`
**Severity:** High
**Impact:** Users not receiving updates successfully
**Expected Response Time:** < 30 minutes

### Symptoms
- Success rate < 99% for 30 minutes
- Increased update failures
- Client error reports

### Diagnosis

1. **Check failure breakdown:**
   ```promql
   sum by (reason) (rate(vbdp_updates_total{success="false"}[5m]))
   ```

2. **Review client error logs:**
   ```bash
   # If you have centralized logging
   grep "update failed" /var/log/vbdp/client-*.log
   ```

3. **Check signature verification:**
   ```promql
   sum(rate(vbdp_signature_verification_failures_total[5m]))
   ```

4. **Check network issues:**
   - CDN availability
   - Regional outages
   - Bandwidth throttling

### Resolution

**If signature verification failing:**

1. **Check publisher signed correctly:**
   - Verify latest version signatures
   - Check signing key matches

2. **Check client public key:**
   - Ensure clients have correct public key
   - Verify key distribution

3. **Temporarily disable signature check (EMERGENCY ONLY):**
   ```toml
   # ONLY in dire emergency
   [security]
   require_signature = false
   ```
   Note: Re-enable ASAP after fixing root cause

**If network/download failures:**

1. **Check CDN status:**
   - CloudFlare/CDN provider dashboard
   - Purge CDN cache if stale

2. **Enable retry mechanism:**
   - Ensure clients retry failed downloads
   - Implement exponential backoff

3. **Use alternate storage:**
   - Configure fallback storage location
   - Replicate to multiple regions

**If diff application failing:**

1. **Check diff quality:**
   - Verify diffs are valid
   - Test locally: `vbdp-test --from X --to Y`

2. **Fallback to full binary:**
   - Temporarily disable differential updates
   - Or increase full binary fallback threshold

### Verification

1. **Success rate > 99%:**
   ```promql
   sum(rate(vbdp_updates_total{success="true"}[5m])) /
   sum(rate(vbdp_updates_total[5m])) > 0.99
   ```

2. **Test end-to-end update:**
   - Install test client
   - Trigger update
   - Verify successful completion

3. **Monitor client feedback:**
   - Check support channels
   - Review error reports

### Prevention

1. **Better error reporting:**
   - Detailed error codes
   - Client-side logging
   - Centralized error aggregation

2. **Pre-release testing:**
   - Test updates before wide release
   - Gradual rollout (10% → 25% → 50% → 100%)
   - Canary deployments

3. **Fallback mechanisms:**
   - Automatic retry with backoff
   - Fallback to full binary
   - Multiple download sources

---

## Runbook 15: High Signature Verification Failures

**Alert:** `VBDPHighSignatureVerificationFailures`
**Severity:** High (Potential Security Issue)
**Impact:** Updates not being applied, possible attack
**Expected Response Time:** < 15 minutes

### Symptoms
- Many signature verification failures
- Clients rejecting updates
- Possible security incident

### Diagnosis

1. **Check failure rate:**
   ```promql
   sum(rate(vbdp_signature_verification_failures_total[5m]))
   ```

2. **Check recent publishes:**
   ```bash
   # Check last published version
   curl "https://updates.example.com/api/versions?app=myapp&limit=5"
   ```

3. **Verify signatures manually:**
   ```bash
   # Download binary and signature
   curl -O "https://updates.example.com/api/download-binary/myapp/1.2.0"
   curl -O "https://updates.example.com/api/signature/myapp/1.2.0"

   # Verify with public key
   vbdp-verify --binary myapp-1.2.0 --signature sig --public-key public.key
   ```

4. **Check for MITM attack:**
   - Compare checksums from multiple locations
   - Verify TLS certificates
   - Check for DNS hijacking

### Resolution

**If publisher signed incorrectly:**

1. **Notify publisher immediately:**
   - Contact via secure channel
   - Request re-signing with correct key

2. **Pause rollout:**
   ```bash
   # Temporarily disable version
   curl -X POST "https://updates.example.com/api/admin/pause-version" \
     -H "Authorization: Bearer ADMIN_API_KEY" \
     -d '{"app": "myapp", "version": "1.2.0"}'
   ```

3. **Publisher re-signs and re-publishes:**
   ```bash
   vbdp-sign --version 1.2.0
   vbdp-publish --version 1.2.0 --force
   ```

**If clients have wrong public key:**

1. **Identify affected clients:**
   - Check client versions
   - Identify distribution of wrong key

2. **Distribute updated client:**
   - Push client update with correct key
   - Use out-of-band distribution if needed

**If potential security incident:**

1. **STOP IMMEDIATELY:**
   - Pause all update distributions
   - Notify security team

2. **Investigate thoroughly:**
   - Check for unauthorized access
   - Review audit logs
   - Verify binary integrity

3. **Follow security incident procedure:**
   - See [SECURITY.md](../SECURITY.md)
   - Contact security@vbdp.example.com

### Verification

1. **Signature verification working:**
   ```bash
   vbdp-verify --binary test.bin --signature test.sig --public-key public.key
   ```

2. **Clients accepting updates:**
   ```bash
   # Monitor success rate
   watch -n 5 'curl -s "http://prometheus:9090/api/v1/query?query=vbdp_updates_total"'
   ```

3. **No security indicators:**
   - No unauthorized access
   - Binaries unchanged
   - Keys not compromised

### Prevention

1. **Automated signature verification:**
   - CI/CD pipeline verifies signatures
   - Block publish if signature invalid

2. **Key management:**
   - Hardware security modules (HSM)
   - Key rotation schedule
   - Backup key recovery

3. **Security monitoring:**
   - Alert on verification failures
   - Log all publish operations
   - Audit trail for key usage

---

## Runbook 17: Certificate Renewal

**Alert:** `VBDPCertificateExpiringSoon`
**Severity:** Low (but becomes critical if ignored)
**Impact:** TLS certificate expires, clients cannot connect
**Expected Response Time:** Within business hours

### Symptoms
- Certificate expires in < 30 days
- Certificate monitoring alerts

### Diagnosis

1. **Check certificate expiry:**
   ```bash
   echo | openssl s_client -connect updates.example.com:443 2>/dev/null | \
     openssl x509 -noout -dates
   ```

2. **Check auto-renewal status:**
   ```bash
   # For Let's Encrypt
   sudo certbot certificates
   ```

### Resolution

**If using Let's Encrypt:**

1. **Renew certificate:**
   ```bash
   sudo certbot renew --nginx
   ```

2. **Test renewal process:**
   ```bash
   sudo certbot renew --dry-run
   ```

3. **Check auto-renewal cron:**
   ```bash
   cat /etc/cron.d/certbot
   ```
   Should have: `0 */12 * * * root certbot renew --quiet`

**If using commercial certificate:**

1. **Purchase/generate new certificate:**
   - Through certificate provider
   - Generate CSR if needed

2. **Install new certificate:**
   ```bash
   sudo cp new-cert.pem /etc/nginx/ssl/
   sudo cp new-key.pem /etc/nginx/ssl/
   ```

3. **Update Nginx configuration:**
   ```bash
   sudo nginx -t
   sudo systemctl reload nginx
   ```

### Verification

1. **Certificate valid and future-dated:**
   ```bash
   echo | openssl s_client -connect updates.example.com:443 2>/dev/null | \
     openssl x509 -noout -dates
   ```

2. **No browser warnings:**
   - Visit https://updates.example.com in browser
   - Check for certificate warnings

3. **SSL Labs check:**
   ```bash
   # Visit: https://www.ssllabs.com/ssltest/analyze.html?d=updates.example.com
   ```
   Should get A or A+ rating

### Prevention

1. **Enable auto-renewal:**
   - Let's Encrypt: automatic with certbot
   - Commercial: calendar reminder 60 days before expiry

2. **Monitoring:**
   - Alert 30 days before expiry
   - Secondary alert 14 days before
   - Critical alert 7 days before

3. **Testing:**
   - Test renewal process quarterly
   - Document renewal procedure

---

## Runbook 18: Emergency Rollback

**Scenario:** Bad version published, need to revert immediately
**Severity:** Variable (critical if causing outages)
**Expected Response Time:** < 15 minutes

### Symptoms
- Reports of broken application after update
- High error rate after new publish
- Critical bug in latest version

### Procedure

**Step 1: Pause Current Version**

1. **Stop rollout immediately:**
   ```bash
   curl -X POST "https://updates.example.com/api/admin/pause-version" \
     -H "Authorization: Bearer ADMIN_API_KEY" \
     -d '{
       "app": "myapp",
       "version": "1.2.0",
       "reason": "Critical bug - emergency rollback"
     }'
   ```

2. **Verify paused:**
   ```bash
   curl "https://updates.example.com/api/versions?app=myapp&active_only=true"
   ```

**Step 2: Identify Rollback Target**

1. **List recent versions:**
   ```bash
   curl "https://updates.example.com/api/versions?app=myapp&limit=10"
   ```

2. **Choose last known good version:**
   - Usually previous version (e.g., 1.1.0)
   - Verify it's stable

**Step 3: Initiate Rollback**

1. **Set rollback version:**
   ```bash
   curl -X POST "https://updates.example.com/api/admin/rollback" \
     -H "Authorization: Bearer ADMIN_API_KEY" \
     -d '{
       "app": "myapp",
       "from_version": "1.2.0",
       "to_version": "1.1.0",
       "notify_users": true
     }'
   ```

2. **Generate reverse diff if needed:**
   ```bash
   # Publisher side
   vbdp-register --version 1.2.0-rollback --binary ./v1.1.0/myapp
   vbdp-sign --version 1.2.0-rollback
   vbdp-publish --version 1.2.0-rollback --rollback-from 1.2.0
   ```

**Step 4: Communication**

1. **Notify stakeholders:**
   - Internal: Post in incident channel
   - External: Status page update
   - Users: In-app notification if possible

2. **Provide guidance:**
   - How to rollback manually if needed
   - ETA for fix
   - Impact assessment

**Step 5: Monitor Rollback**

1. **Track rollback progress:**
   ```promql
   sum(vbdp_updates_total{app="myapp", to_version="1.1.0"})
   ```

2. **Monitor for issues:**
   - Error rate during rollback
   - Success rate of downgrades
   - User feedback

### Verification

1. **Users on safe version:**
   ```promql
   sum by (version) (vbdp_client_version{app="myapp"})
   ```

2. **No new updates to bad version:**
   ```bash
   # Verify 1.2.0 is paused
   curl "https://updates.example.com/api/check-update?app=myapp&version=1.1.0"
   ```
   Should not return 1.2.0

3. **Error rate returned to normal:**
   Check Grafana dashboard

### Post-Rollback

1. **Root cause analysis:**
   - What broke?
   - Why didn't testing catch it?
   - How to prevent in future?

2. **Fix the bug:**
   - Create hotfix branch
   - Fix issue
   - Thorough testing

3. **Re-release:**
   - Version 1.2.1 with fix
   - Gradual rollout
   - Monitor closely

---

## Emergency Contacts

### Primary Contacts
- **On-Call Engineer:** PagerDuty rotation (see schedule)
- **Technical Lead:** lead@vbdp.example.com
- **Operations Manager:** ops@vbdp.example.com

### Escalation Path
1. **On-Call Engineer** (responds within 15 minutes)
2. **Technical Lead** (responds within 30 minutes)
3. **Operations Manager** (responds within 1 hour)
4. **CTO/VP Engineering** (critical only)

### Specialized Teams
- **Security:** security@vbdp.example.com
- **Database:** dba@vbdp.example.com
- **Infrastructure:** infra@vbdp.example.com
- **Network:** netops@vbdp.example.com

### External Vendors
- **CDN Support:** CloudFlare support portal
- **Cloud Provider:** AWS Support (Enterprise tier)
- **Database:** RDS/managed service support

---

## Incident Documentation Template

After resolving an incident, document it:

```markdown
# Incident Report: [Brief Title]

**Date:** YYYY-MM-DD
**Severity:** Critical/High/Medium/Low
**Duration:** X hours Y minutes
**Status:** Resolved

## Summary
[2-3 sentence summary of what happened]

## Impact
- Users affected: X
- Downtime: Y minutes
- Business impact: $Z or functionality loss

## Timeline
- HH:MM - Alert triggered
- HH:MM - On-call engineer acknowledged
- HH:MM - Root cause identified
- HH:MM - Fix applied
- HH:MM - Issue resolved
- HH:MM - Post-mortem completed

## Root Cause
[Detailed explanation of what caused the incident]

## Resolution
[What was done to fix it]

## Prevention
[What we'll do to prevent recurrence]
- [ ] Action item 1 - Owner: Name - Due: Date
- [ ] Action item 2 - Owner: Name - Due: Date

## Lessons Learned
[What did we learn? What went well? What could be better?]
```

---

## Additional Resources

- **Monitoring Dashboard:** https://grafana.example.com/d/vbdp-overview
- **Alert Configuration:** [monitoring/prometheus-alerts.yml](../monitoring/prometheus-alerts.yml)
- **Deployment Checklist:** [DEPLOYMENT_CHECKLIST.md](DEPLOYMENT_CHECKLIST.md)
- **System Design:** [architecture/SYSTEM_DESIGN.md](architecture/SYSTEM_DESIGN.md)
- **Security Model:** [security/SECURITY_MODEL.md](security/SECURITY_MODEL.md)

---

**End of Runbooks Document**
