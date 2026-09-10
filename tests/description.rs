//! Turning `description` / `description_file` into a Debian `Description:`.
//!
//! The field is a one-line synopsis plus an extended part, and Policy caps
//! the synopsis at 80 characters — a README's opening paragraph rarely fits,
//! so most of what is tested here is how prose gets divided.

use py2deb::package::{split_description, split_first_sentence, strip_markdown, trim_article};

#[test]
fn first_line_is_the_synopsis() {
    let (synopsis, extended) = split_description("Short summary\n\nMore detail here.");
    assert_eq!(synopsis, "Short summary");
    assert_eq!(extended, "More detail here.");
}

#[test]
fn a_wrapped_paragraph_becomes_one_line() {
    let (synopsis, _) = split_description("Keyboard layout\nswitcher");
    assert_eq!(synopsis, "Keyboard layout switcher");
}

#[test]
fn leading_articles_are_dropped() {
    // lintian: description-synopsis-starts-with-article
    assert_eq!(trim_article("A keyboard switcher"), "keyboard switcher");
    assert_eq!(trim_article("An evdev wrapper"), "evdev wrapper");
    assert_eq!(trim_article("The layout daemon"), "layout daemon");
    // A word merely starting with those letters is not an article.
    assert_eq!(trim_article("Android helper"), "Android helper");
}

#[test]
fn trailing_full_stop_is_dropped() {
    assert_eq!(trim_article("Layout switcher."), "Layout switcher");
}

#[test]
fn a_short_line_is_left_whole() {
    let (head, tail) = split_first_sentence("Small. Tool.");
    assert_eq!(head, "Small. Tool.");
    assert!(tail.is_empty());
}

#[test]
fn a_long_paragraph_splits_at_its_first_sentence() {
    let text = "Switches keyboard layouts on Linux without an X11 session. \
                It also works on Wayland and on the bare console.";
    let (head, tail) = split_first_sentence(text);

    assert_eq!(head, "Switches keyboard layouts on Linux without an X11 session");
    assert!(tail.starts_with("It also works"), "{tail}");
}

#[test]
fn a_long_paragraph_without_a_sentence_break_splits_on_a_word() {
    let text = "switching keyboard layouts on Linux driven by evdev and requiring \
                no X11 session at all";
    let (head, tail) = split_first_sentence(text);

    assert!("Description: ".len() + head.chars().count() < 80, "synopsis too long: {head}");
    assert!(!tail.is_empty());
    // Nothing may be lost in the split.
    assert_eq!(format!("{head} {tail}"), text.split_whitespace().collect::<Vec<_>>().join(" "));
}

#[test]
fn the_synopsis_fits_what_policy_allows() {
    let long = "A library for switching keyboard layouts on Linux, driven by evdev \
                and requiring no X11 session whatsoever";
    let (synopsis, extended) = split_description(long);

    // Policy 3.4.1: the whole `Description:` line stays under 80 characters.
    assert!(
        "Description: ".len() + synopsis.chars().count() < 80,
        "synopsis too long ({}): {synopsis}",
        synopsis.chars().count()
    );
    assert!(!extended.is_empty());
}

#[test]
fn headings_and_fences_are_stripped() {
    let md = "# Title\n\nReal prose here.\n\n```sh\npip install thing\n```\n";
    assert_eq!(strip_markdown(md), "Real prose here.");
}

#[test]
fn tables_and_rules_are_stripped() {
    let md = "Prose.\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n---\n";
    assert_eq!(strip_markdown(md), "Prose.");
}

#[test]
fn badges_leave_nothing_behind() {
    // A badge is an image wrapped in a link; neither carries prose.
    let md = "[![build](https://img.shields.io/x.svg)](https://ci.example.com)\n\nProse.";
    assert_eq!(strip_markdown(md), "Prose.");
}

#[test]
fn links_keep_their_text() {
    assert_eq!(strip_markdown("See [the docs](https://example.com) for more."),
               "See the docs for more.");
}

#[test]
fn emphasis_markers_are_removed() {
    assert_eq!(strip_markdown("Uses **evdev** and `ioctl` directly."),
               "Uses evdev and ioctl directly.");
}

#[test]
fn paragraph_breaks_survive_but_do_not_lead() {
    // The heading leaves a blank line behind; it must not open the output.
    let md = "# Title\n\nOne.\n\nTwo.\n";
    assert_eq!(strip_markdown(md), "One.\n\nTwo.");
}

#[test]
fn a_readme_shaped_file_produces_a_usable_description() {
    let md = "# gkeyboard\n\n\
              [![build](https://img.shields.io/x.svg)](https://ci.example.com)\n\n\
              A library for **switching keyboard layouts** on Linux, driven by\n\
              `evdev` and requiring no X11 session.\n\n\
              It works on Wayland too.\n\n\
              ## Installation\n\n```sh\npip install gkeyboard\n```\n";

    let (synopsis, extended) = split_description(&strip_markdown(md));

    assert!(!synopsis.is_empty());
    assert!(!synopsis.starts_with('#'), "{synopsis}");
    assert!(!synopsis.contains("!["), "badge leaked: {synopsis}");
    assert!("Description: ".len() + synopsis.chars().count() < 80, "{synopsis}");
    assert!(extended.contains("Wayland"), "{extended}");
    assert!(!extended.contains("pip install"), "code fence leaked: {extended}");
}
