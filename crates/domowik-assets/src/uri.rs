//! Hint URI parsing and normalisation — ASSET-STRATEGY §10.8.
//!
//! Deliberately hand-written and deliberately small. §10.8 does not ask "is
//! this a URL a browser would accept"; it asks a short list of closed
//! questions — is the scheme allowlisted, is there userinfo, is the host an IP
//! literal — and answers them by REJECTING anything it cannot categorise. A
//! general URL parser answers a different question, permissively, and the
//! permissive answers are exactly the dangerous ones here.
//!
//! Nothing in this module dials, resolves, or opens a socket.

use std::fmt;

/// Why a hint URI was refused. Every variant is a parse rejection with an
/// actionable message (§10.5: "invalid hint entries ⇒ parse rejection with
/// actionable error").
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UriError {
    #[error("{0:?} is not ASCII; a hint URI must already be in ASCII form (IDNA A-labels, §10.8)")]
    NotAscii(String),
    #[error("{0:?} has no scheme; a hint URI must be absolute (§10.5)")]
    NoScheme(String),
    #[error("scheme {0:?} is not a valid URI scheme")]
    BadScheme(String),
    #[error("{0:?} has no `//` authority; a hint URI must name a host")]
    NoAuthority(String),
    #[error("{0:?} carries a userinfo component; §10.8 prohibits credential-bearing URIs")]
    HasUserinfo(String),
    #[error("{0:?} has an empty host")]
    EmptyHost(String),
    #[error("host {0:?} is an IP literal; §10.8 rejects IP-literal hint hosts outright in v0.1")]
    IpLiteralHost(String),
    #[error("host {0:?} is not a well-formed DNS name")]
    BadHost(String),
    #[error("port {0:?} is not a decimal port number")]
    BadPort(String),
}

/// A hint URI that passed every §10.8 structural check, in normalised form.
///
/// Holding this type is NOT permission to fetch. Consent (§10.8
/// `RetrievalPolicy.consent`, default `false`) is a separate gate, and this
/// crate performs no network I/O at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintUri {
    scheme: String,
    host: String,
    port: Option<u16>,
    rest: String,
}

impl HintUri {
    /// The scheme, NFKC-folded to lowercase ASCII (§10.8).
    ///
    /// NFKC over an ASCII string is the identity, and a non-ASCII scheme is
    /// rejected before this point, so lowercasing IS the normalisation here —
    /// stated rather than left as a silent equivalence.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// The host, lowercased with any trailing dot stripped (§10.8).
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }
}

impl fmt::Display for HintUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme, self.host)?;
        if let Some(p) = self.port {
            write!(f, ":{p}")?;
        }
        write!(f, "{}", self.rest)
    }
}

/// Parses and normalises a hint URI per §10.8, or says why it cannot.
pub fn parse_hint_uri(raw: &str) -> Result<HintUri, UriError> {
    if !raw.is_ascii() {
        return Err(UriError::NotAscii(raw.to_owned()));
    }
    let (scheme_raw, after) = raw
        .split_once(':')
        .ok_or_else(|| UriError::NoScheme(raw.to_owned()))?;
    if scheme_raw.is_empty()
        || !scheme_raw.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme_raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return Err(UriError::BadScheme(scheme_raw.to_owned()));
    }
    let after = after
        .strip_prefix("//")
        .ok_or_else(|| UriError::NoAuthority(raw.to_owned()))?;

    // The authority ends at the first '/', '?' or '#'. Splitting on '/' alone
    // would let `https://host?x=/y` hide a query inside the host.
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let (authority, rest) = after.split_at(end);

    if authority.contains('@') {
        return Err(UriError::HasUserinfo(raw.to_owned()));
    }

    let (host_raw, port) = split_port(authority)?;
    let host = normalise_host(host_raw)?;

    Ok(HintUri {
        scheme: scheme_raw.to_ascii_lowercase(),
        host,
        port,
        rest: rest.to_owned(),
    })
}

/// Splits `host[:port]`, rejecting a bracketed IPv6 literal on the way.
fn split_port(authority: &str) -> Result<(&str, Option<u16>), UriError> {
    if authority.starts_with('[') {
        // `[::1]` and friends. §10.8 rejects IP literals, and the bracket form
        // exists only to carry one, so it never needs splitting.
        return Err(UriError::IpLiteralHost(authority.to_owned()));
    }
    match authority.rsplit_once(':') {
        Some((host, port_raw)) => {
            let port = port_raw
                .parse::<u16>()
                .map_err(|_| UriError::BadPort(port_raw.to_owned()))?;
            Ok((host, Some(port)))
        }
        None => Ok((authority, None)),
    }
}

/// Lowercases, strips one trailing dot, and refuses anything that is not a
/// DNS name (§10.8: "hint hosts must be DNS names").
fn normalise_host(host: &str) -> Result<String, UriError> {
    if host.is_empty() {
        return Err(UriError::EmptyHost(host.to_owned()));
    }
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if host.is_empty() {
        return Err(UriError::EmptyHost(host));
    }
    if is_ip_literal(&host) {
        return Err(UriError::IpLiteralHost(host));
    }
    // Labels: alphanumeric plus internal hyphens, 1..=63 bytes each.
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(UriError::BadHost(host.clone()));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(UriError::BadHost(host.clone()));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(UriError::BadHost(host.clone()));
        }
    }
    if host.len() > 253 {
        return Err(UriError::BadHost(host));
    }
    Ok(host)
}

/// Whether `host` parses as an IPv4 or IPv6 address.
///
/// Uses the standard library's parsers rather than a pattern, because the
/// shapes that matter are the unusual ones — `0x7f.1`, `2130706433`,
/// `::ffff:169.254.169.254` — and a regex-shaped guess would pass exactly
/// those to a dialler that then resolves them to an address §10.8 forbids.
fn is_ip_literal(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
        // A bare decimal like `2130706433` is 127.0.0.1 to most resolvers and
        // to `inet_aton`, but `IpAddr` rejects it. It is not a DNS name either
        // — a top-level label cannot be all-digits — so it is refused here.
        || host
            .rsplit('.')
            .next()
            .is_some_and(|last| !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()))
}
