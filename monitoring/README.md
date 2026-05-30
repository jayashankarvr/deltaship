# Deltaship Monitoring Resources

This directory contains production-ready monitoring resources for Deltaship infrastructure.

## Contents

### 1. Grafana Dashboard (`grafana-dashboard.json`)

Pre-built Grafana dashboard for comprehensive Deltaship monitoring.

**Features:**
- System health overview (uptime, request rate, error rate)
- API performance metrics (latency percentiles, status codes)
- Update statistics (success rate, bandwidth savings)
- Diff generation performance
- Database performance (query duration, connection pool)
- System resources (CPU, memory, disk, network)
- Storage operations

**Import Instructions:**

1. **Via Grafana UI:**
   - Open Grafana → Dashboards → Import
   - Upload `grafana-dashboard.json`
   - Select Prometheus data source
   - Click Import

2. **Via API:**
   ```bash
   curl -X POST http://grafana:3000/api/dashboards/db \
     -H "Content-Type: application/json" \
     -H "Authorization: Bearer YOUR_API_KEY" \
     -d @grafana-dashboard.json
   ```

3. **Via provisioning (recommended for automation):**
   ```bash
   # Copy to Grafana provisioning directory
   cp grafana-dashboard.json /etc/grafana/provisioning/dashboards/

   # Create provisioning config
   cat > /etc/grafana/provisioning/dashboards/deltaship.yml <<EOF
   apiVersion: 1
   providers:
     - name: 'Deltaship'
       folder: 'Deltaship'
       type: file
       options:
         path: /etc/grafana/provisioning/dashboards
   EOF

   # Restart Grafana
   systemctl restart grafana-server
   ```

**Customization:**
- Update data source name if different from "Prometheus"
- Adjust time ranges as needed
- Add organization-specific panels
- Configure alerting on panels

### 2. Prometheus Alert Rules (`prometheus-alerts.yml`)

Production-ready Prometheus alert rules covering critical to low-severity incidents.

**Alert Categories:**
- **Critical:** Server down, database down, high error rate, out of memory, disk full
- **High:** High latency, low update success rate, connection pool exhaustion, storage failures
- **Medium:** Disk space warning, low bandwidth savings, slow queries, slow diff generation
- **Low:** Certificate expiry, high client errors, unusual traffic patterns
- **Security:** Signature verification failures, unusual API key usage, rate limiting

**Setup Instructions:**

1. **Add to Prometheus configuration:**
   ```yaml
   # In prometheus.yml
   rule_files:
     - "/etc/prometheus/rules/prometheus-alerts.yml"
   ```

2. **Copy alert rules:**
   ```bash
   sudo cp prometheus-alerts.yml /etc/prometheus/rules/
   sudo chown prometheus:prometheus /etc/prometheus/rules/prometheus-alerts.yml
   ```

3. **Reload Prometheus:**
   ```bash
   # Send SIGHUP to reload config
   sudo kill -HUP $(pgrep prometheus)

   # Or use API
   curl -X POST http://localhost:9090/-/reload
   ```

4. **Verify rules loaded:**
   - Visit http://prometheus:9090/rules
   - Check for "deltaship_*_alerts" groups
   - Ensure all rules are green (no errors)

**Alert Testing:**
```bash
# Test alert expression
curl -G 'http://prometheus:9090/api/v1/query' \
  --data-urlencode 'query=up{job="deltaship-server"} == 0'

# Trigger test alert (manually fire)
curl -X POST 'http://alertmanager:9093/api/v1/alerts' \
  -H 'Content-Type: application/json' \
  -d '[{
    "labels": {"alertname": "TestAlert", "severity": "low"},
    "annotations": {"summary": "Test alert"}
  }]'
```

### 3. Alertmanager Configuration

Example Alertmanager configuration for routing Deltaship alerts:

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m

route:
  group_by: ['alertname', 'severity', 'component']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'default'

  routes:
    # Critical alerts → PagerDuty
    - match:
        severity: critical
      receiver: 'pagerduty'
      continue: true

    # High severity → Slack urgent
    - match:
        severity: high
      receiver: 'slack-urgent'

    # Medium/Low → Slack
    - match_re:
        severity: (medium|low)
      receiver: 'slack'

receivers:
  - name: 'default'
    email_configs:
      - to: 'ops@example.com'

  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: 'YOUR_PAGERDUTY_KEY'
        description: '{{ .GroupLabels.alertname }}: {{ .CommonAnnotations.summary }}'

  - name: 'slack-urgent'
    slack_configs:
      - api_url: 'YOUR_SLACK_WEBHOOK'
        channel: '#deltaship-alerts'
        title: '🚨 {{ .GroupLabels.alertname }}'
        text: |
          *Summary:* {{ .CommonAnnotations.summary }}
          *Runbook:* {{ .CommonAnnotations.runbook }}
          <!channel>

  - name: 'slack'
    slack_configs:
      - api_url: 'YOUR_SLACK_WEBHOOK'
        channel: '#deltaship-alerts'
        title: '⚠️ {{ .GroupLabels.alertname }}'
        text: '{{ .CommonAnnotations.summary }}'
