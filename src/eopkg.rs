//! Le format eopkg : `metadata.xml`, `files.xml` et l'assemblage du ZIP à
//! trois membres — dans cet ordre, ce qui compte pour eopkg.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::zipw::ZipWriter;

// ---------------------------------------------------------------- constantes

pub const DIST: &str = "Solus";
pub const DIST_RELEASE: &str = "1";
pub const BUILD_HOST: &str = "solus-local";
pub const PKG_FORMAT: &str = "1.2";
pub const PACKAGER_NAME: &str = "Julien";
pub const PACKAGER_EMAIL: &str = "julien@example.com";
pub const DEFAULT_HOMEPAGE: &str = "https://example.com";
pub const DEFAULT_LICENSE: &str = "BSD-3-Clause";
/// Date portée par le premier `<Update>` de l'historique.
pub const HISTORY_DATE: &str = "2026-10-12";

// -------------------------------------------------------------------- types

/// Un fichier embarqué, tel que décrit dans `files.xml`.
#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub ftype: &'static str,
    pub size: u64,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    /// SHA1 hexadécimal des octets du fichier ; vide pour un lien symbolique.
    pub hash: String,
}

/// Une dépendance d'exécution (`<Dependency>`). Normalement absente : les noms
/// de paquets Debian ne correspondent pas à ceux de Solus.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub constraint: Option<String>,
}

/// Tout ce qui alimente `metadata.xml`.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub summary: String,
    pub description: String,
    pub part_of: String,
    pub license: String,
    pub homepage: String,
    pub packager_name: String,
    pub packager_email: String,
    pub build_host: String,
    pub dist: String,
    pub dist_release: String,
    pub format: String,
    pub history_date: String,
    pub history_comment: String,
    pub deps: Vec<Dependency>,
}

impl PackageInfo {
    /// Métadonnées par défaut du projet pour un paquet donné.
    pub fn new(name: &str, version: &str, arch: &str) -> PackageInfo {
        PackageInfo {
            name: name.to_string(),
            version: version.to_string(),
            release: "1".to_string(),
            arch: arch.to_string(),
            summary: name.to_string(),
            description: name.to_string(),
            part_of: "system.utilities".to_string(),
            license: DEFAULT_LICENSE.to_string(),
            homepage: DEFAULT_HOMEPAGE.to_string(),
            packager_name: PACKAGER_NAME.to_string(),
            packager_email: PACKAGER_EMAIL.to_string(),
            build_host: BUILD_HOST.to_string(),
            dist: DIST.to_string(),
            dist_release: DIST_RELEASE.to_string(),
            format: PKG_FORMAT.to_string(),
            history_date: HISTORY_DATE.to_string(),
            history_comment: "Converted from Debian package".to_string(),
            deps: Vec::new(),
        }
    }

    /// Métadonnées déduites des champs de contrôle d'un `.deb`.
    pub fn from_control(
        fields: &BTreeMap<String, String>,
        name: &str,
        version: &str,
        arch: &str,
    ) -> PackageInfo {
        let mut info = PackageInfo::new(name, version, arch);

        let description = fields
            .get("Description")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        info.summary = summary_of(&description, name);
        info.description = if description.is_empty() {
            name.to_string()
        } else {
            description
        };

        if let Some(h) = non_empty(fields, "Homepage") {
            info.homepage = h;
        }
        if let Some(l) = non_empty(fields, "License") {
            info.license = l;
        }
        if let Some(s) = non_empty(fields, "Section") {
            // Une section Debian peut être hiérarchique (`utils/misc`) ; eopkg
            // attend une forme pointée.
            info.part_of = s.replace('/', ".");
        }
        info
    }
}

fn non_empty(fields: &BTreeMap<String, String>, key: &str) -> Option<String> {
    fields
        .get(key)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

/// Résumé = première ligne de la description, plafonnée à 200 caractères.
fn summary_of(description: &str, fallback: &str) -> String {
    let first = description.lines().next().unwrap_or("").trim();
    let s: String = first.chars().take(200).collect();
    if s.is_empty() {
        fallback.to_string()
    } else {
        s
    }
}

// ---------------------------------------------------------------- heuristiques

/// Type eopkg d'un fichier d'après son chemin.
pub fn classify_type(path: &str) -> &'static str {
    for p in ["usr/bin/", "bin/", "usr/sbin/", "sbin/", "usr/libexec/"] {
        if path.starts_with(p) {
            return "executable";
        }
    }
    if path.starts_with("usr/include/") {
        return "header";
    }
    if path.starts_with("usr/lib/") || path.starts_with("usr/lib64/") {
        return "library";
    }
    if path.starts_with("usr/share/man/") {
        return "man";
    }
    if path.starts_with("usr/share/info/") {
        return "info";
    }
    if path.starts_with("usr/share/locale/") {
        return "localedata";
    }
    if path.starts_with("usr/share/doc/")
        || path.starts_with("usr/share/gtk-doc/")
        || path.starts_with("usr/share/help/")
    {
        return "doc";
    }
    if path.starts_with("etc/") {
        return "config";
    }
    "data"
}

