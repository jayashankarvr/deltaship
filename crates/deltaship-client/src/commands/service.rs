//! Service command - manage systemd service for Deltaship client daemon.

use std::path::PathBuf;
use std::process::Command;

/// Generate the systemd service file content.
pub fn generate_service_file(user_mode: bool) -> String {
    let binary_path = get_binary_path();
    let binary_path_str = binary_path.display();

    if user_mode {
        format!(
            r#"[Unit]
Description=Deltaship Client - Binary Update Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={binary_path_str} --daemon
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
"#
        )
    } else {
        // P2 Issue 69 Fix: Systemd Hardcoded Data Dir
        //
        // The ReadWritePaths directive hardcodes /var/lib/deltaship as the data directory.
        // This is a deliberate security-focused default for system-wide installations.
        //
        // **Default behavior (RECOMMENDED):**
        // - System-mode services use /var/lib/deltaship for data storage
        // - This provides strong security hardening via ProtectSystem=strict
        // - The deltaship-client daemon will create this directory on first run with proper permissions
        //
        // **If you need a custom data directory in system mode, you have three options:**
        //
        // 1. **Manually edit the service file after installation (RECOMMENDED):**
        //    ```bash
        //    # Install the service first
        //    sudo deltaship-client service install
        //
        //    # Edit the service file
        //    sudo systemctl edit --full deltaship-client.service
        //
        //    # Change ReadWritePaths to your custom directory, e.g.:
        //    ReadWritePaths=/opt/custom-deltaship-data
        //
        //    # Reload and restart
        //    sudo systemctl daemon-reload
        //    sudo systemctl restart deltaship-client.service
        //    ```
        //
        // 2. **Use user-mode installation (--user flag):**
        //    User-mode doesn't restrict paths and uses ~/.local/share/deltaship by default
        //    ```bash
        //    deltaship-client service install --user
        //    ```
        //
        // 3. **Create a symlink (NOT RECOMMENDED - bypasses security):**
        //    ```bash
        //    sudo ln -s /your/custom/path /var/lib/deltaship
        //    ```
        //
        // **Why not generate ReadWritePaths dynamically from config?**
        // - Security: Expanding writable paths weakens systemd sandboxing
        // - Simplicity: Static default is easier to audit and reason about
        // - Flexibility: Users who need custom paths can edit the service file
        //
        // For most deployments, the default /var/lib/deltaship is the best choice.
        format!(
            r#"[Unit]
Description=Deltaship Client - Binary Update Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={binary_path_str} --daemon
Restart=on-failure
RestartSec=10

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
# P2 Issue 69: Default data directory for security hardening
# Edit this path if you need a custom data directory (see comments above)
ReadWritePaths=/var/lib/deltaship

[Install]
WantedBy=multi-user.target
"#
        )
    }
}

/// Get the path where the service file should be installed.
pub fn get_service_path(user_mode: bool) -> anyhow::Result<PathBuf> {
    if user_mode {
        let home = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
        let home_path = PathBuf::from(&home);

        if !home_path.is_absolute() {
            anyhow::bail!("HOME environment variable must be an absolute path, got: {}", home);
        }

        if !home_path.exists() {
            anyhow::bail!("HOME directory does not exist: {}", home);
        }

        Ok(home_path
            .join(".config")
            .join("systemd")
            .join("user")
            .join("deltaship-client.service"))
    } else {
        Ok(PathBuf::from("/etc/systemd/system/deltaship-client.service"))
    }
}

/// Get the current executable path.
pub fn get_binary_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/usr/local/bin/deltaship-client"))
}

/// Run a systemctl command with the given arguments.
pub fn run_systemctl(args: &[&str], user_mode: bool) -> anyhow::Result<()> {
    let mut cmd = Command::new("systemctl");

    if user_mode {
        cmd.arg("--user");
    }

    cmd.args(args);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("systemctl failed: {}", stderr);
    }

    Ok(())
}

/// Install the systemd service.
pub fn install_service(user_mode: bool) -> anyhow::Result<()> {
    let service_path = get_service_path(user_mode)?;
    let service_content = generate_service_file(user_mode);

    println!("Creating systemd service file...");

    // Create parent directory if needed (for user mode)
    if user_mode {
        if let Some(parent) = service_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    println!("Installing to {}", service_path.display());

    // Write service file
    std::fs::write(&service_path, service_content)?;

    println!("Reloading systemd daemon...");
    run_systemctl(&["daemon-reload"], user_mode)?;

    println!("Enabling service...");
    run_systemctl(&["enable", "deltaship-client.service"], user_mode)?;

    println!("Starting service...");
    run_systemctl(&["start", "deltaship-client.service"], user_mode)?;

    println!();
    println!("Service installed and started successfully!");
    println!();
    println!(
        "To check status: deltaship-client service status{}",
        if user_mode { " --user" } else { "" }
    );
    println!(
        "To view logs:    deltaship-client service logs --follow{}",
        if user_mode { " --user" } else { "" }
    );

    Ok(())
}

/// Uninstall the systemd service.
pub fn uninstall_service(user_mode: bool) -> anyhow::Result<()> {
    let service_path = get_service_path(user_mode)?;

    println!("Stopping service...");
    // Ignore errors if service is not running
    let _ = run_systemctl(&["stop", "deltaship-client.service"], user_mode);

    println!("Disabling service...");
    // Ignore errors if service is not enabled
    let _ = run_systemctl(&["disable", "deltaship-client.service"], user_mode);

    println!("Removing service file...");
    if service_path.exists() {
        std::fs::remove_file(&service_path)?;
        println!("Removed {}", service_path.display());
    } else {
        println!("Service file not found at {}", service_path.display());
    }

    println!("Reloading systemd daemon...");
    run_systemctl(&["daemon-reload"], user_mode)?;

    println!();
    println!("Service uninstalled successfully!");

    Ok(())
}

/// Show service status.
pub fn show_service_status(user_mode: bool) -> anyhow::Result<()> {
    let mut cmd = Command::new("systemctl");

    if user_mode {
        cmd.arg("--user");
    }

    cmd.args(["status", "deltaship-client.service"]);

    let status = cmd.status()?;

    // systemctl status returns non-zero if service is not running, which is fine
    if !status.success() {
        // Exit code 3 means service is not running, which is informational
        // Exit code 4 means service not found
        if status.code() == Some(4) {
            println!("Service not installed. Run 'deltaship-client service install' to install.");
        }
    }

    Ok(())
}

/// Show service logs.
pub fn show_service_logs(user_mode: bool, follow: bool) -> anyhow::Result<()> {
    let mut cmd = Command::new("journalctl");

    if user_mode {
        cmd.arg("--user");
    }

    cmd.args(["-u", "deltaship-client.service"]);

    if follow {
        cmd.arg("-f");
    }

    let status = cmd.status()?;

    if !status.success() {
        anyhow::bail!("Failed to retrieve logs");
    }

    Ok(())
}
