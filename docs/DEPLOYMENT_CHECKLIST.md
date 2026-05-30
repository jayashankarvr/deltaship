# Production Deployment Checklist

**Document:** Pre-deployment validation checklist for Deltaship components
**Audience:** DevOps engineers, SREs, deployment teams
**Last Updated:** 2026-01-14

> **NOTE:** Email addresses on this page use the RFC 2606 `example.com` domain and are placeholders. Replace with real contacts before publishing.

---

## Overview

This checklist ensures all Deltaship components are properly configured and validated before production deployment. Complete all items and verify each step to prevent deployment failures and ensure system reliability.

**Scope:**
- Update Server deployment validation
- Publisher Toolkit setup verification
- Client installation verification
- Post-deployment smoke tests
- Production readiness assessment

**Deployment Types:**
- ✅ Small Deployment (< 10,000 users)
- ✅ Medium Deployment (10,000 - 100,000 users)
- ✅ Large Deployment (100,000+ users)

---

## Pre-Deployment Prerequisites

### Infrastructure Requirements

#### All Deployment Sizes

- [ ] **DNS Configuration**
  - [ ] DNS record created for update server (e.g., updates.example.com)
  - [ ] DNS propagation verified (`dig updates.example.com`)
  - [ ] TTL set appropriately (300-3600 seconds)
  - [ ] Backup DNS servers configured

