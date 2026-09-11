//! Architecture handling, and in particular what `any` means in a binary
//! package.
//!
//! `any` is a *source* package wildcard. dpkg refuses to install a `.deb`
//! that declares it — "package architecture (any) does not match system" —
//! so it has to become concrete before the package is written.

use std::process::Command;
use std::str::FromStr;

use py2deb::architecture::Architecture;

/// What dpkg says this machine is, which is the answer `any` must resolve to.
fn dpkg_architecture() -> String {
    let out = Command::new("dpkg")
        .arg("--print-architecture")
        .output()
        .expect("dpkg is required for these tests");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn any_is_not_valid_for_a_binary_package() {
    assert!(!Architecture::Any.is_valid_for_binary());
}

#[test]
fn every_other_architecture_is_valid_for_a_binary_package() {
    for arch in Architecture::buildable() {
        assert!(
            arch.is_valid_for_binary(),
            "{arch} is listed as buildable but rejected for binaries"
        );
    }
}

#[test]
fn buildable_never_offers_the_wildcard() {
    assert!(
        !Architecture::buildable().contains(&Architecture::Any),
        "`any` must not be offered as something to build"
    );
}

#[test]
fn host_matches_what_dpkg_reports() {
    let host = Architecture::host().expect("host architecture should be readable");
    assert_eq!(host.as_str(), dpkg_architecture());
}

#[test]
fn any_resolves_to_this_machine() {
    // The whole point: the build just produced something for *this* machine.
    let resolved = Architecture::Any
        .resolve_for_binary()
        .expect("any should resolve");

    assert_eq!(resolved.as_str(), dpkg_architecture());
    assert!(resolved.is_valid_for_binary(), "resolving must yield a usable value");
}

#[test]
fn all_is_left_alone() {
    // `all` is already concrete and means something quite different from the
    // host: replacing it would wrongly tie a pure-Python package to one arch.
    let resolved = Architecture::All.resolve_for_binary().unwrap();
    assert_eq!(resolved, Architecture::All);
}

#[test]
fn a_concrete_architecture_passes_through_untouched() {
    // Cross-building for arm64 on amd64 must keep saying arm64.
    let resolved = Architecture::Arm64.resolve_for_binary().unwrap();
    assert_eq!(resolved, Architecture::Arm64);
}

#[test]
fn uname_spellings_are_normalised_to_dpkg_names() {
    // These are what people type out of habit; dpkg spells them differently.
    for (input, expected) in [
        ("x86_64", Architecture::Amd64),
        ("aarch64", Architecture::Arm64),
        ("ppc64le", Architecture::Ppc64el),
        ("i686", Architecture::I386),
        ("loongarch64", Architecture::Loong64),
    ] {
        assert_eq!(
            Architecture::from_str(input).unwrap(),
            expected,
            "`{input}` should normalise to {expected}"
        );
    }
}

#[test]
fn parsing_is_case_and_space_insensitive() {
    assert_eq!(Architecture::from_str("  AMD64 ").unwrap(), Architecture::Amd64);
}

#[test]
fn an_unknown_architecture_is_refused() {
    let err = Architecture::from_str("pdp11").unwrap_err();
    assert!(err.to_string().contains("unknown architecture"), "{err}");
}
