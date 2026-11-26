//! Every API route as a typed builder. Each path segment is percent-encoded.

use reqwest::Method;
use url::Url;

use crate::error::{CliError, CliResult};

/// One API request target: a method, path segments and a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub method: Method,
    pub segments: Vec<String>,
    pub query: Vec<(&'static str, String)>,
}

impl Endpoint {
    fn new(method: Method, segments: &[&str]) -> Self {
        Self {
            method,
            segments: segments.iter().map(|s| (*s).to_owned()).collect(),
            query: Vec::new(),
        }
    }

    /// The full URL under `base`, keeping `base`'s own path prefix.
    pub fn url(&self, base: &Url) -> Url {
        let mut url = base.clone();
        if let Ok(mut segs) = url.path_segments_mut() {
            segs.pop_if_empty();
            segs.extend(&self.segments);
        }
        if !self.query.is_empty() {
            url.query_pairs_mut().extend_pairs(&self.query);
        }
        url
    }
}

pub fn health() -> Endpoint {
    Endpoint::new(Method::GET, &["api", "health"])
}

pub fn setup() -> Endpoint {
    Endpoint::new(Method::POST, &["api", "setup"])
}

pub fn me() -> Endpoint {
    Endpoint::new(Method::GET, &["api", "me"])
}

pub fn printers() -> Endpoint {
    Endpoint::new(Method::GET, &["api", "printers"])
}

/// `{method} api/printers/{name}/{rest…}`.
fn printer_at(method: Method, name: &str, rest: &[&str]) -> Endpoint {
    let mut segments = vec!["api".to_owned(), "printers".to_owned(), name.to_owned()];
    segments.extend(rest.iter().map(|s| (*s).to_owned()));
    Endpoint {
        method,
        segments,
        query: Vec::new(),
    }
}

pub fn register_printer() -> Endpoint {
    Endpoint::new(Method::POST, &["api", "printers"])
}

pub fn remove_printer(name: &str) -> Endpoint {
    printer_at(Method::DELETE, name, &[])
}

pub fn printer(name: &str) -> Endpoint {
    printer_at(Method::GET, name, &[])
}

pub fn printer_info(name: &str) -> Endpoint {
    printer_at(Method::GET, name, &["info"])
}

pub fn printer_status(name: &str) -> Endpoint {
    printer_at(Method::GET, name, &["status"])
}

pub fn printer_storage(name: &str) -> Endpoint {
    printer_at(Method::GET, name, &["storage"])
}

/// A place on a printer's storage: `<storage>[/<path…>]`. Segments are kept
/// apart so each one is encoded on its own and never interpreted as a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePath {
    pub storage: String,
    /// Empty means the storage root.
    pub segments: Vec<String>,
}

impl RemotePath {
    /// Parses `<storage>[/<path>]`. Returns the path and whether it names a
    /// folder (no path, or a trailing `/` when `allow_folder`).
    pub fn parse(s: &str, allow_folder: bool) -> CliResult<(Self, bool)> {
        let usage = |m: &str| CliError::Usage(m.to_owned());
        if s.contains('\0') {
            return Err(usage("file paths cannot contain NUL bytes"));
        }
        if s.contains('\\') {
            return Err(usage("file paths cannot contain backslashes"));
        }
        let mut parts: Vec<&str> = s.split('/').collect();
        let mut folder = parts.len() == 1;
        if parts.len() > 1 && parts.last() == Some(&"") {
            if !allow_folder {
                return Err(usage("file paths cannot contain empty segments"));
            }
            parts.pop();
            folder = true;
        }
        if parts.iter().any(|p| p.is_empty()) {
            return Err(usage("file paths cannot contain empty segments"));
        }
        if parts.iter().any(|p| matches!(*p, "." | "..")) {
            return Err(usage("file paths cannot contain `.` or `..` segments"));
        }
        let storage = parts[0];
        if !valid_id(storage) {
            return Err(CliError::Usage(format!(
                "invalid storage name `{storage}`: use 1-64 letters, digits, '_' or '-'"
            )));
        }
        Ok((
            Self {
                storage: storage.to_owned(),
                segments: parts[1..].iter().map(|p| (*p).to_owned()).collect(),
            },
            folder,
        ))
    }

    /// Fails for a bare storage name.
    pub fn require_file(&self) -> CliResult<()> {
        if self.segments.is_empty() {
            Err(CliError::Usage(
                "give a file path, e.g. usb/cube.bgcode".into(),
            ))
        } else {
            Ok(())
        }
    }

    /// `usb/My Parts/a.bgcode`.
    pub fn display(&self) -> String {
        std::iter::once(self.storage.as_str())
            .chain(self.segments.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The path without the storage: `My Parts/a.bgcode`.
    pub fn path_display(&self) -> String {
        self.segments.join("/")
    }
}

fn valid_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Camera ids are 1-64 of `[A-Za-z0-9_-]`. `order` and `snap` are taken by
/// static API routes, so a camera cannot have those ids.
pub fn validate_camera_id(id: &str) -> CliResult<()> {
    if !valid_id(id) {
        return Err(CliError::Usage(format!(
            "invalid camera id `{id}`: use 1-64 letters, digits, '_' or '-'"
        )));
    }
    if matches!(id, "order" | "snap") {
        return Err(CliError::Usage(format!(
            "camera id `{id}` is reserved by the API routes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn remote_paths() {
        let (p, folder) = RemotePath::parse("usb", true).unwrap();
        assert_eq!(
            (p.storage.as_str(), p.segments.len(), folder),
            ("usb", 0, true)
        );
        let (p, folder) = RemotePath::parse("usb/My Parts/café #1?%.bgcode", false).unwrap();
        assert_eq!((p.segments.len(), folder), (2, false));
        assert_eq!(p.display(), "usb/My Parts/café #1?%.bgcode");
        let (p, folder) = RemotePath::parse("usb/parts/", true).unwrap();
        assert_eq!((p.segments, folder), (vec!["parts".to_owned()], true));
        for bad in [
            "usb/a/../b",
            "usb//a",
            "usb/./a",
            "usb/a\\b",
            "usb/a\0b",
            "/usb/a",
            "usb/a/",
            "us b/a",
        ] {
            assert!(
                matches!(RemotePath::parse(bad, false), Err(CliError::Usage(_))),
                "{bad:?}"
            );
        }
        assert!(
            RemotePath::parse("usb", false)
                .unwrap()
                .0
                .require_file()
                .is_err()
        );
    }

    #[test]
    fn camera_ids() {
        assert!(validate_camera_id("Z42D4U2NqEX").is_ok());
        for bad in ["", "a/b", "a b", "snap", "order", &"x".repeat(65)] {
            assert!(validate_camera_id(bad).is_err(), "{bad:?}");
        }
    }

    use super::*;

    #[test]
    fn url_keeps_prefix_and_encodes_segments() {
        let ep = Endpoint {
            method: Method::GET,
            segments: [
                "api",
                "printers",
                "mk4",
                "files",
                "usb",
                "My Parts",
                "café #1?%.bgcode",
            ]
            .map(String::from)
            .to_vec(),
            query: vec![],
        };
        assert_eq!(
            ep.url(&Url::parse("https://h.example/starstar").unwrap())
                .as_str(),
            "https://h.example/starstar/api/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode"
        );
        let base = Url::parse("http://127.0.0.1:8080").unwrap();
        assert_eq!(
            health().url(&base).as_str(),
            "http://127.0.0.1:8080/api/health"
        );
        let mut q = health();
        q.query.push(("force", "true".into()));
        assert!(q.url(&base).as_str().ends_with("?force=true"));
    }
}
