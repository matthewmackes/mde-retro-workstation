//! `mde mount <uri>` — mount a remote filesystem and print its local path.
//!
//! Used by the File Explorer's Network (SMB) and Cloud (SFTP) panes (E8.5/E8.8):
//! it mounts `smb://` / `dav://` / … via GVfs (`gio mount`) and `sftp://` /
//! `ssh://` via `sshfs`, with bounded connects (so an unreachable peer fails in
//! seconds, never hanging the caller), and prints the resulting local path on
//! stdout so the caller can navigate into it. On any error it prints a message to
//! stderr and exits non-zero — it never panics.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub fn run(args: &[String]) -> ExitCode {
    let Some(uri) = args.first() else {
        eprintln!("usage: mde mount <uri>   (e.g. smb://host/share, sftp://host/path)");
        return ExitCode::from(2);
    };
    match mount(uri) {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("mde mount: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Validate a `scheme://[user@]host[:port][/path]` URI, returning (scheme, host).
fn parse_uri(uri: &str) -> Result<(String, String), String> {
    let (scheme, rest) = uri
        .split_once("://")
        .ok_or_else(|| format!("not a URI (expected scheme://host…): '{uri}'"))?;
    if scheme.is_empty() {
        return Err(format!("URI has no scheme: '{uri}'"));
    }
    // The authority is everything up to the first '/', minus any user@ and :port.
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        return Err(format!("URI has no host: '{uri}'"));
    }
    Ok((scheme.to_string(), host.to_string()))
}

/// `$XDG_RUNTIME_DIR/gvfs`, where GVfs exposes its FUSE mounts.
fn gvfs_dir() -> Result<PathBuf, String> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|r| PathBuf::from(r).join("gvfs"))
        .ok_or_else(|| "XDG_RUNTIME_DIR not set (no session runtime dir)".to_string())
}

/// The GVfs FUSE entry whose name embeds `host` (GVfs names always do, e.g.
/// `smb-share:server=HOST,share=…`, `sftp:host=HOST`), if currently mounted.
fn find_in_gvfs(gvfs: &Path, host: &str) -> Option<PathBuf> {
    std::fs::read_dir(gvfs)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.contains(host))
                .unwrap_or(false)
        })
}

fn mount(uri: &str) -> Result<PathBuf, String> {
    let (scheme, host) = parse_uri(uri)?;
    // sftp/ssh go straight to sshfs (the cloud-device path) with a bounded connect,
    // so an unreachable peer fails in seconds instead of hanging the caller.
    if scheme == "sftp" || scheme == "ssh" {
        return sshfs_mount(uri, &host);
    }
    // smb/dav/google-drive/…: GVfs. A `timeout` wrapper bounds the connect so an
    // unreachable host fails promptly; an already-mounted share is a no-op success
    // and a credentials prompt can't hang us.
    let gio = Command::new("timeout")
        .arg("12")
        .arg("gio")
        .arg("mount")
        .arg(uri)
        .output();
    if let Ok(g) = gvfs_dir() {
        if let Some(p) = find_in_gvfs(&g, &host) {
            return Ok(p);
        }
    }
    let detail = match gio {
        Ok(o) if o.status.code() == Some(124) => format!("connection to '{host}' timed out"),
        Ok(o) if !o.status.success() => String::from_utf8_lossy(&o.stderr).trim().to_string(),
        Ok(_) => "mounted, but its GVfs path was not found".to_string(),
        Err(e) => format!("gio not available: {e}"),
    };
    Err(if detail.is_empty() {
        format!("could not mount '{uri}'")
    } else {
        detail
    })
}

/// Mount `sftp://[user@]host[:port]/path` with sshfs into a per-host dir under the
/// session runtime dir, returning that mountpoint.
fn sshfs_mount(uri: &str, host: &str) -> Result<PathBuf, String> {
    let rt = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "XDG_RUNTIME_DIR not set".to_string())?;
    let mp = rt.join("mde-mounts").join(host);
    std::fs::create_dir_all(&mp).map_err(|e| format!("could not create {}: {e}", mp.display()))?;
    // scheme://[user@]host[:port]/path  ->  [user@]host:/path  (sshfs remote spec)
    let rest = uri.split_once("://").map(|x| x.1).unwrap_or("");
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let remote = format!("{authority}:{path}");
    // Bounded connect (ssh ConnectTimeout + a hard `timeout` wrapper) so an
    // unreachable peer fails in seconds rather than hanging the caller. `.output()`
    // captures stderr so the wrapper's own noise never leaks to the caller.
    match Command::new("timeout")
        .arg("15")
        .arg("sshfs")
        .arg("-o")
        .arg("ConnectTimeout=8,reconnect=no")
        .arg(&remote)
        .arg(&mp)
        .output()
    {
        Ok(o) if o.status.success() => Ok(mp),
        Ok(o) if o.status.code() == Some(124) => Err(format!("connection to '{host}' timed out")),
        Ok(o) if o.status.code() == Some(127) => Err("sshfs is not installed".to_string()),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            Err(if err.is_empty() {
                format!("sshfs failed for '{uri}'")
            } else {
                err
            })
        }
        Err(e) => Err(format!("sshfs not available: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_good_uris() {
        assert_eq!(
            parse_uri("smb://nas/media").unwrap(),
            ("smb".to_string(), "nas".to_string())
        );
        assert_eq!(
            parse_uri("sftp://user@host:22/home/me").unwrap(),
            ("sftp".to_string(), "host".to_string())
        );
        assert_eq!(
            parse_uri("smb://server").unwrap(),
            ("smb".to_string(), "server".to_string())
        );
    }

    #[test]
    fn rejects_bad_uris() {
        assert!(parse_uri("not-a-uri").is_err()); // no scheme://
        assert!(parse_uri("smb://").is_err()); // no host
        assert!(parse_uri("://host").is_err()); // no scheme
        assert!(parse_uri("smb:///share").is_err()); // empty host
    }
}
