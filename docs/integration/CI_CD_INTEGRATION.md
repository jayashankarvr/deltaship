# CI/CD Integration Guide

**Document:** Integrating VBDP into continuous integration and deployment pipelines
**Audience:** DevOps engineers, release engineers, CI/CD administrators
**Last Updated:** 2026-01-07

---

## Overview

This guide shows how to integrate the VBDP Publisher Toolkit into popular CI/CD platforms to automate the process of:
1. Building your application
2. Registering the new version with VBDP
3. Generating binary diffs
4. Cryptographically signing updates
5. Publishing to the update server

**Supported Platforms:**
- GitHub Actions
- GitLab CI/CD
- Jenkins
- CircleCI
- Travis CI
- Azure Pipelines
- Bitbucket Pipelines

---

## Integration Principles

### 1. Build Once, Sign Once, Deploy Once
- Build artifact in one job
- Sign with production keys in secure environment
- Deploy to update server from CD pipeline

### 2. Secrets Management
- NEVER commit private signing keys to repository
- Use CI/CD secrets management (GitHub Secrets, GitLab CI Variables, etc.)
- Rotate keys annually

### 3. Idempotency
- Re-running pipeline for same version should succeed (not error)
- Use `--skip-if-exists` flag for registration

### 4. Validation
- Always test updates before publishing to production
- Use staging environment for testing
- Automated smoke tests after publish

---

## GitHub Actions Integration

### Basic Workflow

Create `.github/workflows/release.yml`:

```yaml
name: Release and Publish Update

on:
  release:
    types: [published]  # Triggers on GitHub release
  workflow_dispatch:    # Manual trigger

jobs:
  build-and-publish:
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Build application
        run: |
          # Your build commands here
          ./build.sh
          # Produces: ./dist/myapp

      - name: Install VBDP Publisher Toolkit
        run: |
          wget https://releases.vbdp.io/publisher/vbdp-publisher-toolkit_1.0.0_amd64.deb
          sudo dpkg -i vbdp-publisher-toolkit_1.0.0_amd64.deb

      - name: Initialize VBDP (first time only)
        run: |
          if [ ! -d ".vbdp" ]; then
            vbdp-init \
              --app-name "myapp" \
              --update-server "${{ secrets.VBDP_SERVER_URL }}" \
              --no-generate-keys  # Keys managed separately
          fi

      - name: Configure VBDP
        run: |
          # Add API key to config
          echo "api_key = \"${{ secrets.VBDP_API_KEY }}\"" >> .vbdp/config.toml

          # Add signing keys from secrets
          echo "${{ secrets.VBDP_PRIVATE_KEY }}" > .vbdp/keys/private.key
          echo "${{ secrets.VBDP_PUBLIC_KEY }}" > .vbdp/keys/public.key
          chmod 600 .vbdp/keys/private.key

      - name: Register version
        run: |
          vbdp-register \
            --version "${{ github.ref_name }}" \
            --binary ./dist/myapp \
            --platform linux \
            --architecture x86_64 \
            --skip-if-exists

      - name: Sign version
        run: |
          vbdp-sign --version "${{ github.ref_name }}"

      - name: Test update locally
        run: |
          # Test diff application if previous version exists
          vbdp-test \
            --from-latest \
            --to "${{ github.ref_name }}" \
            || echo "No previous version, skipping test"

      - name: Publish to update server
        run: |
          vbdp-publish \
            --version "${{ github.ref_name }}" \
            --changelog CHANGELOG.md \
            --rollout-percentage 10  # Start with 10% rollout
        env:
          VBDP_API_KEY: ${{ secrets.VBDP_API_KEY }}

      - name: Verify publication
        run: |
          curl -f "${{ secrets.VBDP_SERVER_URL }}/api/check-update?app=myapp&version=0.0.0" \
            | jq '.target_version' \
            | grep -q "${{ github.ref_name }}"
```

### Multi-Platform Build

For apps supporting multiple platforms:

```yaml
name: Multi-Platform Release

on:
  release:
    types: [published]

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            platform: linux
            arch: x86_64
            binary: myapp
          - os: windows-latest
            platform: windows
            arch: x86_64
            binary: myapp.exe
          - os: macos-latest
            platform: macos
            arch: x86_64
            binary: myapp
          - os: macos-latest
            platform: macos
            arch: arm64
            binary: myapp-arm64

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Build
        run: ./build-${{ matrix.platform }}.sh

      - name: Install VBDP
        run: |
          # Platform-specific installation
          if [ "${{ matrix.os }}" = "ubuntu-latest" ]; then
            wget https://releases.vbdp.io/publisher/vbdp-publisher-toolkit_amd64.deb
            sudo dpkg -i vbdp-publisher-toolkit_amd64.deb
          elif [ "${{ matrix.os }}" = "macos-latest" ]; then
            brew install vbdp-publisher-toolkit
          elif [ "${{ matrix.os }}" = "windows-latest" ]; then
            choco install vbdp-publisher-toolkit
          fi

      - name: Register and Publish
        run: |
          vbdp-register \
            --version "${{ github.ref_name }}" \
            --binary ./dist/${{ matrix.binary }} \
            --platform ${{ matrix.platform }} \
            --architecture ${{ matrix.arch }}

          vbdp-sign --version "${{ github.ref_name }}" --platform ${{ matrix.platform }}
          vbdp-publish --version "${{ github.ref_name }}" --platform ${{ matrix.platform }}
        env:
          VBDP_API_KEY: ${{ secrets.VBDP_API_KEY }}
          VBDP_PRIVATE_KEY: ${{ secrets.VBDP_PRIVATE_KEY }}
```

---

## GitLab CI/CD Integration

Create `.gitlab-ci.yml`:

```yaml
stages:
  - build
  - publish

variables:
  VBDP_SERVER_URL: $VBDP_SERVER_URL  # Set in GitLab CI/CD Variables

build:
  stage: build
  image: ubuntu:22.04
  script:
    - apt-get update && apt-get install -y build-essential
    - ./build.sh
  artifacts:
    paths:
      - dist/myapp
    expire_in: 1 week

publish:
  stage: publish
  image: ubuntu:22.04
  only:
    - tags  # Only run on tagged commits
  dependencies:
    - build
  before_script:
    - apt-get update && apt-get install -y wget curl jq
    - wget https://releases.vbdp.io/publisher/vbdp-publisher-toolkit_amd64.deb
    - dpkg -i vbdp-publisher-toolkit_amd64.deb

    # Initialize VBDP
    - |
      if [ ! -d ".vbdp" ]; then
        vbdp-init --app-name "myapp" --update-server "$VBDP_SERVER_URL" --no-generate-keys
      fi

    # Configure keys from CI variables
    - mkdir -p .vbdp/keys
    - echo "$VBDP_PRIVATE_KEY" > .vbdp/keys/private.key
    - echo "$VBDP_PUBLIC_KEY" > .vbdp/keys/public.key
    - chmod 600 .vbdp/keys/private.key

    # Add API key to config
    - echo "api_key = \"$VBDP_API_KEY\"" >> .vbdp/config.toml

  script:
    - vbdp-register --version "$CI_COMMIT_TAG" --binary ./dist/myapp --skip-if-exists
    - vbdp-sign --version "$CI_COMMIT_TAG"
    - vbdp-test --from-latest --to "$CI_COMMIT_TAG" || true
    - vbdp-publish --version "$CI_COMMIT_TAG" --rollout-percentage 10
```

---

## Jenkins Integration

Create `Jenkinsfile`:

```groovy
pipeline {
    agent any

    environment {
        VBDP_SERVER_URL = credentials('vbdp-server-url')
        VBDP_API_KEY = credentials('vbdp-api-key')
        VBDP_PRIVATE_KEY = credentials('vbdp-private-key')
        VBDP_PUBLIC_KEY = credentials('vbdp-public-key')
    }

    stages {
        stage('Build') {
            steps {
                sh './build.sh'
            }
        }

        stage('Install VBDP') {
            steps {
                sh '''
                    wget https://releases.vbdp.io/publisher/vbdp-publisher-toolkit_amd64.deb
                    sudo dpkg -i vbdp-publisher-toolkit_amd64.deb
                '''
            }
        }

        stage('Publish Update') {
            when {
                tag pattern: "v\\d+\\.\\d+\\.\\d+", comparator: "REGEXP"
            }
            steps {
                sh '''
                    # Initialize if needed
                    if [ ! -d ".vbdp" ]; then
                        vbdp-init --app-name "myapp" --update-server "$VBDP_SERVER_URL" --no-generate-keys
                    fi

                    # Configure keys
                    mkdir -p .vbdp/keys
                    echo "$VBDP_PRIVATE_KEY" > .vbdp/keys/private.key
                    echo "$VBDP_PUBLIC_KEY" > .vbdp/keys/public.key
                    chmod 600 .vbdp/keys/private.key

                    # Register, sign, publish
                    vbdp-register --version "$TAG_NAME" --binary ./dist/myapp --skip-if-exists
                    vbdp-sign --version "$TAG_NAME"
                    vbdp-publish --version "$TAG_NAME"
                '''
            }
        }
    }

    post {
        success {
            echo "Successfully published version ${TAG_NAME}"
        }
        failure {
            echo "Failed to publish version ${TAG_NAME}"
        }
    }
}
```

