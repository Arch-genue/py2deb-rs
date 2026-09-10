# py2deb

Build Debian packages from Python projects, configured entirely from `pyproject.toml`.

`py2deb` reads a `[tool.py2deb]` section, collects Python sources, and builds a
`.deb` — no `debian/` directory, no `debhelper`, no build dependencies beyond the
binary itself. Archives are assembled in-process, so packaging works the same on any
machine with or without `dpkg-dev` installed.

> **Status: early development.** The configuration format is settled; archive
> generation is still being built out. Not yet published to crates.io.

## Why

Packaging a small Python library for Debian usually means a `debian/` directory,
a `rules` makefile, a changelog with its own version syntax, and a toolchain that
only runs on Debian. For an internal library shipped to a handful of machines,
that is a lot of ceremony.

`py2deb` takes the other approach: describe the package in some lines of TOML
next to the code it describes, and get a `.deb` out.

## Install

```sh
cargo install --path .
```

## Usage

Initialise a configuration in the current project:

```sh
py2deb init
```

This writes a `[tool.py2deb]` section into `pyproject.toml`, inferring the package
name from the project directory. If the section already exists, `py2deb` prints the
current configuration and leaves the file untouched.

Build the package:

```sh
py2deb build
```

The resulting `.deb` is named by Debian convention —
`<package>_<version>_<architecture>.deb`.

