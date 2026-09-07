use anyhow::Result;

/// Debian package architecture.
///
/// Values match `dpkg --print-architecture` output, *not* `uname -m`:
/// dpkg calls 64-bit ARM `arm64` (never `aarch64`) and 32-bit x86 `i386`
/// (never `x86_64`/`i686`). See [`Architecture::from_str`] for the aliases
/// that get normalised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Architecture {
    /// Architecture-independent: pure Python, docs, data. The right choice
    /// for any package without compiled extension modules.
    All,
    /// Wildcard matching every architecture. Only valid in a source package's
    /// `Architecture:` field — a binary `.deb` must name a concrete value.
    Any,

    // Official release architectures.
    /// 64-bit x86 (`x86_64`).
    Amd64,
    /// 64-bit ARM (`aarch64`).
    Arm64,
    /// 32-bit ARM, hard-float ABI — Raspberry Pi OS, most modern ARM boards.
    Armhf,
    /// 32-bit ARM, soft-float ABI — older/embedded ARM.
    Armel,
    /// 32-bit x86 (`i686`).
    I386,
    /// 64-bit MIPS, little-endian.
    Mips64el,
    /// 32-bit MIPS, little-endian.
    Mipsel,
    /// 64-bit PowerPC, little-endian — IBM POWER8 and newer.
    Ppc64el,
    /// 64-bit IBM Z (s390x mainframe).
    S390x,
    /// 64-bit RISC-V, little-endian — an official release arch as of trixie.
    Riscv64,

    // Ports: not part of a stable release, but built by debian-ports.
    /// 64-bit Alpha.
    Alpha,
    /// 32-bit HP PA-RISC.
    Hppa,
    /// 64-bit LoongArch.
    Loong64,
    /// 68k Motorola.
    M68k,
    /// 64-bit PowerPC, big-endian.
    Ppc64,
    /// 32-bit PowerPC, big-endian.
    Powerpc,
    /// 64-bit SPARC.
    Sparc64,
    /// 32-bit SuperH, little-endian.
    Sh4,
    /// 64-bit x86 with 32-bit pointers (x32 ABI).
    X32,
}

impl Architecture {
    /// The canonical dpkg name, e.g. `"arm64"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Any => "any",
            Self::Amd64 => "amd64",
            Self::Arm64 => "arm64",
            Self::Armhf => "armhf",
            Self::Armel => "armel",
            Self::I386 => "i386",
            Self::Mips64el => "mips64el",
            Self::Mipsel => "mipsel",
            Self::Ppc64el => "ppc64el",
            Self::S390x => "s390x",
            Self::Riscv64 => "riscv64",
            Self::Alpha => "alpha",
            Self::Hppa => "hppa",
            Self::Loong64 => "loong64",
            Self::M68k => "m68k",
            Self::Ppc64 => "ppc64",
            Self::Powerpc => "powerpc",
            Self::Sparc64 => "sparc64",
            Self::Sh4 => "sh4",
            Self::X32 => "x32",
        }
    }

    /// Every architecture a binary `.deb` may legally declare, i.e. all of
    /// them except the source-only [`Architecture::Any`] wildcard.
    pub fn buildable() -> &'static [Architecture] {
        &[
            Self::All,
            Self::Amd64,
            Self::Arm64,
            Self::Armhf,
            Self::Armel,
            Self::I386,
            Self::Mips64el,
            Self::Mipsel,
            Self::Ppc64el,
            Self::S390x,
            Self::Riscv64,
            Self::Alpha,
            Self::Hppa,
            Self::Loong64,
            Self::M68k,
            Self::Ppc64,
            Self::Powerpc,
            Self::Sparc64,
            Self::Sh4,
            Self::X32,
        ]
    }

    /// Whether this value may appear in a binary package's `Architecture:`
    /// field. `any` may not — dpkg rejects the resulting `.deb`.
    pub fn is_valid_for_binary(&self) -> bool {
        !matches!(self, Self::Any)
    }
}

impl std::fmt::Display for Architecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Architecture {
    type Err = anyhow::Error;

    /// Parses a dpkg architecture name, case-insensitively, and normalises the
    /// `uname`/GNU spellings people reach for by habit (`aarch64` → `arm64`,
    /// `x86_64` → `amd64`, `ppc64le` → `ppc64el`, …).
    fn from_str(s: &str) -> Result<Self> {
        let normalised = s.trim().to_ascii_lowercase();
        Ok(match normalised.as_str() {
            "all" | "noarch" => Self::All,
            "any" => Self::Any,

            "amd64" | "x86_64" | "x86-64" => Self::Amd64,
            "arm64" | "aarch64" => Self::Arm64,
            "armhf" | "armv7l" | "armv7hl" => Self::Armhf,
            "armel" | "arm" => Self::Armel,
            "i386" | "i486" | "i586" | "i686" | "x86" => Self::I386,
            "mips64el" => Self::Mips64el,
            "mipsel" => Self::Mipsel,
            "ppc64el" | "ppc64le" => Self::Ppc64el,
            "s390x" => Self::S390x,
            "riscv64" => Self::Riscv64,

            "alpha" => Self::Alpha,
            "hppa" | "parisc" => Self::Hppa,
            "loong64" | "loongarch64" => Self::Loong64,
            "m68k" => Self::M68k,
            "ppc64" => Self::Ppc64,
            "powerpc" | "ppc" => Self::Powerpc,
            "sparc64" => Self::Sparc64,
            "sh4" => Self::Sh4,
            "x32" => Self::X32,

            other => anyhow::bail!(
                "unknown architecture `{other}`; expected one of: {}",
                Self::buildable()
                    .iter()
                    .map(Self::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
    }
}

impl<'de> serde::Deserialize<'de> for Architecture {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for Architecture {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
