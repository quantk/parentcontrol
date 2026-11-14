//! The agent's link to its server: address, token, pinned certificate.

use std::fmt;

use parentcontrol_core::protocol::AgentToken;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Port of the server when the URL names none.
const DEFAULT_PORT: u16 = 443;

/// Where the server is, parsed from `https://host[:port][/]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Endpoint {
    host: String,
    port: u16,
}

/// A server URL the agent cannot use.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid server URL {0:?}, expected https://host:port")]
pub(crate) struct InvalidEndpoint(String);

impl Endpoint {
    /// The host name or IP address, also used for TLS server name.
    pub(crate) fn host(&self) -> &str {
        &self.host
    }

    /// `host:port` to connect to, with IPv6 literals in brackets.
    pub(crate) fn address(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// The agent WebSocket URL.
    pub(crate) fn websocket_url(&self) -> String {
        format!("wss://{}/api/agent", self.address())
    }
}

impl TryFrom<String> for Endpoint {
    type Error = InvalidEndpoint;

    fn try_from(url: String) -> Result<Self, Self::Error> {
        let invalid = || InvalidEndpoint(url.clone());
        let authority = url.strip_prefix("https://").ok_or_else(invalid)?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);
        if authority.is_empty() || authority.contains(['/', '?', '#', '@']) {
            return Err(invalid());
        }
        let (host, port) = match authority.rsplit_once(':') {
            // An IPv6 literal without port has colons but ends with `]`.
            Some((host, port)) if !port.ends_with(']') => {
                (host, port.parse().map_err(|_| invalid())?)
            }
            _ => (authority, DEFAULT_PORT),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            host: host.to_owned(),
            port,
        })
    }
}

impl From<Endpoint> for String {
    fn from(endpoint: Endpoint) -> Self {
        endpoint.to_string()
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "https://{}", self.address())
    }
}

/// What enrollment produced; stored next to the policy snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Link {
    pub(crate) server: Endpoint,
    pub(crate) token: AgentToken,
    /// SHA-256 of the server certificate seen at enrollment, lowercase hex.
    pub(crate) fingerprint: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(url: &str) -> Result<Endpoint, InvalidEndpoint> {
        Endpoint::try_from(url.to_owned())
    }

    #[test]
    fn parses_host_and_port() {
        let endpoint = parse("https://192.168.1.10:8443").unwrap();
        assert_eq!(endpoint.address(), "192.168.1.10:8443");
        assert_eq!(
            endpoint.websocket_url(),
            "wss://192.168.1.10:8443/api/agent"
        );
        assert_eq!(
            parse("https://home.lan/").unwrap().address(),
            "home.lan:443"
        );
    }

    #[test]
    fn parses_ipv6_literals() {
        assert_eq!(parse("https://[fd00::1]:8443").unwrap().host(), "fd00::1");
        assert_eq!(
            parse("https://[fd00::1]").unwrap().address(),
            "[fd00::1]:443"
        );
    }

    #[test]
    fn rejects_other_urls() {
        for url in [
            "http://host:1",
            "host:1",
            "https://",
            "https://h/path",
            "https://h:port",
            "https://u@h",
        ] {
            assert!(parse(url).is_err(), "{url}");
        }
    }

    #[test]
    fn displays_as_a_url() {
        assert_eq!(parse("https://h:1").unwrap().to_string(), "https://h:1");
    }
}
