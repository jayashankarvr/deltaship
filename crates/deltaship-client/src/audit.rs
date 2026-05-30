//! Audit logging for sensitive operations.
//!
//! Provides syslog integration for logging security-critical operations
//! such as binary updates, rollbacks, and configuration changes.

use std::fmt;

/// Audit event types for sensitive operations.
#[derive(Debug, Clone)]
pub enum AuditEvent {
    /// Binary update applied
    UpdateApplied {
        binary_name: String,
        version: String,
        install_path: String,
    },
    /// Binary rolled back to previous version
    Rollback {
        binary_name: String,
        from_version: String,
        to_version: String,
    },
    /// Binary added to management
    BinaryAdded {
        binary_name: String,
        install_path: String,
    },
    /// Binary removed from management
    BinaryRemoved {
        binary_name: String,
        install_path: String,
    },
    /// Signature verification failed
    SignatureVerificationFailed {
        binary_name: String,
        version: String,
    },
    /// Update check failed
    UpdateCheckFailed {
        binary_name: String,
        error: String,
    },
}

impl fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditEvent::UpdateApplied {
                binary_name,
                version,
                install_path,
            } => write!(
                f,
                "UPDATE_APPLIED: binary={} version={} path={}",
                binary_name, version, install_path
            ),
            AuditEvent::Rollback {
                binary_name,
                from_version,
                to_version,
            } => write!(
                f,
                "ROLLBACK: binary={} from={} to={}",
                binary_name, from_version, to_version
            ),
            AuditEvent::BinaryAdded {
                binary_name,
                install_path,
            } => write!(
                f,
                "BINARY_ADDED: binary={} path={}",
                binary_name, install_path
            ),
            AuditEvent::BinaryRemoved {
                binary_name,
                install_path,
            } => write!(
                f,
                "BINARY_REMOVED: binary={} path={}",
                binary_name, install_path
            ),
            AuditEvent::SignatureVerificationFailed {
                binary_name,
                version,
            } => write!(
                f,
                "SIGNATURE_VERIFICATION_FAILED: binary={} version={}",
                binary_name, version
            ),
            AuditEvent::UpdateCheckFailed {
                binary_name,
                error,
            } => write!(
                f,
                "UPDATE_CHECK_FAILED: binary={} error={}",
                binary_name, error
            ),
        }
    }
}

/// Log an audit event to syslog (Unix only) and tracing.
///
/// On Unix systems, this will attempt to send the event to syslog.
/// On all systems, it will also log via the tracing crate.
/// If syslog fails, it will fall back to tracing only.
pub fn log_audit_event(event: &AuditEvent) {
    // Always log via tracing for local visibility
    tracing::warn!(event = %event, "AUDIT");

    // On Unix, also attempt to send to syslog
    #[cfg(unix)]
    {
        if let Err(e) = send_to_syslog(event) {
            tracing::debug!("Failed to send audit event to syslog: {}", e);
            // Don't fail the operation if syslog is unavailable
        }
    }
}

#[cfg(unix)]
fn send_to_syslog(event: &AuditEvent) -> Result<(), Box<dyn std::error::Error>> {
    use syslog::{Facility, Formatter3164, BasicLogger};
    use std::sync::{Mutex, OnceLock};
    use log::Log;

    // Persistent syslog logger initialized once and reused
    static SYSLOG_LOGGER: OnceLock<Mutex<Option<BasicLogger>>> = OnceLock::new();

    let logger_mutex = SYSLOG_LOGGER.get_or_init(|| {
        let formatter = Formatter3164 {
            facility: Facility::LOG_USER,
            hostname: None,
            process: "deltaship-client".into(),
            pid: std::process::id(),
        };

        // Try to connect to syslog once
        let logger = syslog::unix(formatter)
            .ok()
            .map(BasicLogger::new);

        Mutex::new(logger)
    });

    // Get the logger from the mutex
    let logger_opt = logger_mutex.lock().expect("audit logger lock poisoned");

    if let Some(logger) = logger_opt.as_ref() {
        // Use warn level for audit events as they represent important security operations
        logger.log(
            &log::Record::builder()
                .args(format_args!("DELTASHIP_AUDIT: {}", event))
                .level(log::Level::Warn)
                .target("deltaship-client")
                .build()
        );

        logger.flush();
        Ok(())
    } else {
        Err("Failed to initialize syslog connection".into())
    }
}
