# Operations and Maintenance Guide

**Document:** Operational procedures for running Deltaship infrastructure
**Audience:** Operations teams, SREs, system administrators
**Last Updated:** 2026-01-07

---

## Overview

This guide provides operational procedures, maintenance tasks, troubleshooting steps, and best practices for running Deltaship (Version-Aware Binary Differential Update System) in production.

**Scope:**
- Daily operations
- Monitoring and alerting
- Backup and recovery
- Performance tuning
- Incident response
- Capacity planning

---

## Daily Operations

### Health Checks

**Automated Health Checks (Recommended):**

**Monitoring System:**
- Prometheus + Alertmanager
- Grafana dashboards
- PagerDuty integration (for critical alerts)

**Manual Health Checks:**

**Server Status:**
```
# Check service running
systemctl status deltaship-server

# Check API health endpoint
curl https://updates.example.com/health

# Expected response:
# {"status": "healthy", "version": "1.0.0", "uptime_seconds": 3600}
```

**Database Connectivity:**
```
# PostgreSQL connection test
psql -h database-host -U deltaship_server -d deltaship -c "SELECT COUNT(*) FROM apps;"

# Expected: Returns count of registered apps
```

**Object Storage:**
```
# S3 bucket access test
aws s3 ls s3://deltaship-updates-prod/ | head

# Expected: Lists objects in bucket
```

**CDN:**
```
# Test CDN cache hit
curl -I https://cdn.example.com/diffs/test.diff

# Check header: X-Cache: HIT (indicates CDN serving from cache)
```

### Log Review

**Check for Errors:**

```
# Recent errors in server logs
journalctl -u deltaship-server --since "1 hour ago" -p err

# Or file logs
grep ERROR /var/log/deltaship/server.log | tail -n 20
```

**Common log patterns to watch:**

**Warning Signs:**
- High rate of signature verification failures (potential attack or key mismatch)
- Many diff generation timeouts (need more compute resources)
- Database connection pool exhaustion (need to scale)
- Storage upload failures (S3 issues or network problems)

**Expected Patterns:**
- Version published: INFO
- Update checked: INFO (should be frequent)
- Update applied: INFO (should correlate with check-update calls)
- Diff generated: INFO (may be slow, but should succeed)

### Metrics Review

**Key Metrics to Monitor Daily:**

**Update Success Rate:**
```
Target: >99%
Alert if: <98% for 1 hour
```

**API Response Time:**
```
p50: <100ms
p95: <500ms
p99: <1s
Alert if: p99 >5s for 10 minutes
```

**Bandwidth Savings:**
```
Target: >90% average
Alert if: <50% (indicates diffs not being used)
```

**Active Installations:**
```
Track: Count of unique device_ids per day
Alert if: Sudden drop >10% (potential issue)
```

**Error Rate:**
```
Target: <1%
Alert if: >2% for 30 minutes
```

---

## Weekly Maintenance

### Database Maintenance

**Vacuum and Analyze (PostgreSQL):**

```
# Automated (recommended)
Configure in postgresql.conf:
autovacuum = on
autovacuum_vacuum_scale_factor = 0.1
autovacuum_analyze_scale_factor = 0.05

# Manual (if needed)
psql -h database-host -U deltaship_server -d deltaship -c "VACUUM ANALYZE;"
```

**Index Optimization:**

```
# Check for missing indexes
psql -h database-host -U deltaship_server -d deltaship

SELECT schemaname, tablename, attname, n_distinct, correlation
FROM pg_stats
WHERE schemaname = 'public'
ORDER BY abs(correlation) DESC;

# Create index if correlation low and queries slow
CREATE INDEX CONCURRENTLY idx_update_events_timestamp ON update_events(timestamp);
```

**Database Size Monitoring:**

```
# Check database size
psql -h database-host -U deltaship_server -d deltaship -c "\l+"

# Check table sizes
psql -h database-host -U deltaship_server -d deltaship -c "
SELECT
  tablename,
  pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
"
```

**Cleanup Old Analytics Data:**

```
# Delete analytics older than retention period (e.g., 90 days)
psql -h database-host -U deltaship_server -d deltaship -c "
DELETE FROM update_events
WHERE timestamp < NOW() - INTERVAL '90 days';
"

# Or configure automated cleanup in application
```

### Storage Cleanup

**Remove Old Diffs:**

**Policy:** Keep diffs for N recent versions (e.g., last 5 versions)

**Manual Cleanup:**

