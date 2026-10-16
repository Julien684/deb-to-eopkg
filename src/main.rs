//! `deb-to-eopkg` — convertit un paquet Debian (`.deb`) en paquet Solus
//! (`.eopkg`).
//!
//! Usage :
//!
//! ```text
//! deb-to-eopkg paquet.deb
//! deb-to-eopkg paquet.deb destination/
//! deb-to-eopkg paquet.deb chemin/vers/paquet.eopkg
//! ```
//!
//! Sans destination, le `.eopkg` est écrit dans le dossier du `.deb`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use deb_to_eopkg::{color, convert, TOOL_NAME, VERSION};

const HELP: &str = "\
deb-to-eopkg — convertit un paquet Debian (.deb) en paquet Solus (.eopkg).

USAGE :
    deb-to-eopkg [OPTIONS] <paquet.deb> [destination]

ARGUMENTS :
    <paquet.deb>    le fichier .deb à convertir
    [destination]   un dossier (le nom du .eopkg est déduit du paquet)
                    ou le chemin complet du fichier .eopkg à produire.
                    Par défaut : le dossier du .deb.

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
    --color WHEN    coloration : auto (défaut, seulement si la sortie est un
                    terminal), always ou never. Respecte NO_COLOR et TERM=dumb.
    -h, --help      affiche cette aide
    -V, --version   affiche la version

La conversion est verbeuse par défaut : elle indique le fichier source, les
membres lus et une progression régulière (elle peut durer plusieurs minutes).
Les dépendances du .deb et, s'il y en a, le CONTENU de ses scripts de
maintenance sont affichés à la fin. Ces scripts ne sont PAS reportés : eopkg
n'exécute aucun script à l'installation (le mécanisme COMAR de pisi n'existe
plus sur Solus).

EXEMPLES :
    deb-to-eopkg mon-logiciel.deb
    # -> ./mon-logiciel-1.2.3-1-1-x86_64.eopkg (à côté du .deb)
    deb-to-eopkg mon-logiciel.deb /tmp/
    # -> /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
    deb-to-eopkg --install mon-logiciel.deb   # ... puis l'installe
";

fn main() -> ExitCode {
    let mut deb = None;
    let mut dest = None;
    let mut keep_deps = false;
    let mut quiet = false;
    let mut install = false;
    let mut color = color::Choice::Auto;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
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
            "--color" => match args.next().as_deref().map(str::parse::<color::Choice>) {
                Some(Ok(choice)) => color = choice,
                _ => {
                    eprintln!("{TOOL_NAME} : erreur : --color attend auto, always ou never.");
                    return ExitCode::from(2);
                }
            },
            _ if arg.starts_with("--color=") => {
                match arg["--color=".len()..].parse::<color::Choice>() {
                    Ok(choice) => color = choice,
                    Err(()) => {
                        eprintln!("{TOOL_NAME} : erreur : --color attend auto, always ou never.");
                        return ExitCode::from(2);
                    }
                }
            }
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

    let Some(deb) = deb else {
        eprintln!("{TOOL_NAME} : erreur : il faut un fichier .deb à convertir.");
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

    // Sans destination, on écrit à côté du `.deb`.
    let dest = match dest {
        Some(dest) => PathBuf::from(dest),
        None => default_destination(deb_path),
    };

    let opts = convert::Options { keep_deps, quiet };
    // Couleurs : une par type de script, une pour les dépendances.
    let palette = color::Palette::for_stderr(color);

    let summary = match convert::convert(deb_path, &dest, opts) {
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
                    Some(constraint) => eprintln!(
                        "    {}",
                        palette.dependency(&format!("{} ({constraint})", dep.name))
                    ),
                    None => eprintln!("    {}", palette.dependency(&dep.name)),
                }
            }
        }
        // Scripts de maintenance : eopkg n'exécute aucun script à
        // l'installation, ils ne peuvent pas être reportés. On affiche leur
        // contenu en entier pour que l'utilisateur voie précisément ce qu'il
        // perd (et puisse le reprendre à la main si nécessaire).
        if !summary.scripts.is_empty() {
            eprintln!(
                "{TOOL_NAME} : scripts de maintenance du .deb ({}), NON reportables :",
                summary.scripts.len()
            );
            for script in &summary.scripts {
                let header = format!(
                    "--- {} ({}) ---",
                    script.name,
                    deb_to_eopkg::convert::human_size(script.size() as u64)
                );
                eprintln!(
                    "{TOOL_NAME} : {}",
                    palette.script_header(&script.name, &header)
                );
                let text = script.text();
                // On retire le saut de ligne final avant de colorer : la
                // séquence de remise à zéro reste ainsi sur la dernière ligne
                // du script au lieu de flotter sur la ligne suivante.
                let body = text.strip_suffix('\n').unwrap_or(&text);
                eprintln!("{}", palette.script_body(&script.name, body));
            }
            eprintln!(
                "{TOOL_NAME} : eopkg ne lance aucun script à l'installation \
                 (le mécanisme COMAR n'existe plus sur Solus) : ces scripts sont ignorés."
            );
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

/// Destination par défaut : le dossier qui contient le `.deb`.
///
/// `Path::parent()` renvoie une chaîne vide pour un fichier situé dans le
/// dossier courant (`deb-to-eopkg paquet.deb`) — on écrit alors dans `.`.
fn default_destination(deb_path: &Path) -> PathBuf {
    deb_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_par_defaut_est_le_dossier_du_deb() {
        assert_eq!(
            default_destination(Path::new("/paquets/mon-logiciel.deb")),
            PathBuf::from("/paquets")
        );
        assert_eq!(
            default_destination(Path::new("/tmp/a/b/c.deb")),
            PathBuf::from("/tmp/a/b")
        );
    }

    #[test]
    fn destination_par_defaut_dans_le_dossier_courant() {
        // `parent()` donne "" pour un fichier sans dossier : on écrit dans `.`.
        assert_eq!(
            default_destination(Path::new("mon-logiciel.deb")),
            PathBuf::from(".")
        );
        assert_eq!(
            default_destination(Path::new("./mon-logiciel.deb")),
            PathBuf::from(".")
        );
    }
}
