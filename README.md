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
| `changelog` | no | `$git` / `$git(N)` to build one from git, or a path to a changelog written by hand. |
| `distribution` | no | Suite the entries are released to, default `unstable`. |
| `urgency` | no | `low`, `medium`, `high`, `emergency` or `critical`, default `medium`. |

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

- **Tests.** Seventeen integration tests under `tests/` cover changelog
  rendering, the `$git` directive and placeholder expansion. Archive layout,
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