Add `--git-version` to stamp the build with its git position, which is what CI
wants; see [Versioning from git](#versioning-from-git).

## Configuration

```toml
[tool.py2deb]
package = "python3-libgkeyboard"
version = "0.1"
arch = "all"
maintainer = "Vlad Kartsaev <vkarcaev@gmail.com>"
dependencies = [
    "python3-libgvcp",
    "python3-evdev",
]
```

| Field | Required | Description |
| --- | --- | --- |
| `package` | yes | Debian package name. Lowercase, `[a-z0-9][a-z0-9+.-]+`; library packages are conventionally prefixed `python3-`. |
| `version` | yes | Upstream version, optionally with a Debian revision (`0.1-2`). |
| `arch` | no | Target architecture, default `all`. |
| `maintainer` | no | RFC 822 form: `Name <email>`. Falls back to git config. |
| `dependencies` | no | Debian package names for the `Depends` field. |
| `description` | no | Synopsis on the first line, extended description after a blank line. |
| `description_file` | no | Path to a file holding the description, default `README.md`. Used when `description` is empty. |
| `symlinks` | no | `[target, link]` pairs, in `ln -s` order. |
| `maintainer-scripts` | no | Directory holding maintainer scripts; a `build` script there runs before packaging. |
| `changelog` | no | `$git` / `$git(N)` to build one from git, or a path to a changelog written by hand. |
| `distribution` | no | Suite the entries are released to, default `unstable`. |
| `urgency` | no | `low`, `medium`, `high`, `emergency` or `critical`, default `medium`. |

### Build script

`maintainer-scripts` names a directory of scripts. A `build` script found there
runs **before** the package is assembled, with the project as its working
directory:

```toml
maintainer-scripts = "debian"
```

```sh
#!/bin/sh
set -e
pyinstaller --onefile src/main.py     # whatever has to happen first
```

A non-zero exit stops the packaging — continuing would ship whatever stale
files happened to be on disk. The script must be executable; `py2deb` refuses
rather than guessing if it is not.

Both of the script's streams go to stderr, unbuffered, so a long build stays
visible as it runs. Its stdout is redirected there rather than inherited,
because `py2deb`'s own stdout carries the path of the finished package and
nothing else — `DEB=$(py2deb build)` keeps working whatever the script prints.

Three variables are set for it: `PY2DEB_PACKAGE`, `PY2DEB_VERSION` and
`PY2DEB_PROJECT`.

The source directory is checked *after* the script runs, so `src` may be
something the script generates.

### Packaging files other than a Python module

`src = "$skip"` skips the copy into `dist-packages` entirely, leaving `include`
to say what the package contains. This is what a package wraps a binary or an
AppImage with, rather than an importable module:

```toml
src = "$skip"
include = [
    ["dist/gvcp.AppImage", "opt/gvcp.AppImage", "0755"],
]
symlinks = [
    ["opt/gvcp.AppImage", "usr/bin/gvcp"],
]
```

An `include` source may be a file or a directory; a directory is copied
recursively, reproducing its tree under the destination the way `cp -r` would.
A destination ending in `/` keeps the source name, anything else renames.

### What never gets packaged

The source tree and every included directory are filtered by the same three
rules, because a file that is junk in one is junk in the other:

1. `.gitignore` — what the project already declares as not-source;
2. `exclude` from the config, for what is tracked but should not ship;
3. a built-in list of build residue.

The built-in list covers `__pycache__/`, `*.pyc`/`*.pyo`, `*.egg-info/`, VCS
metadata (`.git/`, `.hg/`, `.svn/`), tool caches (`.mypy_cache/`,
`.pytest_cache/`, `.ruff_cache/`, `.tox/`, `.coverage`) and editor droppings
(`.DS_Store`, `*.swp`, `*~`).

Bytecode is the one that matters rather than merely tidying: `py3compile`
regenerates `.pyc` files on the target at install time, so shipping the build
machine's copies means either a conflict or files compiled for an interpreter
that is not installed there.

A directory left empty by filtering is dropped rather than shipped as a stub.

### Symlinks

`symlinks` takes `[target, link]` pairs, in the order `ln -s` reads them, so
the entry above puts a `gvcp` command on the path pointing at the AppImage.

Whether the link is written absolute or relative is decided per Policy 10.5,
which lintian enforces from both sides: links crossing between top-level
directories are absolute (`usr/bin/gvcp -> /opt/gvcp.AppImage`), links within
one are relative (`usr/bin/gvcp-run -> ../lib/gvcp/run`). A target written
absolute in the config is kept that way.

Links are archive entries in their own right — dpkg creates them at unpack
time — so they carry no md5sum and add nothing to `Installed-Size`.

### Versioning from git

`version` in `pyproject.toml` is what the package is normally built as. Passing
`--git-version` appends git's position to it, so that every build from an
untagged commit gets a version of its own:

```sh
py2deb build --git-version     # 1.1.8  ->  1.1.8+3.gabc1234
```

The suffix is the number of commits since the newest version tag, then the
abbreviated hash. Sitting exactly on a clean tag leaves the version alone —
that build *is* the release. An uncommitted change adds `.dirty`, because the
result is not reproducible from the hash.

This is for CI, where a rebuild of the same `version` would otherwise collide
with what is already published — an APT registry rejects a second upload of a
version it already has (Gitea answers `409 Conflict`), since `apt` decides what
to upgrade by comparing versions, not contents.

The ordering it produces is the one dpkg agrees with:

```
1.1.8  <  1.1.8+3.gabc1234  <  1.1.8+12.gdef5678  <  1.1.9
```

The commit count leads the suffix deliberately — dpkg compares runs of digits
numerically, so `+12.` outranks `+3.`, whereas two hashes on their own would
only sort by spelling. A `+` suffix sorts above the bare version and below the
next upstream one, so a development build upgrades cleanly in both directions.

`--git-version` fails rather than guesses outside a repository: falling back to
the plain version would produce exactly the collision the flag exists to avoid.
The changelog picks up the resulting version automatically.

### Description

Debian's `Description:` is two things in one field: a one-line synopsis, and an
extended description indented beneath it. `description` supplies both — the
first line is the synopsis, anything after a blank line is the extended part:

```toml
description = """
Keyboard layout switcher for Linux

Works on Wayland and on the bare console, with per-application layout memory.
"""
```

`description_file` reads the same thing from a file instead, and defaults to
`README.md`. A README is written for a different audience, so its Markdown is
reduced to prose first: headings, tables, horizontal rules and fenced code are
dropped, badges leave nothing behind, links keep their text, and emphasis
markers are removed.

Policy caps the synopsis at 80 characters including the `Description: ` prefix,
which an opening paragraph usually exceeds. The first sentence becomes the
synopsis and the remainder moves into the extended description, so nothing is
lost. A leading article and a trailing full stop are trimmed — lintian objects
to both.

`description` takes precedence when both are set. A file that cannot be read is
a warning, not a build failure.

### Changelog

Debian expects `/usr/share/doc/<package>/changelog.gz`, and a changelog is a
history of *releases* rather than of commits. `changelog = "$git"` builds one
from the repository on that understanding:

- Every tag that names a version — `v1.2.3` or `1.2.3` — becomes one entry.
  Tags that are not versions (`nightly`, `latest`) are ignored rather than
  turned into nonsense entries.
- The commits reachable from a tag but not its predecessor become that entry's
  bullet points, subject first and body indented beneath it. Git trailers
  (`Signed-off-by:`, `Co-Authored-By:`) are dropped — they record who touched a
  commit, not what changed.
- Commits made after the newest tag belong to the version being built, which is
  the entry at the top. It is dated with the build time, so it always sorts
  above the release below it.
- An annotated tag supplies its own tagger and date, which is the true moment
  of release; a lightweight tag falls back to the commit it points at.

`$git(N)` keeps only the newest N releases, for repositories whose history is
longer than the package deserves. Tags are ordered the way git orders versions,
so `v1.10.0` correctly sorts above `v1.9.0`.

Anything that is not a `$git` directive is read as a path and copied verbatim,
so a project maintaining its own changelog keeps it. If the directory is not a
git repository, or history cannot be read, the build warns and ships no
changelog rather than failing — the package is installable without one.

The output is checked against `dpkg-parsechangelog` and `lintian`.

### Architecture

`arch` accepts any dpkg architecture — `amd64`, `arm64`, `armhf`, `i386`,
`riscv64`, `s390x`, `ppc64el` and the rest, plus `all`. Common `uname`
spellings are normalised, so `aarch64` becomes `arm64` and `x86_64` becomes
`amd64` rather than being rejected. Values are case-insensitive.

Leave it at `all` for pure Python. Packages containing compiled extension
modules must name a concrete architecture.

### Dependencies

Dependencies are Debian package names, not PyPI ones. The mapping between the two
is not mechanical — `PyYAML` is packaged as `python3-yaml`, `Pillow` as
`python3-pil`, and plenty of PyPI distributions have no Debian counterpart at all.
`py2deb` therefore takes the names verbatim and does not attempt to translate
`[project.dependencies]`.

## How it works

Python sources are installed to `/usr/lib/python3/dist-packages/`, the
version-independent path Debian puts library packages on. A package built this way
keeps working across Python point releases, which is what makes `arch = "all"`
viable for pure-Python code.

Bytecode is deliberately not shipped. `.pyc` files are generated on the target
machine by `py3compile` from the maintainer scripts, so they always match the
interpreter actually installed there, and are removed again on uninstall.

## Verifying output

```sh
dpkg-deb -I package.deb    # control metadata
dpkg-deb -c package.deb    # file listing with permissions
lintian package.deb        # policy checks
```

## Roadmap to 1.0

What still stands between the current state and a release that behaves
correctly on someone else's machine.

### Correctness

- **Debian revisions.** `version = "1.1.8-2"` is not understood yet, so every
  package is built as native. Supporting a revision means parsing the version
  into upstream and revision parts, naming the changelog `changelog.Debian.gz`
  rather than `changelog.gz` for non-native packages, and validating the
  version against Debian's comparison rules (`~` sorts before nothing, digits
  compare numerically).
