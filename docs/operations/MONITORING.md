# Monitoring and Observability

**Document:** Comprehensive monitoring setup for Deltaship infrastructure
**Audience:** Operations teams, SREs, DevOps engineers
**Last Updated:** 2026-01-07

---

## Overview

This document describes monitoring and observability strategies for Deltaship, including metrics, logging, tracing, and alerting configurations.

**Monitoring Stack:**
- **Metrics:** Prometheus + Grafana
- **Logging:** Structured JSON logs + ELK Stack or Loki
- **Tracing:** OpenTelemetry (optional, for advanced debugging)
- **Alerting:** Alertmanager + PagerDuty/Opsgenie

---

## Architecture

```
┌─────────────────┐
│  Deltaship Services  │
│  (Server,       │
│   Client, etc.) │
└────────┬────────┘
         │
         │ Metrics (Prometheus format)
         │ Logs (JSON)
         │ Traces (OpenTelemetry)
         ▼
┌─────────────────┐      ┌──────────────────┐
│   Prometheus    │─────▶│    Grafana       │
│  (Metrics DB)   │      │  (Dashboards)    │
└─────────────────┘      └──────────────────┘
         │
         ▼
┌─────────────────┐      ┌──────────────────┐
│  Alertmanager   │─────▶│    PagerDuty     │
│  (Alert Router) │      │   (Incidents)    │
└─────────────────┘      └──────────────────┘

┌─────────────────┐      ┌──────────────────┐
│   Loki / ELK    │─────▶│    Grafana       │
│  (Log Storage)  │      │  (Log Viewer)    │
└─────────────────┘      └──────────────────┘
```

---

## Metrics

### Prometheus Exposition

**Update Server exposes metrics at:**
```
http://server:9090/metrics
```

**Format:** Prometheus text format

**Example:**
```
# HELP deltaship_api_requests_total Total API requests
# TYPE deltaship_api_requests_total counter
deltaship_api_requests_total{endpoint="/api/check-update",status="200"} 123456

# HELP deltaship_api_request_duration_seconds API request duration
# TYPE deltaship_api_request_duration_seconds histogram
deltaship_api_request_duration_seconds_bucket{endpoint="/api/check-update",le="0.1"} 100000
deltaship_api_request_duration_seconds_bucket{endpoint="/api/check-update",le="0.5"} 120000
deltaship_api_request_duration_seconds_sum{endpoint="/api/check-update"} 45678.9
deltaship_api_request_duration_seconds_count{endpoint="/api/check-update"} 123456
```

### Core Metrics

#### API Metrics

**Request Rate:**
```
deltaship_api_requests_total{endpoint, status}
```
- Labels: endpoint (`/api/check-update`, `/api/download-diff`, etc.), status (HTTP status code)
- Type: Counter

**Request Duration:**
```
deltaship_api_request_duration_seconds{endpoint}
```
- Type: Histogram
- Buckets: 0.01, 0.05, 0.1, 0.5, 1, 2, 5, 10

**Active Connections:**
```
deltaship_api_active_connections
```
- Type: Gauge

#### Update Metrics

**Total Updates:**
```
deltaship_updates_total{app, from_version, to_version, method, success}
```
- Labels:
  - app: Application name
  - from_version, to_version: Version transition
  - method: "diff" or "full"
  - success: "true" or "false"
- Type: Counter

**Update Duration:**
```
deltaship_update_duration_seconds{app, method}
```
- Type: Histogram

**Bandwidth Saved:**
```
deltaship_bandwidth_saved_bytes_total{app}
```
- Type: Counter

#### Diff Generation Metrics

**Diff Generation Duration:**
```
deltaship_diff_generation_duration_seconds{algorithm}
```
- Labels: algorithm ("bsdiff", "courgette", "xdelta3")
- Type: Histogram

**Diff Size:**
```
deltaship_diff_size_bytes{app, from_version, to_version}
```
- Type: Gauge

#### Storage Metrics

**Storage Used:**
```
deltaship_storage_used_bytes{type}
```
- Labels: type ("binaries", "diffs", "signatures")
- Type: Gauge

**Storage Operations:**
```
deltaship_storage_operations_total{operation, status}
```
- Labels: operation ("upload", "download", "delete"), status ("success", "error")
- Type: Counter

#### Database Metrics

**Query Duration:**
```
deltaship_database_query_duration_seconds{query_type}
```
- Labels: query_type ("check_update", "register_version", "analytics")
- Type: Histogram

**Connection Pool:**
```
deltaship_database_connections{state}
```
- Labels: state ("active", "idle", "waiting")
- Type: Gauge

### System Metrics (via Node Exporter)

**CPU Usage:**
```
node_cpu_seconds_total
```

