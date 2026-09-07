# py2deb

Build Debian packages from Python projects, configured entirely from `pyproject.toml`.

`py2deb` reads a `[tool.py2deb]` section, collects your Python sources, and writes a
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

`py2deb` takes the other approach: describe the package in eight lines of TOML
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

Builds are reproducible: file timestamps come from `SOURCE_DATE_EPOCH` when it is
set, and all archive members are owned by `root:root`, so the same sources produce
a byte-identical `.deb`.

## Verifying output

```sh
dpkg-deb -I package.deb    # control metadata
dpkg-deb -c package.deb    # file listing with permissions
lintian package.deb        # policy checks
```

## Acknowledgements

`py2deb` is inspired by [cargo-deb](https://github.com/kornelski/cargo-deb),
which packages Rust binaries straight from `Cargo.toml`. Its central idea —
that a `.deb` should be described in the manifest a project already has, rather
than in a parallel `debian/` directory — is the one this project borrows and
applies to Python. `cargo-deb` builds the `.deb` for `py2deb` itself.

## License

GPL-3.0. See [LICENSE](LICENSE).
