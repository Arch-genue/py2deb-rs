//! Reading a Debian changelog out of the repository's own history.
//!
//! `debian/changelog` is a history of *releases*, not of commits: each entry
//! names one version, the distribution it went to, and the moment it was
//! released. Git already records exactly that — a tag is a release — so the
//! grouping here follows tags, and the commits reachable from one tag but not
//! its predecessor become that entry's bullet points. Commits made after the
//! newest tag belong to the version currently being built, which is the entry
//! at the top.
//!
//! Everything is written to the letter of Debian Policy 4.4, because dpkg
//! parses this file and lintian complains about the rest: two spaces before a
//! `*`, four before a continuation line, and a trailer of exactly one space,
//! `--`, the maintainer, two spaces, and an RFC 2822 date with a numeric zone.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result, bail};

const F: char = '\x1f'; // разделитель полей
const R: char = '\x1e'; // разделитель записей

pub fn config(repo: &Path, key: &str) -> Option<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["config", "--get", key]).output().ok()?;

    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// Runs git inside `repo` and returns stdout, turning a non-zero exit into an
/// error that carries git's own message — those are far more useful than
/// anything this module could invent.
///
/// The directory is always passed explicitly: the process's own cwd is not
/// necessarily the project being built, since `--path` can point elsewhere.
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run `git {}`", args.join(" ")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("`git {}` failed: {}", args.join(" "), stderr.trim());
    }

    String::from_utf8(out.stdout).context("git printed invalid UTF-8")
}

/// Whether the current directory is inside a git work tree, which is what
/// decides if a `$git(...)` changelog can be produced at all.
pub fn is_repository(repo: &Path) -> bool {
    Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .is_ok_and(|out| out.status.success())
}

#[derive(Debug, Clone)]
pub struct Commit {
    pub hash: String,
    pub title: String,   // %s
    pub body: String,    // %b
    pub author: String,
    pub email: String,
    pub date: String,    // RFC2822 — ровно формат debian/changelog
}

impl Commit {
    /// `Name <email>`, the form Debian's trailer line wants.
    pub fn maintainer(&self) -> String {
        format!("{} <{}>", self.author, self.email)
    }
}

/// One released version, as it will appear in the changelog.
#[derive(Debug, Clone)]
pub struct Release {
    /// Version without the tag's `v` prefix; the entry's own version.
    pub version: String,
    /// Commits belonging to this release, newest first.
    pub commits: Vec<Commit>,
    /// Who and when to put on the trailer — the tagger where there is a tag,
    /// otherwise the newest commit in the range.
    pub maintainer: String,
    pub date: String,
}

fn parse_commits(raw: &str) -> Vec<Commit> {
    raw.split(R)
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .filter_map(|rec| {
            let f: Vec<&str> = rec.split(F).collect();
            (f.len() == 6).then(|| Commit {
                hash: f[0].into(),
                title: f[1].trim().into(),
                body: f[2].trim().into(),
                author: f[3].into(),
                email: f[4].into(),
                date: f[5].into(),
            })
        })
        .collect()
}

const PRETTY: &str = "%H\x1f%s\x1f%b\x1f%an\x1f%ae\x1f%aD\x1e";

/// Commits in `range` (any revision range git understands), newest first.
fn commits_in(repo: &Path, range: &str) -> Result<Vec<Commit>> {
    let raw = git(repo, &["log", range, &format!("--pretty=format:{PRETTY}")])?;
    Ok(parse_commits(&raw))
}

/// Tags that look like versions, newest release first.
///
/// Ordering is git's own `version:refname`, which compares the numeric parts
/// numerically, so `v1.10.0` correctly sorts above `v1.9.0` — a plain
/// lexicographic sort gets that backwards. Tags that are not versions (`nightly`,
/// `latest`) are skipped rather than turned into nonsense entries.
fn version_tags(repo: &Path) -> Result<Vec<String>> {
    let raw = git(repo, &["tag", "--sort=-version:refname"])?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .filter(|t| looks_like_version(t))
        .map(String::from)
        .collect())
}

/// A tag names a release if, once an optional `v` is dropped, it starts with a
/// digit — which is what Debian versions must do as well.
pub fn looks_like_version(tag: &str) -> bool {
    strip_v(tag).chars().next().is_some_and(|c| c.is_ascii_digit())
}

pub fn strip_v(tag: &str) -> &str {
    tag.strip_prefix('v').or_else(|| tag.strip_prefix('V')).unwrap_or(tag)
}