**Memory Usage:**
```
node_memory_MemAvailable_bytes
node_memory_MemTotal_bytes
```

**Disk Usage:**
```
node_filesystem_avail_bytes
node_filesystem_size_bytes
```

**Network I/O:**
```
node_network_receive_bytes_total
node_network_transmit_bytes_total
```

---

## Prometheus Configuration

### Scrape Config

Create `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  # Deltaship Update Server
  - job_name: 'deltaship-server'
    static_configs:
      - targets:
          - 'server1.example.com:9090'
          - 'server2.example.com:9090'
          - 'server3.example.com:9090'
    metrics_path: '/metrics'
    scheme: https
    tls_config:
      ca_file: /etc/prometheus/ca.crt

  # Node Exporter (system metrics)
  - job_name: 'node'
    static_configs:
      - targets:
          - 'server1.example.com:9100'
          - 'server2.example.com:9100'

  # PostgreSQL Exporter (database metrics)
  - job_name: 'postgres'
    static_configs:
      - targets:
          - 'db.example.com:9187'

  # Redis Exporter (cache metrics, if using Redis)
  - job_name: 'redis'
    static_configs:
      - targets:
          - 'redis.example.com:9121'
```

### Recording Rules

Create `rules/deltaship.yml`:

```yaml
groups:
  - name: deltaship_aggregations
    interval: 30s
    rules:
      # Request rate per endpoint (per second)
      - record: job:deltaship_api_requests:rate5m
        expr: rate(deltaship_api_requests_total[5m])

      # Error rate
      - record: job:deltaship_api_error_rate:rate5m
        expr: |
          rate(deltaship_api_requests_total{status=~"5.."}[5m])
          /
          rate(deltaship_api_requests_total[5m])

      # Update success rate
      - record: job:deltaship_update_success_rate:rate5m
        expr: |
          rate(deltaship_updates_total{success="true"}[5m])
          /
          rate(deltaship_updates_total[5m])

      # Bandwidth savings percentage
      - record: job:deltaship_bandwidth_savings:ratio
        expr: |
          (
            rate(deltaship_bandwidth_saved_bytes_total[5m])
            /
            (rate(deltaship_bandwidth_saved_bytes_total[5m]) + rate(deltaship_bandwidth_used_bytes_total[5m]))
          )
```

---

## Alerting

### Alertmanager Configuration

Create `alertmanager.yml`:

```yaml
global:
  resolve_timeout: 5m
  slack_api_url: 'https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK'
  pagerduty_url: 'https://events.pagerduty.com/v2/enqueue'

route:
  group_by: ['alertname', 'severity']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'default'

  routes:
    # Critical alerts go to PagerDuty
    - match:
        severity: critical
      receiver: 'pagerduty'
      continue: true

    # High alerts go to Slack with mention
    - match:
        severity: high
      receiver: 'slack-urgent'

    # Medium/Low alerts go to Slack
    - match_re:
        severity: (medium|low)
      receiver: 'slack'

receivers:
  - name: 'default'
    email_configs:
      - to: 'ops@example.com'

  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: 'YOUR_PAGERDUTY_SERVICE_KEY'
        description: '{{ .GroupLabels.alertname }}: {{ .CommonAnnotations.summary }}'

  - name: 'slack-urgent'
    slack_configs:
      - channel: '#deltaship-alerts'
        username: 'Deltaship Alertmanager'
        color: 'danger'
        title: '🚨 {{ .GroupLabels.alertname }}'
        text: |
          *Summary:* {{ .CommonAnnotations.summary }}
          *Description:* {{ .CommonAnnotations.description }}
          <!channel>

  - name: 'slack'
    slack_configs:
      - channel: '#deltaship-alerts'
        username: 'Deltaship Alertmanager'
        color: 'warning'
        title: '⚠️ {{ .GroupLabels.alertname }}'
        text: '{{ .CommonAnnotations.summary }}'
```

### Alert Rules

Create `alerts/deltaship.yml`:

```yaml
groups:
  - name: deltaship_alerts
    rules:
      # CRITICAL: Server Down
      - alert: DeltashipServerDown
        expr: up{job="deltaship-server"} == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Deltaship server {{ $labels.instance }} is down"
          description: "Server has been unreachable for more than 2 minutes"

      # CRITICAL: High Error Rate
      - alert: HighErrorRate
        expr: job:deltaship_api_error_rate:rate5m > 0.05
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "High error rate: {{ $value | humanizePercentage }}"
          description: "Error rate above 5% for 10 minutes"

      # CRITICAL: Database Down
      - alert: DatabaseDown
        expr: pg_up{job="postgres"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "PostgreSQL database is down"
          description: "Cannot connect to database"

      # HIGH: High API Latency
      - alert: HighAPILatency
        expr: histogram_quantile(0.99, rate(deltaship_api_request_duration_seconds_bucket[5m])) > 5
        for: 15m
        labels:
          severity: high
        annotations:
          summary: "API latency p99 is {{ $value }}s (>5s)"
          description: "99th percentile latency above threshold for 15 minutes"

      # HIGH: Low Update Success Rate
      - alert: LowUpdateSuccessRate
        expr: job:deltaship_update_success_rate:rate5m < 0.99
        for: 30m
        labels:
          severity: high
        annotations:
          summary: "Update success rate is {{ $value | humanizePercentage }} (<99%)"
          description: "Update failures above 1% for 30 minutes"

      # MEDIUM: High Disk Usage
      - alert: HighDiskUsage
        expr: |
          (
            node_filesystem_avail_bytes{mountpoint="/var/lib/deltaship"}
            /
            node_filesystem_size_bytes{mountpoint="/var/lib/deltaship"}
          ) < 0.1
        for: 1h
        labels:
          severity: medium
        annotations:
          summary: "Disk usage above 90% on {{ $labels.instance }}"
          description: "Less than 10% disk space remaining"

      # MEDIUM: Low Bandwidth Savings
      - alert: LowBandwidthSavings
        expr: job:deltaship_bandwidth_savings:ratio < 0.80
        for: 1h
        labels:
          severity: medium
        annotations:
          summary: "Bandwidth savings only {{ $value | humanizePercentage }} (<80%)"
          description: "Expected >90% savings, check diff quality"

      # LOW: Certificate Expiry Soon
      - alert: CertificateExpiringSoon
        expr: (probe_ssl_earliest_cert_expiry - time()) / 86400 < 30
        for: 1h
        labels:
          severity: low
        annotations:
          summary: "TLS certificate expires in {{ $value }} days"
          description: "Renew certificate before expiration"
```

---

## Grafana Dashboards

### Dashboard: Overview

**Panels:**

1. **System Health** (Row)
   - Server uptime (singlestat)
   - API request rate (graph)
   - Error rate (graph)
   - Active users (singlestat)

2. **Updates** (Row)
   - Update success rate (singlestat, green if >99%)
   - Updates per minute (graph)
   - Bandwidth saved today (singlestat with trend)
   - Version distribution (pie chart)

3. **Performance** (Row)
   - API latency p50/p95/p99 (graph)
   - Diff generation time (histogram)
   - Database query time (graph)

4. **Resources** (Row)
   - CPU usage (graph)
   - Memory usage (graph)
   - Disk I/O (graph)
   - Network traffic (graph)

**Example Panel JSON:**

```json
{
  "title": "API Latency (Percentiles)",
  "type": "graph",
  "targets": [
    {
      "expr": "histogram_quantile(0.50, rate(deltaship_api_request_duration_seconds_bucket[5m]))",
      "legendFormat": "p50"
    },
    {
      "expr": "histogram_quantile(0.95, rate(deltaship_api_request_duration_seconds_bucket[5m]))",
      "legendFormat": "p95"
    },
    {
      "expr": "histogram_quantile(0.99, rate(deltaship_api_request_duration_seconds_bucket[5m]))",
      "legendFormat": "p99"
    }
  ],
  "yAxes": [
    {
      "format": "s",
      "label": "Latency"
    }
  ]
}
```

### Dashboard: Application-Specific

**Per-Application Metrics:**

- Update timeline (when versions were published)
- Version distribution (% of users on each version)
- Update success rate per version
- Bandwidth savings per version
- Error breakdown by type

**Query Examples:**

**Version Distribution:**
```promql
count by (version) (
  deltaship_updates_total{app="myapp", success="true"}
)
```

**Bandwidth Savings:**
```promql
sum(rate(deltaship_bandwidth_saved_bytes_total{app="myapp"}[1h]))
```

---

## Logging

### Structured Logging Format

**JSON Log Format:**

```json
{
  "timestamp": "2026-01-07T12:34:56.789Z",
  "level": "info",
  "component": "api",
  "event": "update_requested",
  "request_id": "req_abc123",
  "metadata": {
    "app": "myapp",
    "current_version": "1.0.0",
    "target_version": "1.1.0",
    "device_id": "dev_xyz789",
    "ip_address": "192.168.1.100",
    "user_agent": "deltaship-client/1.0.0"
  },
  "duration_ms": 45
}
```

### Log Levels

**ERROR:** Failures requiring attention
**WARN:** Degraded performance or retries
**INFO:** Normal operations (version published, update applied)
**DEBUG:** Detailed troubleshooting (disabled in production)

### Logging Stack

#### Option A: ELK Stack

**Components:**
- **Elasticsearch:** Log storage and search
- **Logstash:** Log processing and enrichment
- **Kibana:** Log visualization and search UI

