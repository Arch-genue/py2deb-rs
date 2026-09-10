use std::{
    path::Path,
    fmt,
    fs
};

use super::architecture::Architecture;
use super::include_entry::IncludeEntry;
use super::symlink_entry::SymlinkEntry;

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// The name of the Debian package
    #[serde(deserialize_with = "validate_package_name")]
    pub package: String,
    /// Package version
    pub version: String,

    #[serde(deserialize_with = "validate_maintainer")]
    /// Maintainer
    pub maintainer: String,
    #[serde(default)]
    /// To whom and when the copyright of the software is granted. 
    /// If not present, the maintainer is used.
    pub copyright: String,
    #[serde(default)]
    /// Project URL, written to the copyright file's `Source:` field so the
    /// package points back at where the code came from.
    pub homepage: String,
    #[serde(default)]
    /// Short licence name for the copyright file, e.g. `GPL-3`, `MIT`,
    /// `Apache-2.0`. Debian's own spellings, which are the file names under
    /// `/usr/share/common-licenses`. Defaults to `GPL-3`.
    pub license: String,
    #[serde(default)]
    /// Location of the license file
    /// If not present, GPL-3.0 will be used.
    pub license_file: String,
    #[serde(default)]
    /// Amount of lines to skip at the top of license file. Default 0
    pub license_file_skip_lines: u32,

    #[serde(default = "default_arch")]
    /// Target architecture, default = all 
    arch: Architecture, // TODO!!! Check for binary files in project
    
    #[serde(default)]
    /// Dpkg depends list
    depends: Vec<String>,
    #[serde(default)]
    /// Dpkg conflicts list
    conflicts: Vec<String>,
    
    #[serde(default)]
    /// Project source path
    pub src: String,
    /// Debian destination path (usr/lib/python3/dist-packages/{dest}), default is project_name
    pub dest: Option<String>,
    
    #[serde(default)]
    /// Include other files
    pub include: Vec<IncludeEntry>,
    #[serde(default)]
    /// Exclude files or directories with pattern
    pub exclude: Vec<String>,
    #[serde(default)]
    /// Symbolic links to create in the package, as `[target, link]` — the
    /// order `ln -s` takes, so `["opt/gvcp.AppImage", "usr/bin/gvcp"]` puts a
    /// `gvcp` command on the path pointing at the AppImage.
    pub symlinks: Vec<SymlinkEntry>,

    #[serde(default)]
    /// Package description. The first line is the synopsis, anything after a
    /// blank line is the extended description. Takes precedence over
    /// `description_file`.
    description: String,
    #[serde(default)]
    /// Path to a file holding the description, relative to the project root.
    /// Read only when `description` is empty; Markdown chrome (headings,
    /// badges, fences) is stripped, so pointing this at `README.md` works.
    description_file: String,

    #[serde(default)]
    /// Dpkg section
    section: String,
    #[serde(default)]
    /// Dpkg priority (//TODO!! VALIDATION, required, optional)
    priority: String,
    #[serde(default, rename = "maintainer-scripts", alias = "maintainer_scripts")]
    /// Directory holding maintainer scripts, relative to the project root.
    /// A `build` script found there runs before the package is assembled.
    pub maintainer_scripts: String,
    #[serde(default)]
    /// Changelog file path (relative!), or `$git` / `$git(N)` to build one
    /// from the repository's tags and commits.
    pub changelog: String,
    #[serde(default)]
    /// Distribution the changelog entries are released to. Debian's own
    /// suites (`stable`, `unstable`) or a derivative's codename; defaults to
    /// `unstable`, which is what a package not aimed at a release should say.
    pub distribution: String,
    #[serde(default)]
    /// How pressing the upload is: `low`, `medium`, `high`, `emergency` or
    /// `critical`. Defaults to `medium`, the value `dch` itself uses.
    pub urgency: String,

    // Runtime fields here
    #[serde(default, skip_serializing)]
    /// Dpkg installed size, update automatically
    pub installed_size: u64,
}