/// Traduit une architecture Debian en architecture eopkg.
pub fn eopkg_arch(deb_arch: &str) -> String {
    match deb_arch {
        "all" => "noarch",
        "amd64" => "x86_64",
        "i386" | "i686" => "i586",
        "arm64" => "aarch64",
        "armhf" => "armv7l",
        "armel" => "armv6l",
        "ppc64el" => "ppc64le",
        _ => deb_arch,
    }
    .to_string()
}

/// Normalise une version Debian en version eopkg (`X.Y.Z`).
///
/// eopkg refuse les versions à plus de trois composants numériques : un
/// `1.2.3-1` produirait le nom de fichier invalide `…-1.2.3-1-1-…`. On garde
/// donc la partie amont (avant la révision Debian) et les chiffres de tête de
/// chaque composant, complétés à trois composants par des zéros.
pub fn normalize_version(deb_version: &str) -> String {
    // On retire d'abord l'epoch éventuel (`1:2.47.3-1`), puis la révision
    // Debian (tout ce qui suit le premier `-`).
    let without_epoch = match deb_version.split_once(':') {
        Some((_, rest)) => rest,
        None => deb_version,
    };
    let upstream = without_epoch.split('-').next().unwrap_or("").trim();
    let mut parts: Vec<String> = upstream
        .split('.')
        .map(|chunk| {
            let digits: String = chunk.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                "0".to_string()
            } else {
                digits
            }
        })
        .collect();
    while parts.len() < 3 {
        parts.push("0".to_string());
    }
    parts.truncate(3);
    parts.join(".")
}

/// Nom de fichier eopkg usuel : `nom-version-release-1-arch.eopkg`.
pub fn eopkg_filename(name: &str, version: &str, arch: &str) -> String {
    format!("{name}-{version}-1-1-{arch}.eopkg")
}

// ------------------------------------------------------------------------ XML

/// Échappe les caractères significatifs en XML (comme le faisait l'outil
/// d'origine : `&`, `<`, `>`, `"`).
pub fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn build_files_xml(files: &[FileRecord]) -> String {
    let mut out = String::from("<Files>\n");
    for f in files {
        out.push_str("  <File>\n");
        out.push_str(&format!("    <Path>{}</Path>\n", xml_escape(&f.path)));
        out.push_str(&format!("    <Type>{}</Type>\n", f.ftype));
        out.push_str(&format!("    <Size>{}</Size>\n", f.size));
        out.push_str(&format!("    <Uid>{}</Uid>\n", f.uid));
        out.push_str(&format!("    <Gid>{}</Gid>\n", f.gid));
        out.push_str(&format!("    <Mode>{:04o}</Mode>\n", f.mode));
        out.push_str(&format!("    <Hash>{}</Hash>\n", f.hash));
        out.push_str("  </File>\n");
    }
    out.push_str("</Files>\n");
    out
}

pub fn build_metadata_xml(info: &PackageInfo, installed_size: u64) -> String {
    let deps = if info.deps.is_empty() {
        String::new()
    } else {
        let mut s = String::from("        <RuntimeDependencies>\n");
        for d in &info.deps {
            match &d.constraint {
                Some(c) => s.push_str(&format!(
                    "            <Dependency name=\"{}\" constraint=\"{}\" />\n",
                    xml_escape(&d.name),
                    xml_escape(c)
                )),
                None => s.push_str(&format!(
                    "            <Dependency name=\"{}\" />\n",
                    xml_escape(&d.name)
                )),
            }
        }
        s.push_str("        </RuntimeDependencies>");
        s
    };

    format!(
        r#"<PISI>
    <Source>
        <Name>{name}</Name>
        <Homepage>{homepage}</Homepage>
        <Packager>
            <Name>{pname}</Name>
            <Email>{pemail}</Email>
        </Packager>
    </Source>
    <Package>
        <Name>{name}</Name>
        <Summary xml:lang="en">{summary}</Summary>
        <Description xml:lang="en">{desc}</Description>
        <PartOf>{partof}</PartOf>
        <License>{license}</License>
{deps}
        <History>
            <Update release="{release}">
                <Date>{date}</Date>
                <Version>{version}</Version>
                <Comment>{comment}</Comment>
                <Name>{pname}</Name>
                <Email>{pemail}</Email>
            </Update>
        </History>
        <BuildHost>{bhost}</BuildHost>
        <Distribution>{dist}</Distribution>
        <DistributionRelease>{distrel}</DistributionRelease>
        <Architecture>{arch}</Architecture>
        <InstalledSize>{size}</InstalledSize>
        <PackageFormat>{fmt}</PackageFormat>
        <Source>
            <Name>{name}</Name>
            <Homepage>{homepage}</Homepage>
            <Packager>
                <Name>{pname}</Name>
                <Email>{pemail}</Email>
            </Packager>
        </Source>
    </Package>
</PISI>
"#,
        name = xml_escape(&info.name),
        homepage = xml_escape(&info.homepage),
        pname = xml_escape(&info.packager_name),
        pemail = xml_escape(&info.packager_email),
        summary = xml_escape(&info.summary),
        desc = xml_escape(&info.description),
        partof = xml_escape(&info.part_of),
        license = xml_escape(&info.license),
        deps = deps,
        release = xml_escape(&info.release),
        date = xml_escape(&info.history_date),
        version = xml_escape(&info.version),
        comment = xml_escape(&info.history_comment),
        bhost = xml_escape(&info.build_host),
        dist = xml_escape(&info.dist),
        distrel = xml_escape(&info.dist_release),
        arch = xml_escape(&info.arch),
        size = installed_size,
        fmt = xml_escape(&info.format),
    )
}

