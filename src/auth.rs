//! Host string parsing, default key discovery, credential resolution.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Parsed `[user@]host[:port]`. Port defaults to 22; an invalid port falls
/// back to 22 to preserve current behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct HostSpec {
    pub user: String,
    pub host: String,
    pub port: u16,
}

pub fn parse_host(input: &str) -> HostSpec {
    let (user, rest) = match input.split_once('@') {
        Some((u, r)) => (u.to_string(), r),
        None => (String::new(), input),
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(22)),
        None => (rest.to_string(), 22),
    };
    HostSpec { user, host, port }
}

/// Default key candidates in stable priority order (FR-003).
pub const DEFAULT_KEY_NAMES: [&str; 5] = [
    "id_ed25519",
    "id_rsa",
    "id_ecdsa",
    "id_ed25519_sk",
    "id_ecdsa_sk",
];

pub fn ssh_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".ssh")
}

/// First existing default key under the given .ssh directory.
pub fn discover_default_key_in(dir: &Path) -> Option<PathBuf> {
    DEFAULT_KEY_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
}

pub fn discover_default_key() -> Option<PathBuf> {
    discover_default_key_in(&ssh_dir())
}

/// Fallback username when none was given: local user name, else "root".
pub fn local_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "root".to_string())
}

/// Prompt for a line on stdout/stdin (CLI only — GUI must never call this).
pub fn prompt_line(prompt: &str) -> std::io::Result<String> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Secure password prompt: raw mode, no echo (CLI only).
pub fn prompt_password(prompt: &str) -> std::io::Result<String> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    print!("{prompt}");
    std::io::stdout().flush()?;
    crossterm::terminal::enable_raw_mode()?;
    let mut pw = String::new();
    let result = loop {
        match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Enter => break Ok(pw.clone()),
                KeyCode::Backspace => {
                    pw.pop();
                }
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    break Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "cancelled",
                    ));
                }
                KeyCode::Char(c) => pw.push(c),
                _ => {}
            },
            _ => {}
        }
    };
    crossterm::terminal::disable_raw_mode()?;
    println!();
    // Best effort: clear the temporary buffer copy on error.
    if result.is_err() {
        let mut z = pw;
        unsafe { std::ptr::write_bytes(z.as_mut_ptr(), 0, z.len()) };
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_host() {
        assert_eq!(
            parse_host("example.com"),
            HostSpec {
                user: "".into(),
                host: "example.com".into(),
                port: 22
            }
        );
    }

    #[test]
    fn parse_host_port() {
        let h = parse_host("example.com:2222");
        assert_eq!(h.host, "example.com");
        assert_eq!(h.port, 2222);
        assert!(h.user.is_empty());
    }

    #[test]
    fn parse_user_host() {
        let h = parse_host("keith@example.com");
        assert_eq!(h.user, "keith");
        assert_eq!(h.host, "example.com");
        assert_eq!(h.port, 22);
    }

    #[test]
    fn parse_user_host_port() {
        let h = parse_host("keith@example.com:2200");
        assert_eq!(h.user, "keith");
        assert_eq!(h.host, "example.com");
        assert_eq!(h.port, 2200);
    }

    #[test]
    fn invalid_port_falls_back_to_22() {
        assert_eq!(parse_host("h:notaport").port, 22);
        assert_eq!(parse_host("h:99999").port, 22);
    }

    #[test]
    fn key_discovery_order_is_stable() {
        let dir = std::env::temp_dir().join(format!("ssh4_keys_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // No keys -> None.
        assert_eq!(discover_default_key_in(&dir), None);
        // id_rsa alone is found.
        std::fs::write(dir.join("id_rsa"), "x").unwrap();
        assert_eq!(discover_default_key_in(&dir), Some(dir.join("id_rsa")));
        // id_ed25519 takes priority over id_rsa.
        std::fs::write(dir.join("id_ed25519"), "x").unwrap();
        assert_eq!(discover_default_key_in(&dir), Some(dir.join("id_ed25519")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