- **`conffiles`.** Files installed under `/etc` must be listed in the control
  archive, or dpkg silently overwrites whatever the user edited on upgrade.
- **`Architecture` detection.** A package containing compiled extension
  modules cannot be `all`; the build should notice `.so` files and refuse, or
  set the concrete architecture itself.
- **`priority` validation.** Only `required`, `important`, `standard`,
  `optional` and `extra` are legal values.

### Reliability

- **Tests.** Sixty-five integration tests under `tests/` cover changelog
  rendering, the `$git` directive, description handling, git versioning,
  symlinks, exclude filtering, placeholder expansion, and end-to-end packaging
  read back with `dpkg-deb`. Archive layout,
  control generation and the config parser are still uncovered; a test that
  builds a fixture package and inspects it with `dpkg-deb` would have caught
  most of what was found by hand during development.
- **`lintian` in CI.** The checks that matter are the ones a Debian archive
  would run.
- **Error paths.** A few `unwrap()` calls remain where the failure is
  plausible rather than impossible. The changelog reader no longer among
  them.

### Usability

- **`py2deb init` defaults.** The generated `maintainer` placeholder does not
  pass the tool's own validation; it should come from `DEBEMAIL`/`DEBFULLNAME`
  or `git config`, the way `dh_make` does.
- **Build hooks.** A command run before the archives are assembled, for
  projects that are not plain Python — compiling Cython extensions, generating
  Qt resources or `.ui` files, compiling translations. Whatever it writes into
  the source tree is picked up by the normal walk, so the package stays a
  description of the result rather than of the build.
- **Custom maintainer script fragments.** `postinst` and `prerm` are generated
  for `py3compile`/`py3clean`; a project should be able to add its own code —
  `update-alternatives`, `systemctl enable`, a cache rebuild — without losing
  the generated part. Appending fragments rather than replacing the file keeps
  both.
- **Changelog from git.** Generation from commit history exists; it still needs
  a decision on how versions map to tags, and what to do with merge commits and
  fixup messages.
- **`install-path`.** `/usr/lib/python3/dist-packages` is currently hardcoded.
- **Library API.** `py2deb` is a binary crate today; the archive-building code
  is worth exposing as a library, which is what the README already implies.

## Acknowledgements

`py2deb` is inspired by [cargo-deb](https://github.com/kornelski/cargo-deb),
which packages Rust binaries straight from `Cargo.toml`. Its central idea —
that a `.deb` should be described in the manifest a project already has, rather
than in a parallel `debian/` directory — is the one this project borrows and
applies to Python. `cargo-deb` builds the `.deb` for `py2deb` itself.