```
# List old diffs
aws s3 ls s3://deltaship-updates-prod/diffs/ --recursive

# Delete diffs older than 6 months
aws s3 ls s3://deltaship-updates-prod/diffs/ --recursive \
  | awk '{if ($1 < "2025-07-01") print $4}' \
  | xargs -I {} aws s3 rm s3://deltaship-updates-prod/{}
```

**Automated Cleanup (S3 Lifecycle Policy):**

```json
{
  "Rules": [
    {
      "Id": "DeleteOldDiffs",
      "Status": "Enabled",
      "Prefix": "diffs/",
      "Expiration": {
        "Days": 180
      }
    }
  ]
}
```

Apply:
```
aws s3api put-bucket-lifecycle-configuration \
  --bucket deltaship-updates-prod \
  --lifecycle-configuration file://lifecycle.json
```

### Log Rotation

**Automated Log Rotation (logrotate):**

Create `/etc/logrotate.d/deltaship`:

```
/var/log/deltaship/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0640 deltaship deltaship
    sharedscripts
    postrotate
        systemctl reload deltaship-server > /dev/null 2>&1 || true
    endscript
}
```

Test:
```
logrotate -d /etc/logrotate.d/deltaship
```

### Certificate Renewal

**Let's Encrypt Certificates:**

**Automated Renewal (certbot):**
```
# Certbot auto-renews via cron/systemd timer
# Check renewal status
certbot renew --dry-run

# Verify timer is active
systemctl status certbot.timer
```

**Manual Renewal (if needed):**
```
certbot renew
systemctl reload nginx  # Or apache2
```

**Commercial Certificates:**
- Check expiration date (60 days before expiry)
- Request renewal from CA
- Install new certificate
- Reload web server

**Monitoring:**
```
# Check certificate expiry
echo | openssl s_client -connect updates.example.com:443 2>/dev/null \
  | openssl x509 -noout -dates

# Alert if expires in <30 days
```

---

## Monthly Maintenance

### Performance Review

**Review Prometheus Metrics:**

**API Performance:**
- Check p99 response times (target: <1s)
- Identify slow endpoints
- Optimize database queries if needed

**Diff Generation:**
- Average generation time per algorithm
- Identify binaries taking >5 minutes to diff
- Consider pre-computation for slow ones

**Database Performance:**
- Review slow query log
- Add indexes where beneficial
- Optimize frequent queries

**Example: Find slow queries (PostgreSQL):**

```sql
SELECT
  query,
  calls,
  total_time,
  mean_time,
  max_time
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 10;
```

### Capacity Planning

**Trend Analysis:**

**User Growth:**
- Plot daily active devices over time
- Forecast growth (linear regression or similar)
- Plan capacity increase if approaching limits

**Storage Growth:**
- Track storage usage trend
- Forecast when disk/bucket space exhausted
- Plan expansion before hitting limits

**Bandwidth Usage:**
- Track monthly bandwidth (CDN + direct)
- Forecast costs based on growth
- Optimize CDN caching if costs increasing

**Example Capacity Planning:**

```
Current: 50,000 active devices
Growth rate: +5% per month
Forecast (6 months): 67,000 devices

Current API capacity: 100,000 requests/second
Current load: 10,000 requests/second peak (10% utilization)
Forecast load (6 months): 13,400 requests/second (13% utilization)

Action: No immediate scaling needed, revisit in 3 months
```

### Security Audit

**Monthly Security Checks:**

**1. Review Access Logs:**
```
# Check for suspicious API access patterns
grep "POST /api/publish" /var/log/nginx/access.log | awk '{print $1}' | sort | uniq -c | sort -rn

# Look for:
# - Unusual IP addresses
# - High request rates from single IP (potential attack)
# - Failed authentication attempts
```

**2. Check for Unauthorized Access:**
```
# Review database access
psql -c "SELECT * FROM pg_stat_activity WHERE usename = 'deltaship_server';"

# Review S3 access (AWS CloudTrail)
aws cloudtrail lookup-events --lookup-attributes AttributeKey=ResourceType,AttributeValue=AWS::S3::Object
```

**3. Verify Signature Keys:**
```
# Ensure publisher public keys haven't been tampered with
# Compare hashes with known-good values

sha256sum /etc/deltaship-server/publisher-keys/*.pub
```

**4. Update Dependencies:**
```
# Check for security updates
apt list --upgradable | grep deltaship

# Or for containers
docker pull deltaship/server:latest
docker images --digests deltaship/server
```

**5. Review Firewall Rules:**
```
# Ensure only necessary ports open
sudo ufw status
# Expected: 80, 443 (HTTP/HTTPS), 22 (SSH, restricted IPs only)
```

### Backup Verification

**Test Database Restore:**

