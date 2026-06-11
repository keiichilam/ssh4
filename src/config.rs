//! Persistent configuration at `~/.ssh4.toml`: profiles, snippets, UI prefs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_dir: Option<String>,
    /// Only persisted when the user explicitly opts in to saving a password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

fn default_port() -> u16 {
    22
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Snippet {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippets: Vec<Snippet>,
    #[serde(default = "default_zoom")]
    pub ui_zoom: f32,
    #[serde(default)]
    pub auto_fit: bool,
    /// Selected theme name; absent means the default theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Send periodic SSH keep-alive probes on idle sessions.
    #[serde(default)]
    pub keep_alive: bool,
}

fn default_zoom() -> f32 {
    1.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profiles: BTreeMap::new(),
            snippets: Vec::new(),
            ui_zoom: 1.0,
            auto_fit: false,
            theme: None,
            keep_alive: false,
        }
    }
}

impl Config {
    /// Resolve `~/.ssh4.toml` via USERPROFILE (Windows) or HOME.
    pub fn default_path() -> PathBuf {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(".ssh4.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::default_path())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&Self::default_path())
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms)?;
        }
        Ok(())
    }

    /// Default profile name for a connection: `user@host` or `host`.
    pub fn default_profile_name(user: &str, host: &str) -> String {
        if user.is_empty() {
            host.to_string()
        } else {
            format!("{user}@{host}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ssh4_test_{name}_{}.toml", std::process::id()))
    }

    #[test]
    fn defaults() {
        let c = Config::default();
        assert!(c.profiles.is_empty());
        assert!(c.snippets.is_empty());
        assert_eq!(c.ui_zoom, 1.0);
        assert!(!c.auto_fit);
    }

    #[test]
    fn load_missing_returns_default() {
        let c = Config::load_from(Path::new("Z:/definitely/not/here.toml"));
        assert_eq!(c, Config::default());
    }

    #[test]
    fn roundtrip_profiles_and_snippets() {
        let mut c = Config::default();
        c.profiles.insert(
            "home".into(),
            Profile {
                host: "example.com".into(),
                port: 2222,
                user: "keith".into(),
                key_path: Some("/home/keith/.ssh/id_ed25519".into()),
                remote_dir: Some("/tmp".into()),
                password: None,
            },
        );
        c.snippets.push(Snippet {
            name: "disk usage".into(),
            command: "df -h".into(),
        });
        c.ui_zoom = 1.25;
        c.auto_fit = true;

        let path = temp_file("roundtrip");
        c.save_to(&path).unwrap();
        let loaded = Config::load_from(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded, c);
    }

    #[test]
    fn loads_existing_schema() {
        // Schema from SOFTWARE_DESIGN.md §6.1 must keep loading.
        let text = r#"
ui_zoom = 1.0
auto_fit = false

[profiles.home]
host = "example.com"
port = 22
user = "keith"
key_path = "/home/keith/.ssh/id_ed25519"
remote_dir = "/tmp"
password = "optional"

[[snippets]]
name = "disk usage"
command = "df -h"
"#;
        let c: Config = toml::from_str(text).unwrap();
        let p = &c.profiles["home"];
        assert_eq!(p.host, "example.com");
        assert_eq!(p.port, 22);
        assert_eq!(p.user, "keith");
        assert_eq!(p.password.as_deref(), Some("optional"));
        assert_eq!(c.snippets[0].command, "df -h");
    }

    #[test]
    fn port_defaults_to_22_when_missing() {
        let c: Config = toml::from_str("[profiles.x]\nhost = \"h\"\n").unwrap();
        assert_eq!(c.profiles["x"].port, 22);
    }

    #[test]
    fn default_profile_name_rules() {
        assert_eq!(Config::default_profile_name("u", "h"), "u@h");
        assert_eq!(Config::default_profile_name("", "h"), "h");
    }
}
