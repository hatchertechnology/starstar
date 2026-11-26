//! Every API route as a typed builder. Each path segment is percent-encoded.

use reqwest::Method;
use url::Url;

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

#[cfg(test)]
mod tests {
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
