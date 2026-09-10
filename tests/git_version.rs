//! `--git-version`: giving every build from an untagged commit its own version.
//!
//! The suffix exists so a rebuild of unchanged-version code does not collide
//! with what is already published. That only works if dpkg orders the results
//! the way a person would expect, which is what most of these check.

use py2deb::git::{version_with_commit, Describe};

/// A position described against tag `1.1.8` — the case where the config and
/// the newest tag already agree.
fn described(distance: u32, hash: &str, dirty: bool) -> Describe {
    Describe { tag: Some("1.1.8".into()), distance, hash: hash.into(), dirty }
}

/// A position in a repository carrying no version tags at all.
fn untagged(distance: u32, hash: &str, dirty: bool) -> Describe {
    Describe { tag: None, distance, hash: hash.into(), dirty }
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

#[test]
fn the_newest_tag_beats_a_stale_config_version() {
    // The tag declares the release. A config nobody remembered to bump must
    // not silently republish the previous version.
    let described = Describe {
        tag: Some("2.4.4".into()),
        distance: 0,
        hash: "71daaee".into(),
        dirty: false,
    };

    assert_eq!(version_with_commit("2.4.3", &described), "2.4.4");
}

#[test]
fn commits_past_the_newest_tag_build_on_that_tag() {
    let described = Describe {
        tag: Some("2.4.4".into()),
        distance: 2,
        hash: "abc1234".into(),
        dirty: false,
    };

    assert_eq!(version_with_commit("2.4.3", &described), "2.4.4+2.gabc1234");
}

#[test]
fn without_tags_the_config_version_is_the_base() {
    // Nothing else to go on, so the config is all there is.
    assert_eq!(
        version_with_commit("0.1.0", &untagged(7, "abc1234", false)),
        "0.1.0+7.gabc1234"
    );
}

#[test]
fn an_untagged_repository_on_its_first_commit_still_gets_a_suffix() {
    // Distance is measured from nothing, so it is never 0 here in practice;
    // guard the shape anyway.
    assert_eq!(
        version_with_commit("0.1.0", &untagged(1, "abc1234", false)),
        "0.1.0+1.gabc1234"
    );
}

#[test]
fn a_tag_newer_than_the_config_still_sorts_correctly() {
    let described = Describe {
        tag: Some("2.4.4".into()),
        distance: 0,
        hash: "71daaee".into(),
        dirty: false,
    };
    let built = version_with_commit("2.4.3", &described);

    assert!(lt("2.4.3", &built), "{built} should sort above the stale 2.4.3");
    assert!(lt(&built, "2.4.5"), "{built} should sort below 2.4.5");
}
