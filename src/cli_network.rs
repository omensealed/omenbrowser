//! Typed command-local network overrides for the browser CLI.

use anyhow::Context;

#[derive(Clone, PartialEq, Eq)]
pub struct TcpClientOverride {
    host: String,
    port: u16,
    network_name: Option<String>,
    passphrase: Option<String>,
}

impl TcpClientOverride {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        network_name: Option<String>,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            network_name,
            passphrase,
        }
    }

    pub fn empty() -> Self {
        Self::new(String::new(), 0, None, None)
    }

    pub fn parse_endpoint(value: &str) -> anyhow::Result<Self> {
        let (host, port) = value
            .rsplit_once(':')
            .ok_or_else(|| anyhow::anyhow!("TCP client endpoint must be host:port"))?;
        if host.trim().is_empty() {
            return Err(anyhow::anyhow!("TCP client host must not be empty"));
        }
        let port = port
            .parse::<u16>()
            .with_context(|| format!("invalid TCP client port in {value}"))?;
        Ok(Self::new(host, port, None, None))
    }

    pub fn inherit_credentials(&mut self, existing: Self) {
        self.network_name = existing.network_name;
        self.passphrase = existing.passphrase;
    }

    pub fn set_network_name(&mut self, network_name: String) {
        self.network_name = Some(network_name);
    }

    pub fn set_passphrase(&mut self, passphrase: String) {
        self.passphrase = Some(passphrase);
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn network_name(&self) -> Option<&str> {
        self.network_name.as_deref()
    }

    pub fn passphrase(&self) -> Option<&str> {
        self.passphrase.as_deref()
    }

    pub fn into_parts(self) -> (String, u16, Option<String>, Option<String>) {
        (self.host, self.port, self.network_name, self.passphrase)
    }
}

impl std::fmt::Debug for TcpClientOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpClientOverride")
            .field("host", &self.host)
            .field("port", &self.port)
            .field(
                "network_name",
                &self.network_name.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "passphrase",
                &self.passphrase.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parser_preserves_compatibility_and_error_contracts() {
        assert_eq!(
            TcpClientOverride::parse_endpoint("127.0.0.1:4242")
                .expect("IPv4 endpoint")
                .into_parts(),
            ("127.0.0.1".into(), 4242, None, None)
        );
        assert_eq!(
            TcpClientOverride::parse_endpoint("::1:4242")
                .expect("unbracketed IPv6 endpoint")
                .host(),
            "::1"
        );
        assert_eq!(
            TcpClientOverride::parse_endpoint("127.0.0.1")
                .expect_err("missing port")
                .to_string(),
            "TCP client endpoint must be host:port"
        );
        assert_eq!(
            TcpClientOverride::parse_endpoint(":4242")
                .expect_err("missing host")
                .to_string(),
            "TCP client host must not be empty"
        );
        assert!(TcpClientOverride::parse_endpoint("127.0.0.1:notaport")
            .expect_err("invalid port")
            .to_string()
            .contains("invalid TCP client port in 127.0.0.1:notaport"));
    }

    #[test]
    fn endpoint_replacement_preserves_explicit_credentials() {
        let existing = TcpClientOverride::new(
            "",
            0,
            Some("private-network".into()),
            Some("secret-value".into()),
        );
        let mut replacement =
            TcpClientOverride::parse_endpoint("gateway.example:4242").expect("endpoint");
        replacement.inherit_credentials(existing);
        assert_eq!(replacement.network_name(), Some("private-network"));
        assert_eq!(replacement.passphrase(), Some("secret-value"));
    }

    #[test]
    fn debug_output_redacts_the_passphrase() {
        let value = TcpClientOverride::new(
            "gateway.example",
            4242,
            Some("private".into()),
            Some("debug-secret-value".into()),
        );
        let debug = format!("{value:?}");
        assert!(!debug.contains("private"));
        assert!(!debug.contains("debug-secret-value"));
        assert!(debug.contains("<redacted>"));
    }
}
