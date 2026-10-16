//! Colorisation ANSI de la sortie.
//!
//! Les couleurs ne sont émises que si le flux visé est un terminal (sauf
//! demande explicite `--color always`), et jamais si la variable `NO_COLOR`
//! est définie ou si `TERM=dumb` — convention respectée par la plupart des
//! outils en ligne de commande.

use std::io::IsTerminal;
use std::str::FromStr;

/// Quand émettre des séquences de couleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    /// Seulement si la sortie est un terminal (défaut).
    #[default]
    Auto,
    /// Toujours.
    Always,
    /// Jamais.
    Never,
}

impl FromStr for Choice {
    type Err = ();

    fn from_str(value: &str) -> Result<Choice, ()> {
        match value {
            "auto" => Ok(Choice::Auto),
            "always" => Ok(Choice::Always),
            "never" => Ok(Choice::Never),
            _ => Err(()),
        }
    }
}

impl Choice {
    /// Décide si les couleurs sont actives. `is_terminal` décrit le flux visé.
    pub fn enabled(self, is_terminal: bool) -> bool {
        match self {
            Choice::Always => true,
            Choice::Never => false,
            Choice::Auto => {
                is_terminal
                    && std::env::var("NO_COLOR").map_or(true, |v| v.is_empty())
                    && std::env::var("TERM").map_or(true, |t| t != "dumb")
            }
        }
    }

    /// Réglage déduit de l'environnement pour la sortie d'erreur.
    pub fn detected() -> Choice {
        if Choice::Auto.enabled(std::io::stderr().is_terminal()) {
            Choice::Always
        } else {
            Choice::Never
        }
    }
}

/// Codes ANSI utilisés.
mod code {
    /// Gras.
    pub const BOLD: &str = "1";
    /// Nom de dépendance.
    pub const DEPENDENCY: &str = "94";
    /// Script `preinst`.
    pub const PREINST: &str = "36";
    /// Script `postinst`.
    pub const POSTINST: &str = "32";
    /// Script `prerm`.
    pub const PRERM: &str = "33";
    /// Script `postrm`.
    pub const POSTRM: &str = "35";
}

/// Applique (ou non) les couleurs.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    enabled: bool,
}

impl Palette {
    pub fn new(enabled: bool) -> Palette {
        Palette { enabled }
    }

    /// Palette active ou neutre selon le choix et le flux visé.
    pub fn for_stderr(choice: Choice) -> Palette {
        Palette::new(choice.enabled(std::io::stderr().is_terminal()))
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Encadre `text` par la séquence `code`, si les couleurs sont actives.
    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    /// Une dépendance du `.deb`.
    pub fn dependency(&self, text: &str) -> String {
        self.paint(code::DEPENDENCY, text)
    }

    /// En-tête d'un script de maintenance, dans la couleur de son type.
    pub fn script_header(&self, name: &str, text: &str) -> String {
        let color = script_color(name);
        if self.enabled {
            format!("\x1b[{color};{}m{text}\x1b[0m", code::BOLD)
        } else {
            text.to_string()
        }
    }

    /// Corps d'un script de maintenance, dans la couleur de son type.
    pub fn script_body(&self, name: &str, text: &str) -> String {
        self.paint(script_color(name), text)
    }
}

/// Couleur associée à un script de maintenance.
fn script_color(name: &str) -> &'static str {
    match name {
        "preinst" => code::PREINST,
        "postinst" => code::POSTINST,
        "prerm" => code::PRERM,
        "postrm" => code::POSTRM,
        _ => "0",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choix_par_defaut_et_analyse() {
        assert_eq!(Choice::default(), Choice::Auto);
        assert_eq!("auto".parse(), Ok(Choice::Auto));
        assert_eq!("always".parse(), Ok(Choice::Always));
        assert_eq!("never".parse(), Ok(Choice::Never));
        assert_eq!("parfois".parse::<Choice>(), Err(()));
    }

    #[test]
    fn always_et_never_ne_dependent_pas_du_flux() {
        assert!(Choice::Always.enabled(false));
        assert!(!Choice::Never.enabled(true));
        // Auto : rien à colorer hors terminal.
        assert!(!Choice::Auto.enabled(false));
    }

    #[test]
    fn palette_neutre_rend_le_texte_tel_quel() {
        let p = Palette::new(false);
        assert_eq!(p.dependency("libc6"), "libc6");
        assert_eq!(p.script_header("preinst", "h"), "h");
        assert_eq!(p.script_body("postinst", "corps"), "corps");
    }

    #[test]
    fn chaque_type_de_script_a_sa_couleur() {
        let p = Palette::new(true);
        let corps = |name| p.script_body(name, "x");
        let couleurs: Vec<String> = ["preinst", "postinst", "prerm", "postrm"]
            .iter()
            .map(|n| corps(n))
            .collect();
        // Quatre séquences distinctes...
        let mut uniques = couleurs.clone();
        uniques.sort();
        uniques.dedup();
        assert_eq!(uniques.len(), 4, "couleurs non distinctes : {couleurs:?}");
        // ...toutes encadrées par une remise à zéro.
        for c in &couleurs {
            assert!(c.starts_with('\u{1b}') && c.ends_with("\u{1b}[0m"), "{c:?}");
        }
        // Et les dépendances ont la leur, différente des scripts.
        let dep = p.dependency("libc6");
        assert!(!couleurs.contains(&dep));
    }
}
