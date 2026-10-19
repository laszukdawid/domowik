//! Retrieval policy — ASSET-STRATEGY §10.8.
//!
//! **This module does not fetch anything.** There is no HTTP client here, no
//! resolver, no socket. What it provides is the policy object and the pure
//! predicates a fetcher would have to satisfy before dialling, so that the
//! security-critical decisions — is this scheme allowlisted, is this address
//! private, is this destination user-trusted — are testable on their own
//! rather than buried inside a network path nobody can exercise.
//!
//! Consent defaults to `false` (§10.8's first row). With consent off, §10.6's
//! third lookup step does not happen at all and unresolved references are
//! reported through the BEHAVIOR-SPEC §9.6 offline summary.

use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};

use crate::uri::HintUri;

/// A destination the user has explicitly trusted. §10.8: "never
/// document-supplied, never hint-inferred."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Destination {
    /// A fully-qualified domain name.
    Host(String),
    /// A network in CIDR form.
    Cidr(IpNetwork),
}

/// An IP network, parsed strictly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpNetwork {
    base: IpAddr,
    prefix: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("{0:?} is not `address/prefix`")]
    NotCidr(String),
    #[error("{0:?} does not contain an IP address")]
    BadAddress(String),
    #[error("prefix /{prefix} is out of range for {family}")]
    BadPrefix {
        prefix: String,
        family: &'static str,
    },
    #[error(
        "{0:?} has host bits set; §10.8 requires a network address and rejects rather than masks"
    )]
    HostBitsSet(String),
    #[error("an empty scheme is not a valid allowlist entry (§10.8)")]
    EmptyScheme,
}

impl IpNetwork {
    /// Parses `address/prefix`, refusing a network address with host bits set.
    ///
    /// §10.8 says reject, don't mask, and the reason is that `10.1.2.3/8` is
    /// almost always someone meaning one host and accidentally trusting
    /// sixteen million. Masking silently grants the wider trust.
    pub fn parse(raw: &str) -> Result<Self, PolicyError> {
        let (addr_raw, prefix_raw) = raw
            .split_once('/')
            .ok_or_else(|| PolicyError::NotCidr(raw.to_owned()))?;
        let base: IpAddr = addr_raw
            .parse()
            .map_err(|_| PolicyError::BadAddress(raw.to_owned()))?;
        let prefix: u8 = prefix_raw.parse().map_err(|_| PolicyError::BadPrefix {
            prefix: prefix_raw.to_owned(),
            family: if base.is_ipv4() { "IPv4" } else { "IPv6" },
        })?;
        let width = if base.is_ipv4() { 32 } else { 128 };
        if prefix > width {
            return Err(PolicyError::BadPrefix {
                prefix: prefix_raw.to_owned(),
                family: if base.is_ipv4() { "IPv4" } else { "IPv6" },
            });
        }
        let network = Self { base, prefix };
        if !network.is_network_address() {
            return Err(PolicyError::HostBitsSet(raw.to_owned()));
        }
        Ok(network)
    }

    fn bits(addr: &IpAddr) -> u128 {
        match addr {
            IpAddr::V4(v4) => u32::from_be_bytes(v4.octets()) as u128,
            IpAddr::V6(v6) => u128::from_be_bytes(v6.octets()),
        }
    }

    fn mask(&self) -> u128 {
        let width = if self.base.is_ipv4() { 32 } else { 128 };
        if self.prefix == 0 {
            0
        } else {
            u128::MAX << (width - self.prefix as u32)
        }
    }

    fn is_network_address(&self) -> bool {
        let bits = Self::bits(&self.base);
        bits & self.mask() == bits
    }

    /// Whether `addr` falls inside this network. Families never mix: an IPv4
    /// address is not inside an IPv6 network even when the IPv6 network is a
    /// mapped range, because §10.8 rejects IPv4-mapped IPv6 outright.
    pub fn contains(&self, addr: &IpAddr) -> bool {
        if self.base.is_ipv4() != addr.is_ipv4() {
            return false;
        }
        Self::bits(addr) & self.mask() == Self::bits(&self.base) & self.mask()
    }
}