**Monthly procedure:**

1. **Download recent backup:**
   ```
   aws s3 cp s3://deltaship-backups/deltaship-$(date +%Y%m%d).sql.gz /tmp/
   ```

2. **Restore to test database:**
   ```
   createdb deltaship_test
   gunzip -c /tmp/deltaship-*.sql.gz | psql deltaship_test
   ```

3. **Verify data integrity:**
   ```
   psql deltaship_test -c "SELECT COUNT(*) FROM apps;"
   psql deltaship_test -c "SELECT COUNT(*) FROM versions;"
   ```

4. **Check recent data present:**
   ```
   psql deltaship_test -c "SELECT MAX(created_at) FROM versions;"
   # Should be recent (within 24 hours)
   ```

5. **Cleanup:**
   ```
   dropdb deltaship_test
   ```

**Test Object Storage Restore:**

```
# Download recent version from backup bucket
aws s3 cp s3://deltaship-backups/binaries/test-app-1.0.0.bin /tmp/

# Verify hash matches
sha256sum /tmp/test-app-1.0.0.bin
# Compare with hash in database
```

---

## Incident Response

### Common Incidents

### Incident 1: High Error Rate

**Symptoms:**
- Alert: "Update failure rate >5%"
- Users reporting update failures

**Investigation:**

1. **Check error distribution:**
   ```
   # Group errors by type
   SELECT error_message, COUNT(*)
   FROM update_events
   WHERE success = false AND timestamp > NOW() - INTERVAL '1 hour'
   GROUP BY error_message
   ORDER BY COUNT(*) DESC;
   ```

2. **Common error causes:**

**"Signature verification failed":**
- **Cause:** Publisher uploaded unsigned version or wrong key used
- **Fix:** Verify publisher signed correctly, re-sign if needed

**"Diff download failed":**
- **Cause:** CDN or S3 issue, network connectivity
- **Fix:** Check CDN status, test S3 access, check network

**"Patch application failed":**
- **Cause:** Corrupted diff, insufficient disk space, incompatible versions
- **Fix:** Regenerate diff, advise users to free disk space

3. **Mitigation:**

**Temporary:**
```
# Fallback to full binary download for affected version
# Update rollout config to pause differential updates
deltaship-rollback --version 1.1.0 --action fallback-to-full
```

**Permanent:**
```
# Fix root cause (re-sign, regenerate diff, etc.)
# Re-enable differential updates
```

### Incident 2: Server Unresponsive

**Symptoms:**
- Alert: "API health check failed"
- Website/API returning 502/503 errors

**Investigation:**

1. **Check service status:**
   ```
   systemctl status deltaship-server
   ```

2. **Check resource usage:**
   ```
   top
   df -h
   free -h
   ```

3. **Common causes:**

**Out of Memory:**
- **Cause:** Diff generation for large binary, memory leak
- **Fix:** Restart service, increase memory, investigate leak

**Database Connection Pool Exhausted:**
- **Cause:** Too many concurrent connections, slow queries
- **Fix:** Increase pool size, optimize queries

**Disk Full:**
- **Cause:** Logs filling disk, large temp files
- **Fix:** Clear logs, cleanup temp files, increase disk

**Resolution:**

```
# Restart service
sudo systemctl restart deltaship-server

# If persistent, scale up
# - Add more server instances (horizontal scaling)
# - Or increase resources (vertical scaling)
```

### Incident 3: Database Failure

**Symptoms:**
- Alert: "Database connection failed"
- API returning 500 errors

**Investigation:**

1. **Check database status:**
   ```
   systemctl status postgresql  # Self-hosted
   # Or check cloud provider dashboard
   ```

2. **Check disk space:**
   ```
   df -h /var/lib/postgresql
   ```

3. **Check logs:**
   ```
   tail -n 100 /var/log/postgresql/postgresql-14-main.log
   ```

**Recovery:**

**If primary down:**
```
# Promote standby to primary (if using replication)
# AWS RDS:
aws rds failover-db-cluster --db-cluster-identifier deltaship-cluster

# Manual PostgreSQL:
pg_ctl promote -D /var/lib/postgresql/14/main

# Update application config to new primary
```

**If data corrupted:**
```
# Restore from backup
# 1. Stop application
systemctl stop deltaship-server

# 2. Restore database
dropdb deltaship
createdb deltaship
gunzip -c /backups/deltaship-latest.sql.gz | psql deltaship

# 3. Restart application
systemctl start deltaship-server
```

### Incident 4: Malicious Update Detected

**Symptoms:**
- User reports suspicious update behavior
- Antivirus flagging updated binary

