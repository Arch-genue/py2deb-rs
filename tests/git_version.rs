//! `--git-version`: giving every build from an untagged commit its own version.
//!
//! The suffix exists so a rebuild of unchanged-version code does not collide
//! with what is already published. That only works if dpkg orders the results
//! the way a person would expect, which is what most of these check.

use py2deb::git::{version_with_commit, Describe};

fn described(distance: u32, hash: &str, dirty: bool) -> Describe {
    Describe { distance, hash: hash.into(), dirty }
}

#[test]
fn a_clean_tag_keeps_the_plain_version() {
    // This build *is* the release; a suffix would sort it above itself.
    assert_eq!(version_with_commit("1.1.8", &described(0, "abc1234", false)), "1.1.8");
}

#[test]
fn commits_past_a_tag_get_a_suffix() {
    assert_eq!(
        version_with_commit("1.1.8", &described(3, "abc1234", false)),
        "1.1.8+3.gabc1234"
    );
}

#[test]
fn a_dirty_tree_is_marked() {
    // Uncommitted work is not reproducible from the hash, so it says so.
    assert_eq!(
        version_with_commit("1.1.8", &described(3, "abc1234", true)),
        "1.1.8+3.gabc1234.dirty"
    );
}

#[test]
fn a_dirty_tree_on_a_tag_still_gets_a_suffix() {
    // Distance is zero but the tree does not match the tag, so the version
    // must not claim to be the release.
    assert_eq!(
        version_with_commit("1.1.8", &described(0, "abc1234", true)),
        "1.1.8+0.gabc1234.dirty"
    );
}

/// dpkg's own comparison, which is the only opinion that matters here.
fn lt(a: &str, b: &str) -> bool {
    std::process::Command::new("dpkg")
        .args(["--compare-versions", a, "lt", b])
        .status()
        .map(|s| s.success())
        .unwrap_or_else(|_| panic!("dpkg is required to verify version ordering"))
}

#[test]
fn a_dev_build_sorts_above_the_release_it_follows() {
    let release = "1.1.8";
    let dev = version_with_commit(release, &described(3, "abc1234", false));
    assert!(lt(release, &dev), "{release} should sort below {dev}");
}

#[test]
fn a_dev_build_sorts_below_the_next_release() {
    let dev = version_with_commit("1.1.8", &described(3, "abc1234", false));
    assert!(lt(&dev, "1.1.9"), "{dev} should sort below 1.1.9");
}

#[test]
fn later_commits_sort_above_earlier_ones() {
    // The count leads the suffix precisely so this holds: dpkg compares digit
    // runs numerically, whereas two hashes alone would only sort by spelling.
    let earlier = version_with_commit("1.1.8", &described(3, "fff9999", false));
    let later = version_with_commit("1.1.8", &described(12, "aaa1111", false));

    assert!(lt(&earlier, &later), "{earlier} should sort below {later}");
}

#[test]
fn a_dirty_build_sorts_above_the_commit_it_came_from() {
    let clean = version_with_commit("1.1.8", &described(3, "abc1234", false));
    let dirty = version_with_commit("1.1.8", &described(3, "abc1234", true));

    assert!(lt(&clean, &dirty), "{clean} should sort below {dirty}");
}
