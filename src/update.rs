//! Update check against the GitHub releases page. Musify asks GitHub once at startup
//! for the newest release, and can install it:
//! - AppImage: downloads the new AppImage and swaps it in place of the running one;
//! - RPM: downloads the new RPM and installs it with `pkexec dnf` (asks for your password);
//! - anything else (built from source): offers the release page instead.

use std::path::PathBuf;

const REPO: &str = "jamesisboredcoding/musify";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Release {
    pub version: String,
    pub page: String,
    /// Download for how this copy was installed, if the release has one.
    pub asset: Option<String>,
}

/// How this copy of Musify was installed, which decides how it updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    AppImage,
    Rpm,
    Source,
}

pub fn install() -> Install {
    if std::env::var_os("APPIMAGE").is_some() {
        return Install::AppImage;
    }
    let exe = std::env::current_exe().unwrap_or_default();
    if exe.starts_with("/usr/bin") {
        let owned = std::process::Command::new("rpm").arg("-qf").arg(&exe).output();
        if owned.is_ok_and(|o| o.status.success()) {
            return Install::Rpm;
        }
    }
    Install::Source
}

fn parse(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-']).map_while(|p| p.parse().ok()).collect()
}

pub fn newer(remote: &str, local: &str) -> bool {
    parse(remote) > parse(local)
}

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GhAsset>,
}

#[derive(serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// The newest release if it's newer than this build.
pub async fn check(http: reqwest::Client, install: Install) -> Option<Release> {
    let r: GhRelease = http
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    if r.draft || r.prerelease || !newer(&r.tag_name, CURRENT) {
        return None;
    }
    let arch = std::env::consts::ARCH;
    let suffix = match install {
        Install::AppImage => format!("{arch}.AppImage"),
        Install::Rpm => format!("{arch}.rpm"),
        Install::Source => String::new(),
    };
    let asset = (!suffix.is_empty())
        .then(|| r.assets.iter().find(|a| a.name.ends_with(&suffix)).map(|a| a.browser_download_url.clone()))
        .flatten();
    Some(Release { version: r.tag_name.trim_start_matches('v').to_string(), page: r.html_url, asset })
}

async fn download(http: &reqwest::Client, url: &str, to: &PathBuf) -> Result<(), String> {
    let bytes = http
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Couldn't download the update ({e})"))?
        .bytes()
        .await
        .map_err(|e| format!("Couldn't download the update ({e})"))?;
    tokio::fs::write(to, &bytes).await.map_err(|e| format!("Couldn't save the update ({e})"))
}

/// Installs the release. Returns the program to start for the new version.
pub async fn apply(install: Install, release: Release) -> Result<PathBuf, String> {
    // Its own client: the app's one gives up after 15 s, too short for a whole AppImage.
    let http = reqwest::Client::builder()
        .user_agent(concat!("Musify/", env!("CARGO_PKG_VERSION"), " (Linux music player)"))
        .build()
        .map_err(|e| e.to_string())?;
    let url = release.asset.ok_or("This release has no download for your install.")?;
    match install {
        Install::AppImage => {
            let current = PathBuf::from(std::env::var_os("APPIMAGE").ok_or("Not running as an AppImage.")?);
            // Download next to the old one, then swap: a rename is atomic, and the running
            // copy keeps working from the old (now unlinked) file until it exits.
            let fresh = current.with_extension("AppImage.new");
            download(&http, &url, &fresh).await?;
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755)).await.map_err(|e| e.to_string())?;
            tokio::fs::rename(&fresh, &current).await.map_err(|e| format!("Couldn't replace the AppImage ({e})"))?;
            Ok(current)
        }
        Install::Rpm => {
            let dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join("musify");
            tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
            let rpm = dir.join(format!("musify-{}.rpm", release.version));
            download(&http, &url, &rpm).await?;
            // pkexec shows the system's password prompt.
            let status = tokio::process::Command::new("pkexec")
                .args(["dnf", "install", "-y"])
                .arg(&rpm)
                .status()
                .await
                .map_err(|e| format!("Couldn't start the installer ({e})"))?;
            let _ = tokio::fs::remove_file(&rpm).await;
            if !status.success() {
                return Err("The update wasn't installed (password prompt cancelled or dnf failed).".into());
            }
            Ok(PathBuf::from("/usr/bin/musify"))
        }
        Install::Source => Err("Built from source: pull and rebuild to update.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("0.1.10", "0.1.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
    }
}
