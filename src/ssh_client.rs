//! SSH connection logic, isolated from UI. GUI callers receive errors as
//! strings and must never trigger stdin prompts from here.

use ssh::{LocalSession, LocalShell, SshError, TerminalSize};
use std::net::TcpStream;
use std::time::Duration;

pub type Session = LocalSession<TcpStream>;
pub type Shell = LocalShell<TcpStream>;

/// A secret string whose bytes are overwritten on drop so it does not
/// linger in freed memory.
#[derive(Clone, Default, PartialEq)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Secret(s.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        let bytes = unsafe { self.0.as_bytes_mut() };
        for b in bytes.iter_mut() {
            // Volatile write so the zeroing is not optimized away.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl From<String> for Secret {
    fn from(s: String) -> Self {
        Secret(s)
    }
}

/// Canonical in-memory connection bundle.
#[derive(Clone, Debug)]
pub struct ConnParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<Secret>,
    pub key_path: Option<String>,
    pub timeout_secs: u64,
    pub verbose: bool,
}

impl Default for ConnParams {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            user: String::new(),
            password: None,
            key_path: None,
            timeout_secs: 15,
            verbose: false,
        }
    }
}

impl ConnParams {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn identity(&self) -> String {
        if self.user.is_empty() {
            self.host.clone()
        } else {
            format!("{}@{}", self.user, self.host)
        }
    }
}

/// Map ssh-rs errors to user-facing messages.
pub fn friendly_error(e: &SshError) -> String {
    match e {
        SshError::AuthError => {
            "Authentication failed: check username, password, or key".to_string()
        }
        SshError::TimeoutError => "Connection timed out".to_string(),
        SshError::VersionDismatchError { our, their } => {
            format!("SSH version mismatch (local {our}, remote {their})")
        }
        SshError::KexError(m) => format!("Key exchange failed: {m}"),
        SshError::SshPubKeyError(m) => format!("Could not parse private key: {m}"),
        SshError::IoError(io) => match io.kind() {
            std::io::ErrorKind::ConnectionRefused => "Connection refused by host".to_string(),
            std::io::ErrorKind::TimedOut => "Connection timed out".to_string(),
            std::io::ErrorKind::UnexpectedEof => "Connection closed by remote host".to_string(),
            _ => format!("Network error: {io}"),
        },
        other => format!("SSH error: {other}"),
    }
}

/// Open an SSH session from connection parameters. Never prompts; returns a
/// friendly error string on failure.
pub fn connect_session(params: &ConnParams) -> Result<Session, String> {
    if params.host.is_empty() {
        return Err("No host given".to_string());
    }
    if params.user.is_empty() {
        return Err("No username given".to_string());
    }
    if params.password.is_none() && params.key_path.is_none() {
        return Err("No password or key available for authentication".to_string());
    }

    let mut builder = ssh::create_session()
        .username(&params.user)
        .timeout(Some(Duration::from_secs(params.timeout_secs)));
    if let Some(pw) = &params.password {
        builder = builder.password(pw.expose());
    }
    if let Some(key) = &params.key_path {
        builder = builder.private_key_path(key);
    }

    let connector = builder
        .connect(params.addr())
        .map_err(|e| friendly_error(&e))?;
    Ok(connector.run_local())
}

/// Open a remote shell with the requested PTY size (cols x rows).
pub fn open_shell(session: &mut Session, cols: u16, rows: u16) -> Result<Shell, String> {
    session
        .open_shell_terminal(TerminalSize::from(cols as u32, rows as u32))
        .map_err(|e| friendly_error(&e))
}

/// Read timeout used by interactive loops so the worker can poll its
/// stop flag and input queue (SOFTWARE_DESIGN.md §12: 20 ms).
pub const READ_TIMEOUT: Duration = Duration::from_millis(20);

/// Chunk size for writes to the remote PTY.
pub const SSH_WRITE_CHUNK: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_formats() {
        let p = ConnParams {
            host: "h".into(),
            user: "u".into(),
            ..Default::default()
        };
        assert_eq!(p.identity(), "u@h");
        assert_eq!(p.addr(), "h:22");
    }

    #[test]
    fn debug_hides_password() {
        let p = ConnParams {
            password: Some(Secret::new("secret")),
            ..Default::default()
        };
        let s = format!("{p:?}");
        assert!(!s.contains("secret"));
    }

    #[test]
    fn secret_exposes_value() {
        let s = Secret::new("hunter2");
        assert_eq!(s.expose(), "hunter2");
        assert_eq!(format!("{s:?}"), "Secret(***)");
    }

    #[test]
    fn connect_requires_credentials() {
        let p = ConnParams {
            host: "h".into(),
            user: "u".into(),
            ..Default::default()
        };
        let err = match connect_session(&p) {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert!(err.contains("password or key"));
    }
}