**Immediate Response:**

1. **Pause rollout immediately:**
   ```
   deltaship-rollback --version 1.1.0 --action pause --emergency
   ```

2. **Isolate affected version:**
   ```
   # Remove from download servers
   aws s3 rm s3://deltaship-updates-prod/binaries/myapp-1.1.0.bin
   aws s3 rm s3://deltaship-updates-prod/diffs/myapp-*-to-1.1.0.diff
   ```

3. **Notify users:**
   ```
   # Send notification via update API (if supported)
   # Or: Email, website notification
   ```

**Investigation:**

1. **Verify publisher identity:**
   - Check who published the version
   - Verify API key used
   - Check IP address of publish request

2. **Analyze binary:**
   - Download binary for analysis
   - Scan with antivirus
   - Compare with previous version (diff analysis)
   - Check signature (is it valid but from wrong key?)

3. **Check for compromise:**
   - Audit publisher account access
   - Check for stolen/leaked API keys
   - Review recent access logs

**Resolution:**

**If malicious:**
```
# Rollback all users
deltaship-rollback --version 1.1.0 --action rollback --rollback-to 1.0.1

# Revoke compromised API key
# Investigate security breach
# Publish statement to users
```

**If false positive:**
```
# Contact antivirus vendor (false positive report)
# Re-enable rollout after clarification
# Communicate with users
```

---

## Performance Tuning

### Database Tuning

**PostgreSQL Configuration:**

Edit `/etc/postgresql/14/main/postgresql.conf`:

```
# Memory settings
shared_buffers = 4GB  # 25% of RAM
effective_cache_size = 12GB  # 75% of RAM
work_mem = 64MB
maintenance_work_mem = 1GB

# Connection settings
max_connections = 200
shared_preload_libraries = 'pg_stat_statements'

# Checkpoint settings
checkpoint_completion_target = 0.9
wal_buffers = 16MB
default_statistics_target = 100

# Query optimization
random_page_cost = 1.1  # For SSD
effective_io_concurrency = 200
```

Restart:
```
sudo systemctl restart postgresql
```

**Connection Pooling (PgBouncer):**

Install:
```
sudo apt install pgbouncer
```

Configure `/etc/pgbouncer/pgbouncer.ini`:
```
[databases]
deltaship = host=localhost port=5432 dbname=deltaship

[pgbouncer]
listen_addr = 127.0.0.1
listen_port = 6432
auth_type = md5
auth_file = /etc/pgbouncer/userlist.txt
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 25
```

Application connection string:
```
postgresql://deltaship_server:password@localhost:6432/deltaship
```

### API Server Tuning

**Increase Worker Threads:**

Edit `/etc/deltaship-server/config.toml`:
```toml
[server]
workers = 16  # 2x CPU cores
max_connections = 1000
keepalive_timeout_seconds = 75
```

**Enable HTTP/2:**

Nginx configuration:
```
listen 443 ssl http2;
```

**Enable Compression:**

```
gzip on;
gzip_types application/json application/octet-stream;
gzip_min_length 1000;
```

### CDN Optimization

**Cache Configuration:**

**CloudFlare Page Rules:**
- Cache Level: Cache Everything
- Edge Cache TTL: 1 month (for binaries), 1 week (for diffs)
- Browser Cache TTL: 4 hours

**Purge Strategy:**
- Purge specific URLs when new version published
- Use API:
  ```
  curl -X POST "https://api.cloudflare.com/client/v4/zones/{zone_id}/purge_cache" \
    -H "Authorization: Bearer {api_token}" \
    -H "Content-Type: application/json" \
    --data '{"files":["https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0.diff"]}'
  ```

**Pre-warming CDN Cache:**

After publishing new version, pre-fetch from CDN:
```
curl -I https://cdn.example.com/binaries/myapp-1.1.0.bin
curl -I https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0.diff
```

Ensures first users get fast downloads (cache hit).

---

## Monitoring and Alerting

### Alert Configuration

**Critical Alerts (PagerDuty):**

**Server Down:**
```yaml
alert: DeltashipServerDown
expr: up{job="deltaship-server"} == 0
for: 2m
severity: critical
annotations:
  summary: "Deltaship server is down"
  description: "Server {{ $labels.instance }} has been down for more than 2 minutes"
```

**High Error Rate:**
```yaml
alert: HighUpdateFailureRate
expr: (rate(deltaship_updates_total{success="false"}[5m]) / rate(deltaship_updates_total[5m])) > 0.05
for: 10m
severity: critical
annotations:
  summary: "High update failure rate"
  description: "Update failure rate is {{ $value | humanizePercentage }} (>5%)"
```