fn validate_package_name<'de, D>(d: D) -> Result<String, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de::Error;
    let raw = String::deserialize(d)?;

    if raw.len() < 2 {
        return Err(D::Error::custom("package name must be at least 2 characters"));
    }

    if !raw.chars().next().unwrap().is_ascii_alphanumeric() {
        return Err(D::Error::custom(
            format!("package name must start with a letter or digit, got `{raw}`")
        ));
    }
    if !raw.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c)) {
        return Err(D::Error::custom(
            format!("package name may only certain a-z, 0-9, +, -, . - got `{raw}`")
        ));
    }

    Ok(raw)
}

fn validate_maintainer<'de, D>(d: D) -> Result<String, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de::Error;
    let raw = String::deserialize(d)?;
    if raw.is_empty() {
        return Err(D::Error::custom(
            "maintainer must not be empty; dpkg requires it as `Name <you@example.com>`",
        ));
    }

    if !raw.contains('<') {
        return Err(D::Error::custom(
            format!("maintainer must include an email in angle brackets: `Name <you@example.com>` got - `{raw}`")
        ));
    }

    Ok(raw)
}

fn default_arch() -> Architecture {
    Architecture::All
}

impl Package {
    pub fn new(package: impl Into<String>, version: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            version: version.into(),
            
            maintainer: "Name <user@example.com>".into(),
            copyright: "".into(),
            homepage: "".into(),
            license: "GPL-3".into(),
            license_file: "".into(),
            license_file_skip_lines: 0,

            arch: Architecture::All,
            depends: Vec::new(),
            conflicts: Vec::new(),
            src: "src".into(),
            dest: None,
            include: Vec::new(),
            exclude: Vec::new(),
            symlinks: Vec::new(),
            description: description.into(),
            description_file: "README.md".into(),
            section: "".into(),
            priority: "optional".into(),
            maintainer_scripts: "".into(),
            changelog: "$git".into(),
            distribution: "unstable".into(),
            urgency: "medium".into(),
            installed_size: 0,
        }
    }
    pub fn from_config(path: &Path) -> Result<Self> {
        let config_toml = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let value: toml::Value = toml::from_str(&config_toml)
            .with_context(|| format!("{} is not valid TOML", path.display()))?;

        let section = value.get("tool").and_then(|t| t.get("py2deb"))
            .with_context(|| "Cannot find [tool.py2deb] section. Init project first".to_string())?;

        section.clone().try_into()
            .with_context(|| format!("Invalid [tool.py2deb] section in {}", path.display()))
    }
    pub fn get_package_file_name(&self) -> String {
        format!("{}_{}_{}.deb", self.package, self.version, self.arch)
    }
    /// Whether `src` is the `$skip` sentinel, meaning nothing is packaged from
    /// the source tree and the payload comes from `include` alone.
    pub fn skips_source(&self) -> bool {
        self.src.trim().eq_ignore_ascii_case("$skip")
    }

    /// The changelog's distribution, validated against what Debian accepts.
    ///
    /// An unknown value is passed through rather than rejected — derivatives
    /// use their own codenames (`noble`, `bookworm`), and this tool cannot
    /// know them all — but an empty one becomes the default.
    pub fn distribution(&self) -> String {
        let value = self.distribution.trim();
        if value.is_empty() { "unstable".to_string() } else { value.to_string() }
    }

    /// The changelog's urgency. Unlike the distribution this is a closed set
    /// in Policy 4.4, so anything else is corrected with a warning.
    pub fn urgency(&self) -> String {
        const LEVELS: [&str; 5] = ["low", "medium", "high", "emergency", "critical"];
        let value = self.urgency.trim().to_ascii_lowercase();

        if value.is_empty() {
            return "medium".to_string();
        }
        if LEVELS.contains(&value.as_str()) {
            return value;
        }

        eprintln!(
            "{:>15} urgency `{}` is not one of {}; using medium",
            "Warning".yellow().bold(),
            self.urgency,
            LEVELS.join(", ")
        );
        "medium".to_string()
    }

    /// The value of a field addressed by name, as `$name` in a template.
    ///
    /// Only fields worth interpolating are exposed; anything else is left
    /// alone by [`Package::expand_dollar_properties`] so an unrelated `$`
    /// in the text survives untouched.
    pub fn get_dollar_property(&self, name: &str) -> Option<String> {
        Some(match name {
            "package" => self.package.clone(),
            "version" => self.version.clone(),
            "arch" | "architecture" => self.arch.to_string(),
            "maintainer" => self.maintainer.clone(),
            "description" => self.description.clone(),
            "section" => self.section.clone(),
            "priority" => self.priority.clone(),
            "installed_size" => self.installed_size.to_string(),
            _ => return None,
        })
    }

    /// Replaces every `$name` in `template` with that field's value.
    ///
    /// A name runs until the first character that cannot be part of one, so
    /// `$package_1.0` reads as `$package` followed by `_1.0`. `$$` is a
    /// literal dollar sign, and an unknown `$name` is left as written rather
    /// than silently becoming empty.
    pub fn expand_dollar_properties(&self, template: &str) -> String {
        let mut out = String::with_capacity(template.len());
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c != '$' {
                out.push(c);
                continue;
            }

            if chars.peek() == Some(&'$') {
                chars.next();
                out.push('$');
                continue;
            }

            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }

            match self.get_dollar_property(&name) {
                Some(value) => out.push_str(&value),
                None => {
                    out.push('$');
                    out.push_str(&name);
                }
            }
        }

        out
    }
    /// The package description, as `(synopsis, extended)`.
    ///
    /// `description` wins when set; otherwise `description_file` is read and
    /// stripped of Markdown, which is what makes the default of `README.md`
    /// usable. A file that cannot be read is a warning rather than an error —
    /// the package is still installable, just poorer.
    fn resolve_description(&self, root: &Path) -> (String, String) {
        let inline = self.description.trim();
        if !inline.is_empty() {
            return split_description(inline);
        }

        if self.description_file.is_empty() {
            return (String::new(), String::new());
        }

        // Relative to the project, not to wherever py2deb was invoked from.
        let path = root.join(&self.description_file);
        match fs::read_to_string(&path) {
            Ok(text) => split_description(&strip_markdown(&text)),
            Err(error) => {
                eprintln!(
                    "{:>15} cannot read description file {}: {}",
                    "Warning".yellow().bold(),
                    path.display(),
                    error
                );
                (String::new(), String::new())
            }
        }
    }

    /// Renders the `Description:` field.
    ///
    /// Debian's shape is a one-line synopsis followed by an extended
    /// description indented by a single space, where a blank line is written
    /// ` .` — a truly empty line would end the field. lintian rejects a
    /// package whose extended part is missing, so a synopsis with nothing
    /// after it gets a minimal body rather than none.
    fn format_description(&self, root: &Path) -> String {
        let (synopsis, extended) = self.resolve_description(root);

        if synopsis.is_empty() {
            eprintln!(
                "{:>15} no description set; add `description` or `description_file` to [tool.py2deb]",
                "Warning".yellow().bold()
            );
            return format!("Description: {}\n .\n", self.package);
        }

        let mut out = format!("Description: {synopsis}\n");
        if extended.is_empty() {
            // Repeating the synopsis is what dh_make does when upstream gives
            // nothing more; an empty extended part is a lintian error.
            out.push_str(" .\n");
        } else {
            for line in extended.lines() {
                let line = line.trim_end();
                if line.is_empty() {
                    out.push_str(" .\n");
                } else {
                    out.push_str(&format!(" {line}\n"));
                }
            }
        }

        out
    }

    pub fn generate_control(&mut self, root: &Path) -> String {
        let mut control_str = format!("
Package: {}
Version: {}
Architecture: {}
Maintainer: {}
Installed-Size: {}
Priority: {}\n",
            self.package,
            self.version,
            self.arch,
            self.maintainer,
            self.installed_size,
            self.priority,
        );
        if !self.section.is_empty() {
            control_str.push_str(format!("Section: {}\n", self.section).as_str());
        } else {
            control_str.push_str("Section: python\n");
        }

        if !self.depends.is_empty() {
            control_str.push_str(format!("Depends: python3:any, {}\n", self.depends.join(", ")).as_str());
        } else {
            control_str.push_str("Depends: python3:any\n");
        }
        if !self.conflicts.is_empty() {
            control_str.push_str(format!("Conflicts: {}\n", self.conflicts.join(", ")).as_str());
        }

        // Last, because its continuation lines would otherwise swallow
        // whatever field came next.
        control_str.push_str(&self.format_description(root));

        control_str.trim_start().to_string()
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} {}", "Package:".bold(), self.package.bold().green())?;
        writeln!(f, "Version: {}", self.version.blue())?;
        writeln!(f, "Maintainer: {}", self.maintainer)?;
        writeln!(f, "Architecture: {}", self.arch)?;
        if !self.depends.is_empty() {
            writeln!(f, "Dependencies: {}", self.depends.join(", "))?;
        }
        if !self.conflicts.is_empty() {
            writeln!(f, "Conflicts: {}", self.conflicts.join(", "))?;
        }
        Ok(())
    }
}

