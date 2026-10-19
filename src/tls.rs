use rustls::crypto::aws_lc_rs;
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;

/// Charge les certificats et construit la configuration TLS de NOVA.
pub fn load_tls_config() -> ServerConfig {
    let cert_file = File::open("certs/nova.pem").expect("Impossible d'ouvrir certs/nova.pem");

    let key_file =
        File::open("certs/nova-key.pem").expect("Impossible d'ouvrir certs/nova-key.pem");

    let mut cert_reader = BufReader::new(cert_file);
    let mut key_reader = BufReader::new(key_file);

    let cert_chain = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .expect("Impossible de lire le certificat");

    let private_key = private_key(&mut key_reader)
        .expect("Impossible de lire la clé privée")
        .expect("Aucune clé privée trouvée");

    aws_lc_rs::default_provider()
        .install_default()
        .expect("Impossible d'installer le provider crypto Rustls");

    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .expect("Impossible de créer la configuration TLS")
}
