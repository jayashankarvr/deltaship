# Existing Systems Integration

**Document:** Integration with existing update systems and infrastructure
**Audience:** System integrators, enterprise architects, DevOps teams
**Last Updated:** 2026-01-07

---

## Overview

This document describes how VBDP integrates with existing software update systems, package managers, deployment tools, and enterprise infrastructure. VBDP is designed to complement, not replace, existing systems.

**Integration Philosophy:**
- **Coexist:** Work alongside existing update mechanisms
- **Interoperate:** API-first design for easy integration
- **Augment:** Enhance existing systems with differential updates
- **Migrate:** Gradual migration path, no big-bang replacement

---

## Integration Scenarios

### Scenario 1: Standalone Application Updates

**Use Case:** Desktop application with custom updater

**Current State:**
- Application has built-in update checker
- Downloads full binary from CDN
- Users wait minutes for large downloads

**Integration Approach:**

**Replace update logic with VBDP client library:**

Instead of downloading full binary, integrate VBDP client SDK:

**Benefits:**
- 95-99% bandwidth reduction
- Faster updates (seconds vs minutes)
- Automatic rollback on failure
- Cryptographic signature verification

**Implementation Steps:**

1. **Embed VBDP client library** in your application
2. **Replace update check** with VBDP API call
3. **Replace download logic** with VBDP diff download
4. **Keep UI** (progress bar, notifications) - just wire to VBDP events
5. **Maintain fallback** to full download if diff fails

**Example integration points:**

**Before (traditional updater):**
```
User clicks "Check for updates"
  → App queries: https://yoursite.com/latest-version.json
  → Response: {version: "2.0.0", download_url: "https://cdn.com/app-2.0.0.exe"}
  → App downloads: 100 MB binary
  → App installs: new binary
```

**After (with VBDP):**
```
User clicks "Check for updates"
  → App queries: VBDP server API
  → Response: {version: "2.0.0", diff_url: "...", diff_size: "1 MB"}
  → App downloads: 1 MB diff
  → App applies: patch in-place
  → App verifies: signature and hash
```

**Code integration:**
- Link against VBDP client library (C, C++, Rust, Go, Python)
- Replace HTTP download code with VBDP API calls
- Keep existing UI (just update progress from VBDP callbacks)

### Scenario 2: Package Manager Integration

**Use Case:** Linux package manager (apt, yum, pacman)

**Current State:**
- Users run: `apt update && apt upgrade`
- Downloads complete .deb/.rpm packages
- Large bandwidth usage for frequent updates

**Integration Approach:**

**Create plugin for package manager:**

**apt-vbdp Plugin (Debian/Ubuntu):**
- Intercept package download requests
- Check if VBDP diff available for package
- If yes: download diff instead of full package
- Apply diff to create new .deb
- Pass to apt for installation

**Implementation:**
- APT supports custom download methods (`/usr/lib/apt/methods/`)
- Create `vbdp` method handler
- Configure in `/etc/apt/sources.list.d/vbdp.list`

**Benefits:**
- Transparent to user (still uses `apt upgrade`)
- Bandwidth savings for system updates
- Faster updates on slow connections

**Limitations:**
- Requires VBDP support on package repository server
- Not all packages benefit (very small packages have overhead)

### Scenario 3: Continuous Deployment (CD) Integration

**Use Case:** Deploy application updates to server fleet

**Current State:**
- CI/CD pipeline builds new version
- Ansible/Puppet/Chef pushes update to servers
- Full binary copied to all servers
- Large bandwidth usage in data center

**Integration Approach:**

**Integrate VBDP into deployment pipeline:**

**CI/CD Pipeline:**
```
Build → Test → Package → Register with VBDP → Sign → Publish
```

**Deployment:**
```
Servers run VBDP client → Check for updates → Download diff → Apply patch
```

**Benefits:**
- Faster rollouts (less data transferred)
- Lower network congestion in data center
- Gradual rollout support (canary deployments)

**Example with Ansible:**

**Traditional playbook:**
```yaml
- name: Update application
  copy:
    src: /builds/app-2.0.0
    dest: /opt/app/bin/app
  notify: restart app
```

**With VBDP:**
```yaml
- name: Trigger VBDP update
  command: vbdp check --apply-immediately
  notify: restart app
```

VBDP client on server handles download and patching.

### Scenario 4: Container/Docker Integration

**Use Case:** Docker image updates

**Current State:**
- Build new Docker image for each version
- Push entire image to registry (even if only small changes)
- Kubernetes pulls full images

**Integration Approach:**

**Option A: VBDP for application binaries inside container**

- Container includes VBDP client
- Application binary updated via VBDP
- Container image rarely changes (only for dependency updates)

**Benefits:**
- Smaller image pulls (base image cached)
- Application updates via VBDP (fast)

