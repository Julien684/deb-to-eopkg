//! `pack-eopkg` — construit le `.eopkg` installable de ce projet.
//!
//! Outil de développement : il empaquette le binaire `deb-to-eopkg` compilé
//! (ainsi que le README et la licence) dans un paquet Solus autonome, sans
//! dépendance d'exécution.
//!
//! Usage :
//!
//! ```text
//! pack-eopkg <binaire deb-to-eopkg> [dossier_de_sortie]
//! ```
//!
//! Produit `dossier_de_sortie/deb-to-eopkg-<version>-1-1-x86_64.eopkg`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use deb_to_eopkg::eopkg::{self, FileRecord, PackageInfo, TarSource};
use deb_to_eopkg::payload::PayloadWriter;
use deb_to_eopkg::TOOL_NAME;

const PACKAGE_NAME: &str = "deb-to-eopkg";
const PART_OF: &str = "system.utilities";
const PACKAGE_LICENSE: &str = "GPL-3.0";
/// Incrémenté à chaque republication d'une même version (eopkg met en cache
/// par nom de fichier).
const RELEASE: &str = "1";
// Métadonnées volontairement en ASCII : eopkg affiche les champs via un flux
// encodé ASCII quand la locale n'est pas UTF-8, et lève alors une
// UnicodeEncodeError sur un caractère accentué (vérifié sur Solus 4.9).
const HISTORY_COMMENT: &str = "Rust rewrite of the conversion tool.";
const SUMMARY: &str = "Convert a Debian .deb package into a Solus .eopkg package.";
const DESCRIPTION: &str = "\
deb-to-eopkg is a small Rust tool that converts a Debian .deb package into an \
installable Solus .eopkg package. It reads the ar+tar structure of the .deb \
directly (no dpkg-deb) - data.tar compressed with gzip, xz, bzip2, lzma or zstd \
- and writes a conformant .eopkg (ZIP of metadata.xml, files.xml and \
install.tar.xz), preserving file types, modes and symlinks/hardlinks. Static \
binary, no runtime dependencies. Usage: deb-to-eopkg package.deb destination/";

/// Documentation embarquée dans l'exécutable de packaging : le paquet produit
/// est donc toujours celui du checkout courant.
const README: &[u8] = include_bytes!("../../README.md");
const LICENSE: &[u8] = include_bytes!("../../LICENSE");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("usage : pack-eopkg <binaire deb-to-eopkg> [dossier_de_sortie]");
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("{TOOL_NAME} pack-eopkg {}", deb_to_eopkg::VERSION);
        return ExitCode::SUCCESS;
    }

    let Some(binary) = args.first() else {
        eprintln!("pack-eopkg : erreur : chemin du binaire absent.");
        eprintln!("usage : pack-eopkg <binaire deb-to-eopkg> [dossier_de_sortie]");
        return ExitCode::from(2);
    };
    let out_dir = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("dist"));

    match run(Path::new(binary), &out_dir) {
        Ok(path) => {
            println!("{path}", path = path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("pack-eopkg : erreur : {err}");
            ExitCode::from(1)
        }
    }
}

fn run(binary: &Path, out_dir: &Path) -> std::io::Result<PathBuf> {
    let bin = std::fs::read(binary)?;

    // Le chargeur install.tar.xz : binaire + documentation + licence.
    let payload: [(&str, &[u8], u32); 3] = [
        ("usr/bin/deb-to-eopkg", &bin, 0o755),
        (
            "usr/share/doc/deb-to-eopkg/README.md",
            README,
            0o644,
        ),
        (
            "usr/share/licenses/deb-to-eopkg/LICENSE",
            LICENSE,
            0o644,
        ),
    ];

    // Tar xz en mémoire : le paquet complet pèse quelques mégaoctets.
    let xz = xz2::write::XzEncoder::new(Vec::new(), 6);
    let mut pw = PayloadWriter::new(xz);
    let mut records = Vec::new();
    for (path, data, mode) in payload {
        pw.add_regular(path, mode, data.len() as u64, data)?;
        records.push(FileRecord {
            path: path.to_string(),
            ftype: eopkg::classify_type(path),
            size: data.len() as u64,
            uid: 0,
            gid: 0,
            mode,
            hash: sha1_hex(data),
        });
    }
    let tar_xz = pw.finish()?.finish()?;

    let mut info = PackageInfo::new(PACKAGE_NAME, deb_to_eopkg::VERSION, "x86_64");
    info.release = RELEASE.to_string();
    info.summary = SUMMARY.to_string();
    info.description = DESCRIPTION.to_string();
    info.part_of = PART_OF.to_string();
    info.license = PACKAGE_LICENSE.to_string();
    info.history_comment = HISTORY_COMMENT.to_string();

    let out = out_dir.join(eopkg::eopkg_filename(
        &info.name,
        &info.version,
        &info.release,
        &info.arch,
    ));
    eopkg::write_eopkg(&out, &info, &records, TarSource::Bytes(&tar_xz))?;
    Ok(out)
}

fn sha1_hex(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