/// Splits description text into a synopsis and an extended part.
///
/// The synopsis is the first line; the extended description is what follows
/// the first blank line. Debian asks the synopsis to read as a noun phrase
/// completing "a package that is…", so a leading article is trimmed —
/// lintian flags `description-synopsis-starts-with-article`.
pub fn split_description(text: &str) -> (String, String) {
    let text = text.trim();
    if text.is_empty() {
        return (String::new(), String::new());
    }

    let mut parts = text.splitn(2, "\n\n");
    let first = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();

    // A synopsis wrapped across lines is still one line of prose.
    let paragraph = first.split_whitespace().collect::<Vec<_>>().join(" ");

    // Policy 3.4.1 caps the synopsis at 80 characters, and a README's opening
    // paragraph is usually longer. Its first sentence is what was meant as the
    // summary, so the remainder is pushed down into the extended description
    // rather than truncated away.
    let (head, tail) = split_first_sentence(&paragraph);
    let synopsis = trim_article(&head);

    let extended = match (tail.is_empty(), rest.is_empty()) {
        (true, _) => rest.to_string(),
        (false, true) => tail,
        (false, false) => format!("{tail}\n\n{rest}"),
    };

    (synopsis, extended)
}

/// Splits prose after its first sentence, leaving the rest for the extended
/// description.
///
/// Only splits when the paragraph actually overruns what a synopsis may hold;
/// a short opening line is left whole even if it contains a full stop.
pub fn split_first_sentence(text: &str) -> (String, String) {
    // Policy 3.4.1 measures the whole line, and `Description: ` is 13 of it.
    const MAX_SYNOPSIS: usize = 80 - "Description: ".len() - 1;

    if text.chars().count() <= MAX_SYNOPSIS {
        return (text.to_string(), String::new());
    }

    // A full stop followed by a space ends a sentence; one inside `0.1` or
    // `e.g.` does not, so the next character must start a new word.
    let bytes = text.as_bytes();
    let mut end = None;
    for (i, window) in bytes.windows(2).enumerate() {
        if window[0] == b'.' && window[1] == b' ' {
            end = Some(i + 1);
            break;
        }
    }

    match end {
        Some(i) => (text[..i].trim_end_matches('.').trim().to_string(), text[i..].trim().to_string()),
        // No sentence break: fall back to a word boundary so nothing is lost.
        None => match text[..].char_indices().take_while(|(i, _)| *i <= MAX_SYNOPSIS).filter(|(_, c)| *c == ' ').last() {
            Some((i, _)) => (text[..i].trim().to_string(), text[i..].trim().to_string()),
            None => (text.to_string(), String::new()),
        },
    }
}