/// Who and when a tag was made.
///
/// An annotated tag carries its own tagger, which is the true release moment;
/// a lightweight tag has none, so git's `%(taggerdate)` comes back empty and
/// the commit it points at answers instead.
fn tag_metadata(repo: &Path, tag: &str) -> Option<(String, String)> {
    let raw = git(repo, &[
        "for-each-ref",
        &format!("refs/tags/{tag}"),
        "--format=%(taggername)\x1f%(taggeremail)\x1f%(taggerdate:rfc2822)",
    ])
    .ok()?;

    let f: Vec<&str> = raw.trim_end().split(F).collect();
    if f.len() != 3 || f[2].trim().is_empty() {
        return None;
    }

    // `%(taggeremail)` already includes the angle brackets.
    let email = f[1].trim();
    let name = f[0].trim();
    (!name.is_empty()).then(|| (format!("{name} {email}"), f[2].trim().to_string()))
}

/// The current UTC time in the RFC 2822 form a changelog trailer needs.
///
/// Formatted here rather than through a date crate: this is the one date the
/// tool has to produce itself, and civil time from a Unix timestamp is a
/// dozen lines. UTC (`+0000`) sidesteps the local zone entirely, which is
/// also what a reproducible build wants — the same source must not produce a
/// different changelog on a machine set to a different zone.
pub fn now_rfc2822() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // 1 Jan 1970 was a Thursday, index 4 in a week starting on Sunday.
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let weekday = WEEKDAYS[((days + 4) % 7) as usize];

    let (year, month, day) = civil_from_days(days as i64);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_name = MONTHS[(month - 1) as usize];

    format!("{weekday}, {day} {month_name} {year} {hour:02}:{minute:02}:{second:02} +0000")
}

/// Days since the Unix epoch to a civil `(year, month, day)`.
///
/// Howard Hinnant's `civil_from_days`, which shifts the era to start in March
/// so the leap day falls at the end of a year and needs no special case.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;

    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// What the working tree looks like relative to the last release.
#[derive(Debug, Clone)]
pub struct Describe {
    /// Commits since the newest version tag, 0 when sitting on it.
    pub distance: u32,
    /// Abbreviated hash of `HEAD`.
    pub hash: String,
    /// Whether tracked files differ from `HEAD`.
    pub dirty: bool,
}

/// Describes `HEAD` against the newest version tag.
///
/// `git describe` is not used directly: it picks the nearest tag of any shape,
/// so a `nightly` tag would derail it, and the same `looks_like_version` rule
/// that drives the changelog has to apply here too.
pub fn describe(repo: &Path) -> Result<Describe> {
    let hash = git(repo, &["rev-parse", "--short", "HEAD"])?.trim().to_string();
    if hash.is_empty() {
        bail!("the repository has no commits");
    }

    let distance = match version_tags(repo)?.first() {
        Some(tag) => git(repo, &["rev-list", "--count", &format!("{tag}..HEAD")])?
            .trim()
            .parse()
            .unwrap_or(0),
        // With no tags, every commit counts as distance from nothing.
        None => git(repo, &["rev-list", "--count", "HEAD"])?
            .trim()
            .parse()
            .unwrap_or(0),
    };

    // `diff-index` needs a refreshed index, or unchanged files whose mtime
    // moved (a fresh checkout, a touch) read as modified.
    let _ = git(repo, &["update-index", "--refresh"]);
    let dirty = git(repo, &["diff-index", "--quiet", "HEAD", "--"]).is_err();

    Ok(Describe { distance, hash, dirty })
}

/// Appends git's position to `base`, giving every build its own version.
///
/// The suffix goes after `+`, which dpkg sorts *after* the bare version and
/// before the next upstream one — `1.1.8 < 1.1.8+3.gabc1234 < 1.1.9` — so a
/// development build upgrades cleanly in both directions. The commit count
/// leads it because dpkg compares digit runs numerically, making `+12.` newer
/// than `+3.`; two hashes on their own would only sort alphabetically, which
/// says nothing about which came first.
///
/// Sitting exactly on a clean tag returns `base` untouched: that build *is*
/// the release, and giving it a suffix would make it sort above the version
/// it claims to be.
pub fn version_with_commit(base: &str, described: &Describe) -> String {
    if described.distance == 0 && !described.dirty {
        return base.to_string();
    }

    let mut version = format!("{base}+{}.g{}", described.distance, described.hash);
    if described.dirty {
        // Uncommitted changes are not reproducible from the hash alone, so the
        // version says so rather than impersonating that commit.
        version.push_str(".dirty");
    }

    version
}

