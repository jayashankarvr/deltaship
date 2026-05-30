//! Version information command

/// Display detailed version information
pub fn run() {
    let version = env!("CARGO_PKG_VERSION");
    let name = env!("CARGO_PKG_NAME");

    // Determine build type
    let build_type = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    // Construct platform string from cfg values
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let platform = format!("{}-{}", arch, os);

    println!("{} {}", name, version);
    println!("Build: {}", build_type);
    println!("Platform: {}", platform);
}
