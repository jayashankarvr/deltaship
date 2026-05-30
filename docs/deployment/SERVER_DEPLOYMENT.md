# Server Deployment Guide

**Document:** Deployment procedures for Deltaship update server
**Audience:** System administrators, DevOps engineers, infrastructure teams
**Last Updated:** 2026-01-07

---

## Overview

This guide describes how to deploy the Deltaship Update Server, which serves binary diffs and manages update distribution to client patchers. The server can be deployed in various configurations from single-instance to globally distributed.

**Deployment Time:** 30 minutes (small) to 4 hours (production)
**Prerequisites:** Server infrastructure, database, object storage (for production)
**Supported Platforms:** Linux (primary), Docker/Kubernetes

---

## Deployment Options

### Small Deployment (< 10,000 users)

**Architecture:**
- Single server (all-in-one)
- Local filesystem storage
- SQLite database
- Direct client connections (no CDN)

**Resources:**
- 2 CPU cores
- 4 GB RAM
- 100 GB disk
- 10 Mbps bandwidth

**Cost:** ~$20-50/month (VPS)

**Use Cases:**
- Internal company tools
- Beta testing
- Small open-source projects

### Medium Deployment (10,000 - 100,000 users)

**Architecture:**
- 2-3 load-balanced API servers
- PostgreSQL database (with standby)
- S3/Azure Blob for storage
- CDN for global distribution

**Resources:**
- API Servers: 4 CPU cores, 8 GB RAM each
- Database: 4 CPU cores, 16 GB RAM
- Object storage: Pay-per-use
- CDN: Pay-per-bandwidth

**Cost:** ~$300-800/month

**Use Cases:**
- Commercial applications
- Popular open-source projects
- Enterprise internal deployment

### Large Deployment (100,000+ users)

**Architecture:**
- Auto-scaling API server fleet (10+ instances)
- PostgreSQL cluster (primary + 2 read replicas)
- Multi-region object storage
- Global CDN with edge caching
- Redis for metadata caching
- Separate analytics database

**Resources:**
- API Servers: Auto-scaling (4-50+ instances)
- Database cluster: High-availability setup
- Object storage: Multi-region replication
- CDN: Global PoPs (Points of Presence)

**Cost:** $2,000-10,000+/month (scales with usage)

**Use Cases:**
- Large-scale commercial software
- OS-level update systems
- Global SaaS platforms

---

## Prerequisites

### Infrastructure Requirements

**Compute:**
- Linux server (Ubuntu 22.04 LTS, Debian 12, or RHEL 9 recommended)
- x86_64 or ARM64 architecture
- Root or sudo access

