//! The `changelog = "$git(N)"` directive, as read from `pyproject.toml`.

use py2deb::deb::build::parse_git_directive;

#[test]
fn bare_git_means_everything() {
    assert_eq!(parse_git_directive("$git"), Some(usize::MAX));
    assert_eq!(parse_git_directive("  $git  "), Some(usize::MAX));
}

#[test]
fn a_count_is_honoured() {
    assert_eq!(parse_git_directive("$git(5)"), Some(5));
    assert_eq!(parse_git_directive("$git( 5 )"), Some(5));
}

#[test]
fn paths_are_not_directives() {
    assert_eq!(parse_git_directive("debian/changelog"), None);
    assert_eq!(parse_git_directive("docs/$git-notes.md"), None);
}

#[test]
fn a_bad_count_falls_back_instead_of_failing() {
    assert_eq!(parse_git_directive("$git(abc)"), Some(usize::MAX));
    assert_eq!(parse_git_directive("$git(0)"), Some(usize::MAX));
}
