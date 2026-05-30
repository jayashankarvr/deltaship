//! Configuration management commands

use std::io::{self, Write};
use std::path::Path;

use deltaship_db::PublisherDb;

use crate::config::{
    get_config_description, get_default_value, validate_config_value, ConfigKey, ALL_CONFIG_KEYS,
    DB_FILE,
};

/// Run the config get command
pub async fn run_get(key: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    let db = PublisherDb::open(db_path).await?;

    match key {
        Some(key_str) => {
            // Show specific config value
            let config_key: ConfigKey = key_str.parse().map_err(|e: String| e)?;
            let value = db.get_config(config_key.as_db_key()).await?;

            match value {
                Some(v) => {
                    println!("{} = {}", config_key, v);
                }
                None => {
                    if let Some(default) = get_default_value(&config_key) {
                        println!("{} = {} (default)", config_key, default);
                    } else {
                        println!("{} is not set", config_key);
                    }
                }
            }
        }
        None => {
            // Show all config values (except secrets)
            println!("Configuration:");
            println!();

            for config_key in ALL_CONFIG_KEYS {
                let value = db.get_config(config_key.as_db_key()).await?;

                match value {
                    Some(v) => {
                        // Truncate public key for display
                        let display_value = if *config_key == ConfigKey::PublicKey && v.len() > 16 {
                            format!("{}...", &v[..16])
                        } else {
                            v
                        };
                        println!("  {} = {}", config_key, display_value);
                    }
                    None => {
                        if let Some(default) = get_default_value(config_key) {
                            println!("  {} = {} (default)", config_key, default);
                        } else {
                            println!("  {} = (not set)", config_key);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Run the config set command
pub async fn run_set(key: String, value: String) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    let config_key: ConfigKey = key.parse().map_err(|e: String| e)?;

    // Check if key is read-only
    if config_key.is_read_only() {
        return Err(format!(
            "Cannot set '{}': this configuration is read-only.",
            config_key
        )
        .into());
    }

    // Validate the value
    validate_config_value(&config_key, &value)?;

    let db = PublisherDb::open(db_path).await?;
    db.set_config(config_key.as_db_key(), &value).await?;

    println!("Set {} = {}", config_key, value);

    Ok(())
}

/// Run the config list command
pub async fn run_list() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new(DB_FILE);

    // We can show the list even without a project initialized
    let db = if db_path.exists() {
        Some(PublisherDb::open(db_path).await?)
    } else {
        None
    };

    println!("Available configuration keys:");
    println!();
    println!("  {:<20} {:<50} STATUS", "KEY", "DESCRIPTION");
    println!("  {:-<20} {:-<50} {:-<15}", "", "", "");

    for config_key in ALL_CONFIG_KEYS {
        let description = get_config_description(config_key);
        let status = if let Some(ref db) = db {
            let value = db.get_config(config_key.as_db_key()).await?;
            match value {
                Some(_) => "set".to_string(),
                None => {
                    if let Some(default) = get_default_value(config_key) {
                        format!("default ({})", default)
                    } else {
                        "not set".to_string()
                    }
                }
            }
        } else if let Some(default) = get_default_value(config_key) {
            format!("default ({})", default)
        } else {
            "not set".to_string()
        };

        let readonly_marker = if config_key.is_read_only() {
            " [read-only]"
        } else {
            ""
        };

        println!(
            "  {:<20} {:<50} {}{}",
            config_key.to_string(),
            description,
            status,
            readonly_marker
        );
    }

    println!();
    println!("Use 'deltaship-publisher config get <key>' to view a specific value.");
    println!("Use 'deltaship-publisher config set <key> <value>' to change a value.");

    Ok(())
}

/// Run the config reset command
pub async fn run_reset(key: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    let db = PublisherDb::open(db_path).await?;

    match key {
        Some(key_str) => {
            // Reset specific key
            let config_key: ConfigKey = key_str.parse().map_err(|e: String| e)?;

            if config_key.is_read_only() {
                return Err(format!(
                    "Cannot reset '{}': this configuration is read-only.",
                    config_key
                )
                .into());
            }

            if let Some(default) = get_default_value(&config_key) {
                db.set_config(config_key.as_db_key(), default).await?;
                println!("Reset {} to default value: {}", config_key, default);
            } else {
                // For keys without defaults, we set to empty string
                db.set_config(config_key.as_db_key(), "").await?;
                println!("Reset {} (cleared value)", config_key);
            }
        }
        None => {
            // Reset all keys - ask for confirmation
            print!("Reset all configuration to defaults? This cannot be undone. [y/N] ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim().to_lowercase() != "y" {
                println!("Cancelled.");
                return Ok(());
            }

            for config_key in ALL_CONFIG_KEYS {
                if config_key.is_read_only() {
                    continue;
                }

                if let Some(default) = get_default_value(config_key) {
                    db.set_config(config_key.as_db_key(), default).await?;
                    println!("  Reset {} to: {}", config_key, default);
                } else {
                    db.set_config(config_key.as_db_key(), "").await?;
                    println!("  Reset {} (cleared)", config_key);
                }
            }

            println!();
            println!("All configuration reset to defaults.");
        }
    }

    Ok(())
}
