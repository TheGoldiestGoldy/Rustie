use std::{env, path::PathBuf, process::Command};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo manifest"));
    let git_output = Command::new("git")
        .current_dir(&manifest_dir)
        .args(["rev-parse", "--short=12", "HEAD"])
        .output();
    let sha = git_output
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_owned())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown-source".to_owned());
    // A pinned installer build forces Cargo to re-run this script whenever the
    // source revision changes, even when worktree HEAD points to a stable branch ref.
    println!("cargo:rerun-if-env-changed=RUSTY_BUILD_REVISION");
    if let Ok(expected) = env::var("RUSTY_BUILD_REVISION") {
        assert_eq!(
            expected, sha,
            "Refusing to stamp a Rusty binary with the wrong Git revision"
        );
    }
    println!("cargo:rustc-env=RUSTY_BUILD_SHA={sha}");
    // Rebuild the version label after HEAD moves to a new revision in a worktree.
    if let Ok(output) = Command::new("git")
        .current_dir(&manifest_dir)
        .args(["rev-parse", "--git-path", "HEAD"])
        .output()
        && output.status.success()
        && let Ok(path) = String::from_utf8(output.stdout)
    {
        let raw = PathBuf::from(path.trim());
        let resolved = if raw.is_absolute() {
            raw
        } else {
            manifest_dir.join(raw)
        };
        println!("cargo:rerun-if-changed={}", resolved.display());
    }
    println!("cargo:rerun-if-changed=build.rs");
}
