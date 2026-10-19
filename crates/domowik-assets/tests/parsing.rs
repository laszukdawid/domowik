//! Hint URIs (§10.8), UTC timestamps (§10.2/§10.7), destination policy
//! (§10.8), and descriptor round-tripping (§10.2).

use std::net::IpAddr;

use domowik_assets::descriptor::{AssetDescriptor, Provenance, Requirement};
use domowik_assets::policy::{is_private_or_link_local, Destination, IpNetwork, PolicyError};
use domowik_assets::time::{is_rfc3339_utc, parse_rfc3339_utc};
use domowik_assets::uri::{parse_hint_uri, UriError};
use domowik_assets::RetrievalPolicy;

#[test]
fn a_hint_uri_is_normalised_not_merely_accepted() {
    // §10.8: lowercase scheme, lowercase host, trailing dot stripped.
    let uri = parse_hint_uri("HTTPS://Example.COM./assets/scan.png").expect("valid");
    assert_eq!(uri.scheme(), "https");
    assert_eq!(uri.host(), "example.com");
    assert_eq!(uri.port(), None);
    assert_eq!(uri.to_string(), "https://example.com/assets/scan.png");

    let ported = parse_hint_uri("https://example.com:8443/x").expect("valid");
    assert_eq!(ported.port(), Some(8443));
}

#[test]
fn credential_bearing_and_ip_literal_hints_are_parse_rejections() {
    // §10.8: "userinfo URLs invalid"; "IP-literal hint hosts are rejected
    // outright in v0.1". These are the SSRF-shaped inputs, so each is asserted
    // by its own error rather than by "some error happened".
    assert!(matches!(
        parse_hint_uri("https://user:pass@example.com/x"),
        Err(UriError::HasUserinfo(_))
    ));
    assert!(matches!(
        parse_hint_uri("https://evil.test@example.com/x"),
        Err(UriError::HasUserinfo(_))
    ));
    for literal in [
        "https://127.0.0.1/x",
        "https://169.254.169.254/latest/meta-data",
        "https://[::1]/x",
        "https://[fd00::1]:443/x",
        // Decimal and dotted-decimal forms that resolvers accept as addresses
        // but that are not DNS names.
        "https://2130706433/x",
        "https://10.0.0.1/x",
    ] {
        assert!(
            matches!(parse_hint_uri(literal), Err(UriError::IpLiteralHost(_))),
            "{literal} was not rejected as an IP literal: {:?}",
            parse_hint_uri(literal)
        );
    }
}

#[test]
fn structurally_broken_hints_are_refused_with_their_own_reason() {
    assert!(matches!(
        parse_hint_uri("example.com/x"),
        Err(UriError::NoScheme(_))
    ));
    assert!(matches!(
        parse_hint_uri("https:example.com/x"),
        Err(UriError::NoAuthority(_))
    ));
    assert!(matches!(
        parse_hint_uri("https:///x"),
        Err(UriError::EmptyHost(_))
    ));
    assert!(matches!(
        parse_hint_uri("https://exa\u{0301}mple.com/x"),
        Err(UriError::NotAscii(_))
    ));
    assert!(matches!(
        parse_hint_uri("https://-bad-.example.com/x"),
        Err(UriError::BadHost(_))
    ));
    assert!(matches!(
        parse_hint_uri("https://example.com:notaport/x"),
        Err(UriError::BadPort(_))
    ));
}

#[test]
fn a_query_string_cannot_smuggle_a_host_past_the_authority_split() {
    // Splitting the authority on '/' alone would read the host of
    // `https://good.example?x=/@evil.example/` as everything up to the slash,
    // which includes an '@'. The authority must end at the first of / ? #.
    let uri = parse_hint_uri("https://good.example?x=/@evil.example/").expect("valid");
    assert_eq!(uri.host(), "good.example");
}

#[test]
fn the_scheme_allowlist_matches_exactly_and_defaults_to_https_only() {
    let policy = RetrievalPolicy::default();
    assert!(!policy.consent, "§10.8: retrieval is disabled by default");
    assert!(policy.allows_scheme("https"));
    assert!(policy.allows_scheme("HTTPS"), "normalisation is lowercase");
    assert!(!policy.allows_scheme("http"));
    assert!(!policy.allows_scheme("file"));
    assert!(!policy.allows_scheme(""), "§10.8: empty entries invalid");
    assert!(!policy.allows_scheme("https\u{0131}"));
}

#[test]
fn private_and_link_local_addresses_are_refused_unless_user_trusted() {
    let policy = RetrievalPolicy::default();
    for blocked in [
        "127.0.0.1",
        "10.1.2.3",
        "192.168.0.1",
        "172.16.0.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "::1",
        "fd00::1",
        "fe80::1",
        "ff02::1",
        // §10.8 names IPv4-mapped IPv6 as its own rejected class.
        "::ffff:93.184.216.34",
    ] {
        let addr: IpAddr = blocked.parse().expect("address");
        assert!(
            is_private_or_link_local(&addr),
            "{blocked} passed the class test"
        );
        assert!(!policy.permits_address(&addr), "{blocked} was dialable");
    }

    let public: IpAddr = "93.184.216.34".parse().expect("address");
    assert!(!is_private_or_link_local(&public));
    assert!(policy.permits_address(&public));
}

