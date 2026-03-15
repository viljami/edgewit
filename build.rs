use std::env;
use std::process::Command;

fn main() {
    // Get the short git commit hash
    let commit = env::var("GIT_COMMIT_HASH").unwrap_or_else(|_| {
        Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    });

    // Get the build date
    let build_date = env::var("BUILD_DATE").unwrap_or_else(|_| {
        Command::new("date")
            .args(["+%Y-%m-%dT%H:%M:%SZ", "-u"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    });

    // Export them as environment variables that can be used via env!()
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", commit);
    println!("cargo:rustc-env=BUILD_DATE={}", build_date);

    // Re-run the build script if HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}
