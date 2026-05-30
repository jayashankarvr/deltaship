# Client Installation Guide

**Document:** Installation procedures for end-user client patcher
**Audience:** End users, system administrators, IT departments
**Last Updated:** 2026-01-07

---

## Overview

This guide describes how to install the VBDP Client Patcher on end-user devices. The client patcher is a lightweight background service that automatically keeps software up-to-date using minimal bandwidth.

**Installation Time:** 2-5 minutes
**Prerequisites:** Administrator/sudo access (for system-wide installation)
**Supported Platforms:** Linux, Windows, macOS

---

## Installation Overview

### Installation Types

**System-Wide Installation (Recommended):**
- Installed for all users on the device
- Runs as system service
- Requires administrator privileges
- Updates system-level applications

**User-Level Installation:**
- Installed for single user
- Runs when user logged in
- No admin privileges required
- Updates user-level applications only

**Deployment Methods:**
- **Interactive Installer:** GUI or command-line wizard (easiest for end users)
- **Silent/Unattended:** No user interaction (for enterprise deployment)
- **Package Manager:** System package manager (apt, dnf, chocolatey, homebrew)
- **Manual:** Download and install from archive (advanced users)

---

## Linux Installation

### Method 1: Package Manager (Recommended)

**Debian/Ubuntu (.deb package):**

**Prerequisites:**
- Ubuntu 20.04+ or Debian 11+
- sudo access

**Installation Steps:**

1. **Download Package:**
   - From: https://releases.vbdp.io/client/vbdp-client_1.0.0_amd64.deb
   - Or: Add VBDP repository for automatic updates

2. **Install Package:**
   - Double-click .deb file (opens Software Center)
   - Or command-line: `sudo dpkg -i vbdp-client_1.0.0_amd64.deb`
   - Resolve dependencies: `sudo apt-get install -f`

3. **Verify Installation:**
   - Check service: `systemctl status vbdp`
   - Should show: "active (running)"

4. **Configure (Optional):**
   - Edit: `/etc/vbdp/config.toml`
   - Restart service: `sudo systemctl restart vbdp`

**Using APT Repository (Auto-Updates):**

1. **Add Repository:**
   ```
   Add VBDP GPG key:
   wget -qO- https://releases.vbdp.io/gpg.key | sudo apt-key add -

   Add repository:
   echo "deb https://releases.vbdp.io/apt stable main" | sudo tee /etc/apt/sources.list.d/vbdp.list

   Update package list:
   sudo apt-get update
   ```

2. **Install:**
   ```
   sudo apt-get install vbdp-client
   ```

3. **Auto-Updates:**
   - VBDP client will be updated through APT automatically
   - Same as system updates

**RHEL/Fedora/CentOS (.rpm package):**

**Prerequisites:**
- RHEL 8+, Fedora 35+, CentOS 8+
- sudo access

**Installation Steps:**

1. **Download Package:**
   - From: https://releases.vbdp.io/client/vbdp-client-1.0.0-1.x86_64.rpm

2. **Install Package:**
   - Command: `sudo dnf install vbdp-client-1.0.0-1.x86_64.rpm`
   - Or: `sudo rpm -i vbdp-client-1.0.0-1.x86_64.rpm`

3. **Verify Installation:**
   - Check service: `systemctl status vbdp`

4. **Configure (Optional):**
   - Edit: `/etc/vbdp/config.toml`
   - Restart: `sudo systemctl restart vbdp`

**Using DNF Repository:**

1. **Add Repository:**
   ```
   Add repository file:
   sudo tee /etc/yum.repos.d/vbdp.repo <<EOF
   [vbdp]
   name=VBDP Repository
   baseurl=https://releases.vbdp.io/rpm/
   enabled=1
   gpgcheck=1
   gpgkey=https://releases.vbdp.io/gpg.key
   EOF
   ```

2. **Install:**
   ```
   sudo dnf install vbdp-client
   ```

### Method 2: Distribution-Agnostic (Flatpak)

**Prerequisites:**
- Flatpak installed
- Any Linux distribution

**Installation Steps:**

