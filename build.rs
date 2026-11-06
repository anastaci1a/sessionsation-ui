//! Embeds the git commit being built (`SESSIONSATION_COMMIT`, else `git rev-parse HEAD`) so
//! `sessionsation-ui --version` names the source it was built from and the deploy script can
//! refuse a stale artifact.
use std::process::Command;
fn main() {
    let commit = std::env::var("SESSIONSATION_COMMIT")
        .ok()
        .filter(|c| !c.is_empty())
        .or_else(|| {
            let out = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        });
    println!(
        "cargo:rustc-env=SESSIONSATION_UI_BUILD_COMMIT={}",
        commit.unwrap_or_else(|| "unknown".into())
    );
    println!("cargo:rerun-if-env-changed=SESSIONSATION_COMMIT");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