---

## CircleCI Integration

Create `.circleci/config.yml`:

```yaml
version: 2.1

jobs:
  build:
    docker:
      - image: ubuntu:22.04
    steps:
      - checkout
      - run:
          name: Build application
          command: ./build.sh
      - persist_to_workspace:
          root: .
          paths:
            - dist/myapp

  publish:
    docker:
      - image: ubuntu:22.04
    steps:
      - checkout
      - attach_workspace:
          at: .
      - run:
          name: Install VBDP
          command: |
            apt-get update && apt-get install -y wget
            wget https://releases.vbdp.io/publisher/vbdp-publisher-toolkit_amd64.deb
            dpkg -i vbdp-publisher-toolkit_amd64.deb
      - run:
          name: Publish update
          command: |
            vbdp-init --app-name "myapp" --update-server "$VBDP_SERVER_URL" --no-generate-keys
            echo "$VBDP_PRIVATE_KEY" > .vbdp/keys/private.key
            echo "$VBDP_PUBLIC_KEY" > .vbdp/keys/public.key

            vbdp-register --version "$CIRCLE_TAG" --binary ./dist/myapp
            vbdp-sign --version "$CIRCLE_TAG"
            vbdp-publish --version "$CIRCLE_TAG"

workflows:
  build-and-publish:
    jobs:
      - build:
          filters:
            tags:
              only: /^v.*/
      - publish:
          requires:
            - build
          filters:
            tags:
              only: /^v.*/
            branches:
              ignore: /.*/
```

---

## Azure Pipelines Integration

Create `azure-pipelines.yml`:

```yaml
trigger:
  tags:
    include:
      - v*

pool:
  vmImage: 'ubuntu-latest'

steps:
  - task: Bash@3
    displayName: 'Build application'
    inputs:
      targetType: 'inline'
      script: |
        ./build.sh

  - task: Bash@3
    displayName: 'Install VBDP'
    inputs:
      targetType: 'inline'
      script: |
        wget https://releases.vbdp.io/publisher/vbdp-publisher-toolkit_amd64.deb
        sudo dpkg -i vbdp-publisher-toolkit_amd64.deb

  - task: Bash@3
    displayName: 'Publish update'
    env:
      VBDP_API_KEY: $(vbdp-api-key)
      VBDP_PRIVATE_KEY: $(vbdp-private-key)
    inputs:
      targetType: 'inline'
      script: |
        vbdp-init --app-name "myapp" --update-server "$(vbdp-server-url)" --no-generate-keys
        echo "$VBDP_PRIVATE_KEY" > .vbdp/keys/private.key

        vbdp-register --version "$(Build.SourceBranchName)" --binary ./dist/myapp
        vbdp-sign --version "$(Build.SourceBranchName)"
        vbdp-publish --version "$(Build.SourceBranchName)"
```

---

## Docker-Based CI/CD

For platform-agnostic CI/CD using Docker:

```dockerfile
# Dockerfile.vbdp-publisher
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    wget \
    curl \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Install VBDP Publisher Toolkit
RUN wget https://releases.vbdp.io/publisher/vbdp-publisher-toolkit_amd64.deb \
    && dpkg -i vbdp-publisher-toolkit_amd64.deb \
    && rm vbdp-publisher-toolkit_amd64.deb

WORKDIR /workspace

ENTRYPOINT ["/usr/bin/vbdp-publish"]
```

Use in any CI system:

```bash
docker build -t vbdp-publisher -f Dockerfile.vbdp-publisher .

docker run --rm \
  -v $(pwd):/workspace \
  -e VBDP_API_KEY=$VBDP_API_KEY \
  -e VBDP_PRIVATE_KEY="$VBDP_PRIVATE_KEY" \
  vbdp-publisher \
  --version "$VERSION" \
  --binary ./dist/myapp
```

---

## Best Practices

### 1. Secrets Management

**DO:**
- Use CI/CD platform's secrets management
- Rotate keys annually
- Use separate keys for staging and production
- Audit key access

