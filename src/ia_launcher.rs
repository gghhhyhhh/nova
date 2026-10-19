use std::net::TcpStream;
use std::path::Path;
use std::process::Command;
use std::thread;

/// Lance automatiquement l'API Python ia_nova sur le port 3020.
/// Si l'API est déjà lancée, on ne démarre pas une deuxième instance.
pub fn start_ia_nova() {
    thread::spawn(|| {
        let Ok(home) = std::env::var("HOME") else {
            eprintln!("⚠️ Variable HOME introuvable, ia_nova non démarré.");
            return;
        };

        let ia_nova_dir = format!("{home}/nova/ia_nova");
        let api_file = Path::new(&ia_nova_dir).join("api.py");

        if !api_file.exists() {
            eprintln!(
                "⚠️ {} introuvable — ia_nova ne sera pas démarré.",
                api_file.display()
            );
            return;
        }

        // Vérifie si ia_nova est déjà lancé sur le port 3020.
        if TcpStream::connect("127.0.0.1:3020").is_ok() {
            println!("✅ ia_nova est déjà lancé sur http://127.0.0.1:3020");
            return;
        }

        println!("🤖 Démarrage automatique de ia_nova...");

        match Command::new("python3")
            .arg("api.py")
            .current_dir(&ia_nova_dir)
            .spawn()
        {
            Ok(_) => {
                println!("✅ ia_nova démarré sur http://127.0.0.1:3020");
            }
            Err(e) => {
                eprintln!("⚠️ Impossible de démarrer ia_nova : {e}");
                eprintln!("Lancement manuel : cd {ia_nova_dir} && python3 api.py");
            }
        }
    });
}