// ------------------------------------------------------------------ assemblage

/// D'où vient le chargeur `install.tar.xz`.
pub enum TarSource<'a> {
    /// Un fichier temporaire (l'archive est recopiée en flux).
    File(&'a Path),
    /// Des octets déjà en mémoire.
    Bytes(&'a [u8]),
}

/// Assemble le `.eopkg` final.
///
/// L'ordre des membres — `metadata.xml`, `files.xml`, `install.tar.xz` — est
/// celui qu'écrit eopkg ; on le respecte à la lettre.
pub fn write_eopkg(
    out: &Path,
    info: &PackageInfo,
    files: &[FileRecord],
    tar: TarSource<'_>,
) -> io::Result<()> {
    let installed_size: u64 = files.iter().map(|f| f.size).sum();
    let files_xml = build_files_xml(files);
    let metadata_xml = build_metadata_xml(info, installed_size);

    if let Some(dir) = out.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }

    let mut z = ZipWriter::new(BufWriter::new(File::create(out)?));
    z.add_deflated("metadata.xml", metadata_xml.as_bytes())?;
    z.add_deflated("files.xml", files_xml.as_bytes())?;
    match tar {
        TarSource::File(p) => z.add_stored_file("install.tar.xz", p)?,
        TarSource::Bytes(b) => z.add_stored("install.tar.xz", b)?,
    }
    z.finish()?.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_normalisees() {
        assert_eq!(normalize_version("1.2.3-1"), "1.2.3");
        assert_eq!(normalize_version("1.2"), "1.2.0");
        assert_eq!(normalize_version("5"), "5.0.0");
        assert_eq!(normalize_version("2.0~rc1-2"), "2.0.0");
        assert_eq!(normalize_version("1.2.3.4"), "1.2.3");
        assert_eq!(normalize_version("1:2.47.3-0+deb13u1"), "2.47.3");
        assert_eq!(normalize_version(""), "0.0.0");
    }

    #[test]
    fn architectures() {
        assert_eq!(eopkg_arch("amd64"), "x86_64");
        assert_eq!(eopkg_arch("all"), "noarch");
        assert_eq!(eopkg_arch("arm64"), "aarch64");
        assert_eq!(eopkg_arch("riscv64"), "riscv64");
    }

    #[test]
    fn types_de_fichiers() {
        assert_eq!(classify_type("usr/bin/tool"), "executable");
        assert_eq!(classify_type("usr/share/man/man1/t.1.gz"), "man");
        assert_eq!(classify_type("etc/conf"), "config");
        assert_eq!(classify_type("opt/App/app"), "data");
        assert_eq!(classify_type("usr/lib/libfoo.so"), "library");
    }

    #[test]
    fn echappement_xml() {
        assert_eq!(xml_escape("a<b>&\"c\""), "a&lt;b&gt;&amp;&quot;c&quot;");
    }

    #[test]
    fn resume_depuis_description() {
        assert_eq!(summary_of("Court\nLong", "x"), "Court");
        assert_eq!(summary_of("", "fallback"), "fallback");
    }

    #[test]
    fn nom_de_fichier() {
        assert_eq!(
            eopkg_filename("equibop", "3.3.1", "x86_64"),
            "equibop-3.3.1-1-1-x86_64.eopkg"
        );
    }
}