#[test]
fn a_user_configured_cidr_can_re_admit_a_private_range_and_nothing_else_can() {
    // §10.8: rejected "unless covered by a user-configured
    // trustedDestinations entry — never document-supplied, never inferred
    // from hints."
    let policy = RetrievalPolicy {
        trusted_destinations: vec![Destination::Cidr(
            IpNetwork::parse("10.0.0.0/8").expect("cidr"),
        )],
        ..RetrievalPolicy::default()
    };
    assert!(policy.permits_address(&"10.1.2.3".parse().unwrap()));
    // A different private range is still refused.
    assert!(!policy.permits_address(&"192.168.0.1".parse().unwrap()));
    // A trusted HOST entry never authorises an address: a hostname match
    // cannot vouch for where it resolved to.
    let host_only = RetrievalPolicy {
        trusted_destinations: vec![Destination::Host("internal.example".to_owned())],
        ..RetrievalPolicy::default()
    };
    assert!(!host_only.permits_address(&"10.1.2.3".parse().unwrap()));
}

#[test]
fn a_cidr_with_host_bits_set_is_rejected_rather_than_masked() {
    // §10.8: "network addresses with host bits set invalid (reject, don't
    // mask)". Masking 10.1.2.3/8 silently widens the trust to 16 million
    // hosts.
    assert!(matches!(
        IpNetwork::parse("10.1.2.3/8"),
        Err(PolicyError::HostBitsSet(_))
    ));
    assert!(matches!(
        IpNetwork::parse("fd00::1/8"),
        Err(PolicyError::HostBitsSet(_))
    ));
    assert!(IpNetwork::parse("10.0.0.0/8").is_ok());
    assert!(IpNetwork::parse("10.1.2.3/32").is_ok());
    assert!(matches!(
        IpNetwork::parse("10.0.0.0/33"),
        Err(PolicyError::BadPrefix { .. })
    ));
    assert!(matches!(
        IpNetwork::parse("10.0.0.0"),
        Err(PolicyError::NotCidr(_))
    ));

    // Families never mix.
    let v4 = IpNetwork::parse("0.0.0.0/0").expect("cidr");
    assert!(!v4.contains(&"::1".parse().unwrap()));
    assert!(v4.contains(&"8.8.8.8".parse().unwrap()));
}

#[test]
fn utc_timestamps_parse_and_non_utc_ones_do_not() {
    assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z").expect("epoch"), 0);
    assert_eq!(
        parse_rfc3339_utc("1970-01-02T00:00:00Z").expect("day one"),
        86_400
    );
    // A leap day the calendar actually has, and one it does not.
    assert!(is_rfc3339_utc("2024-02-29T12:00:00Z"));
    assert!(!is_rfc3339_utc("2023-02-29T12:00:00Z"));
    // Fractional seconds are accepted and do not change the second.
    assert_eq!(
        parse_rfc3339_utc("2026-09-12T00:00:00.250Z").expect("fraction"),
        parse_rfc3339_utc("2026-09-12T00:00:00Z").expect("plain")
    );
    assert!(is_rfc3339_utc("2026-09-12T00:00:00+00:00"));
    // §10.2 and §10.7 say UTC. An offset is a rejection, not a conversion.
    for bad in [
        "2026-09-12T00:00:00+02:00",
        "2026-09-12",
        "2026-13-01T00:00:00Z",
        "2026-09-12T24:00:00Z",
        "2026-09-12T00:00:00.Z",
        "not a timestamp",
        "",
    ] {
        assert!(!is_rfc3339_utc(bad), "{bad:?} was accepted");
    }
}

#[test]
fn a_descriptor_round_trips_and_keeps_fields_this_version_does_not_know() {
    // FORMAT-SPEC §8 U2 forbids silent stripping everywhere, not only on
    // entities. A descriptor written by a later version must survive a
    // read/write cycle here unchanged.
    let json = serde_json::json!({
        "digest": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "byteLength": 0,
        "mediaType": "image/png",
        "purpose": "background-image",
        "encoding": "identity",
        "requirement": "required",
        "provenance": { "displayName": "scan.png" },
        "placeholderDimensions": { "widthMm": 600.0, "heightMm": 750.0, "depthMm": 400.0 },
        "hints": [{ "url": "https://example.com/scan.png", "addedAt": "2026-09-12T00:00:00Z" }],
        "futureField": { "nested": [1, 2, 3] }
    });
    let descriptor: AssetDescriptor = serde_json::from_value(json.clone()).expect("parses");
    assert_eq!(descriptor.requirement, Requirement::Required);
    assert_eq!(descriptor.purpose, "background-image");
    assert_eq!(
        descriptor
            .placeholder_dimensions
            .expect("declared")
            .width_mm,
        600.0
    );
    assert!(descriptor.unknown.contains_key("futureField"));

    let written = serde_json::to_value(&descriptor).expect("serialises");
    assert_eq!(written, json, "a round trip changed the descriptor");
}

#[test]
fn an_absent_optional_block_stays_absent_on_write() {
    // A descriptor with no licence must not acquire a `"license": null`: §10.2
    // makes absence meaningful ("absence permitted for user-imported content
    // and surfaced informationally by `validate`"), and null is not absence.
    let descriptor = AssetDescriptor {
        digest: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            .to_owned(),
        byte_length: 3,
        media_type: "image/png".to_owned(),
        purpose: "background-image".to_owned(),
        encoding: "identity".to_owned(),
        requirement: Requirement::Optional,
        provenance: Provenance::default(),
        license: None,
        placeholder_dimensions: None,
        hints: Vec::new(),
        unknown: serde_json::Map::new(),
    };
    let written = serde_json::to_value(&descriptor).expect("serialises");
    let object = written.as_object().expect("object");
    for absent in ["license", "placeholderDimensions", "hints"] {
        assert!(!object.contains_key(absent), "{absent} was written anyway");
    }
    assert_eq!(object["provenance"], serde_json::json!({}));
    assert_eq!(object["requirement"], serde_json::json!("optional"));
}
