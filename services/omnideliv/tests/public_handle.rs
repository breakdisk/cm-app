//! Claiming a public storefront handle.
//!
//! Pure domain validation — no database, no broker — same as `venue_setup.rs`.
//!
//! These matter more than most validation because both values end up resolved
//! for anonymous callers: a slug from a URL a stranger typed, a domain from a
//! `Host` header. There is no tenant in scope at that moment, so the shape of
//! what gets stored is the only thing standing between a public lookup and an
//! ambiguous one.

use logisticos_omnideliv::domain::entities::{check_custom_domain, check_slug, HandleInvalid};

#[test]
fn a_slug_is_lowercased_and_trimmed() {
    assert_eq!(check_slug("  Kanto-Freestyle  ").unwrap(), "kanto-freestyle");
    assert_eq!(check_slug("KANTO123").unwrap(), "kanto123");
}

#[test]
fn a_slug_must_be_url_safe() {
    // Anything that would need escaping, split a path, or read as a different
    // URL is refused rather than mangled into something else.
    for bad in [
        "ab",                 // too short
        "-leading",
        "trailing-",
        "has space",
        "has/slash",
        "has_underscore",
        "café",
        "",
    ] {
        assert_eq!(
            check_slug(bad),
            Err(HandleInvalid::SlugShape),
            "{bad:?} should be refused",
        );
    }
    assert_eq!(check_slug(&"x".repeat(51)), Err(HandleInvalid::SlugShape));
}

#[test]
fn a_slug_is_never_silently_rewritten() {
    // The caller suggests and we verify — we do not slugify for them. A slug is
    // a permanent public URL, and quietly turning what someone typed into
    // something else is how a vendor prints a link they never chose.
    assert!(check_slug("Kanto Freestyle").is_err(), "spaces are refused, not replaced");
    assert!(check_slug("kanto_freestyle").is_err(), "underscores are refused, not replaced");
}

#[test]
fn a_custom_domain_is_normalised() {
    assert_eq!(check_custom_domain(" Menu.Kanto.PH ").unwrap(), "menu.kanto.ph");
    // A trailing dot is a legal FQDN but never what a Host header carries, so
    // storing it would mean the lookup never matches.
    assert_eq!(check_custom_domain("menu.kanto.ph.").unwrap(), "menu.kanto.ph");
}

#[test]
fn a_custom_domain_must_look_like_one() {
    for bad in ["nodot", "", "a.b", ".leading.com", "trailing.com-", "has space.com", "a..b.com"] {
        assert_eq!(
            check_custom_domain(bad),
            Err(HandleInvalid::DomainShape),
            "{bad:?} should be refused",
        );
    }
}

#[test]
fn a_vendor_cannot_claim_a_platform_domain() {
    // The one that matters. Custom domains are resolved from the `Host` header,
    // so a vendor holding `os.cargomarket.net` would have every request to the
    // main site — login included — rewritten to their storefront.
    for hijack in [
        "cargomarket.net",
        "os.cargomarket.net",
        "OS.CargoMarket.NET",
        "anything.logisticos.io",
        // A dotted localhost subdomain reaches the reserved check; bare
        // `localhost` is refused one step earlier for having no dot at all,
        // which is asserted separately below.
        "app.localhost",
    ] {
        match check_custom_domain(hijack) {
            Err(HandleInvalid::DomainReserved(_)) => {}
            other => panic!("{hijack:?} must be reserved, got {other:?}"),
        }
    }

    // Refused, but for shape rather than reservation: a bare hostname is not a
    // domain anyone can CNAME. Either way it cannot be claimed, which is the
    // property that matters.
    assert_eq!(check_custom_domain("localhost"), Err(HandleInvalid::DomainShape));

    // A domain that merely *contains* one of ours is not ours — the check is a
    // suffix match on a label boundary, not a substring. Getting this wrong in
    // the other direction would refuse legitimate domains for no reason.
    assert!(check_custom_domain("cargomarket.net.example.com").is_ok());
    assert!(check_custom_domain("notcargomarket.net").is_ok());
}