1. **Install Flatpak (if needed):**
   - Ubuntu/Debian: `sudo apt install flatpak`
   - Fedora: Pre-installed
   - Other: See https://flatpak.org/setup/

2. **Add Flathub Repository:**
   ```
   flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
   ```

3. **Install VBDP Client:**
   ```
   flatpak install flathub io.vbdp.Client
   ```

4. **Run:**
   - Auto-starts on login
   - Manual start: `flatpak run io.vbdp.Client`

**Limitations:**
- Flatpak version runs in sandbox (limited system access)
- May not update system-level applications
- Best for user-level applications only

### Method 3: Binary Archive (Advanced)

**Prerequisites:**
- Linux (x86_64 or ARM64)
- systemd (for service management)

**Installation Steps:**

1. **Download Archive:**
   - From: https://releases.vbdp.io/client/vbdp-client-1.0.0-linux-x86_64.tar.gz

2. **Extract:**
   ```
   tar -xzf vbdp-client-1.0.0-linux-x86_64.tar.gz
   cd vbdp-client-1.0.0
   ```

3. **Install:**
   ```
   sudo ./install.sh
   ```
   - Copies binary to /usr/bin/vbdp
   - Creates config directory /etc/vbdp/
   - Installs systemd service file
   - Starts service

4. **Verify:**
   ```
   systemctl status vbdp
   ```

### Configuration (Linux)

**Configuration File:** `/etc/vbdp/config.toml`

**Basic Configuration:**
```
[updates]
auto_check = true
check_interval_hours = 4

[server]
update_server_url = "https://updates.example.com"

[network]
allow_metered = false
```

**Apply Changes:**
```
sudo systemctl restart vbdp
```

**View Logs:**
```
journalctl -u vbdp -f
```

---

## Windows Installation

### Method 1: MSI Installer (Recommended)

**Prerequisites:**
- Windows 10 version 1809 or later, Windows 11, or Windows Server 2019+
- Administrator privileges

**Installation Steps:**

1. **Download Installer:**
   - From: https://releases.vbdp.io/client/VBDPClient-1.0.0-x64.msi
   - Save to Downloads folder

2. **Run Installer:**
   - Double-click MSI file
   - User Account Control (UAC) prompt → Click "Yes"
   - Installation wizard appears

3. **Wizard Steps:**
   - **Welcome:** Click "Next"
   - **License Agreement:** Accept → "Next"
   - **Installation Type:**
     - "System-wide" (all users) - Recommended
     - "Current user only" (no admin needed)
   - **Update Server URL:**
     - Enter: https://updates.example.com
     - Or leave default if pre-configured by organization
   - **Installation Folder:**
     - Default: C:\Program Files\VBDP\
     - Or choose custom location
   - **Start Service:**
     - Checkbox: "Start VBDP Client Service" (recommended)
   - Click "Install"

4. **Installation Progress:**
   - Copies files
   - Installs Windows Service
   - Configures firewall rules
   - Starts service

5. **Completion:**
   - "Installation Complete" message
   - System tray icon appears (optional)
   - Click "Finish"

6. **Verify Installation:**
   - Open Services (services.msc)
   - Find "VBDP Client Patcher"
   - Status should be "Running"
   - Startup Type: "Automatic"

### Method 2: Silent Installation (Enterprise)

**For IT Administrators deploying to many machines**

**Command-Line Installation:**

```
msiexec /i VBDPClient-1.0.0-x64.msi /quiet /norestart
```

**With Custom Configuration:**

```
msiexec /i VBDPClient-1.0.0-x64.msi /quiet /norestart ^
  UPDATE_SERVER_URL="https://updates.company.com" ^
  INSTALL_DIR="C:\Program Files\VBDP" ^
  SHOW_TRAY_ICON=0
```

**Parameters:**
- `/quiet` - No UI, completely silent
- `/passive` - Progress bar only, no user input
- `/norestart` - Don't restart automatically
- `UPDATE_SERVER_URL` - Custom update server
- `SHOW_TRAY_ICON` - 1 = show, 0 = hide

**Group Policy Deployment:**

1. **Share MSI File:**
   - Place MSI on network share: `\\server\share\VBDPClient.msi`

