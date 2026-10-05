//! Expériences sans affichage : balayages de paramètres, mesures, export CSV.
//! Voir la skill `.claude/skills/balayage-parametres`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("sweep") => {
            eprintln!(
                "La sous-commande `sweep` n'est pas encore écrite. Lance la skill /balayage-parametres pour la construire."
            );
            std::process::exit(1);
        }
        _ => {
            println!("Usage : cargo run --release -p lab -- <sous-commande>");
            println!("Sous-commandes prévues : sweep");
        }
    }
}