**Option B: Layer-aware differential pulls**

- Extend Docker registry to serve layer diffs
- Use VBDP algorithm for layer diffs
- Docker client applies diffs to existing layers

**Current Status:**
- Experimental (requires Docker/containerd modifications)
- Potential for future integration

### Scenario 5: Mobile App Stores (Limited)

**Use Case:** Mobile apps (iOS, Android)

**Current State:**
- App updates distributed via App Store / Play Store
- Full APK/IPA downloaded
- OS applies update

**VBDP Integration (Limited):**

**Android:**
- VBDP can update:
  - APK expansion files (OBB)
  - In-app assets (game content, databases)
  - Native libraries (SO files)
- **Cannot:** Update main APK (Google Play restriction)

**iOS:**
- VBDP can update:
  - In-app data files
  - Asset bundles
- **Cannot:** Update application binary (App Store restriction)

**Use Case:** Games with large asset files

- Base game: 50 MB (via App Store)
- Assets: 2 GB (downloaded via VBDP)
- Asset updates: VBDP diff (50 MB instead of 2 GB)

---

## Integration with Enterprise Systems

### Active Directory / Group Policy (Windows)

**Deployment:**
- Deploy VBDP client via Group Policy
- Configure update server URL via ADMX template
- Enforce policies (auto-update, schedule)

**Group Policy Settings:**

**Administrative Template (vbdp.admx):**
```xml
<policy name="UpdateServerURL" class="Machine">
  <parentCategory ref="VBDP" />
  <supportedOn ref="windows.10" />
  <elements>
    <text id="UpdateServerURL" valueName="UpdateServerURL" required="true" />
  </elements>
</policy>

<policy name="AutoUpdateEnabled" class="Machine">
  <parentCategory ref="VBDP" />
  <enabledValue><decimal value="1" /></enabledValue>
  <disabledValue><decimal value="0" /></disabledValue>
</policy>
```

**Deployment via GPO:**
1. Create GPO: "VBDP Client Configuration"
2. Link to appropriate OU
3. Settings:
   - Update Server URL: `https://updates.company.internal`
   - Auto-update: Enabled
   - Check interval: 4 hours
4. Apply to all workstations

### SCCM (System Center Configuration Manager)

**Application Deployment:**

1. **Create Application:**
   - In SCCM Console: Software Library → Applications
   - New Application: VBDP Client Patcher
   - Deployment Type: Windows Installer (MSI)
   - Detection Method: Registry key or file version

2. **Distribute Content:**
   - Distribute to distribution points

3. **Deploy:**
   - Deploy to device collections
   - Installation behavior: Install for system
   - Deployment purpose: Required
   - Availability: As soon as possible

**Package for VBDP-Enabled Applications:**

- Deploy application with VBDP public key embedded
- VBDP client patcher handles updates automatically
- Monitor via SCCM compliance reports

### Jamf Pro (macOS MDM)

**Configuration Profile:**

Create configuration profile (`.mobileconfig`) for VBDP client:

```xml
<dict>
  <key>PayloadType</key>
  <string>io.vbdp.client</string>
  <key>PayloadVersion</key>
  <integer>1</integer>
  <key>UpdateServerURL</key>
  <string>https://updates.company.com</string>
  <key>AutoUpdateEnabled</key>
  <true/>
  <key>CheckIntervalHours</key>
  <integer>4</integer>
</dict>
```

**Deploy via Jamf:**
1. Upload PKG installer
2. Create policy
3. Scope to computer groups
4. Deploy configuration profile
5. Monitor installation status

### Puppet / Ansible / Chef

**Puppet Module:**

```puppet
class vbdp::client (
  $update_server_url = 'https://updates.example.com',
  $auto_update = true,
  $check_interval_hours = 4,
) {
  package { 'vbdp-client':
    ensure => installed,
  }

  file { '/etc/vbdp/config.toml':
    ensure  => file,
    content => template('vbdp/config.toml.erb'),
    notify  => Service['vbdp'],
  }

  service { 'vbdp':
    ensure => running,
    enable => true,
  }
}
```

**Ansible Role:**

```yaml
---
- name: Install VBDP client
  hosts: all
  roles:
    - role: vbdp.client
      vbdp_update_server: https://updates.example.com
      vbdp_auto_update: true
```

**Chef Cookbook:**

```ruby
package 'vbdp-client'

template '/etc/vbdp/config.toml' do
  source 'config.toml.erb'
  variables({
    update_server_url: 'https://updates.example.com'
  })
  notifies :restart, 'service[vbdp]'
end

service 'vbdp' do
  action [:enable, :start]
end
```

---

## API Integration

### REST API Integration

**Use Case:** Custom integration from existing system

**VBDP Server provides REST API:**

**Check for Update:**
```
GET /api/check-update?app=myapp&version=1.0.0&platform=linux
```