```

## Quick Start

### Minimal Setup (5 minutes)

1. **Import Grafana dashboard:**
   ```bash
   # Assumes Grafana running on localhost:3000
   curl -X POST http://admin:admin@localhost:3000/api/dashboards/db \
     -H "Content-Type: application/json" \
     -d @grafana-dashboard.json
   ```

2. **Add Prometheus alerts:**
   ```bash
   sudo cp prometheus-alerts.yml /etc/prometheus/rules/
   sudo kill -HUP $(pgrep prometheus)
   ```

3. **Verify monitoring:**
   - Grafana: http://localhost:3000
   - Prometheus: http://localhost:9090
   - Alerts: http://localhost:9090/alerts

### Production Setup

See [../docs/operations/MONITORING.md](../docs/operations/MONITORING.md) for comprehensive setup including:
- Prometheus scrape configuration
- Alertmanager setup
- High availability configuration
- Log aggregation (ELK/Loki)
- Distributed tracing (optional)

## Metrics Reference

### Core Metrics

**API Metrics:**
- `deltaship_api_requests_total` - Total API requests by endpoint and status
- `deltaship_api_request_duration_seconds` - Request duration histogram
- `deltaship_api_active_connections` - Current active connections

**Update Metrics:**
- `deltaship_updates_total` - Total updates by app, method, success
- `deltaship_update_duration_seconds` - Update duration histogram
- `deltaship_bandwidth_saved_bytes_total` - Bandwidth saved via diffs

**Diff Metrics:**
- `deltaship_diff_generation_duration_seconds` - Diff generation time
- `deltaship_diff_size_bytes` - Diff size by version transition

**Database Metrics:**
- `deltaship_database_query_duration_seconds` - Query latency
- `deltaship_database_connections` - Connection pool state

**Storage Metrics:**
- `deltaship_storage_used_bytes` - Storage usage by type
- `deltaship_storage_operations_total` - Storage operations by type and status

**System Metrics (via Node Exporter):**
- `node_cpu_seconds_total` - CPU usage
- `node_memory_*` - Memory metrics
- `node_filesystem_*` - Disk metrics
- `node_network_*` - Network I/O

See [MONITORING.md](../docs/operations/MONITORING.md) for complete metric definitions.

## Alert Severity Guide

### Critical (Immediate Response)
- **Impact:** Service down or severely degraded
- **Response:** Immediate (< 5 minutes)
- **Notification:** PagerDuty page
- **Examples:** Server down, database down, out of memory

### High (Urgent Action)
- **Impact:** Service degraded, affecting users
- **Response:** Within 15-30 minutes
- **Notification:** Slack @channel mention
- **Examples:** High latency, low success rate, high CPU

### Medium (Should Address)
- **Impact:** Potential future issues
- **Response:** Within business hours
- **Notification:** Slack notification
- **Examples:** Disk space warning, slow queries

### Low (Informational)
- **Impact:** Informational or minor issues
- **Response:** During normal maintenance
- **Notification:** Slack notification
- **Examples:** Certificate expiry (30 days), unusual patterns

## Troubleshooting

### Dashboard Not Showing Data

1. **Check Prometheus scraping:**
   ```bash
   curl http://localhost:9090/api/v1/targets
   ```
   Verify `deltaship-server` target is UP

2. **Check metrics endpoint:**
   ```bash
   curl http://deltaship-server:8080/metrics
   ```
   Should return Prometheus metrics

3. **Check data source in Grafana:**
   - Settings → Data Sources → Prometheus
   - Test connection
   - Verify URL is correct

### Alerts Not Firing

1. **Check alert evaluation:**
   ```bash
   curl http://localhost:9090/api/v1/rules
   ```

2. **Test alert query manually:**
   ```bash
   curl -G 'http://localhost:9090/api/v1/query' \
     --data-urlencode 'query=up{job="deltaship-server"} == 0'
   ```

3. **Check Alertmanager:**
   ```bash
   curl http://localhost:9093/api/v1/alerts
   ```

### High Cardinality Issues

If Prometheus performance degrades:

1. **Check series count:**
   ```promql
   count({__name__=~".+"})
   ```

2. **Identify high-cardinality metrics:**
   ```promql
   topk(10, count by (__name__)({__name__=~".+"}))
   ```

3. **Consider:**
   - Reducing label dimensions
   - Increasing retention settings
   - Using recording rules

## Related Documentation

- **Monitoring Guide:** [../docs/operations/MONITORING.md](../docs/operations/MONITORING.md)
- **Runbooks:** [../docs/RUNBOOKS.md](../docs/RUNBOOKS.md)
- **Deployment Checklist:** [../docs/DEPLOYMENT_CHECKLIST.md](../docs/DEPLOYMENT_CHECKLIST.md)
- **System Design:** [../docs/architecture/SYSTEM_DESIGN.md](../docs/architecture/SYSTEM_DESIGN.md)

## Support

- **Documentation:** https://docs.deltaship.io
- **Issues:** https://github.com/jayashankarvr/deltaship/issues
- **Discussions:** https://github.com/jayashankarvr/deltaship/discussions

---

**Last Updated:** 2026-01-14
