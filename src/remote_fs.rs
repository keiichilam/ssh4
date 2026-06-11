//! Remote file operations over SSH exec + SCP (TR-001).
//!
//! Listing and mutations run shell commands over a one-shot exec channel
//! (the vendored ssh-rs has no SFTP); transfers use SCP. Every remote path
//! is single-quoted with [`sh_quote`] — never interpolated raw. Parsing and
//! quoting live here so they are unit-testable without a server.

use crate::ssh_client::{connect_session, friendly_error, ConnParams};
use std::path::Path;

/// One entry from a remote `ls -la` listing.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    /// rwx permission string without the type char, e.g. `rwxr-xr-x`.
    pub perms: String,
    /// Modification date as printed by `ls` (three fields).
    pub modified: String,
}

/// POSIX single-quote escaping: `'` becomes `'\''`.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Join a remote POSIX directory and name.
pub fn join(dir: &str, name: &str) -> String {
    crate::transfer::remote_join(dir, name)
}

/// Parent of a remote POSIX path (`/` is its own parent).
pub fn parent(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
    }
}

/// Parse one long-format `ls -la` line. Returns `None` for the `total` line,
/// `.`/`..`, and anything that doesn't look like a listing row.
pub fn parse_ls_line(line: &str) -> Option<RemoteEntry> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 9 {
        return None;
    }
    let mode = fields[0];
    let type_ch = mode.chars().next()?;
    if !"-dlbcps".contains(type_ch) || mode.len() < 10 {
        return None;
    }
    let size: u64 = fields[4].parse().unwrap_or(0);
    let modified = format!("{} {} {}", fields[5], fields[6], fields[7]);
    // Name starts at the 9th field; re-find it in the raw line so names with
    // spaces survive. Links show as `name -> target`; keep only the name.
    let prefix_end = {
        let mut count = 0;
        let mut idx = 0;
        let bytes = line.as_bytes();
        let mut in_field = false;
        for (i, b) in bytes.iter().enumerate() {
            let ws = b.is_ascii_whitespace();
            if !ws && !in_field {
                count += 1;
                in_field = true;
                if count == 9 {
                    idx = i;
                    break;
                }
            } else if ws {
                in_field = false;
            }
        }
        idx
    };
    let mut name = line[prefix_end..].trim_end().to_string();
    let is_link = type_ch == 'l';
    if is_link {
        if let Some(pos) = name.find(" -> ") {
            name.truncate(pos);
        }
    }
    if name == "." || name == ".." || name.is_empty() {
        return None;
    }
    Some(RemoteEntry {
        name,
        is_dir: type_ch == 'd',
        is_link,
        size,
        perms: mode[1..10].to_string(),
        modified,
    })
}

