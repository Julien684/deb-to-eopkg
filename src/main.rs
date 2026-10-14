//! `deb-to-eopkg` — convertit un paquet Debian (`.deb`) en paquet Solus
//! (`.eopkg`).
//!
//! Usage :
//!
//! ```text
//! deb-to-eopkg paquet.deb destination/
//! deb-to-eopkg paquet.deb chemin/vers/paquet.eopkg
//! ```

use std::path::Path;
use std::process::{Command, ExitCode};

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
    -i, --install   installe le paquet avec eopkg après la conversion
                    (`sudo eopkg install <fichier>`). La conversion doit avoir
                    réussi. eopkg demande confirmation, vous pouvez donc
                    renoncer à l'installation. À réserver aux systèmes Solus :
                    sudo est requis, donc le mot de passe peut être demandé.
    --deps          conserve les dépendances Depends du .deb dans l'eopkg.
                    Par défaut elles sont ignorées : les noms de paquets
                    Debian (libc6) ne correspondent pas à ceux de Solus
                    (glibc), et un paquet installé depuis une URL ne peut pas
                    résoudre ses dépendances. La liste reste affichée à la fin
                    de la conversion, pour information.
    -q, --quiet     mode silencieux : n'affiche ni progression ni
                    avertissements. Le chemin du fichier produit reste
                    affiché sur la sortie standard.
    -h, --help      affiche cette aide
    -V, --version   affiche la version

La conversion est verbeuse par défaut : elle indique le fichier source, les
membres lus et une progression régulière (elle peut durer plusieurs minutes).

EXEMPLE :
    deb-to-eopkg mon-logiciel.deb /tmp/
    # -> /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
    deb-to-eopkg --install mon-logiciel.deb /tmp/   # ... puis l'installe
";

fn main() -> ExitCode {
    let mut deb = None;
    let mut dest = None;
    let mut keep_deps = false;
    let mut quiet = false;
    let mut install = false;

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
            "-i" | "-it" | "--install" => install = true,
            "-q" | "--quiet" => quiet = true,
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

    let deb_path = Path::new(&deb);
    if !deb_path.is_file() {
        eprintln!("{TOOL_NAME} : erreur : fichier .deb introuvable : {deb}");
        return ExitCode::from(2);
    }
    if !deb.to_lowercase().ends_with(".deb") && !quiet {
        eprintln!("{TOOL_NAME} : avertissement : {deb} ne finit pas par .deb");
    }

    let opts = convert::Options { keep_deps, quiet };

    let summary = match convert::convert(deb_path, Path::new(&dest), opts) {
        Ok(summary) => summary,
        Err(err) => {
            eprintln!("{TOOL_NAME} : erreur : {err}");
            return ExitCode::from(1);
        }
    };

    if !quiet {
        eprintln!(
            "{TOOL_NAME} : génération : {} ({} fichiers, {} de données, arch={})",
            summary.out.display(),
            summary.files,
            deb_to_eopkg::convert::human_size(summary.bytes),
            summary.arch
        );
        // La liste des dépendances du .deb, pour information : elle n'est pas
        // inscrite dans l'eopkg sans `--deps`, mais elle est utile à l'œil.
        if summary.deps.is_empty() {
            eprintln!("{TOOL_NAME} : dépendances du .deb : aucune");
        } else {
            eprintln!(
                "{TOOL_NAME} : dépendances du .deb ({}) :",
                summary.deps.len()
            );
            for dep in &summary.deps {
                match &dep.constraint {
                    Some(constraint) => eprintln!("    {} ({constraint})", dep.name),
                    None => eprintln!("    {}", dep.name),
                }
            }
        }
    }
    // Le chemin produit sur la sortie standard, pour l'enchaîner dans
    // un script (`out=$(deb-to-eopkg …)`).
    println!("{}", summary.out.display());

    if install {
        return install_package(&summary.out, quiet);
    }

    ExitCode::SUCCESS
}

/// Installe le `.eopkg` fraîchement produit avec `sudo eopkg install`.
///
/// On ne passe volontairement pas `-y` : eopkg affiche alors sa question de
/// confirmation et l'utilisateur peut renoncer à l'installation. La sortie
/// d'eopkg est reliée au terminal, donc la question reste interactive même
/// quand notre propre sortie est en mode `--quiet`.
fn install_package(eopkg: &Path, quiet: bool) -> ExitCode {
    if !quiet {
        eprintln!(
            "{TOOL_NAME} : installation : sudo eopkg install {}",
            eopkg.display()
        );
    }
    match Command::new("sudo")
        .arg("eopkg")
        .arg("install")
        .arg(eopkg)
        .status()
    {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!(
                "{TOOL_NAME} : erreur : `sudo eopkg install` a échoué (code {})",
                status.code().unwrap_or(-1)
            );
            ExitCode::from(1)
        }
        Err(err) => {
            eprintln!("{TOOL_NAME} : erreur : impossible de lancer `sudo` : {err}");
            ExitCode::from(1)
        }
    }
}