/// Drops a leading article, and the trailing full stop Debian does not want.
pub fn trim_article(synopsis: &str) -> String {
    let trimmed = synopsis.trim_end_matches('.').trim();

    for article in ["a ", "an ", "the "] {
        if trimmed.len() > article.len()
            && trimmed[..article.len()].eq_ignore_ascii_case(article)
        {
            return trimmed[article.len()..].trim().to_string();
        }
    }

    trimmed.to_string()
}

/// Reduces Markdown to the plain prose a `Description:` field can hold.
///
/// This is deliberately not a Markdown parser: the goal is only to make the
/// common shape of a README — a title, some badges, a paragraph — usable as a
/// description. Fenced code, tables and headings carry layout that means
/// nothing once indented into a control field, so they are dropped; inline
/// emphasis and links are unwrapped to the text they display.
pub fn strip_markdown(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        // Headings, tables, and horizontal rules are pure layout.
        if trimmed.starts_with('#')
            || trimmed.starts_with('|')
            || is_horizontal_rule(trimmed)
        {
            continue;
        }

        // A badge-only line is a row of images and links with no prose left
        // once they are removed.
        let cleaned = strip_inline_markup(trimmed);
        if cleaned.is_empty() {
            // Keep the paragraph break a blank line represents, but never
            // start the description with one.
            if !out.is_empty() && !out.last().is_some_and(String::is_empty) {
                out.push(String::new());
            }
            continue;
        }

        out.push(cleaned);
    }

    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }

    out.join("\n")
}

