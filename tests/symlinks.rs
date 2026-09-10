//! Symbolic links declared in `symlinks = [[target, link], ...]`.
//!
//! Policy 10.5 splits these two ways: a link within one top-level directory is
//! relative, a link crossing between them is absolute. lintian enforces both
//! halves, so most of what is tested here is which side of that line a given
//! pair falls on.

use std::path::{Path, PathBuf};
use py2deb::package::Package;
use py2deb::symlink_entry::SymlinkEntry;

fn pkg() -> Package {
    Package::new("gvcp", "1.0.0", "GVCP launcher")
}

fn entry(target: &str, link: &str) -> SymlinkEntry {
    SymlinkEntry { target: target.into(), link: link.into() }
}

fn resolve(target: &str, link: &str) -> (PathBuf, PathBuf) {
    let package = pkg();
    let e = entry(target, link);
    let link_path = e.resolve_link(&package);
    let target_path = e.resolve_target(&package, &link_path);
    (link_path, target_path)
}

#[test]
fn a_link_across_top_level_directories_is_absolute() {
    // lintian: relative-symlink is an error for these.
    let (link, target) = resolve("opt/gvcp.AppImage", "usr/bin/gvcp");
    assert_eq!(link, Path::new("usr/bin/gvcp"));
    assert_eq!(target, Path::new("/opt/gvcp.AppImage"));
}

#[test]
fn a_link_inside_one_top_level_directory_is_relative() {
    // lintian: absolute-symlink-in-top-level-folder is an error for these.
    let (_, target) = resolve("usr/lib/gvcp/run", "usr/bin/gvcp-run");
    assert_eq!(target, Path::new("../lib/gvcp/run"));
}

#[test]
fn a_link_beside_its_target_needs_no_hops() {
    let (_, target) = resolve("usr/bin/gvcp-real", "usr/bin/gvcp");
    assert_eq!(target, Path::new("gvcp-real"));
}

#[test]
fn a_deeper_link_climbs_out_far_enough() {
    let (_, target) = resolve("usr/share/gvcp/data.bin", "usr/lib/gvcp/nested/data");
    assert_eq!(target, Path::new("../../../share/gvcp/data.bin"));
}

#[test]
fn an_absolute_target_is_kept_as_written() {
    let (_, target) = resolve("/opt/vendor/thing", "usr/bin/thing");
    assert_eq!(target, Path::new("/opt/vendor/thing"));
}

#[test]
fn placeholders_are_expanded_on_both_sides() {
    let (link, target) = resolve("opt/$package/run", "usr/bin/$package");
    assert_eq!(link, Path::new("usr/bin/gvcp"));
    assert_eq!(target, Path::new("/opt/gvcp/run"));
}

#[test]
fn a_leading_slash_on_the_link_is_dropped() {
    // Archive paths are package-root-relative; a leading slash would escape.
    let (link, _) = resolve("opt/x", "/usr/bin/x");
    assert_eq!(link, Path::new("usr/bin/x"));
}

#[derive(Debug, serde::Deserialize)]
struct Wrapper {
    s: Vec<SymlinkEntry>,
}

fn parse(toml: &str) -> Result<Vec<SymlinkEntry>, toml::de::Error> {
    toml::from_str::<Wrapper>(toml).map(|w| w.s)
}

#[test]
fn a_well_formed_entry_parses() {
    let entries = parse(r#"s = [["opt/x", "usr/bin/x"]]"#).expect("should parse");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].target, "opt/x");
    assert_eq!(entries[0].link, "usr/bin/x");
}

#[test]
fn a_config_entry_needs_exactly_two_elements() {
    let err = parse(r#"s = [["opt/x"]]"#).unwrap_err();
    assert!(err.to_string().contains("symlink entry needs"), "{err}");

    let err = parse(r#"s = [["a", "b", "c"]]"#).unwrap_err();
    assert!(err.to_string().contains("symlink entry needs"), "{err}");
}

#[test]
fn a_link_may_not_name_a_directory() {
    // A trailing slash has no meaning for a link and is almost always a typo.
    let err = parse(r#"s = [["opt/x", "usr/bin/"]]"#).unwrap_err();
    assert!(err.to_string().contains("must name the link"), "{err}");
}
