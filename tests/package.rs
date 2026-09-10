//! `$name` expansion in configuration values.

use py2deb::package::Package;

fn pkg() -> Package {
    Package::new("python3-libgkeyboard", "1.1.8", "GKeyboard library")
}

#[test]
fn substitutes_known_fields() {
    let p = pkg();
    assert_eq!(p.expand_dollar_properties("$package v$version"), "python3-libgkeyboard v1.1.8");
}

#[test]
fn leaves_unknown_names_alone() {
    let p = pkg();
    assert_eq!(p.expand_dollar_properties("cost is $nonexistent"), "cost is $nonexistent");
}

#[test]
fn double_dollar_is_a_literal() {
    let p = pkg();
    assert_eq!(p.expand_dollar_properties("$$5 for $package"), "$5 for python3-libgkeyboard");
}

#[test]
fn name_stops_at_punctuation() {
    let p = pkg();
    assert_eq!(p.expand_dollar_properties("$package-doc"), "python3-libgkeyboard-doc");
    assert_eq!(p.expand_dollar_properties("v$version, released"), "v1.1.8, released");
}
