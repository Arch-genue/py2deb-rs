//! End-to-end packaging: build a fixture and read back what landed in the
//! archive.
//!
//! These drive `DebianBuild` the way `main` does, so they cover the pieces
//! that only show up once a real archive exists — the source walk, `$skip`,
//! recursive includes, symlinks and the build script.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use py2deb::deb::build::DebianBuild;
use py2deb::package::Package;
use py2deb::Verbosity;

/// A throwaway project directory, removed when the test ends.
struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("py2deb-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("cannot create fixture");
        Self { path }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let target = self.path.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("cannot create parent");
        }
        fs::write(&target, contents).expect("cannot write fixture file");
        target
    }

    fn write_executable(&self, relative: &str, contents: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let target = self.write(relative, contents);
        let mut perms = fs::metadata(&target).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target, perms).unwrap();
        target
    }

    /// Builds the package and returns every path inside `data.tar.gz`.
    fn build(&self, package: Package) -> Vec<String> {
        let mut build = DebianBuild::new(package, self.path.clone())
            .with_verbosity(Verbosity::Quiet);
        let info = build.build().expect("build should succeed");

        entries(&info.deb_path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// `dpkg-deb -c` output, one line per archive member.
fn entries(deb: &Path) -> Vec<String> {
    let out = Command::new("dpkg-deb")
        .args(["-c".as_ref(), deb.as_os_str()])
        .output()
        .expect("dpkg-deb is required for these tests");
    assert!(out.status.success(), "dpkg-deb failed on {}", deb.display());

    String::from_utf8(out.stdout)
        .expect("dpkg-deb printed invalid UTF-8")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Whether any archive member's path ends with `suffix`.
fn has(entries: &[String], suffix: &str) -> bool {
    entries.iter().any(|line| {
        line.split_whitespace().last().is_some_and(|p| p.ends_with(suffix))
    })
}

fn base(name: &str) -> Package {
    let mut package = Package::new(name, "1.0.0", "Fixture package");
    package.maintainer = "Alice <alice@example.com>".into();
    // Nothing here is a git repository, and a changelog is not what is tested.
    package.changelog = String::new();
    package
}

#[test]
fn source_files_land_in_dist_packages() {
    let fixture = Fixture::new("src");
    fixture.write("src/module.py", "x = 1\n");

    let mut package = base("python3-fixture");
    package.dest = Some("fixture".into());

    let entries = fixture.build(package);
    assert!(
        has(&entries, "usr/lib/python3/dist-packages/fixture/module.py"),
        "{entries:#?}"
    );
}

#[test]
fn skip_packages_nothing_from_the_source_tree() {
    let fixture = Fixture::new("skip");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("dist/app.bin", "binary\n");

    let mut package = base("fixture-app");
    package.src = "$skip".into();
    package.include = vec![
        toml::from_str::<Wrap>(r#"i = [["dist/app.bin", "opt/app.bin", "0755"]]"#)
            .unwrap()
            .i
            .remove(0),
    ];

    let entries = fixture.build(package);
    assert!(has(&entries, "opt/app.bin"), "{entries:#?}");
    assert!(
        !entries.iter().any(|l| l.contains("dist-packages")),
        "source tree was packaged despite $skip: {entries:#?}"
    );
}

#[derive(serde::Deserialize)]
struct Wrap {
    i: Vec<py2deb::include_entry::IncludeEntry>,
}

#[derive(serde::Deserialize)]
struct WrapLinks {
    s: Vec<py2deb::symlink_entry::SymlinkEntry>,
}

#[test]
fn an_included_directory_is_copied_recursively() {
    let fixture = Fixture::new("incdir");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("assets/one.txt", "a\n");
    fixture.write("assets/icons/deep/two.png", "b\n");

    let mut package = base("python3-assets");
    package.dest = Some("assets_pkg".into());
    package.include = toml::from_str::<Wrap>(r#"i = [["assets", "usr/share/thing/"]]"#)
        .unwrap()
        .i;

    let entries = fixture.build(package);
    assert!(has(&entries, "usr/share/thing/assets/one.txt"), "{entries:#?}");
    assert!(
        has(&entries, "usr/share/thing/assets/icons/deep/two.png"),
        "nested file missing: {entries:#?}"
    );
}

#[test]
fn a_symlink_is_archived_as_a_link() {
    let fixture = Fixture::new("link");
    fixture.write("dist/app.bin", "binary\n");

    let mut package = base("fixture-link");
    package.src = "$skip".into();
    package.include = toml::from_str::<Wrap>(r#"i = [["dist/app.bin", "opt/app.bin", "0755"]]"#)
        .unwrap()
        .i;
    package.symlinks = toml::from_str::<WrapLinks>(r#"s = [["opt/app.bin", "usr/bin/app"]]"#)
        .unwrap()
        .s;

    let entries = fixture.build(package);
    let link = entries
        .iter()
        .find(|l| l.contains("usr/bin/app"))
        .unwrap_or_else(|| panic!("no link entry: {entries:#?}"));

    // dpkg-deb renders a symlink as `lrwxrwxrwx ... path -> target`.
    assert!(link.starts_with('l'), "not archived as a link: {link}");
    assert!(link.contains("-> /opt/app.bin"), "wrong target: {link}");
}

#[test]
fn a_build_script_runs_before_packaging() {
    let fixture = Fixture::new("buildok");
    fixture.write_executable(
        "debian/build",
        "#!/bin/sh\nset -e\nmkdir -p src\necho 'generated = True' > src/generated.py\n",
    );

    let mut package = base("python3-generated");
    package.dest = Some("generated".into());
    package.maintainer_scripts = "debian".into();

    // The file does not exist until the script has run.
    let entries = fixture.build(package);
    assert!(
        has(&entries, "dist-packages/generated/generated.py"),
        "build script output was not packaged: {entries:#?}"
    );
}

#[test]
fn a_failing_build_script_stops_the_build() {
    let fixture = Fixture::new("buildfail");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write_executable("debian/build", "#!/bin/sh\nexit 3\n");

    let mut package = base("python3-broken");
    package.dest = Some("broken".into());
    package.maintainer_scripts = "debian".into();

    let mut build = DebianBuild::new(package, fixture.path.clone())
        .with_verbosity(Verbosity::Quiet);
    let error = build.build().expect_err("a failing script must stop the build");

    assert!(
        format!("{error:#}").contains("status 3"),
        "the exit status should be reported: {error:#}"
    );
}

#[test]
fn a_non_executable_build_script_is_refused() {
    let fixture = Fixture::new("buildperm");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("debian/build", "#!/bin/sh\ntrue\n"); // no +x

    let mut package = base("python3-perm");
    package.dest = Some("perm".into());
    package.maintainer_scripts = "debian".into();

    let mut build = DebianBuild::new(package, fixture.path.clone())
        .with_verbosity(Verbosity::Quiet);
    let error = build.build().expect_err("a non-executable script must be refused");

    assert!(
        format!("{error:#}").contains("not executable"),
        "the reason should be clear: {error:#}"
    );
}

#[test]
fn no_maintainer_scripts_directory_is_not_an_error() {
    let fixture = Fixture::new("noscripts");
    fixture.write("src/module.py", "x = 1\n");

    let mut package = base("python3-plain");
    package.dest = Some("plain".into());

    let entries = fixture.build(package);
    assert!(has(&entries, "dist-packages/plain/module.py"), "{entries:#?}");
}

/// Writes a `.gitignore` and initialises a repository, so the ignore rules
/// the walk reads are the ones a real project would have.
fn init_repo(fixture: &Fixture, gitignore: &str) {
    fixture.write(".gitignore", gitignore);
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        let ok = Command::new("git")
            .current_dir(&fixture.path)
            .args(&args)
            .status()
            .expect("git is required for these tests")
            .success();
        assert!(ok, "git {args:?} failed");
    }
}

#[test]
fn bytecode_never_ships_from_the_source_tree() {
    // py3compile regenerates these on the target; the build machine's copies
    // would either conflict or match an interpreter that is not there.
    let fixture = Fixture::new("srcjunk");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("src/stale.pyc", "junk\n");
    fixture.write("src/__pycache__/module.cpython-311.pyc", "junk\n");

    let mut package = base("python3-srcjunk");
    package.dest = Some("srcjunk".into());

    let entries = fixture.build(package);
    assert!(has(&entries, "dist-packages/srcjunk/module.py"), "{entries:#?}");
    assert!(!has(&entries, ".pyc"), "bytecode shipped: {entries:#?}");
    assert!(
        !entries.iter().any(|l| l.contains("__pycache__")),
        "__pycache__ shipped: {entries:#?}"
    );
}

#[test]
fn bytecode_never_ships_from_an_included_directory() {
    // The same rule has to hold for includes, or junk simply moves house.
    let fixture = Fixture::new("incjunk");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("assets/keep.txt", "data\n");
    fixture.write("assets/stale.pyc", "junk\n");
    fixture.write("assets/__pycache__/x.cpython-311.pyc", "junk\n");

    let mut package = base("python3-incjunk");
    package.dest = Some("incjunk".into());
    package.include = toml::from_str::<Wrap>(r#"i = [["assets", "usr/share/thing/"]]"#)
        .unwrap()
        .i;

    let entries = fixture.build(package);
    assert!(has(&entries, "usr/share/thing/assets/keep.txt"), "{entries:#?}");
    assert!(!has(&entries, ".pyc"), "bytecode shipped: {entries:#?}");
    assert!(
        !entries.iter().any(|l| l.contains("__pycache__")),
        "__pycache__ shipped: {entries:#?}"
    );
}

#[test]
fn gitignored_files_are_left_out_of_includes() {
    let fixture = Fixture::new("incgitignore");
    init_repo(&fixture, "*.log\n");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("assets/keep.txt", "data\n");
    fixture.write("assets/debug.log", "noise\n");

    let mut package = base("python3-incignore");
    package.dest = Some("incignore".into());
    package.include = toml::from_str::<Wrap>(r#"i = [["assets", "usr/share/thing/"]]"#)
        .unwrap()
        .i;

    let entries = fixture.build(package);
    assert!(has(&entries, "usr/share/thing/assets/keep.txt"), "{entries:#?}");
    assert!(!has(&entries, "debug.log"), "gitignored file shipped: {entries:#?}");
}

#[test]
fn the_exclude_list_applies_to_includes() {
    let fixture = Fixture::new("incexclude");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("assets/keep.txt", "data\n");
    fixture.write("assets/notes.tmp", "scratch\n");

    let mut package = base("python3-incexclude");
    package.dest = Some("incexclude".into());
    package.exclude = vec!["*.tmp".into()];
    package.include = toml::from_str::<Wrap>(r#"i = [["assets", "usr/share/thing/"]]"#)
        .unwrap()
        .i;

    let entries = fixture.build(package);
    assert!(has(&entries, "usr/share/thing/assets/keep.txt"), "{entries:#?}");
    assert!(!has(&entries, "notes.tmp"), "excluded file shipped: {entries:#?}");
}

#[test]
fn a_directory_emptied_by_filtering_is_not_shipped() {
    // An empty stub directory is junk of its own kind.
    let fixture = Fixture::new("incempty");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("assets/keep.txt", "data\n");
    fixture.write("assets/cache/only.pyc", "junk\n");

    let mut package = base("python3-incempty");
    package.dest = Some("incempty".into());
    package.include = toml::from_str::<Wrap>(r#"i = [["assets", "usr/share/thing/"]]"#)
        .unwrap()
        .i;

    let entries = fixture.build(package);
    assert!(has(&entries, "usr/share/thing/assets/keep.txt"), "{entries:#?}");
    assert!(
        !entries.iter().any(|l| l.contains("assets/cache")),
        "empty directory shipped: {entries:#?}"
    );
}

#[test]
fn nesting_survives_filtering() {
    let fixture = Fixture::new("incnest");
    fixture.write("src/module.py", "x = 1\n");
    fixture.write("assets/deep/nested/real.txt", "keep\n");
    fixture.write("assets/deep/nested/stale.pyc", "junk\n");

    let mut package = base("python3-incnest");
    package.dest = Some("incnest".into());
    package.include = toml::from_str::<Wrap>(r#"i = [["assets", "usr/share/thing/"]]"#)
        .unwrap()
        .i;

    let entries = fixture.build(package);
    assert!(
        has(&entries, "usr/share/thing/assets/deep/nested/real.txt"),
        "nesting lost: {entries:#?}"
    );
    assert!(!has(&entries, ".pyc"), "bytecode shipped: {entries:#?}");
}

#[test]
fn the_build_script_never_writes_to_our_stdout() {
    // `DEB=$(py2deb build)` reads stdout, which must carry the package path
    // and nothing else — whatever the script decides to print.
    let fixture = Fixture::new("buildstdout");
    fixture.write_executable(
        "debian/build",
        "#!/bin/sh\nset -e\necho 'chatty script output'\nmkdir -p src\necho 'x = 1' > src/m.py\n",
    );
    fixture.write("pyproject.toml", &format!(
        "[tool.py2deb]\n\
         package = \"python3-stdout\"\n\
         version = \"1.0.0\"\n\
         maintainer = \"Alice <alice@example.com>\"\n\
         description = \"Fixture\"\n\
         src = \"src\"\n\
         dest = \"stdout_fixture\"\n\
         maintainer-scripts = \"debian\"\n"
    ));

    let out = Command::new(env!("CARGO_BIN_EXE_py2deb"))
        .args(["--path".as_ref(), fixture.path.as_os_str(), "build".as_ref()])
        .output()
        .expect("cannot run py2deb");

    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));

    let stdout = String::from_utf8(out.stdout).expect("stdout was not UTF-8");
    let stderr = String::from_utf8(out.stderr).expect("stderr was not UTF-8");

    assert!(
        !stdout.contains("chatty script output"),
        "the script polluted stdout: {stdout:?}"
    );
    assert!(
        stderr.contains("chatty script output"),
        "the script output went missing rather than to stderr: {stderr:?}"
    );

    // What remains must be exactly one line naming the built package.
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "stdout should be one line, got {lines:?}");
    assert!(lines[0].ends_with(".deb"), "stdout should name the package: {lines:?}");
}
