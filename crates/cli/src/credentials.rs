//! `credentials.json`: one sign-in per server, guarded by an advisory lock.
//!
//! The lock lives on a sibling file, `credentials.json.lock`. Writes go to a
//! temp file in the same directory (mode 0600) that is renamed over the
//! credentials file, so a crash never leaves a half-written file.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{CliError, CliResult};

/// One server's sign-in. Deliberately not `Debug`.
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: i64,
    pub scope: String,
    pub issuer: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct FileShape {
    version: u32,
    servers: BTreeMap<String, StoredToken>,
}

impl Default for FileShape {
    fn default() -> Self {
        Self {
            version: 1,
            servers: BTreeMap::new(),
        }
    }
}

/// The key of a server's entry: its origin plus its path without a trailing slash.
pub fn server_key(api: &Url) -> String {
    format!(
        "{}{}",
        api.origin().ascii_serialization(),
        api.path().trim_end_matches('/')
    )
}

/// `$XDG_CONFIG_HOME/starstar/credentials.json`, or under `$HOME/.config`.
pub fn path_from(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg {
        Some(x) if !x.is_empty() && Path::new(x).is_absolute() => PathBuf::from(x),
        _ => match home {
            Some(h) if !h.is_empty() => Path::new(h).join(".config"),
            _ => return None,
        },
    };
    Some(base.join("starstar").join("credentials.json"))
}

