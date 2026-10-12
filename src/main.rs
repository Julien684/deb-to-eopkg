//! `deb-to-eopkg` — convertit un paquet Debian (`.deb`) en paquet Solus
//! (`.eopkg`).
//!
//! Usage :
//!
//! ```text
//! deb-to-eopkg paquet.deb destination/
//! deb-to-eopkg paquet.deb chemin/vers/paquet.eopkg
//! ```

use std::process::ExitCode;

use deb_to_eopkg::{convert, TOOL_NAME, VERSION};

const HELP: &str = "\
deb-to-eopkg — convertit un paquet Debian (.deb) en paquet Solus (.eopkg).

USAGE :
    deb-to-eopkg [OPTIONS] <paquet.deb> <destination>

ARGUMENTS :
    <paquet.deb>    le fichier .deb à convertir
    <destination>   un dossier (le nom du .eopkg est déduit du paquet)
                    ou le chemin complet du fichier .eopkg à produire

OPTIONS :
    --deps          conserve les dépendances Depends du .deb dans l'eopkg.
                    Par défaut elles sont ignorées : les noms de paquets
                    Debian (libc6) ne correspondent pas à ceux de Solus
                    (glibc), et un paquet installé depuis une URL ne peut pas
                    résoudre ses dépendances.
    -h, --help      affiche cette aide
    -V, --version   affiche la version

EXEMPLE :
    deb-to-eopkg mon-logiciel.deb /tmp/
    # -> /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
";

fn main() -> ExitCode {
    let mut deb = None;
    let mut dest = None;
    let mut keep_deps = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("{TOOL_NAME} {VERSION}");
                return ExitCode::SUCCESS;
            }
            "--deps" => keep_deps = true,
            _ if arg.starts_with('-') && arg != "-" => {
                eprintln!("{TOOL_NAME} : erreur : option inconnue : {arg}");
                eprintln!("Essayez `{TOOL_NAME} --help`.");
                return ExitCode::from(2);
            }
            _ => {
                if deb.is_none() {
                    deb = Some(arg);
                } else if dest.is_none() {
                    dest = Some(arg);
                } else {
                    eprintln!("{TOOL_NAME} : erreur : argument en trop : {arg}");
                    eprintln!("Essayez `{TOOL_NAME} --help`.");
                    return ExitCode::from(2);
                }
            }
        }
    }

    let (Some(deb), Some(dest)) = (deb, dest) else {
        eprintln!("{TOOL_NAME} : erreur : il faut un fichier .deb et une destination.");
        eprintln!("Essayez `{TOOL_NAME} --help`.");
        return ExitCode::from(2);
    };

    let deb_path = std::path::Path::new(&deb);
    if !deb_path.is_file() {
        eprintln!("{TOOL_NAME} : erreur : fichier .deb introuvable : {deb}");
        return ExitCode::from(2);
    }
    if !deb.to_lowercase().ends_with(".deb") {
        eprintln!("{TOOL_NAME} : avertissement : {deb} ne finit pas par .deb");
    }

    match convert::convert(deb_path, std::path::Path::new(&dest), keep_deps) {
        Ok(summary) => {
            eprintln!(
                "{TOOL_NAME} : génération : {} ({} fichiers, {} octets de données, arch={})",
                summary.out.display(),
                summary.files,
                summary.bytes,
                summary.arch
            );
            // Le chemin produit sur la sortie standard, pour l'enchaîner dans
            // un script (`out=$(deb-to-eopkg …)`).
            println!("{}", summary.out.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{TOOL_NAME} : erreur : {err}");
            ExitCode::from(1)
        }
    }
}