/// Parse full `ls -la` output, directories first then files, each sorted.
pub fn parse_ls_output(out: &str) -> Vec<RemoteEntry> {
    let mut entries: Vec<RemoteEntry> = out.lines().filter_map(parse_ls_line).collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// rwx permission string to octal (e.g. `rwxr-xr-x` → `755`).
pub fn perms_to_octal(perms: &str) -> String {
    let mut digits = String::new();
    for chunk in perms.as_bytes().chunks(3).take(3) {
        let mut v = 0;
        if chunk.first().is_some_and(|&c| c != b'-') {
            v += 4;
        }
        if chunk.get(1).is_some_and(|&c| c != b'-') {
            v += 2;
        }
        // s/t in the execute slot still grants execute.
        if chunk
            .get(2)
            .is_some_and(|&c| matches!(c, b'x' | b's' | b't'))
        {
            v += 1;
        }
        digits.push_str(&v.to_string());
    }
    digits
}

/// Validate a chmod mode string: 3–4 octal digits.
pub fn valid_mode(mode: &str) -> bool {
    (3..=4).contains(&mode.len()) && mode.bytes().all(|b| (b'0'..=b'7').contains(&b))
}

/// Run one command over a fresh exec channel. Returns trimmed output, or an
/// error containing the command output when the exit status is non-zero.
fn exec(params: &ConnParams, cmd: &str) -> Result<String, String> {
    let mut session = connect_session(params)?;
    let result = (|| {
        let mut chan = session.open_exec().map_err(|e| friendly_error(&e))?;
        chan.exec_command(cmd).map_err(|e| friendly_error(&e))?;
        let out = chan.get_output().map_err(|e| friendly_error(&e))?;
        let text = String::from_utf8_lossy(&out).into_owned();
        let code = chan.exit_status().unwrap_or(0);
        if code != 0 {
            let msg = text.trim();
            Err(if msg.is_empty() {
                format!("Remote command failed (exit {code})")
            } else {
                format!("{msg} (exit {code})")
            })
        } else {
            Ok(text)
        }
    })();
    session.close();
    result
}

/// List a remote directory.
pub fn list(params: &ConnParams, dir: &str) -> Result<Vec<RemoteEntry>, String> {
    let out = exec(params, &format!("LC_ALL=C ls -la -- {}", sh_quote(dir)))?;
    Ok(parse_ls_output(&out))
}

pub fn mkdir(params: &ConnParams, path: &str) -> Result<(), String> {
    exec(params, &format!("mkdir -p -- {}", sh_quote(path))).map(|_| ())
}

pub fn touch(params: &ConnParams, path: &str) -> Result<(), String> {
    exec(params, &format!("touch -- {}", sh_quote(path))).map(|_| ())
}

pub fn rename(params: &ConnParams, from: &str, to: &str) -> Result<(), String> {
    exec(
        params,
        &format!("mv -- {} {}", sh_quote(from), sh_quote(to)),
    )
    .map(|_| ())
}

/// Delete a remote path recursively. The UI must confirm first.
pub fn delete(params: &ConnParams, path: &str) -> Result<(), String> {
    exec(params, &format!("rm -rf -- {}", sh_quote(path))).map(|_| ())
}

pub fn chmod(params: &ConnParams, mode: &str, path: &str) -> Result<(), String> {
    if !valid_mode(mode) {
        return Err(format!("Invalid mode: {mode} (use octal, e.g. 644)"));
    }
    exec(params, &format!("chmod {} -- {}", mode, sh_quote(path))).map(|_| ())
}

/// Download a remote path to a local path over a fresh SCP connection.
pub fn download(params: &ConnParams, remote: &str, local: &Path) -> Result<(), String> {
    let mut session = connect_session(params)?;
    let result = (|| {
        let scp = session.open_scp().map_err(|e| friendly_error(&e))?;
        scp.download(local.as_os_str(), std::ffi::OsStr::new(remote))
            .map_err(|e| format!("Download failed: {e}"))
    })();
    session.close();
    result
}

/// Upload a local path to a remote directory (reuses the transfer path).
pub fn upload(params: &ConnParams, local: &Path, remote_dir: &str) -> Result<String, String> {
    let name = local
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Bad local filename".to_string())?;
    let remote = join(remote_dir, name);
    crate::transfer::scp_upload(params, local, &remote)?;
    Ok(remote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_plain_and_embedded_quote() {
        assert_eq!(sh_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
        // Metacharacters are inert inside single quotes.
        assert_eq!(sh_quote("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    #[test]
    fn parent_paths() {
        assert_eq!(parent("/home/kei/dir"), "/home/kei");
        assert_eq!(parent("/home"), "/");
        assert_eq!(parent("/"), "/");
        assert_eq!(parent("/a/b/"), "/a");
    }

    #[test]
    fn parses_ls_file_line() {
        let e = parse_ls_line("-rw-r--r--  1 kei  users   1234 Jun  9 14:30 notes.txt").unwrap();
        assert_eq!(e.name, "notes.txt");
        assert!(!e.is_dir);
        assert_eq!(e.size, 1234);
        assert_eq!(e.perms, "rw-r--r--");
        assert_eq!(e.modified, "Jun 9 14:30");
    }

    #[test]
    fn parses_dir_link_and_spaces() {
        let d = parse_ls_line("drwxr-xr-x 2 kei users 4096 Jun  9 14:30 my dir").unwrap();
        assert!(d.is_dir);
        assert_eq!(d.name, "my dir");
        let l = parse_ls_line("lrwxrwxrwx 1 kei users 9 Jun  9 14:30 link -> /etc/host").unwrap();
        assert!(l.is_link);
        assert_eq!(l.name, "link");
    }

    #[test]
    fn skips_total_dot_and_garbage() {
        assert!(parse_ls_line("total 48").is_none());
        assert!(parse_ls_line("drwxr-xr-x 2 kei users 4096 Jun  9 14:30 .").is_none());
        assert!(parse_ls_line("drwxr-xr-x 2 kei users 4096 Jun  9 14:30 ..").is_none());
        assert!(parse_ls_line("").is_none());
    }

    #[test]
    fn listing_sorts_dirs_first() {
        let out = "total 8\n\
                   -rw-r--r-- 1 k u 10 Jun 9 14:30 zfile\n\
                   drwxr-xr-x 2 k u 4096 Jun 9 14:30 adir\n\
                   -rw-r--r-- 1 k u 10 Jun 9 14:30 afile\n";
        let names: Vec<String> = parse_ls_output(out).into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["adir", "afile", "zfile"]);
    }

    #[test]
    fn perms_octal_roundtrip() {
        assert_eq!(perms_to_octal("rwxr-xr-x"), "755");
        assert_eq!(perms_to_octal("rw-r--r--"), "644");
        assert_eq!(perms_to_octal("rwsr-xr-t"), "755");
        assert!(valid_mode("644"));
        assert!(valid_mode("0755"));
        assert!(!valid_mode("99"));
        assert!(!valid_mode("abc"));
    }
}