**DON'T:**
- Commit keys to repository (even private repos)
- Share keys between applications
- Use same keys for dev and prod
- Log private keys in CI output

### 2. Versioning

**Semantic Versioning:**
```bash
# Git tag triggers release
git tag -a v1.2.3 -m "Release version 1.2.3"
git push origin v1.2.3
```

**Automatic Versioning:**
```yaml
# Use commit SHA for nightly builds
- name: Generate version
  run: |
    VERSION="nightly-$(git rev-parse --short HEAD)"
    echo "VERSION=$VERSION" >> $GITHUB_ENV
```

### 3. Testing Before Publishing

**Staging Environment:**
```yaml
- name: Publish to staging
  run: |
    vbdp-publish \
      --version "$VERSION" \
      --environment staging \
      --rollout-percentage 100

- name: Run smoke tests
  run: |
    ./smoke-tests.sh staging

- name: Publish to production
  if: success()
  run: |
    vbdp-publish \
      --version "$VERSION" \
      --environment production \
      --rollout-percentage 10
```

### 4. Gradual Rollout Automation

**Progressive Rollout:**
```yaml
- name: Publish with 10% rollout
  run: vbdp-publish --version "$VERSION" --rollout-percentage 10

- name: Monitor for 24 hours
  run: |
    # In practice, this would be a separate scheduled job
    sleep 86400

    # Check error rate
    ERROR_RATE=$(vbdp-stats --version "$VERSION" --metric error-rate)
    if [ "$ERROR_RATE" -lt "1" ]; then
      vbdp-publish --version "$VERSION" --rollout-percentage 25
    fi
```

### 5. Artifact Verification

**Checksum Verification:**
```yaml
- name: Build and verify
  run: |
    ./build.sh

    # Generate checksum
    sha256sum ./dist/myapp > myapp.sha256

    # Verify in CI logs
    cat myapp.sha256
```

---

## Troubleshooting

### Issue: "Signature verification failed"

**Cause:** Private key mismatch

**Solution:**
```yaml
- name: Debug keys
  run: |
    # Verify public key matches
    PUBKEY_FINGERPRINT=$(sha256sum .vbdp/keys/public.key | cut -d' ' -f1)
    echo "Public key fingerprint: $PUBKEY_FINGERPRINT"

    # Should match your known good fingerprint
```

### Issue: "Version already exists"

**Cause:** Re-running pipeline for same version

**Solution:**
Use `--skip-if-exists` flag:
```bash
vbdp-register --version "$VERSION" --skip-if-exists
```

### Issue: "API authentication failed"

**Cause:** Incorrect API key or signature

**Solution:**
```yaml
- name: Test API key
  run: |
    curl -H "X-VBDP-API-Key: $VBDP_API_KEY" \
      "$VBDP_SERVER_URL/api/apps"
```

---

## Advanced: Custom Pipeline Steps

### Pre-Release Validation

```yaml
- name: Validate binary
  run: |
    # Check binary size (reject if >500MB)
    SIZE=$(stat -f%z ./dist/myapp)
    if [ $SIZE -gt 524288000 ]; then
      echo "Binary too large: $SIZE bytes"
      exit 1
    fi

    # Verify binary format
    file ./dist/myapp | grep -q "ELF 64-bit"
```

### Changelog Generation

```yaml
- name: Generate changelog
  run: |
    # Generate changelog from git commits
    git log --pretty=format:"- %s" $(git describe --tags --abbrev=0)..HEAD > CHANGELOG-$VERSION.md

    vbdp-publish \
      --version "$VERSION" \
      --changelog CHANGELOG-$VERSION.md
```

### Notification Integration

```yaml
- name: Notify on publish
  if: success()
  run: |
    curl -X POST "$SLACK_WEBHOOK_URL" \
      -H 'Content-Type: application/json' \
      -d "{
        \"text\": \"Published version $VERSION to VBDP update server\"
      }"
```

---

## Next Steps

**For DevOps Teams:**
- Choose appropriate CI/CD platform integration
- Set up secrets management
- Test pipeline in staging environment
- Enable notifications for failed publishes

**Related Documents:**
- [Publisher Setup](../deployment/PUBLISHER_SETUP.md) - Manual publishing guide
- [System Design](../architecture/SYSTEM_DESIGN.md) - Architecture overview
- [Security Model](../security/SECURITY_MODEL.md) - Key management best practices

---

**End of CI/CD Integration Guide**