fn is_horizontal_rule(line: &str) -> bool {
    line.len() >= 3 && line.chars().all(|c| c == '-' || c == '=' || c == '*')
}

/// Unwraps links and images to their text and drops emphasis markers.
fn strip_inline_markup(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            // `![alt](url)` — an image contributes nothing readable. Its alt
            // text goes too: on a badge it is a label like "build", which
            // says nothing about the package.
            '!' if chars.get(i + 1) == Some(&'[') => match closing_link(&chars, i + 1) {
                Some(end) => i = end,
                None => {
                    out.push(chars[i]);
                    i += 1;
                }
            },
            // `[text](url)` keeps its text. A badge is an image wrapped in a
            // link — `[![alt](img)](href)` — so the inner image is skipped
            // first, which leaves the outer link with nothing to contribute.
            '[' => {
                if chars.get(i + 1) == Some(&'!') && chars.get(i + 2) == Some(&'[') 
                    && let Some(inner_end) = closing_link(&chars, i + 2) {
                    // Step over the image, then over the link closing it.
                    i = match closing_link_from(&chars, inner_end) {
                        Some(end) => end,
                        None => inner_end,
                    };
                    continue;
                }

                match link_text(&chars, i) {
                    Some((text, end)) => {
                        out.push_str(&text);
                        i = end;
                    }
                    None => {
                        out.push(chars[i]);
                        i += 1;
                    }
                }
            }
            // Emphasis and inline code are markup, not content.
            '*' | '_' | '`' => i += 1,
            c => {
                out.push(c);
                i += 1;
            }
        }
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The index just past a `[...](...)` starting at `open`, if it is one.
fn closing_link(chars: &[char], open: usize) -> Option<usize> {
    let bracket = chars[open..].iter().position(|&c| c == ']')? + open;
    if chars.get(bracket + 1) != Some(&'(') {
        return None;
    }
    let paren = chars[bracket + 1..].iter().position(|&c| c == ')')? + bracket + 1;
    Some(paren + 1)
}

/// The index just past a `](...)` that begins at `at` — the tail of a link
/// whose text has already been consumed, as with a badge's wrapping link.
fn closing_link_from(chars: &[char], at: usize) -> Option<usize> {
    if chars.get(at) != Some(&']') || chars.get(at + 1) != Some(&'(') {
        return None;
    }
    let paren = chars[at + 1..].iter().position(|&c| c == ')')? + at + 1;
    Some(paren + 1)
}

/// The display text of a `[text](url)` at `open`, and the index just past it.
///
/// The closing bracket is found by depth, so `[see [1]](url)` keeps its whole
/// text rather than stopping at the first `]`.
fn link_text(chars: &[char], open: usize) -> Option<(String, usize)> {
    let mut depth = 0usize;
    let mut bracket = None;

    for (offset, &c) in chars[open..].iter().enumerate() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    bracket = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }

    let bracket = bracket?;
    if chars.get(bracket + 1) != Some(&'(') {
        return None;
    }
    let paren = chars[bracket + 1..].iter().position(|&c| c == ')')? + bracket + 1;

    Some((chars[open + 1..bracket].iter().collect(), paren + 1))
}