**Filebeat Configuration:**

```yaml
filebeat.inputs:
  - type: log
    enabled: true
    paths:
      - /var/log/deltaship/server.log
    json.keys_under_root: true
    json.add_error_key: true

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
  index: "deltaship-%{+yyyy.MM.dd}"
```

**Kibana Index Pattern:**
- Pattern: `deltaship-*`
- Time field: `@timestamp`

#### Option B: Loki

**Promtail Configuration:**

```yaml
server:
  http_listen_port: 9080

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://loki:3100/loki/api/v1/push

scrape_configs:
  - job_name: deltaship
    static_configs:
      - targets:
          - localhost
        labels:
          job: deltaship-server
          __path__: /var/log/deltaship/*.log
    pipeline_stages:
      - json:
          expressions:
            level: level
            component: component
            event: event
      - labels:
          level:
          component:
          event:
```

**Grafana Loki Query:**

```
{job="deltaship-server"} |= "error" | json | level="error"
```

### Log Retention

**Policy:**
- **Hot storage (Elasticsearch/Loki):** 30 days
- **Cold storage (S3):** 1 year
- **Deletion:** After 1 year

**Implementation:**

```bash
# Elasticsearch Index Lifecycle Management (ILM)
curl -X PUT "localhost:9200/_ilm/policy/deltaship_policy" -H 'Content-Type: application/json' -d'
{
  "policy": {
    "phases": {
      "hot": {
        "actions": {
          "rollover": {
            "max_age": "1d",
            "max_size": "50gb"
          }
        }
      },
      "delete": {
        "min_age": "30d",
        "actions": {
          "delete": {}
        }
      }
    }
  }
}
'
```

---

## Distributed Tracing (Optional)

### OpenTelemetry Integration

**For debugging complex flows:**

**Instrumentation:**

```
Span: Update Flow
  ├─ Span: Check Update API
  ├─ Span: Database Query
  ├─ Span: Diff Generation
  │   ├─ Span: Load Old Binary
  │   ├─ Span: Load New Binary
  │   └─ Span: Compute Diff
  ├─ Span: Sign Diff
  └─ Span: Upload to S3
```

**Jaeger UI:**
- Visualize request flow through system
- Identify bottlenecks
- Debug distributed transactions

**When to Use:**
- Investigating latency issues
- Understanding complex failures
- Optimizing performance

---

## Monitoring Best Practices

### 1. The Four Golden Signals

**Latency:** How long requests take
- Metric: `deltaship_api_request_duration_seconds`
- Target: p99 <1s

**Traffic:** How much demand
- Metric: `rate(deltaship_api_requests_total[5m])`
- Baseline: Track daily/weekly patterns

**Errors:** Rate of failed requests
- Metric: `rate(deltaship_api_requests_total{status=~"5.."}[5m])`
- Target: <1%

**Saturation:** How full the system is
- Metrics: CPU, memory, disk, connection pool usage
- Target: <80% under normal load

### 2. Avoid Alert Fatigue

**DO:**
- Alert on symptoms (user impact), not causes
- Group related alerts
- Use proper severity levels
- Set appropriate thresholds (not too sensitive)

**DON'T:**
- Alert on every single error
- Use same severity for everything
- Send alerts without actionable info

### 3. SLIs and SLOs

**Service Level Indicators (SLIs):**
- Update success rate
- API latency (p99)
- System uptime

**Service Level Objectives (SLOs):**
- Update success rate: >99%
- API latency p99: <1s
- Uptime: 99.9%

**Error Budget:**
- 99.9% uptime = 43 minutes downtime per month
- Track error budget consumption
- Alert when 50% of budget consumed

---

## Dashboards and Views

### Operations Dashboard

**For on-call engineers:**
- System health at a glance
- Active incidents
- Recent deployments
- Error rate trending

### Business Dashboard

**For stakeholders:**
- Total users
- Active updates per day
- Bandwidth savings (cost impact)
- Version adoption rates

### Debug Dashboard

**For troubleshooting:**
- Detailed error logs
- Slow queries
- Resource utilization
- Request traces

---

## Next Steps

**For Operations Teams:**
- Deploy Prometheus and Grafana
- Configure alerting rules
- Set up on-call rotation (PagerDuty)
- Create runbooks for common alerts

**For Developers:**
- Instrument code with metrics
- Add structured logging
- Test monitoring in staging

**Related Documents:**
- [Maintenance Guide](MAINTENANCE.md) - Operational procedures
- [System Design](../architecture/SYSTEM_DESIGN.md) - Architecture
- [Performance Targets](../technical/PERFORMANCE_TARGETS.md) - SLO targets

---

**End of Monitoring and Observability Guide**