**Network:**
- Public IP address (or load balancer)
- DNS record pointing to server
- Firewall access (ports 80, 443)
- TLS certificate (Let's Encrypt or commercial)

**Database:**
- **Small:** SQLite (bundled)
- **Medium/Large:** PostgreSQL 14+ (self-hosted or managed)
- Minimum 10 GB storage for database
- Backup strategy required

**Object Storage:**
- **Small:** Local filesystem
- **Medium/Large:** S3-compatible storage (AWS S3, MinIO, Azure Blob, GCS)
- Minimum 100 GB for binaries and diffs
- Lifecycle policies for old versions

### Software Requirements

**Operating System:**
- Ubuntu 22.04 LTS (recommended)
- Debian 12
- RHEL 9 / Rocky Linux 9
- Other systemd-based Linux distributions

**Runtime:**
- Rust 1.70+ (if building from source)
- Or: Use pre-built binaries

**Optional:**
- Docker 20.10+ and Docker Compose (for containerized deployment)
- Kubernetes 1.24+ (for orchestrated deployment)
- Nginx or Caddy (reverse proxy, optional)
- Redis 6+ (for caching, optional but recommended for medium/large)

---

## Small Deployment (All-in-One Server)

### Step 1: Server Preparation

**1.1 Update System:**

```
sudo apt update && sudo apt upgrade -y
```

**1.2 Install Dependencies:**

```
sudo apt install -y curl wget git sqlite3
```

**1.3 Create Service User:**

```
sudo useradd --system --shell /usr/sbin/nologin --create-home --home-dir /var/lib/deltaship deltaship
```

### Step 2: Install Deltaship Server

**Option A: Package Installation (Recommended)**

**Download package:**
```
wget https://releases.deltaship.io/server/deltaship-server_1.0.0_amd64.deb
```

**Install:**
```
sudo dpkg -i deltaship-server_1.0.0_amd64.deb
```

**Option B: Binary Installation**

**Download binary:**
```
wget https://releases.deltaship.io/server/deltaship-server-1.0.0-linux-x86_64.tar.gz
tar -xzf deltaship-server-1.0.0-linux-x86_64.tar.gz
```

**Install:**
```
sudo cp deltaship-server /usr/local/bin/
sudo chmod +x /usr/local/bin/deltaship-server
```

**Create directories:**
```
sudo mkdir -p /etc/deltaship-server
sudo mkdir -p /var/lib/deltaship/storage
sudo mkdir -p /var/lib/deltaship/database
sudo mkdir -p /var/log/deltaship
sudo chown -R deltaship:deltaship /var/lib/deltaship /var/log/deltaship
```

### Step 3: Configuration

**Create configuration file:**

```
sudo nano /etc/deltaship-server/config.toml
```

**Basic configuration:**

```toml
[server]
listen_address = "0.0.0.0:8080"
public_url = "https://updates.example.com"
workers = 4  # Number of worker threads

[database]
type = "sqlite"
path = "/var/lib/deltaship/database/deltaship.db"

[storage]
type = "filesystem"
root_path = "/var/lib/deltaship/storage"
max_storage_gb = 100

[diff]
default_algorithm = "bsdiff"
max_diff_size_mb = 500
precompute_recent_versions = 3  # Pre-compute diffs for last 3 versions

[security]
require_api_key = true
api_keys_file = "/etc/deltaship-server/api-keys.txt"

[analytics]
enabled = true
retention_days = 90

[logging]
level = "info"  # debug, info, warn, error
output = "file"
path = "/var/log/deltaship/server.log"
```

**Set permissions:**
```
sudo chown deltaship:deltaship /etc/deltaship-server/config.toml
sudo chmod 600 /etc/deltaship-server/config.toml
```

### Step 4: Create systemd Service

**Create service file:**

```
sudo nano /etc/systemd/system/deltaship-server.service
```

**Service configuration:**

```ini
[Unit]
Description=Deltaship Update Server
Documentation=https://docs.deltaship.io
After=network.target

[Service]
Type=simple
User=deltaship
Group=deltaship
WorkingDirectory=/var/lib/deltaship
ExecStart=/usr/local/bin/deltaship-server --config /etc/deltaship-server/config.toml
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/deltaship /var/log/deltaship

[Install]
WantedBy=multi-user.target
```

**Enable and start service:**

```
sudo systemctl daemon-reload
sudo systemctl enable deltaship-server
sudo systemctl start deltaship-server
```

**Verify:**

```
sudo systemctl status deltaship-server
```

Expected: "active (running)"

### Step 5: Reverse Proxy (Nginx)

**Install Nginx:**

```
sudo apt install -y nginx certbot python3-certbot-nginx
```

**Configure Nginx:**

```
sudo nano /etc/nginx/sites-available/deltaship
```

**Nginx configuration:**

```nginx
server {
    listen 80;
    server_name updates.example.com;

    # Redirect HTTP to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name updates.example.com;

    # TLS Certificate (will be configured by certbot)
    ssl_certificate /etc/letsencrypt/live/updates.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/updates.example.com/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "DENY" always;

    # Increase upload size (for large binaries)
    client_max_body_size 1G;

    # Proxy to Deltaship server
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Timeouts for long diff downloads
        proxy_connect_timeout 60s;
        proxy_send_timeout 300s;
        proxy_read_timeout 300s;
    }

    # Health check endpoint
    location /health {
        proxy_pass http://127.0.0.1:8080/health;
        access_log off;
    }
}
```

**Enable site:**

```
sudo ln -s /etc/nginx/sites-available/deltaship /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl restart nginx
```

**Obtain TLS certificate:**

```
sudo certbot --nginx -d updates.example.com
```

Follow prompts, certificate auto-renews.

### Step 6: Verification

**Check server health:**

```
curl https://updates.example.com/health
```

Expected response:
```json
{"status": "healthy", "version": "1.0.0", "uptime_seconds": 120}
```

**Test API:**

```
curl https://updates.example.com/api/check-update?app=test&version=1.0.0
```

Expected: JSON response (even if no updates available)

---

## Medium Deployment (Load-Balanced + PostgreSQL + S3)

### Architecture Overview

```
                  [CloudFlare CDN]
                         │
                    [Load Balancer]
                         │
          ┌──────────────┼──────────────┐
          │              │              │
    [API Server 1] [API Server 2] [API Server 3]
          │              │              │
          └──────────────┼──────────────┘
                         │
                 [PostgreSQL DB]
                   (with standby)
                         │
                   [Redis Cache]
                         │
                   [S3 Storage]
```

### Step 1: Database Setup (PostgreSQL)

**Option A: Managed Database (Recommended)**

Use managed PostgreSQL service:
- AWS RDS PostgreSQL
- Azure Database for PostgreSQL
- Google Cloud SQL
- DigitalOcean Managed Database

**Benefits:**
- Automatic backups
- High availability
- Managed updates

**Configuration:**
- Instance size: 2-4 vCPUs, 8-16 GB RAM
- Storage: 100 GB SSD (auto-scaling)
- Enable automated backups (7-30 day retention)
- Enable read replica (for analytics queries)

**Option B: Self-Hosted PostgreSQL**

**Install PostgreSQL:**

```
sudo apt install -y postgresql-14
```

**Create database and user:**

```
sudo -u postgres psql

CREATE DATABASE deltaship;
CREATE USER deltaship_server WITH ENCRYPTED PASSWORD 'STRONG_PASSWORD_HERE';
GRANT ALL PRIVILEGES ON DATABASE deltaship TO deltaship_server;
\q
```

**Configure PostgreSQL for remote access:**

Edit `/etc/postgresql/14/main/postgresql.conf`:
```
listen_addresses = '*'
max_connections = 200
shared_buffers = 4GB
```

Edit `/etc/postgresql/14/main/pg_hba.conf`:
```
host    deltaship    deltaship_server    10.0.0.0/8    md5
```

**Restart PostgreSQL:**
```
sudo systemctl restart postgresql
```

### Step 2: Object Storage Setup (S3)

**Option A: AWS S3**

**Create S3 bucket:**
```
aws s3 mb s3://deltaship-updates-prod
```

**Enable versioning:**
```
aws s3api put-bucket-versioning --bucket deltaship-updates-prod --versioning-configuration Status=Enabled
```

**Set lifecycle policy (delete old versions after 90 days):**

Create policy file `lifecycle.json`:
```json
{
  "Rules": [
    {
      "Id": "DeleteOldVersions",
      "Status": "Enabled",
      "NoncurrentVersionExpiration": {
        "NoncurrentDays": 90
      }
    }
  ]
}
```

Apply:
```
aws s3api put-bucket-lifecycle-configuration --bucket deltaship-updates-prod --lifecycle-configuration file://lifecycle.json
```

**Create IAM user with S3 access:**
```
aws iam create-user --user-name deltaship-server
aws iam create-access-key --user-name deltaship-server
```

Save access key ID and secret.

**Option B: MinIO (Self-Hosted S3-Compatible)**

**Install MinIO:**

```
wget https://dl.min.io/server/minio/release/linux-amd64/minio
sudo mv minio /usr/local/bin/
sudo chmod +x /usr/local/bin/minio
```

**Create storage directory:**
```
sudo mkdir -p /mnt/minio/data
```

**Run MinIO:**
```
minio server /mnt/minio/data --console-address ":9001"
```

**Access MinIO Console:** http://server-ip:9001

**Create bucket:** "deltaship-updates"

### Step 3: Redis Setup (Optional but Recommended)

**Install Redis:**

```
sudo apt install -y redis-server
```

**Configure Redis:**

Edit `/etc/redis/redis.conf`:
```
maxmemory 2gb
maxmemory-policy allkeys-lru
bind 127.0.0.1
```

**Restart Redis:**
```
sudo systemctl restart redis
```

### Step 4: API Server Configuration

**Update configuration for production:**

```toml
[server]
listen_address = "0.0.0.0:8080"
public_url = "https://updates.example.com"
workers = 8

[database]
type = "postgresql"
host = "postgres.internal.example.com"
port = 5432
database = "deltaship"
username = "deltaship_server"
password = "STRONG_PASSWORD_HERE"
pool_size = 20

[storage]
type = "s3"
bucket = "deltaship-updates-prod"
region = "us-east-1"
access_key_id = "AKIAxxxxxxxxxxxx"
secret_access_key = "SECRET_KEY_HERE"
# Or use IAM role (recommended on AWS)
use_iam_role = true

[cache]
type = "redis"
url = "redis://127.0.0.1:6379"
ttl_seconds = 3600
enabled = true

[diff]
default_algorithm = "bsdiff"
max_diff_size_mb = 500
precompute_recent_versions = 5
on_demand_enabled = true
on_demand_cache_ttl_days = 30

[rollout]
gradual_rollout_enabled = true
default_rollout_percentage = 10  # Start with 10% of users
rollout_increase_interval_hours = 24

[analytics]
enabled = true
retention_days = 365
export_to_prometheus = true

[security]
require_api_key = true
rate_limit_enabled = true
rate_limit_requests_per_minute = 1000

[logging]
level = "info"
output = "json"  # Structured logging for aggregation
path = "/var/log/deltaship/server.log"
```

### Step 5: Load Balancer Setup

**Option A: Cloud Load Balancer**

**AWS Application Load Balancer:**
- Create ALB in AWS console
- Add target group (API servers on port 8080)
- Configure health check: `/health`
- Enable HTTPS listener (upload TLS cert)
- Enable sticky sessions (for long downloads)

**Option B: HAProxy (Self-Hosted)**

**Install HAProxy:**
```
sudo apt install -y haproxy
```

**Configure:**

Edit `/etc/haproxy/haproxy.cfg`:
```
frontend deltaship_front
    bind *:443 ssl crt /etc/ssl/private/deltaship.pem
    default_backend deltaship_servers

backend deltaship_servers
    balance leastconn
    option httpchk GET /health
    server server1 10.0.1.10:8080 check
    server server2 10.0.1.11:8080 check
    server server3 10.0.1.12:8080 check
```

**Restart HAProxy:**
```
sudo systemctl restart haproxy
```

### Step 6: CDN Setup (CloudFlare)

**Add domain to CloudFlare:**
- Sign up at cloudflare.com
- Add domain: updates.example.com
- Update nameservers at domain registrar

**Configure caching:**
- Page Rules → Create rule:
  - URL: `updates.example.com/api/download-diff/*`
  - Cache Level: Cache Everything
  - Edge Cache TTL: 1 week

- Page Rules → Create rule:
  - URL: `updates.example.com/api/download-binary/*`
  - Cache Level: Cache Everything
  - Edge Cache TTL: 1 month

**Purge cache when new version published:**
- Use CloudFlare API to purge specific URLs
- Integrate with publisher toolkit

---

## Large Deployment (Kubernetes)

### Prerequisites

- Kubernetes cluster (GKE, EKS, AKS, or self-hosted)
- kubectl configured
- Helm 3 installed

### Step 1: Namespace Creation

```
kubectl create namespace deltaship
```

### Step 2: Secrets

**Database credentials:**

```
kubectl create secret generic deltaship-db-credentials \
  --from-literal=username=deltaship_server \
  --from-literal=password=STRONG_PASSWORD \
  -n deltaship
```

**S3 credentials:**

```
kubectl create secret generic deltaship-s3-credentials \
  --from-literal=access-key-id=AKIAxxxxxx \
  --from-literal=secret-access-key=SECRET_KEY \
  -n deltaship
```

### Step 3: ConfigMap

Create `deltaship-config.yaml`:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: deltaship-server-config
  namespace: deltaship
data:
  config.toml: |
    [server]
    listen_address = "0.0.0.0:8080"
    public_url = "https://updates.example.com"
    workers = 8

    [database]
    type = "postgresql"
    host = "postgres-service.deltaship.svc.cluster.local"
    port = 5432
    database = "deltaship"
    pool_size = 20

    [storage]
    type = "s3"
    bucket = "deltaship-updates-prod"
    region = "us-east-1"
    use_iam_role = true

    [cache]
    type = "redis"
    url = "redis://redis-service.deltaship.svc.cluster.local:6379"
    enabled = true
```

Apply:
```
kubectl apply -f deltaship-config.yaml
```

### Step 4: Deployment

Create `deltaship-deployment.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: deltaship-server
  namespace: deltaship
spec:
  replicas: 10
  selector:
    matchLabels:
      app: deltaship-server
  template:
    metadata:
      labels:
        app: deltaship-server
    spec:
      containers:
      - name: deltaship-server
        image: deltaship/server:1.0.0
        ports:
        - containerPort: 8080
        env:
        - name: DB_USERNAME
          valueFrom:
            secretKeyRef:
              name: deltaship-db-credentials
              key: username
        - name: DB_PASSWORD
          valueFrom:
            secretKeyRef:
              name: deltaship-db-credentials
              key: password
        volumeMounts:
        - name: config
          mountPath: /etc/deltaship-server
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5
      volumes:
      - name: config
        configMap:
          name: deltaship-server-config
---
apiVersion: v1
kind: Service
metadata:
  name: deltaship-server-service
  namespace: deltaship
spec:
  selector:
    app: deltaship-server
  ports:
  - protocol: TCP
    port: 80
    targetPort: 8080
  type: LoadBalancer
```

Apply:
```
kubectl apply -f deltaship-deployment.yaml
```

### Step 5: Horizontal Pod Autoscaling

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: deltaship-server-hpa
  namespace: deltaship
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: deltaship-server
  minReplicas: 5
  maxReplicas: 50
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

Apply:
```
kubectl apply -f deltaship-hpa.yaml
```

---

## Post-Deployment

### Verification

**Health check:**
```
curl https://updates.example.com/health
```

**API test:**
```
curl -H "Authorization: Bearer YOUR_API_KEY" \
  https://updates.example.com/api/check-update?app=test&version=1.0.0
```

**Database connection:**
```
Check logs for "Database connected successfully"
```

**Object storage:**
```
Upload test file, verify retrieval
```

### Monitoring Setup

**Prometheus Metrics:**

Configure Prometheus to scrape:
```
http://server-ip:8080/metrics
```

**Key metrics to monitor:**
- `api_request_duration_seconds` - Response time
- `api_request_total` - Request count
- `diff_generation_duration_seconds` - Diff computation time
- `database_query_duration_seconds` - DB performance
- `storage_upload_duration_seconds` - S3 performance

**Grafana Dashboard:**

Import Deltaship dashboard template:
```
Dashboard ID: 12345 (from grafana.com)
```

**Alerts:**

Set up alerts for:
- High error rate (>1%)
- Slow response time (p99 >5s)
- Database connection failures
- Storage upload failures

### Backup Strategy

**Database Backups:**

**Automated (managed services):**
- Enable automated backups (7-30 day retention)

**Manual (self-hosted):**
```
pg_dump -h postgres-host -U deltaship_server deltaship > backup.sql
```

**Schedule daily via cron:**
```
0 2 * * * pg_dump deltaship | gzip > /backups/deltaship-$(date +\%Y\%m\%d).sql.gz
```

**Object Storage Backups:**

- S3 versioning enabled (built-in backup)
- Cross-region replication (disaster recovery)
- Glacier archival for long-term retention

### Security Hardening

**Firewall Rules:**
- Allow only ports 80, 443 (HTTPS)
- Restrict database port (5432) to API servers only
- Restrict SSH (port 22) to specific IPs

**TLS Configuration:**
- Use TLS 1.2+ only
- Strong cipher suites
- Enable HSTS header

**API Key Rotation:**
- Rotate API keys quarterly
- Use separate keys per publisher
- Revoke unused keys

**Database Security:**
- Use strong passwords
- Enable SSL/TLS for database connections
- Regular security updates

---

## Troubleshooting

### Common Issues

**Issue: High latency**

**Diagnosis:**
- Check Prometheus metrics for slow endpoints
- Review database query performance
- Check network latency to S3

**Solutions:**
- Add database indexes
- Enable Redis caching
- Use CDN for static content
- Scale API servers horizontally

**Issue: Database connection errors**

**Diagnosis:**
- Check database server status
- Review connection pool settings
- Check network connectivity

**Solutions:**
- Increase connection pool size
- Check database max_connections setting
- Verify credentials

**Issue: Storage upload failures**

**Diagnosis:**
- Check S3 credentials
- Review IAM permissions
- Check network connectivity to S3

**Solutions:**
- Verify S3 access key is valid
- Ensure bucket permissions allow writes
- Check for bucket storage limits

---

## Scaling Guide

### When to Scale

**Indicators:**
- CPU usage >70% sustained
- Memory usage >80%
- Response time p99 >1s
- Database connections >80% of pool

**Vertical Scaling:**
- Increase server resources (CPU, RAM)
- Upgrade database instance size
- Quick fix, limited scaling potential

**Horizontal Scaling:**
- Add more API server instances
- Add database read replicas
- Add Redis cache cluster
- Unlimited scaling potential

---

## Next Steps

**For Publishers:**
- Read: [Publisher Setup Guide](PUBLISHER_SETUP.md)
- Configure: Publisher toolkit to use your server

**For Monitoring:**
- Set up: Prometheus + Grafana
- Configure: Alerts for critical metrics

**For Operations:**
- Read: [Maintenance Guide](../operations/MAINTENANCE.md)
- Schedule: Regular backups and updates

---

**End of Server Deployment Guide**