/// Groups history into releases, newest first.
///
/// `version` is what the package is being built as, and it labels the entry
/// holding everything committed after the newest tag. `limit` caps how many
/// released versions are included, so a long-lived repository does not ship a
/// changelog longer than the package.
pub fn releases(repo: &Path, version: &str, limit: usize) -> Result<Vec<Release>> {
    let tags = version_tags(repo)?;
    let mut releases = Vec::new();

    // Everything since the newest tag is the version being built now. With no
    // tags at all, that is the entire history.
    let unreleased_range = match tags.first() {
        Some(newest) => format!("{newest}..HEAD"),
        None => "HEAD".to_string(),
    };

    let unreleased = commits_in(repo, &unreleased_range)?;
    if !unreleased.is_empty() {
        let head = &unreleased[0];
        releases.push(Release {
            version: version.to_string(),
            maintainer: head.maintainer(),
            // This entry is the release being made now, not the moment its
            // last commit was written, so it is stamped with the build time.
            // That also keeps it strictly newer than the tag below it, which
            // lintian requires (`latest-changelog-entry-without-new-date`).
            date: now_rfc2822(),
            commits: unreleased,
        });
    }

    for (i, tag) in tags.iter().enumerate() {
        if releases.len() >= limit {
            break;
        }

        let range = match tags.get(i + 1) {
            Some(previous) => format!("{previous}..{tag}"),
            None => tag.to_string(), // the oldest tag takes everything before it
        };

        let commits = commits_in(repo, &range)?;
        if commits.is_empty() {
            continue;
        }

        // The tag itself is the release; only fall back to its commit when the
        // tag is lightweight and carries no tagger of its own.
        let (maintainer, date) = tag_metadata(repo, tag)
            .unwrap_or_else(|| (commits[0].maintainer(), commits[0].date.clone()));

        releases.push(Release {
            version: strip_v(tag).to_string(),
            commits,
            maintainer,
            date,
        });
    }

    if releases.is_empty() {
        bail!("the repository has no commits, so no changelog can be built");
    }

    Ok(releases)
}

/// Renders one changelog entry.
///
/// The commit subject becomes the bullet; its body follows as continuation
/// lines, because a body explains the change and dropping it loses the part a
/// reader actually needs. Trailer lines git conventionally carries
/// (`Signed-off-by:`, `Co-Authored-By:`) are dropped — they say who touched
/// the commit, not what changed, and belong nowhere in a changelog.
fn render_entry(pkg: &str, dist: &str, urgency: &str, release: &Release) -> String {
    let mut s = format!(
        "{pkg} ({}) {dist}; urgency={urgency}\n\n",
        release.version
    );

    for c in &release.commits {
        // A title can be empty on a malformed commit; the hash keeps the entry
        // pointing at something real instead of showing a bare `*`.
        let title = if c.title.is_empty() {
            format!("commit {}", &c.hash[..c.hash.len().min(8)])
        } else {
            c.title.clone()
        };
        s += &format!("  * {}\n", wrap_bullet(&title, "  * ", "    "));

        for line in c.body.lines() {
            let line = line.trim();
            if line.is_empty() || is_trailer(line) {
                continue;
            }
            s += &format!("    {}\n", wrap_bullet(line, "    ", "      "));
        }
    }

    // Exactly one leading space, two spaces before the date — dpkg's parser
    // is strict about both.
    s + &format!("\n -- {}  {}\n", release.maintainer, release.date)
}

/// Git trailers, which describe authorship rather than the change itself.
fn is_trailer(line: &str) -> bool {
    const KEYS: [&str; 8] = [
        "signed-off-by:", "co-authored-by:", "reviewed-by:", "acked-by:",
        "tested-by:", "reported-by:", "suggested-by:", "cc:",
    ];
    let lower = line.to_ascii_lowercase();
    KEYS.iter().any(|k| lower.starts_with(k))
}

/// Folds a long line to roughly 80 columns, the width changelogs are read at.
///
/// The first line already sits behind `prefix`, so only the room left over is
/// available to it; every wrapped line after that is indented by `continuation`.
fn wrap_bullet(text: &str, prefix: &str, continuation: &str) -> String {
    const WIDTH: usize = 79;

    let mut out = String::with_capacity(text.len());
    let mut column = prefix.len();

    for word in text.split_whitespace() {
        // Always place the first word, even if it alone overruns the width —
        // breaking inside a word would corrupt a URL or an identifier.
        if !out.is_empty() && column + 1 + word.chars().count() > WIDTH {
            out.push('\n');
            out.push_str(continuation);
            column = continuation.len();
        } else if !out.is_empty() {
            out.push(' ');
            column += 1;
        }

        out.push_str(word);
        column += word.chars().count();
    }

    out
}

/// The whole changelog: the version being built, then previous releases.
pub fn changelog(pkg: &str, dist: &str, urgency: &str, releases: &[Release]) -> String {
    releases
        .iter()
        .map(|r| render_entry(pkg, dist, urgency, r))
        .collect::<Vec<_>>()
        .join("\n")
}