**Database Down:**
```yaml
alert: DatabaseDown
expr: deltaship_database_up == 0
for: 1m
severity: critical
annotations:
  summary: "Database is unreachable"
```

**Warning Alerts (Email):**

**High Latency:**
```yaml
alert: HighAPILatency
expr: histogram_quantile(0.99, rate(deltaship_api_request_duration_seconds_bucket[5m])) > 5
for: 15m
severity: warning
annotations:
  summary: "API latency high"
  description: "p99 latency is {{ $value }}s (>5s threshold)"
```

**Low Bandwidth Savings:**
```yaml
alert: LowBandwidthSavings
expr: (deltaship_bandwidth_saved_bytes_total / deltaship_bandwidth_total_bytes) < 0.50
for: 1h
severity: warning
annotations:
  summary: "Low bandwidth savings"
  description: "Only {{ $value | humanizePercentage }} bandwidth saved (expected >90%)"
```

### Dashboard Templates

**Grafana Dashboard Panels:**

**Overview Row:**
- Active Devices (gauge)
- Update Success Rate (stat, green if >99%)
- Bandwidth Saved Today (stat, with trend arrow)
- API Request Rate (graph, requests/second)

**Performance Row:**
- API Response Time (graph, p50/p95/p99)
- Diff Generation Time (histogram)
- Database Query Time (graph)

**Operations Row:**
- Version Distribution (pie chart)
- Error Rate by Type (stacked bar chart)
- Rollout Progress (table showing versions and % rollout)

---

## Disaster Recovery Procedures

### Complete Service Failure

**Scenario:** Primary region completely down

**Recovery Steps:**

1. **Activate DR Site:**
   ```
   # Update DNS to point to DR region
   aws route53 change-resource-record-sets --hosted-zone-id Z123 --change-batch file://failover.json
   ```

2. **Verify DR Services:**
   ```
   curl https://updates-dr.example.com/health
   ```

3. **Monitor Traffic:**
   - Check traffic shifting to DR
   - Monitor error rates

4. **Communicate:**
   - Update status page
   - Notify stakeholders

**Recovery Time Objective (RTO):** 15 minutes
**Recovery Point Objective (RPO):** 15 minutes (data loss window)

### Data Corruption

**Scenario:** Database corrupted, needs restore

**Procedure:**

1. **Identify corruption extent:**
   ```
   # Check recent transactions
   psql -c "SELECT * FROM versions ORDER BY created_at DESC LIMIT 10;"
   ```

2. **Determine restore point:**
   - Last known-good backup
   - Time before corruption occurred

3. **Stop services:**
   ```
   systemctl stop deltaship-server
   ```

4. **Restore database:**
   ```
   dropdb deltaship
   createdb deltaship
   gunzip -c /backups/deltaship-YYYYMMDD-HHMM.sql.gz | psql deltaship
   ```

5. **Restore object storage (if needed):**
   ```
   aws s3 sync s3://deltaship-backups/binaries/ s3://deltaship-updates-prod/binaries/ --dryrun
   # Remove --dryrun after verification
   ```

6. **Verify integrity:**
   ```
   # Run consistency checks
   psql deltaship < /opt/deltaship/scripts/verify-integrity.sql
   ```

7. **Restart services:**
   ```
   systemctl start deltaship-server
   ```

8. **Monitor:**
   - Check error rates
   - Verify updates working

---

## Best Practices

**Automation:**
- Automate routine tasks (backups, log rotation, monitoring)
- Use configuration management (Ansible, Puppet)
- Infrastructure as Code (Terraform, CloudFormation)

**Documentation:**
- Keep runbooks up-to-date
- Document all procedures
- Maintain change log

**Testing:**
- Test disaster recovery quarterly
- Conduct failure drills
- Validate backups monthly

**Communication:**
- Maintain status page (status.example.com)
- Alert stakeholders of planned maintenance
- Post-mortem after incidents

**Continuous Improvement:**
- Review metrics weekly
- Optimize based on data
- Implement lessons learned from incidents

---

## Next Steps

**For Operations Teams:**
- Set up monitoring and alerting
- Configure automated backups
- Test disaster recovery procedures

**For SREs:**
- Review capacity planning
- Implement performance tuning
- Automate common tasks

**For More Information:**
- Read: [Server Deployment](../deployment/SERVER_DEPLOYMENT.md)
- Read: [System Design](../architecture/SYSTEM_DESIGN.md)
- Read: [Security Model](../security/SECURITY_MODEL.md)

---

**End of Operations and Maintenance Guide**