pub fn default_path() -> CliResult<PathBuf> {
    path_from(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
    .ok_or_else(|| {
        CliError::Other("cannot find a config directory: set HOME or XDG_CONFIG_HOME".into())
    })
}

pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(".lock");
        self.path.with_file_name(name)
    }

    fn corrupt(&self) -> CliError {
        CliError::Other(format!(
            "credentials file {} is corrupt; delete it and run `starstar login`",
            self.path.display()
        ))
    }

    fn open_lock(&self, create_dir: bool) -> CliResult<Option<File>> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        if !dir.exists() {
            if !create_dir {
                return Ok(None);
            }
            create_private_dir(dir)?;
        }
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        Ok(Some(opts.open(self.lock_path())?))
    }

    fn read(&self) -> CliResult<FileShape> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FileShape::default()),
            Err(e) => return Err(e.into()),
        };
        let shape: FileShape = serde_json::from_slice(&bytes).map_err(|_| self.corrupt())?;
        if shape.version != 1 {
            return Err(self.corrupt());
        }
        Ok(shape)
    }

    fn write(&self, shape: &FileShape) -> CliResult<()> {
        let mut tmp_name = self.path.file_name().unwrap_or_default().to_os_string();
        tmp_name.push(".tmp");
        let tmp = self.path.with_file_name(tmp_name);
        let result = (|| -> CliResult<()> {
            let mut opts = OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            let mut f = opts.open(&tmp)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                f.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            let json = serde_json::to_vec_pretty(shape)
                .map_err(|e| CliError::Other(format!("could not encode credentials: {e}")))?;
            f.write_all(&json)?;
            f.sync_all()?;
            fs::rename(&tmp, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    /// The entry for `key`, under a shared lock. No file means no entry.
    pub fn get(&self, key: &str) -> CliResult<Option<StoredToken>> {
        let Some(lock) = self.open_lock(false)? else {
            return Ok(None);
        };
        lock.lock_shared()?;
        Ok(self.read()?.servers.remove(key))
    }

    pub fn put(&self, key: &str, token: StoredToken) -> CliResult<()> {
        self.update(key, |_| Ok((Some(token), ())))
    }

    pub fn remove(&self, key: &str) -> CliResult<Option<StoredToken>> {
        if !self.path.exists() {
            return Ok(None);
        }
        self.update(key, |cur| Ok((None, cur)))
    }

    /// Holds the exclusive lock while `f` runs, network calls included. `f`
    /// gets the current entry and returns the entry to store (`None` removes
    /// it) and a result. Other servers' entries are kept.
    pub fn update<R>(
        &self,
        key: &str,
        f: impl FnOnce(Option<StoredToken>) -> CliResult<(Option<StoredToken>, R)>,
    ) -> CliResult<R> {
        let lock = self
            .open_lock(true)?
            .ok_or_else(|| CliError::Other("cannot create the config directory".into()))?;
        lock.lock()?;
        let mut shape = self.read()?;
        let (new, result) = f(shape.servers.remove(key))?;
        let has_entry = new.is_some();
        if let Some(t) = new {
            shape.servers.insert(key.to_owned(), t);
        }
        // Removing from a store that was never written needs no file.
        if has_entry || self.path.exists() {
            self.write(&shape)?;
        }
        Ok(result)
    }
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(dir)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;

    fn tok(access: &str) -> StoredToken {
        StoredToken {
            access_token: access.into(),
            refresh_token: "rt".into(),
            expires_at: 1,
            scope: "starstar:operator".into(),
            issuer: "http://x".into(),
            token_endpoint: "http://x/oauth/token".into(),
            revocation_endpoint: None,
        }
    }

    fn store() -> (tempfile::TempDir, CredentialStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = CredentialStore::at(dir.path().join("cfg/starstar/credentials.json"));
        (dir, s)
    }

    #[test]
    fn server_keys_normalize() {
        let k = |s: &str| server_key(&Url::parse(s).unwrap());
        assert_eq!(k("http://127.0.0.1:8080/"), "http://127.0.0.1:8080");
        assert_eq!(k("http://127.0.0.1:8080"), "http://127.0.0.1:8080");
        assert_eq!(k("https://H.Example:443/x/"), "https://h.example/x");
    }

    #[test]
    fn put_get_remove_round_trip_and_keeps_other_servers() {
        let (_d, s) = store();
        assert!(s.get("a").unwrap().is_none());
        s.put("a", tok("a1")).unwrap();
        s.put("b", tok("b1")).unwrap();
        assert_eq!(s.get("a").unwrap().unwrap().access_token, "a1");
        assert_eq!(s.remove("a").unwrap().unwrap().access_token, "a1");
        assert!(s.get("a").unwrap().is_none());
        assert_eq!(s.get("b").unwrap().unwrap().access_token, "b1");
    }

    #[cfg(unix)]
    #[test]
    fn file_and_dir_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, s) = store();
        s.put("a", tok("a1")).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(s.path()), 0o600);
        assert_eq!(mode(s.path().parent().unwrap()), 0o700);

        let (_d2, s2) = store();
        fs::create_dir_all(s2.path().parent().unwrap()).unwrap();
        fs::write(s2.path(), r#"{"version":1,"servers":{}}"#).unwrap();
        fs::set_permissions(s2.path(), fs::Permissions::from_mode(0o644)).unwrap();
        s2.put("a", tok("a1")).unwrap();
        assert_eq!(mode(s2.path()), 0o600);
    }

    #[test]
    fn corrupt_file_is_reported() {
        for content in ["{not json", ""] {
            let (_d, s) = store();
            fs::create_dir_all(s.path().parent().unwrap()).unwrap();
            fs::write(s.path(), content).unwrap();
            let Err(CliError::Other(m)) = s.get("a") else {
                panic!("expected corrupt error for {content:?}")
            };
            assert!(m.contains("starstar login"), "{m}");
        }
        let (_d, s) = store();
        fs::create_dir_all(s.path().parent().unwrap()).unwrap();
        fs::write(s.path(), r#"{"version":2,"servers":{}}"#).unwrap();
        assert!(s.get("a").is_err());
    }

    #[test]
    fn update_holds_the_lock() {
        let (_d, s) = store();
        let s = Arc::new(s);
        s.put("k", tok("a1")).unwrap();
        let a = {
            let s = s.clone();
            std::thread::spawn(move || {
                s.update("k", |_| {
                    std::thread::sleep(Duration::from_millis(300));
                    Ok((Some(tok("a2")), ()))
                })
                .unwrap();
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        let start = Instant::now();
        let got = s.get("k").unwrap().unwrap();
        assert_eq!(got.access_token, "a2");
        assert!(start.elapsed() >= Duration::from_millis(200));
        a.join().unwrap();
    }

    #[test]
    fn update_none_removes_entry() {
        let (_d, s) = store();
        s.put("k", tok("a1")).unwrap();
        s.update("k", |cur| {
            assert!(cur.is_some());
            Ok((None, ()))
        })
        .unwrap();
        assert!(s.get("k").unwrap().is_none());
    }

    #[test]
    fn default_path_rules() {
        let p = |x, h| path_from(x, h);
        assert_eq!(
            p(Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/starstar/credentials.json"))
        );
        let home_default = Some(PathBuf::from("/h/.config/starstar/credentials.json"));
        assert_eq!(p(Some(""), Some("/h")), home_default);
        assert_eq!(p(Some("rel"), Some("/h")), home_default);
        assert_eq!(p(None, None), None);
    }
}
