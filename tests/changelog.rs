//! The changelog rendered from git history.
//!
//! These run against the library as an outside caller would, which is the
//! same surface `main` uses — a test that passes here cannot be relying on
//! anything the binary keeps to itself.

use py2deb::git::*;

fn commit(title: &str, body: &str) -> Commit {
    Commit {
        hash: "0123456789abcdef".into(),
        title: title.into(),
        body: body.into(),
        author: "Vlad Kartsaev".into(),
        email: "vkarcaev@gmail.com".into(),
        date: "Wed, 10 Sep 2026 12:00:00 +0300".into(),
    }
}

fn release(version: &str, commits: Vec<Commit>) -> Release {
    Release {
        version: version.into(),
        maintainer: "Vlad Kartsaev <vkarcaev@gmail.com>".into(),
        date: "Wed, 10 Sep 2026 12:00:00 +0300".into(),
        commits,
    }
}

#[test]
fn entry_follows_policy_layout() {
    let out = changelog("py2deb", "stable", "low", &[release("0.5.3", vec![commit("Add changelog", "")])]);
    let lines: Vec<&str> = out.lines().collect();

    assert_eq!(lines[0], "py2deb (0.5.3) stable; urgency=low");
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "  * Add changelog");
    assert_eq!(lines[3], "");
    assert_eq!(lines[4], " -- Vlad Kartsaev <vkarcaev@gmail.com>  Wed, 10 Sep 2026 12:00:00 +0300");
}

#[test]
fn body_becomes_continuation_lines() {
    let out = changelog("py2deb", "stable", "low", &[release("1.0", vec![commit("Title", "detail one\ndetail two")])]);
    assert!(out.contains("  * Title\n    detail one\n    detail two\n"), "{out}");
}

#[test]
fn trailers_are_dropped() {
    let out = changelog("py2deb", "stable", "low",
        &[release("1.0", vec![commit("Title", "why it changed\nSigned-off-by: Someone <a@b.c>")])]);

    assert!(out.contains("why it changed"));
    assert!(!out.contains("Signed-off-by"), "{out}");
}

#[test]
fn versions_are_separated_by_a_blank_line() {
    let out = changelog("py2deb", "stable", "low", &[
        release("1.1", vec![commit("New", "")]),
        release("1.0", vec![commit("Old", "")]),
    ]);
    assert!(out.contains("+0300\n\npy2deb (1.0)"), "{out}");
}

#[test]
fn long_titles_wrap_under_the_bullet() {
    let long = "Add a very long commit subject that will certainly exceed the seventy nine column limit used here";
    let out = changelog("py2deb", "stable", "low", &[release("1.0", vec![commit(long, "")])]);

    for line in out.lines() {
        assert!(line.chars().count() <= 79, "line too long: {line}");
    }
    // Wrapped text lines up under the bullet's text, not the asterisk.
    assert!(out.contains("\n    "), "{out}");
}

#[test]
fn empty_title_falls_back_to_the_hash() {
    let out = changelog("py2deb", "stable", "low", &[release("1.0", vec![commit("", "")])]);
    assert!(out.contains("  * commit 01234567"), "{out}");
}

#[test]
fn civil_dates_match_known_days() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(59), (1970, 3, 1));
    // 2000 was a leap year; 1900 was not, which is where naive maths break.
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    assert_eq!(civil_from_days(20_667), (2026, 8, 2));
}

#[test]
fn now_is_shaped_like_a_changelog_date() {
    let now = now_rfc2822();
    // `Thu, 10 Sep 2026 09:47:15 +0000`
    assert!(now.ends_with(" +0000"), "{now}");
    let parts: Vec<&str> = now.split(' ').collect();
    assert_eq!(parts.len(), 6, "{now}");
    assert!(parts[0].ends_with(','), "{now}");
    assert_eq!(parts[4].len(), 8, "{now}");
}

#[test]
fn version_prefix_is_stripped() {
    assert_eq!(strip_v("v1.2.3"), "1.2.3");
    assert_eq!(strip_v("1.2.3"), "1.2.3");
    assert!(looks_like_version("v0.5.3"));
    assert!(!looks_like_version("nightly"));
}