impl Serialize for IpNetwork {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{}/{}", self.base, self.prefix))
    }
}

impl<'de> Deserialize<'de> for IpNetwork {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// §10.8's configuration schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalPolicy {
    /// §10.8: retrieval is disabled by default, and fetching without consent
    /// never occurs. Consent covers revealing the client's IP and the time of
    /// access to the hint host.
    pub consent: bool,
    pub schemes: Vec<String>,
    pub trusted_destinations: Vec<Destination>,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            consent: false,
            schemes: vec!["https".to_owned()],
            trusted_destinations: Vec::new(),
        }
    }
}

impl RetrievalPolicy {
    /// Whether `scheme` passes the allowlist after §10.8's normalisation.
    ///
    /// NFKC → lowercase → ASCII, then EXACT match. Non-ASCII cannot survive
    /// that pipeline as anything an allowlist entry would match, so it is
    /// refused rather than transliterated.
    pub fn allows_scheme(&self, scheme: &str) -> bool {
        if !scheme.is_ascii() || scheme.is_empty() {
            return false;
        }
        let scheme = scheme.to_ascii_lowercase();
        self.schemes
            .iter()
            .any(|s| !s.is_empty() && s.is_ascii() && s.to_ascii_lowercase() == scheme)
    }

    /// Whether a hint's scheme passes. Holding a [`HintUri`] already proves the
    /// structural checks; this is the allowlist half.
    pub fn allows(&self, uri: &HintUri) -> bool {
        self.allows_scheme(uri.scheme())
    }

    /// Whether a resolved address may be dialled.
    ///
    /// Private and link-local destinations are refused unless a
    /// user-configured `trustedDestinations` entry covers them. This is the
    /// SSRF gate, and it is checked against the resolved ADDRESS rather than
    /// the hostname, because a hostname that resolves to 169.254.169.254 is
    /// the attack.
    pub fn permits_address(&self, addr: &IpAddr) -> bool {
        if !is_private_or_link_local(addr) {
            return true;
        }
        self.trusted_destinations.iter().any(|d| match d {
            Destination::Cidr(net) => net.contains(addr),
            Destination::Host(_) => false,
        })
    }
}

/// §10.8's rejected destination classes: loopback, RFC 1918, link-local
/// (including 169.254.169.254), ULA, link-local multicast, IPv4-mapped IPv6.
///
/// Unspecified addresses (`0.0.0.0`, `::`) are included: they are not in the
/// specification's list because they are not a destination anyone names, and
/// on most stacks they dial loopback.
pub fn is_private_or_link_local(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => {
            // §10.8 rejects IPv4-mapped IPv6 as a CLASS, not only when the
            // embedded address is itself private: the mapped form exists here
            // only to present one address to a policy check and another to a
            // dialler, so the whole class goes.
            if v6.to_ipv4_mapped().is_some() {
                return true;
            }
            v6.is_loopback()
                || v6.is_unspecified()
                // ULA fc00::/7.
                || (v6.octets()[0] & 0xfe) == 0xfc
                // Link-local unicast fe80::/10.
                || (v6.octets()[0] == 0xfe && (v6.octets()[1] & 0xc0) == 0x80)
                // Link-local multicast ff02::/16 and interface-local ff01::/16.
                || (v6.octets()[0] == 0xff && matches!(v6.octets()[1], 0x01 | 0x02))
        }
    }
}

fn is_private_v4(v4: &Ipv4Addr) -> bool {
    v4.is_private()
        || v4.is_loopback()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_unspecified()
        || v4.is_documentation()
        // 100.64.0.0/10, carrier-grade NAT: not "private" to the standard
        // library, and not a destination a floor-plan editor should dial.
        || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
        // 0.0.0.0/8 and the 240.0.0.0/4 reserved space.
        || v4.octets()[0] == 0
        || v4.octets()[0] >= 240
}
