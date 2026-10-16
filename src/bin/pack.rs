//! `pack-eopkg` — construit le `.eopkg` installable de ce projet.
//!
//! Outil de développement : il empaquette le binaire `deb-to-eopkg` compilé
//! (ainsi que le README et la licence) dans un paquet Solus autonome, sans
//! dépendance d'exécution.
//!
//! Usage :
//!
//! ```text
//! pack-eopkg [--release N] <binaire deb-to-eopkg> [dossier_de_sortie]
//! ```
//!
//! Produit `dossier_de_sortie/deb-to-eopkg-<version>-<release>-1-x86_64.eopkg`.
//!
//! Le *release* doit **strictement augmenter** à chaque publication. C'est le
//! seul champ qui compte pour eopkg : `pisi/operations/upgrade.py` compare
//! `int(release_installe) < int(release_depot)` et ne regarde jamais la chaîne
//! de version (`pisi/db/packagedb.py` lit ce numéro depuis l'attribut
//! `release` de la balise `<Update>` de `<History>`). Republier une version
//! avec un release inchangé rend le paquet invisible à `eopkg up`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use deb_to_eopkg::eopkg::{self, FileRecord, PackageInfo, TarSource};
use deb_to_eopkg::payload::PayloadWriter;
use deb_to_eopkg::TOOL_NAME;

const PACKAGE_NAME: &str = "deb-to-eopkg";
const PART_OF: &str = "system.utilities";
const PACKAGE_LICENSE: &str = "GPL-3.0";
/// Release par défaut. **À incrémenter à chaque publication** : eopkg ne
/// compare que ce numéro (voir le commentaire en tête de fichier). Surchargeable
/// par `--release N`.
const RELEASE: &str = "6";
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
        println!(
            "usage : pack-eopkg [--release N] <binaire deb-to-eopkg> [dossier_de_sortie]\n\
             \n\
             Le release doit augmenter à chaque publication, sinon `eopkg up`\n\
             ne verra jamais le paquet (eopkg ne compare que ce numéro)."
        );
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("{TOOL_NAME} pack-eopkg {}", deb_to_eopkg::VERSION);
        return ExitCode::SUCCESS;
    }

    // `--release N` (ou `--release=N`) : le reste des arguments garde son sens.
    let mut release = RELEASE.to_string();
    let mut positional: Vec<String> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--release" {
            match iter.next() {
                Some(value) => release = value.clone(),
                None => {
                    eprintln!("pack-eopkg : erreur : --release attend un numéro.");
                    return ExitCode::from(2);
                }
            }
        } else if let Some(value) = arg.strip_prefix("--release=") {
            release = value.to_string();
        } else {
            positional.push(arg.clone());
        }
    }
    if release.parse::<u32>().is_err() {
        eprintln!("pack-eopkg : erreur : release invalide : {release} (un entier attendu).");
        return ExitCode::from(2);
    }

    let Some(binary) = positional.first() else {
        eprintln!("pack-eopkg : erreur : chemin du binaire absent.");
        eprintln!("usage : pack-eopkg [--release N] <binaire deb-to-eopkg> [dossier_de_sortie]");
        return ExitCode::from(2);
    };
    let out_dir = PathBuf::from(positional.get(1).map(String::as_str).unwrap_or("dist"));

    match run(Path::new(binary), &out_dir, &release) {
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

fn run(binary: &Path, out_dir: &Path, release: &str) -> std::io::Result<PathBuf> {
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
    info.release = release.to_string();
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