**Response:**
```json
{
  "update_available": true,
  "target_version": "1.1.0",
  "diff_url": "https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0.diff",
  "diff_size": 1048576,
  "signature_url": "https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0.sig",
  "full_binary_url": "https://cdn.example.com/binaries/myapp-1.1.0.bin"
}
```

**Download Diff:**
```
GET https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0.diff
```

**Verify Signature:**
- Download signature file
- Verify using Ed25519 public key

**Apply Patch:**
- Use bspatch library to apply diff
- Verify resulting binary hash

**Report Status (Optional):**
```
POST /api/report-status
Content-Type: application/json

{
  "app": "myapp",
  "old_version": "1.0.0",
  "new_version": "1.1.0",
  "success": true,
  "duration_seconds": 15,
  "bandwidth_saved_bytes": 98000000
}
```

### SDK Integration

**Language SDKs (Planned for Phase 3):**

**JavaScript/TypeScript:**
```javascript
import { VBDPClient } from '@vbdp/client';

const client = new VBDPClient({
  updateServerUrl: 'https://updates.example.com',
  appName: 'myapp',
  currentVersion: '1.0.0',
  publicKey: PUBLIC_KEY_BYTES
});

// Check for update
const update = await client.checkForUpdate();

if (update.available) {
  // Download and apply
  await client.applyUpdate(update, {
    onProgress: (progress) => {
      console.log(`Progress: ${progress.percent}%`);
    }
  });
}
```

**Python:**
```python
from vbdp import VBDPClient

client = VBDPClient(
    update_server_url='https://updates.example.com',
    app_name='myapp',
    current_version='1.0.0',
    public_key=PUBLIC_KEY_BYTES
)

# Check for update
update = client.check_for_update()

if update.available:
    # Download and apply
    client.apply_update(update, progress_callback=print_progress)
```

**Go:**
```go
import "github.com/vbdp/go-client"

client := vbdp.NewClient(vbdp.Config{
    UpdateServerURL: "https://updates.example.com",
    AppName:         "myapp",
    CurrentVersion:  "1.0.0",
    PublicKey:       publicKeyBytes,
})

// Check for update
update, err := client.CheckForUpdate(context.Background())
if err != nil {
    log.Fatal(err)
}

if update.Available {
    // Apply update
    err = client.ApplyUpdate(context.Background(), update,
        vbdp.WithProgressCallback(printProgress))
}
```

---

## Monitoring Integration

### Prometheus Integration

**VBDP Server exposes Prometheus metrics:**

**Scrape Configuration:**

Add to `prometheus.yml`:
```yaml
scrape_configs:
  - job_name: 'vbdp-server'
    static_configs:
      - targets: ['updates.example.com:9090']
    metrics_path: '/metrics'
```

**Available Metrics:**
- `vbdp_api_requests_total{endpoint, status}`
- `vbdp_api_request_duration_seconds{endpoint}`
- `vbdp_diff_generation_duration_seconds{algorithm}`
- `vbdp_updates_total{app, from_version, to_version, method, success}`
- `vbdp_bandwidth_saved_bytes_total`

**Grafana Dashboard:**

Import pre-built dashboard (ID: 12345 from grafana.com) or create custom:

**Panels:**
- Update success rate (time series)
- Bandwidth saved (cumulative)
- Response time p50/p95/p99 (graph)
- Version distribution (pie chart)
- Active updates in progress (gauge)

### SIEM Integration (Splunk, ELK)

**Log Export:**

VBDP server outputs structured JSON logs:

```json
{
  "timestamp": "2026-01-07T12:34:56Z",
  "level": "info",
  "event": "update_applied",
  "app": "myapp",
  "old_version": "1.0.0",
  "new_version": "1.1.0",
  "device_id": "abc123",
  "success": true,
  "duration_seconds": 12,
  "bandwidth_saved_bytes": 99000000
}
```

**Splunk:**

**Forwarder configuration:**
```
[monitor:///var/log/vbdp/server.log]
disabled = false
sourcetype = vbdp
index = applications
```

**Search queries:**
```
sourcetype=vbdp event=update_applied
| stats count by app, new_version
| sort -count
```

**ELK Stack:**

**Filebeat configuration:**
```yaml
filebeat.inputs:
  - type: log
    enabled: true
    paths:
      - /var/log/vbdp/server.log
    json.keys_under_root: true
    json.add_error_key: true

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
  index: "vbdp-%{+yyyy.MM.dd}"
```

**Kibana visualization:**
- Create index pattern: `vbdp-*`
- Visualize update success rate, bandwidth saved, version distribution

### DataDog / New Relic Integration

**DataDog Agent:**

**Custom metrics:**
```python
from datadog import statsd

# In VBDP server code
statsd.increment('vbdp.update.success', tags=['app:myapp', 'version:1.1.0'])
statsd.histogram('vbdp.diff.size', diff_size_bytes)
```