2. **Create GPO:**
   - Open Group Policy Management
   - Create new GPO: "Deploy VBDP Client"
   - Edit GPO

3. **Assign Software:**
   - Navigate: Computer Configuration → Policies → Software Settings → Software Installation
   - Right-click → New → Package
   - Select: `\\server\share\VBDPClient.msi`
   - Deployment method: "Assigned"

4. **Apply GPO:**
   - Link GPO to appropriate OU (Organizational Unit)
   - Computers will install VBDP on next reboot

### Method 3: Chocolatey

**Prerequisites:**
- Chocolatey installed (see https://chocolatey.org/install)
- Administrator PowerShell

**Installation:**

```
choco install vbdp-client
```

**With Parameters:**

```
choco install vbdp-client --params="'/UpdateServerURL:https://updates.example.com /NoTrayIcon'"
```

**Benefits:**
- Easy updates: `choco upgrade vbdp-client`
- Uninstall: `choco uninstall vbdp-client`

### Method 4: Winget (Microsoft Package Manager)

**Prerequisites:**
- Windows 10 1809+ or Windows 11
- Winget installed (pre-installed on Windows 11)

**Installation:**

```
winget install VBDP.Client
```

**Interactive Configuration:**
- Winget prompts for update server URL
- Or use configuration file

### Configuration (Windows)

**Configuration File:** `C:\ProgramData\VBDP\config.toml`

**Edit with Notepad:**
```
notepad C:\ProgramData\VBDP\config.toml
```

**Restart Service:**
- Services → VBDP Client Patcher → Restart
- Or PowerShell: `Restart-Service VBDPClient`

**View Logs:**
- Event Viewer → Windows Logs → Application
- Filter by source: "VBDP Client"
- Or file logs: `C:\ProgramData\VBDP\logs\vbdp.log`

---

## macOS Installation

### Method 1: PKG Installer (Recommended)

**Prerequisites:**
- macOS 12 (Monterey) or later
- Administrator password

**Installation Steps:**

1. **Download Installer:**
   - From: https://releases.vbdp.io/client/VBDPClient-1.0.0.pkg
   - Save to Downloads

2. **Run Installer:**
   - Double-click .pkg file
   - Gatekeeper warning (first run):
     - Right-click → Open
     - Click "Open" in dialog
   - Installation wizard appears

3. **Wizard Steps:**
   - **Introduction:** Read → "Continue"
   - **License:** Agree → "Continue"
   - **Installation Type:**
     - Standard Install (all users)
     - Click "Install"
   - **Authentication:**
     - Enter admin password
     - Click "Install Software"
   - **Summary:**
     - "Installation was successful"
     - Click "Close"

4. **Verify Installation:**
   - Open Terminal
   - Check launchd: `sudo launchctl list | grep vbdp`
   - Should show: `io.vbdp.client.patcher`

5. **Configuration Prompt:**
   - First run may show setup dialog
   - Enter update server URL: https://updates.example.com
   - Click "Save"

### Method 2: Homebrew

**Prerequisites:**
- Homebrew installed (see https://brew.sh)
- Terminal access

**Installation:**

```
brew tap vbdp/client
brew install vbdp-client
```

**Start Service:**

```
brew services start vbdp-client
```

**Benefits:**
- Easy updates: `brew upgrade vbdp-client`
- Familiar for developers

### Method 3: DMG with App Bundle

**Prerequisites:**
- macOS 11+

**Installation Steps:**

1. **Download DMG:**
   - From: https://releases.vbdp.io/client/VBDPClient-1.0.0.dmg

2. **Mount and Install:**
   - Double-click DMG to mount
   - Drag "VBDP Client" to Applications folder
   - Eject DMG

3. **First Launch:**
   - Open Applications folder
   - Double-click "VBDP Client"
   - Gatekeeper warning → Right-click → Open
   - Setup wizard guides initial configuration

4. **Install Daemon:**
   - App prompts: "Install background daemon?"
   - Click "Install" (requires admin password)
   - Daemon installed to system

### Configuration (macOS)

**Configuration File:** `/Library/Application Support/VBDP/config.toml`

**Edit:**
```
sudo nano "/Library/Application Support/VBDP/config.toml"
```

**Restart Service:**
```
sudo launchctl stop io.vbdp.client.patcher
sudo launchctl start io.vbdp.client.patcher
```

**View Logs:**
- Console app → Search "vbdp"
- Or Terminal: `tail -f /var/log/vbdp/vbdp.log`

---

## Post-Installation

### Verification Steps

**Check Service Status:**

**Linux:**
```
systemctl status vbdp
```
Expected: "active (running)"

**Windows:**
```
sc query VBDPClient
```
Expected: "STATE: 4 RUNNING"

**macOS:**
```
sudo launchctl list | grep vbdp
```
Expected: Process ID shown

**Test Update Check:**

**All Platforms:**
```
vbdp check --verbose
```

Expected output:
```
Checking for updates...
Contacting server: https://updates.example.com
Apps registered: 3
  - MyApp v1.2.0 (up-to-date)
  - OtherApp v2.5.1 (update available: v2.6.0)
  - ThirdApp v0.9.0 (up-to-date)
```

### Initial Configuration

**Configure Update Server:**

Edit configuration file (see platform-specific paths above):

```toml
[server]
update_server_url = "https://updates.yourcompany.com"
```

**Configure Update Frequency:**

```toml
[updates]
auto_check = true
check_interval_hours = 4  # Check every 4 hours
auto_download = true
auto_apply = true  # Apply updates automatically (or false for manual approval)
```

**Network Settings:**

```toml
[network]
allow_metered = false  # Don't update on metered connections (mobile hotspot, etc.)
max_download_speed_kbps = 0  # 0 = unlimited, or set limit like 1024 for 1 MB/s
```

**Restart service after configuration changes**

### Registering Applications

**Automatic Registration:**
- Client patcher scans common installation directories
- Detects applications with VBDP metadata
- Registers automatically

**Manual Registration:**

```
vbdp register --app "MyApp" --binary "/path/to/myapp" --version "1.0.0"
```

**Verify Registered Apps:**

```
vbdp list
```

Expected output:
```
Registered applications:
- MyApp v1.0.0 (/usr/local/bin/myapp)
- OtherApp v2.5.1 (/opt/otherapp/bin/otherapp)
```

---

## Uninstallation

### Linux

**Debian/Ubuntu:**
```
sudo apt-get remove vbdp-client
```

**To remove configuration too:**
```
sudo apt-get purge vbdp-client
```

**RHEL/Fedora:**
```
sudo dnf remove vbdp-client
```

**Manual/Binary Installation:**
```
sudo systemctl stop vbdp
sudo systemctl disable vbdp
sudo rm /usr/bin/vbdp
sudo rm /etc/systemd/system/vbdp.service
sudo rm -rf /etc/vbdp/
sudo rm -rf /var/lib/vbdp/
```

### Windows

**Control Panel:**
- Settings → Apps → Apps & Features
- Search "VBDP"
- Click → Uninstall

**Command-Line:**
```
msiexec /x {PRODUCT-GUID} /quiet
```

**Chocolatey:**
```
choco uninstall vbdp-client
```

**Remove Configuration:**
- Delete: `C:\ProgramData\VBDP\`

### macOS

**PKG Installation:**
```
sudo /Library/Application Support/VBDP/uninstall.sh
```

**Homebrew:**
```
brew services stop vbdp-client
brew uninstall vbdp-client
```

**App Bundle:**
- Delete from Applications folder
- Remove daemon:
  ```
  sudo launchctl unload /Library/LaunchDaemons/io.vbdp.client.patcher.plist
  sudo rm /Library/LaunchDaemons/io.vbdp.client.patcher.plist
  ```

**Remove Configuration:**
```
sudo rm -rf "/Library/Application Support/VBDP/"
```

---

## Enterprise Deployment

### Mass Deployment Strategies

**Configuration Management:**

**Puppet:**
```
Use vbdp module from Puppet Forge
puppet module install vbdp-client
```

**Ansible:**
```
Use vbdp role from Ansible Galaxy
ansible-galaxy install vbdp.client
```

**Chef:**
```
Use vbdp cookbook
knife cookbook site install vbdp-client
```

**SCCM (Windows):**
- Create application package with MSI
- Deploy to device collections
- Monitor installation status

**Jamf (macOS):**
- Upload PKG to Jamf Pro
- Create policy for distribution
- Scope to computer groups

### Pre-Configuration

**Create Configuration Template:**

Prepare `config.toml` with organization defaults:

```toml
[server]
update_server_url = "https://updates.company.internal"
fallback_servers = ["https://updates-backup.company.internal"]

[updates]
auto_check = true
check_interval_hours = 2
auto_apply = true

[network]
allow_metered = false
max_download_speed_kbps = 5120  # 5 MB/s limit

[security]
verify_signatures = true
log_verification_failures = true
```

**Include in Deployment:**
- Linux: Place in `/etc/vbdp/config.toml` before starting service
- Windows: Place in `C:\ProgramData\VBDP\config.toml`
- macOS: Place in `/Library/Application Support/VBDP/config.toml`

### Centralized Management

**Group Policy (Windows):**
- Administrative Templates for VBDP
- Deploy configuration via ADMX files
- Override user settings

**MDM Profiles (macOS/Mobile):**
- Configuration profile (plist)
- Deploy via Jamf, Workspace ONE, etc.
- Enforce organization policies

**Policy Enforcement:**

Example: Force specific update server (cannot be changed by user)

```toml
[server]
update_server_url = "https://updates.company.com"
# POLICY_LOCKED prevents user override
```

---

## Troubleshooting

### Common Issues

**Issue: Service not starting**

**Symptoms:**
- `systemctl status vbdp` shows "failed"
- Windows Service shows "stopped"

**Solutions:**
1. Check logs for error messages
2. Verify configuration file syntax
3. Check network connectivity to update server
4. Verify permissions (service user has read access to config)

**Linux:**
```
journalctl -u vbdp -n 50
```

**Windows:**
```
Event Viewer → Application log → Filter by source "VBDP"
```

**Issue: Updates not downloading**

**Symptoms:**
- Service running but no updates applied
- "No updates available" but you know updates exist

**Solutions:**
1. Check update server URL in config
2. Verify firewall allows outbound HTTPS (port 443)
3. Test connectivity: `curl https://updates.example.com/api/health`
4. Check proxy settings if behind corporate proxy

**Issue: High bandwidth usage**

**Symptoms:**
- Network saturated during update checks

**Solutions:**
1. Set bandwidth limit in config: `max_download_speed_kbps = 1024`
2. Adjust check frequency: `check_interval_hours = 12`
3. Disable metered connection usage: `allow_metered = false`

**Issue: Signature verification failures**

**Symptoms:**
- Logs show "signature verification failed"
- Updates not applying

**Solutions:**
1. Verify system time is correct (signatures have timestamp)
2. Check if public key is up-to-date
3. Reinstall client (may have corrupted keys)
4. Contact publisher (signature may be invalid)

### Getting Help

**Documentation:**
- Full docs: https://docs.vbdp.io
- FAQ: https://docs.vbdp.io/faq

**Community Support:**
- Forum: https://community.vbdp.io
- Discord: https://discord.gg/vbdp

**Enterprise Support:**
- Email: support@vbdp.io
- Phone: +1-XXX-XXX-XXXX (enterprise customers)

**Bug Reports:**
- GitHub Issues: https://github.com/vbdp/client/issues
- Include: OS, version, logs, configuration

---

## Next Steps

After installation:
1. **Configure applications:** Register apps to be updated
2. **Test update flow:** Trigger manual update check
3. **Monitor:** Check logs periodically
4. **Review settings:** Adjust update frequency and network settings

**For Administrators:**
- Read: [Enterprise Features](../tools/CLIENT_PATCHER.md#enterprise-features)
- Read: [Operations Guide](../operations/MAINTENANCE.md)
- Set up: Centralized monitoring dashboard

**For Developers:**
- Read: [Publisher Toolkit](../tools/PUBLISHER_TOOLKIT.md)
- Learn: How to publish updates for your applications

---

**End of Client Installation Guide**
