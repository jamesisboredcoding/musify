//! Signing in to YouTube Music by reusing a Firefox session.
//!
//! Cookies are read from Firefox's `cookies.sqlite`, filtered to youtube.com, and kept in
//! `~/.local/share/musify/youtube-cookies.txt` (mode 0600, Netscape format so yt-dlp can use
//! the same file). Nothing is written anywhere else.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone)]
struct Cookie {
    host: String,
    path: String,
    secure: bool,
    expiry: i64,
    name: String,
    value: String,
}

#[derive(Clone)]
pub struct Jar {
    cookies: Vec<Cookie>,
}

// Never print cookie values.
impl std::fmt::Debug for Jar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Jar({} cookies)", self.cookies.len())
    }
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("musify")
}

pub fn jar_path() -> PathBuf {
    data_dir().join("youtube-cookies.txt")
}

fn signin_profile() -> PathBuf {
    data_dir().join("firefox-signin")
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

impl Jar {
    /// The value YouTube's private API signs requests with.
    pub fn sapisid(&self) -> Option<&str> {
        ["__Secure-3PAPISID", "SAPISID"]
            .iter()
            .find_map(|n| self.cookies.iter().find(|c| c.name == *n).map(|c| c.value.as_str()))
    }

    pub fn header(&self) -> String {
        let mut seen = std::collections::HashSet::new();
        self.cookies
            .iter()
            .filter(|c| seen.insert(c.name.clone()))
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn load() -> Option<Jar> {
        let text = fs::read_to_string(jar_path()).ok()?;
        let cookies: Vec<Cookie> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() == 7).then(|| Cookie {
                    host: f[0].into(),
                    path: f[2].into(),
                    secure: f[3] == "TRUE",
                    expiry: f[4].parse().unwrap_or(0),
                    name: f[5].into(),
                    value: f[6].into(),
                })
            })
            .collect();
        let jar = Jar { cookies };
        jar.sapisid().is_some().then_some(jar)
    }

    pub fn save(&self) -> std::io::Result<()> {
        private_dir(&data_dir())?;
        let mut out = String::from("# Netscape HTTP Cookie File\n# Written by Musify. Contains your YouTube login; keep it private.\n");
        for c in &self.cookies {
            let sub = if c.host.starts_with('.') { "TRUE" } else { "FALSE" };
            let secure = if c.secure { "TRUE" } else { "FALSE" };
            out += &format!("{}\t{sub}\t{}\t{secure}\t{}\t{}\t{}\n", c.host, c.path, c.expiry, c.name, c.value);
        }
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(jar_path())?;
        f.write_all(out.as_bytes())?;
        fs::set_permissions(jar_path(), fs::Permissions::from_mode(0o600))
    }

    pub fn forget() {
        let _ = fs::remove_file(jar_path());
    }
}

/// Firefox profile folders that hold cookies, Musify's own sign-in profile last.
fn firefox_profiles() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let roots = [
        home.join(".config/mozilla/firefox"),
        home.join(".mozilla/firefox"),
        home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
        home.join(".var/app/org.mozilla.firefox/config/mozilla/firefox"),
    ];
    let mut profiles: Vec<PathBuf> = roots
        .iter()
        .filter_map(|r| fs::read_dir(r).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("cookies.sqlite").exists())
        .collect();
    profiles.push(signin_profile());
    profiles
}

fn read_profile(profile: &Path) -> Result<Vec<Cookie>, String> {
    let db = profile.join("cookies.sqlite");
    if !db.exists() {
        return Err("no cookies".into());
    }
    // Firefox keeps the database locked and recent writes in the -wal file; read a private copy.
    let tmp = data_dir().join("import-tmp");
    private_dir(&tmp).map_err(|e| e.to_string())?;
    let copy = tmp.join("cookies.sqlite");
    for suffix in ["", "-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", db.display()));
        let to = PathBuf::from(format!("{}{suffix}", copy.display()));
        let _ = fs::remove_file(&to);
        if from.exists() {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    let result = (|| {
        let conn = rusqlite::Connection::open(&copy).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT host, path, isSecure, expiry, name, value FROM moz_cookies WHERE host = 'youtube.com' OR host LIKE '%.youtube.com'")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                let expiry: i64 = r.get(3)?;
                Ok(Cookie {
                    host: r.get(0)?,
                    path: r.get(1)?,
                    secure: r.get::<_, i64>(2)? != 0,
                    // Newer Firefox stores milliseconds.
                    expiry: if expiry > 100_000_000_000 { expiry / 1000 } else { expiry },
                    name: r.get(4)?,
                    value: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })();
    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Finds a Firefox profile signed in to YouTube and saves its YouTube cookies.
pub fn import_firefox() -> Result<Jar, String> {
    for profile in firefox_profiles() {
        if let Ok(cookies) = read_profile(&profile) {
            let jar = Jar { cookies };
            if jar.sapisid().is_some() {
                jar.save().map_err(|e| format!("Couldn't save your login: {e}"))?;
                return Ok(jar);
            }
        }
    }
    Err("No Firefox profile is signed in to YouTube.".into())
}

/// Opens a Firefox window with Musify's own profile on the YouTube Music sign-in page.
/// Read the cookies after it closes, once Firefox has written them out.
pub fn open_signin() -> Result<Child, String> {
    let profile = signin_profile();
    private_dir(&profile).map_err(|e| e.to_string())?;
    Command::new("firefox")
        .arg("--no-remote")
        .arg("--profile")
        .arg(&profile)
        .arg("--new-window")
        .arg("https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn't open Firefox: {e}"))
}