**Dashboard:**
- Update success rate by app
- Diff generation time (histogram)
- Bandwidth saved (counter)

**New Relic:**

**APM Integration:**
- Instrument VBDP server with New Relic APM agent
- Monitor API endpoint performance
- Track errors and exceptions

---

## Migration Strategies

### Strategy 1: Gradual Migration (Recommended)

**Phase 1: Pilot (Month 1-2)**
- Deploy VBDP for single application
- 10-20% of users (internal beta)
- Monitor metrics, gather feedback

**Phase 2: Expansion (Month 3-4)**
- Roll out to 50% of users
- Add more applications
- Tune configuration

**Phase 3: Full Deployment (Month 5-6)**
- All users migrated
- Deprecate old update mechanism
- Monitor long-term stability

**Benefits:**
- Low risk (can rollback at any phase)
- Time to fix issues before full deployment
- Gradual learning curve for operations team

### Strategy 2: Parallel Operation

**Long-term coexistence:**
- VBDP for frequent updates (daily/weekly)
- Traditional for major version upgrades
- Users on slow connections use VBDP
- Users on fast connections may use either

**Example:**
- Minor updates (1.0.0 → 1.0.1): VBDP diff
- Major updates (1.x → 2.0): Full download with installer

**Benefits:**
- Flexibility
- Optimize for each use case
- No forced migration

### Strategy 3: Hybrid Approach

**Per-application decision:**
- Large applications (>100 MB): Use VBDP
- Small utilities (<10 MB): Traditional updates (overhead not worth it)
- Frequently updated apps: VBDP
- Rarely updated apps: Traditional

**Benefits:**
- Right tool for right job
- Maximize efficiency

---

## Compatibility Considerations

### Backward Compatibility

**Supporting old clients:**

**Scenario:** Users on old version without VBDP client

**Solution:**
- VBDP server provides full binary download
- Fallback URL in update metadata
- Old clients download full binary as before

**Migration:**
- New installations include VBDP client
- Existing users upgrade to VBDP-enabled version
- Eventually, all users on VBDP

### Cross-Platform Considerations

**Same update server, multiple platforms:**

**API supports platform parameter:**
```
GET /api/check-update?app=myapp&version=1.0.0&platform=windows&arch=x86_64
```

**Server returns platform-specific diffs:**
- `myapp-1.0.0-to-1.1.0-windows-x86_64.diff`
- `myapp-1.0.0-to-1.1.0-linux-x86_64.diff`
- `myapp-1.0.0-to-1.1.0-macos-arm64.diff`

**Benefits:**
- Single infrastructure for all platforms
- Consistent update experience
- Centralized monitoring

---

## Troubleshooting Integration Issues

### Issue: Old system and VBDP conflict

**Symptoms:**
- Both systems trying to update simultaneously
- Version mismatch

**Solutions:**
- Disable old update mechanism
- Or: Use VBDP as exclusive updater
- Or: Coordinate update schedules

### Issue: Firewall blocking VBDP

**Symptoms:**
- Updates not downloading
- "Connection timeout" errors

**Solutions:**
- Allow outbound HTTPS (port 443) to update server
- Whitelist update server domain in firewall/proxy
- Configure proxy settings in VBDP client

### Issue: Performance degradation

**Symptoms:**
- Slow updates
- High CPU usage during patch application

**Solutions:**
- Adjust CPU limits in configuration
- Schedule updates during off-peak hours
- Use faster diff algorithm (trade diff size for speed)

---

## Best Practices

**Start Small:**
- Pilot with single application
- Monitor closely
- Expand gradually

**Maintain Fallback:**
- Always provide full binary download option
- Automatic fallback if diff fails
- Don't remove old update mechanism immediately

**Monitor Everything:**
- Success rates
- Bandwidth savings
- Error rates
- User feedback

**Plan for Rollback:**
- Document rollback procedure
- Test rollback before production
- Have manual override available

**Communicate:**
- Inform users about new update mechanism
- Set expectations (faster updates)
- Provide support channels

---

## Next Steps

**For System Integrators:**
- Identify integration points in your current system
- Plan pilot deployment
- Set up monitoring and metrics

**For Enterprise IT:**
- Review deployment options (GPO, SCCM, Jamf)
- Plan rollout to user base
- Configure centralized management

**For Developers:**
- Integrate VBDP SDK into your application
- Test update flow
- Publish first VBDP-enabled release

**For More Information:**
- Read: [Client Installation](../deployment/CLIENT_INSTALLATION.md)
- Read: [Server Deployment](../deployment/SERVER_DEPLOYMENT.md)
- Read: [Operations Guide](../operations/MAINTENANCE.md)

---

**End of Existing Systems Integration Guide**