- [ ] **TLS Certificates**
  - [ ] Valid TLS certificate obtained (Let's Encrypt, commercial, or internal CA)
  - [ ] Certificate includes correct domain name
  - [ ] Certificate validity period checked (>30 days remaining)
  - [ ] Certificate chain complete (intermediate certificates included)
  - [ ] Private key secured with appropriate permissions (600)
  - [ ] Certificate auto-renewal configured (if using Let's Encrypt)

- [ ] **Firewall Rules**
  - [ ] Inbound port 443 (HTTPS) allowed
  - [ ] Inbound port 80 (HTTP) allowed (for redirect to HTTPS)
  - [ ] Outbound database connection allowed (port 5432 for PostgreSQL)
  - [ ] Outbound object storage connection allowed (port 443)
  - [ ] SSH access restricted to specific IP ranges
  - [ ] Unnecessary ports blocked

- [ ] **Network Connectivity**
  - [ ] Server has stable internet connection
  - [ ] Bandwidth meets requirements (minimum 10 Mbps for small, 100+ Mbps for large)
  - [ ] Network latency to database < 10ms
  - [ ] Network latency to object storage < 50ms
  - [ ] CDN connectivity verified (if using)

#### Medium/Large Deployments

- [ ] **Load Balancer**
  - [ ] Load balancer configured with health checks
  - [ ] Health check endpoint: `/health`
  - [ ] Health check interval: 10 seconds
  - [ ] Unhealthy threshold: 3 consecutive failures
  - [ ] Session persistence configured (for long downloads)
  - [ ] TLS termination configured (if applicable)

- [ ] **Database Cluster**
  - [ ] PostgreSQL primary instance deployed
  - [ ] Read replica(s) configured (for analytics)
  - [ ] Automated backups enabled (daily minimum)
  - [ ] Point-in-time recovery configured
  - [ ] Connection pooling configured (PgBouncer or similar)
  - [ ] Database credentials secured in secrets manager

- [ ] **Object Storage**
  - [ ] S3 bucket created with appropriate name
  - [ ] Bucket versioning enabled
  - [ ] Lifecycle policies configured (delete old versions after 90 days)
  - [ ] Access credentials created (IAM user or role)
  - [ ] Bucket permissions validated (read/write for server)
  - [ ] Cross-region replication enabled (if applicable)

- [ ] **CDN Configuration**
  - [ ] CDN account created (CloudFlare, Fastly, CloudFront, etc.)
  - [ ] Origin configured (update server)
  - [ ] Cache rules configured for `/api/download-*` endpoints
  - [ ] Cache TTL set appropriately (1 week for diffs, 1 month for binaries)
  - [ ] Cache purge mechanism tested
  - [ ] SSL/TLS configured on CDN

---

## Update Server Deployment Validation

### Step 1: Server Installation

- [ ] **Binary Installation**
  - [ ] Deltaship server binary downloaded from official source
  - [ ] Binary signature verified (if available)
  - [ ] Binary installed to `/usr/local/bin/deltaship-server`
  - [ ] Execute permission set (`chmod +x`)
  - [ ] Version verified: `deltaship-server --version`

- [ ] **Directory Structure**
  - [ ] Configuration directory created: `/etc/deltaship-server/`
  - [ ] Data directory created: `/var/lib/deltaship/`
  - [ ] Log directory created: `/var/log/deltaship/`
  - [ ] Storage directory created (if using filesystem storage)
  - [ ] Correct ownership: `chown -R deltaship:deltaship /var/lib/deltaship /var/log/deltaship`
  - [ ] Correct permissions: configuration `600`, data directories `755`

- [ ] **Service User**
  - [ ] System user created: `deltaship`
  - [ ] User has no shell access (`/usr/sbin/nologin`)
  - [ ] User has home directory: `/var/lib/deltaship`
  - [ ] User cannot login interactively

### Step 2: Configuration Validation

- [ ] **Main Configuration File** (`/etc/deltaship-server/config.toml`)
  - [ ] File exists and readable
  - [ ] Server listen address configured: `0.0.0.0:8080` or specific IP
  - [ ] Public URL configured correctly: `https://updates.example.com`
  - [ ] Worker count set appropriately (number of CPU cores)
  - [ ] Configuration syntax validated: `deltaship-server --config /etc/deltaship-server/config.toml --check-config`

- [ ] **Database Configuration**
  - [ ] Database type specified: `sqlite` or `postgresql`
  - [ ] Connection string correct (host, port, database name)
  - [ ] Credentials valid and tested
  - [ ] Connection pool size set: 10-20 for medium, 20-50 for large
  - [ ] Database connection timeout configured: 30 seconds
  - [ ] SSL/TLS enabled for database connection (production)

- [ ] **Storage Configuration**
  - [ ] Storage type specified: `filesystem`, `s3`, `azure`, or `gcs`
  - [ ] Storage path/bucket configured correctly
  - [ ] Credentials configured (for cloud storage)
  - [ ] Storage capacity limits set: `max_storage_gb`
  - [ ] Write permissions verified: test file upload/download

- [ ] **Security Configuration**
  - [ ] API key authentication enabled: `require_api_key = true`
  - [ ] API keys file created: `/etc/deltaship-server/api-keys.txt`
  - [ ] API keys file permissions: `600`
  - [ ] At least one API key configured for publisher
  - [ ] Rate limiting enabled: `rate_limit_enabled = true`
  - [ ] Rate limit threshold set: 100-1000 requests per minute

- [ ] **Logging Configuration**
  - [ ] Log level set: `info` for production (not `debug`)
  - [ ] Log output configured: `file` or `json` for structured logging
  - [ ] Log file path specified: `/var/log/deltaship/server.log`
  - [ ] Log rotation configured (via logrotate or similar)
  - [ ] Log retention policy set (30-90 days)

### Step 3: Database Setup

- [ ] **Database Initialization**
  - [ ] Database created (if PostgreSQL)
  - [ ] Database user created with appropriate permissions
  - [ ] Schema migrations applied: `deltaship-server migrate --config /etc/deltaship-server/config.toml`
  - [ ] Migration status verified: all migrations successful
  - [ ] Database tables created (check with SQL client)

- [ ] **Database Verification**
  - [ ] Connection test successful: `deltaship-server test-db-connection`
  - [ ] Query performance tested: simple SELECT should return < 10ms
  - [ ] Indexes created automatically (verify with `\d` in psql)
  - [ ] Database backup mechanism tested

- [ ] **Database Performance** (Medium/Large)
  - [ ] Connection pooling verified (PgBouncer status check)
  - [ ] Slow query logging enabled
  - [ ] Query performance baseline established
  - [ ] Read replica lag < 1 second

### Step 4: systemd Service Configuration

- [ ] **Service File**
  - [ ] Service file created: `/etc/systemd/system/deltaship-server.service`
  - [ ] Service user set: `User=deltaship`
  - [ ] Working directory set: `/var/lib/deltaship`
  - [ ] ExecStart command correct: `/usr/local/bin/deltaship-server --config /etc/deltaship-server/config.toml`
  - [ ] Restart policy configured: `Restart=on-failure`
  - [ ] Security hardening options enabled: `NoNewPrivileges=true`, `ProtectSystem=strict`

- [ ] **Service Activation**
  - [ ] Service enabled: `systemctl enable deltaship-server`
  - [ ] Service started: `systemctl start deltaship-server`
  - [ ] Service status verified: `systemctl status deltaship-server` shows "active (running)"
  - [ ] Service logs checked: `journalctl -u deltaship-server -n 50` shows no errors
  - [ ] Service auto-restart tested: kill process, verify it restarts

### Step 5: Reverse Proxy Configuration

- [ ] **Nginx/Caddy Installation**
  - [ ] Reverse proxy installed and running
  - [ ] Configuration file created: `/etc/nginx/sites-available/deltaship`
  - [ ] Site enabled: symlink in `/etc/nginx/sites-enabled/`
  - [ ] Configuration syntax validated: `nginx -t`

- [ ] **Proxy Settings**
  - [ ] HTTP to HTTPS redirect configured (port 80 → 443)
  - [ ] Proxy pass to backend: `proxy_pass http://127.0.0.1:8080`
  - [ ] Headers forwarded: `X-Real-IP`, `X-Forwarded-For`, `X-Forwarded-Proto`
  - [ ] Client max body size increased: `client_max_body_size 1G`
  - [ ] Timeouts configured: 60s connect, 300s read/send

- [ ] **TLS Configuration**
  - [ ] TLS certificate installed
  - [ ] TLS protocols: TLS 1.2 and 1.3 only
  - [ ] Strong cipher suites configured
  - [ ] HSTS header enabled: `Strict-Transport-Security`
  - [ ] Security headers added: `X-Content-Type-Options`, `X-Frame-Options`
  - [ ] SSL Labs test: Grade A or higher (https://www.ssllabs.com/ssltest/)

### Step 6: Health Checks and Monitoring

- [ ] **Health Endpoint**
  - [ ] Health endpoint accessible: `curl http://localhost:8080/health`
  - [ ] Returns HTTP 200 status
  - [ ] Returns JSON with status, version, uptime
  - [ ] Health check through reverse proxy: `curl https://updates.example.com/health`
  - [ ] Health check includes database connectivity

- [ ] **Metrics Endpoint**
  - [ ] Metrics endpoint accessible: `curl http://localhost:8080/metrics`
  - [ ] Returns Prometheus format metrics
  - [ ] Key metrics present: `deltaship_api_requests_total`, `deltaship_api_request_duration_seconds`
  - [ ] Metrics endpoint not publicly accessible (internal only)

- [ ] **Monitoring Setup**
  - [ ] Prometheus configured to scrape metrics
  - [ ] Grafana dashboard imported (see monitoring/ directory)
  - [ ] Alert rules configured (see monitoring/prometheus-alerts.yml)
  - [ ] Alertmanager configured with notification channels
  - [ ] Test alert sent and received

---

## Publisher Toolkit Setup Verification

### Step 1: Toolkit Installation

- [ ] **Installation**
  - [ ] Publisher toolkit installed on build machine
  - [ ] All tools available in PATH: `deltaship-init`, `deltaship-register`, `deltaship-sign`, `deltaship-publish`
  - [ ] Version verified: `deltaship-init --version`
  - [ ] Dependencies installed (Rust, OpenSSL, etc.)

- [ ] **Project Initialization**
  - [ ] Project initialized: `deltaship-init --app-name "AppName" --update-server "https://updates.example.com"`
  - [ ] `.deltaship/` directory created
  - [ ] Configuration file created: `.deltaship/config.toml`
  - [ ] Versions database created: `.deltaship/versions.db`
  - [ ] Key pair generated: `.deltaship/keys/private.key` and `.deltaship/keys/public.key`

### Step 2: Key Management

- [ ] **Signing Keys**
  - [ ] Private key secured with permissions `600`
  - [ ] Private key added to `.gitignore`
  - [ ] Private key backed up to secure location
  - [ ] Public key committed to repository
  - [ ] Public key distributed to clients (embedded in application)

- [ ] **Key Validation**
  - [ ] Private key readable: `cat .deltaship/keys/private.key`
  - [ ] Public key matches private key (sign and verify test)
  - [ ] Key format correct (Ed25519)

### Step 3: Configuration Validation

- [ ] **Publisher Configuration** (`.deltaship/config.toml`)
  - [ ] Application name set correctly
  - [ ] Binary pattern configured: `binary_pattern`
  - [ ] Platform specified: `linux`, `windows`, or `macos`
  - [ ] Architecture specified: `x86_64` or `arm64`
  - [ ] Update server URL correct
  - [ ] API key configured (or environment variable set)

- [ ] **Diff Settings**
  - [ ] Diff algorithm chosen: `bsdiff`, `courgette`, or `auto`
  - [ ] Compression format set: `zstd`, `gzip`, or `none`
  - [ ] Max diff size limit: `max_size_mb` (500 MB recommended)

### Step 4: API Key Configuration

- [ ] **Server API Key**
  - [ ] API key obtained from server administrator
  - [ ] API key added to configuration or environment variable
  - [ ] API key tested: `curl -H "Authorization: Bearer API_KEY" https://updates.example.com/api/health`
  - [ ] API key has publish permissions

### Step 5: Test Publishing Workflow

- [ ] **Version Registration**
  - [ ] Sample binary built
  - [ ] Version registered: `deltaship-register --version 1.0.0 --binary ./path/to/binary`
  - [ ] Registration successful (no errors)
  - [ ] Version recorded in database
  - [ ] Binary hash computed

- [ ] **Signing**
  - [ ] Version signed: `deltaship-sign --version 1.0.0`
  - [ ] Signature created: `.deltaship/signatures/1.0.0.sig`
  - [ ] Signature file readable

- [ ] **Local Testing**
  - [ ] Second version created (1.0.1)
  - [ ] Diff generated between versions
  - [ ] Local test executed: `deltaship-test --from 1.0.0 --to 1.0.1`
  - [ ] Test passes (diff applies correctly)
  - [ ] Signature verification succeeds

- [ ] **Dry Run Publishing**
  - [ ] Dry run executed: `deltaship-publish --version 1.0.1 --dry-run`
  - [ ] No upload performed but validation passes
  - [ ] All files prepared for upload

---

## Client Installation Verification

### Step 1: Installation

- [ ] **Package Installation**
  - [ ] Client installer downloaded for target platform
  - [ ] Installation completed without errors
  - [ ] Installation method documented (package, binary, etc.)

- [ ] **Service Installation**
  - [ ] Background service installed
  - [ ] Service configured to start on boot
  - [ ] Service user created (if applicable)

### Step 2: Configuration

- [ ] **Client Configuration**
  - [ ] Configuration file created (platform-specific path)
  - [ ] Update server URL configured: `update_server_url`
  - [ ] Update frequency set: `check_interval_hours`
  - [ ] Auto-update settings configured: `auto_apply`
  - [ ] Network settings configured: `allow_metered`, `max_download_speed_kbps`

- [ ] **Public Key Distribution**
  - [ ] Publisher public key installed in client
  - [ ] Key location correct (embedded or file path)
  - [ ] Key readable by client service

### Step 3: Service Verification

- [ ] **Service Status**
  - [ ] Service running: `systemctl status deltaship` (Linux) or equivalent
  - [ ] Service logs accessible
  - [ ] No errors in startup logs
  - [ ] Service restarts on failure

- [ ] **Connectivity Test**
  - [ ] Client can reach update server: `curl https://updates.example.com/health`
  - [ ] Manual update check: `deltaship check --verbose`
  - [ ] Server communication successful

### Step 4: Application Registration

- [ ] **App Registration**
  - [ ] Test application registered: `deltaship register --app "TestApp" --binary /path/to/app --version 1.0.0`
  - [ ] Registration successful
  - [ ] App listed: `deltaship list` shows registered app

---

## Post-Deployment Smoke Tests

### Server Smoke Tests

- [ ] **Basic Functionality**
  - [ ] Health check: `curl https://updates.example.com/health` returns 200 OK
  - [ ] API root: `curl https://updates.example.com/` returns server info
  - [ ] Invalid endpoint: `curl https://updates.example.com/invalid` returns 404

- [ ] **API Endpoints**
  - [ ] Check update (no versions): `curl "https://updates.example.com/api/check-update?app=test&version=1.0.0"` returns "no update available"
  - [ ] API key required: request without key returns 401 Unauthorized
  - [ ] Invalid API key: request with wrong key returns 403 Forbidden

- [ ] **Database Connectivity**
  - [ ] Database queries working (check logs for successful queries)
  - [ ] Connection pool healthy (no connection errors)
  - [ ] Query latency acceptable (<10ms for simple queries)

- [ ] **Storage Connectivity**
  - [ ] File upload test (via publisher)
  - [ ] File download test
  - [ ] Storage quota monitoring working

### End-to-End Flow Test

- [ ] **Publisher → Server → Client Flow**
  1. [ ] Build test application v1.0.0
  2. [ ] Register version: `deltaship-register --version 1.0.0 --binary ./app`
  3. [ ] Sign version: `deltaship-sign --version 1.0.0`
  4. [ ] Publish version: `deltaship-publish --version 1.0.0`
  5. [ ] Verify upload: check server logs, storage bucket
  6. [ ] Client registers app: `deltaship register --app TestApp --binary ./app --version 1.0.0`
  7. [ ] Build test application v1.1.0
  8. [ ] Register v1.1.0: `deltaship-register --version 1.1.0 --binary ./app`
  9. [ ] Diff generated automatically
  10. [ ] Sign version: `deltaship-sign --version 1.1.0`
  11. [ ] Publish v1.1.0: `deltaship-publish --version 1.1.0`
  12. [ ] Client checks for update: `deltaship check`
  13. [ ] Client finds update available (v1.1.0)
  14. [ ] Client downloads diff (not full binary)
  15. [ ] Client applies diff
  16. [ ] Client verifies signature
  17. [ ] Update successful: app now at v1.1.0
  18. [ ] Verify bandwidth savings: diff size << full binary size

- [ ] **Results Validation**
  - [ ] Update applied successfully (no errors)
  - [ ] Application runs correctly after update
  - [ ] Bandwidth savings achieved (>90% reduction)
  - [ ] Update time acceptable (<30 seconds for typical app)

### Performance Tests

- [ ] **API Performance**
  - [ ] Response time p50 < 100ms
  - [ ] Response time p95 < 500ms
  - [ ] Response time p99 < 1s
  - [ ] Concurrent requests handled (10+ simultaneous)

- [ ] **Diff Generation Performance**
  - [ ] Diff generation time < 2 minutes (for 100 MB binary)
  - [ ] Diff size < 5% of full binary (for typical patch)
  - [ ] CPU usage acceptable during generation (<80%)

- [ ] **Download Performance**
  - [ ] Download speed matches bandwidth limit (if set)
  - [ ] Download resumes after interruption
  - [ ] Concurrent downloads work (multiple clients)

### Security Tests

- [ ] **TLS/SSL**
  - [ ] HTTPS enforced (HTTP redirects to HTTPS)
  - [ ] Valid certificate presented
  - [ ] No certificate warnings in browser
  - [ ] TLS 1.2+ only (TLS 1.0/1.1 disabled)

- [ ] **Authentication**
  - [ ] Unauthenticated requests rejected
  - [ ] Invalid API key rejected
  - [ ] Valid API key accepted

- [ ] **Signature Verification**
  - [ ] Client verifies signatures
  - [ ] Invalid signature rejected
  - [ ] Tampered binary rejected
  - [ ] Correct signature accepted

- [ ] **Rate Limiting**
  - [ ] Excessive requests rate-limited (429 Too Many Requests)
  - [ ] Rate limit resets after time window
  - [ ] Legitimate traffic not affected

---

## Production Readiness Checklist

### Operational Readiness

- [ ] **Documentation**
  - [ ] Deployment guide reviewed and accurate
  - [ ] Configuration documented
  - [ ] Runbooks created for common incidents (see docs/RUNBOOKS.md)
  - [ ] Contact information documented (on-call, escalation)

- [ ] **Monitoring and Alerting**
  - [ ] Monitoring dashboard accessible (Grafana)
  - [ ] All key metrics being collected
  - [ ] Alert rules configured (Prometheus)
  - [ ] Alerts routing to correct channels (PagerDuty, Slack, email)
  - [ ] On-call rotation established
  - [ ] Test alert sent and acknowledged

- [ ] **Backup and Recovery**
  - [ ] Database backups automated (daily minimum)
  - [ ] Backup retention policy configured (30 days minimum)
  - [ ] Backup restoration tested successfully
  - [ ] Disaster recovery plan documented
  - [ ] Recovery Time Objective (RTO) defined
  - [ ] Recovery Point Objective (RPO) defined

- [ ] **Capacity Planning**
  - [ ] Expected user load estimated
  - [ ] Server resources sized appropriately
  - [ ] Storage capacity planned (6-12 months runway)
  - [ ] Bandwidth capacity planned
  - [ ] Scaling plan documented

### Security Hardening

- [ ] **Server Security**
  - [ ] OS security updates applied
  - [ ] Unnecessary services disabled
  - [ ] SSH key-only authentication (password disabled)
  - [ ] Fail2ban or similar intrusion prevention installed
  - [ ] Security scan completed (vulnerability assessment)

- [ ] **Application Security**
  - [ ] Security headers configured (CSP, HSTS, X-Frame-Options)
  - [ ] CORS policy configured appropriately
  - [ ] Input validation enabled
  - [ ] SQL injection prevention (parameterized queries)
  - [ ] Secrets stored securely (not in config files)

- [ ] **Access Control**
  - [ ] Administrative access restricted (VPN, IP whitelist)
  - [ ] Least privilege principle applied
  - [ ] API keys rotated regularly (documented schedule)
  - [ ] Audit logging enabled

### Compliance and Legal

- [ ] **Privacy**
  - [ ] Privacy policy reviewed
  - [ ] User data handling compliant (GDPR, CCPA, etc.)
  - [ ] Analytics data anonymized
  - [ ] Data retention policy documented

- [ ] **Licensing**
  - [ ] Software licenses reviewed
  - [ ] Open source compliance verified
  - [ ] License files included

### Rollout Strategy

- [ ] **Phased Rollout Plan**
  - [ ] Phase 1: Internal testing (1-10 users)
  - [ ] Phase 2: Beta users (10-100 users)
  - [ ] Phase 3: Gradual rollout (10%, 25%, 50%, 100%)
  - [ ] Rollback plan documented
  - [ ] Success criteria defined for each phase

- [ ] **Communication Plan**
  - [ ] User notification prepared (update available)
  - [ ] Changelog published
  - [ ] Support team briefed
  - [ ] Announcement scheduled

---

## Final Verification

### Pre-Launch Checklist

- [ ] All deployment checklist items completed ✓
- [ ] All smoke tests passed ✓
- [ ] Monitoring and alerting operational ✓
- [ ] Backup and recovery tested ✓
- [ ] Security hardening completed ✓
- [ ] Documentation complete and accurate ✓
- [ ] Support team ready ✓
- [ ] Rollback plan prepared ✓

### Launch Approval

- [ ] Technical lead approval: _____________________ Date: _______
- [ ] Operations lead approval: ____________________ Date: _______
- [ ] Security review approval: ____________________ Date: _______
- [ ] Product/business approval: ___________________ Date: _______

### Post-Launch Monitoring

**First 24 Hours:**
- [ ] Monitor error rates (should be <1%)
- [ ] Monitor response times (p99 <1s)
- [ ] Monitor update success rate (should be >99%)
- [ ] Check for unusual patterns in logs
- [ ] Verify no critical alerts triggered

**First Week:**
- [ ] Review bandwidth savings metrics
- [ ] Analyze user adoption rate
- [ ] Check storage growth rate
- [ ] Review performance trends
- [ ] Collect user feedback

**First Month:**
- [ ] Capacity review (are we on track?)
- [ ] Performance optimization opportunities
- [ ] Cost analysis (vs. projections)
- [ ] Feature request collection
- [ ] Lessons learned documentation

---

## Troubleshooting Common Issues

### Issue: Health Check Fails

**Symptoms:** `/health` endpoint returns 500 or times out

**Checks:**
- [ ] Server process running: `systemctl status deltaship-server`
- [ ] Server logs for errors: `journalctl -u deltaship-server -n 100`
- [ ] Database connectivity: `deltaship-server test-db-connection`
- [ ] Configuration file syntax: `deltaship-server --check-config`

### Issue: Cannot Upload to Storage

**Symptoms:** Publisher publish fails with storage error

**Checks:**
- [ ] Storage credentials valid
- [ ] Storage bucket exists and accessible
- [ ] Network connectivity to storage endpoint
- [ ] Bucket permissions (write access)
- [ ] Storage quota not exceeded

### Issue: Client Cannot Reach Server

**Symptoms:** Client shows "cannot connect to update server"

**Checks:**
- [ ] DNS resolution working: `dig updates.example.com`
- [ ] Server reachable: `curl https://updates.example.com/health`
- [ ] Firewall not blocking client
- [ ] TLS certificate valid (not expired)
- [ ] Update server URL correct in client config

### Issue: Signature Verification Fails

**Symptoms:** Client rejects updates with "signature invalid"

**Checks:**
- [ ] Client has correct public key
- [ ] Publisher signed with correct private key
- [ ] Binary not modified after signing
- [ ] System time correct on client and server
- [ ] Signature file not corrupted

---

## Deployment Sign-Off

**Deployment Date:** _______________

**Deployed By:** _______________

**Deployment Type:** [ ] Small [ ] Medium [ ] Large

**Components Deployed:**
- [ ] Update Server
- [ ] Publisher Toolkit
- [ ] Client Patcher

**Verification Status:**
- [ ] All prerequisites met
- [ ] All validation steps completed
- [ ] All smoke tests passed
- [ ] Production readiness confirmed

**Notes:**
_________________________________________________________________
_________________________________________________________________
_________________________________________________________________

**Next Review Date:** _______________

---

## Appendix: Quick Reference

### Essential Commands

**Server:**
- Health check: `curl https://updates.example.com/health`
- Service status: `systemctl status deltaship-server`
- View logs: `journalctl -u deltaship-server -f`
- Test config: `deltaship-server --check-config`

**Publisher:**
- Register: `deltaship-register --version X.Y.Z --binary ./app`
- Sign: `deltaship-sign --version X.Y.Z`
- Test: `deltaship-test --from X.Y.Z --to X.Y.Z+1`
- Publish: `deltaship-publish --version X.Y.Z`

**Client:**
- Check updates: `deltaship check`
- List apps: `deltaship list`
- View logs: Platform-specific (journalctl, Event Viewer, Console.app)

### Support Contacts

- **Technical Support:** support@deltaship.example.com
- **Security Issues:** security@deltaship.example.com
- **Documentation:** https://docs.deltaship.io
- **Community:** GitHub Discussions

---

**End of Deployment Checklist**
