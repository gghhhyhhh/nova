# 🌌 Galaxy Search

Moteur de recherche méta hybride écrit en Rust, combinant :
- une **base locale d'articles** (SQLite + moteur TF-IDF maison) ;
- des **résultats web en temps réel** via une instance [SearXNG](https://docs.searxng.org/) auto-hébergée.

Les deux sources sont fusionnées et affichées dans une interface web unique (onglets Web / Images / Vidéos / News / Maps).

---

## 📋 Prérequis

- [Rust](https://www.rust-lang.org/tools/install) (via `rustup`, pas via `apt` — voir la note plus bas)
- [Docker](https://docs.docker.com/get-docker/) + Docker Compose (pour SearXNG)
- `sqlite3` (CLI) pour initialiser la base locale

> ⚠️ **Important** : installe Rust avec `rustup`, pas avec `apt install cargo`. La version fournie par les dépôts Ubuntu est souvent trop ancienne (1.75) pour certaines dépendances récentes et peut provoquer des erreurs `feature 'edition2024' is required`.
> ```bash
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
> rustup update stable
> ```

---

## 🚀 Installation

### 1. Docker + SearXNG

```bash
sudo apt update
sudo apt install -y docker.io docker-compose
sudo groupadd docker 2>/dev/null || true
sudo usermod -aG docker $USER
sudo systemctl start docker
sudo systemctl enable docker
```

Configuration de SearXNG :

```bash
mkdir -p ~/searxng/core-config
cd ~/searxng

sudo tee ~/searxng/core-config/settings.yml << 'EOF'
use_default_settings: true

server:
  secret_key: "change_this_to_a_random_string_32_chars_long!!"
  limiter: false
  image_proxy: true

search:
  formats:
    - html
    - json

ui:
  static_use_hash: true
EOF

sudo docker run -d \
  --name searxng \
  -p 8081:8080 \
  -v ~/searxng/core-config:/etc/searxng \
  -e SEARXNG_SECRET="change_this_to_a_random_string_32_chars_long!!" \
  searxng/searxng:latest
```

Vérifie que ça répond :

```bash
curl "http://localhost:8081/search?q=test&format=json"
```

### 2. Base de données locale

Le projet vient avec un `schema.sql` contenant quelques articles de test. Il faut l'appliquer une seule fois :

```bash
cd ~/nova
sqlite3 articles.db < schema.sql
```

Sans cette étape, `articles.db` reste vide et la recherche locale ne renverra jamais rien (l'appli se lance quand même, mais avec 0 article chargé).

### 3. Compiler et lancer Galaxy Search

```bash
cd ~/nova
cargo build
cargo run
```

Au démarrage, tu dois voir dans les logs :

```
📚 10 article(s) chargé(s) depuis articles.db
```

---

## 🖥️ Utilisation

Ouvre dans le navigateur :

```
http://localhost:8080
```

(ou le port que tu as configuré dans `src/main.rs`, voir [Configuration](#-configuration))

Tape une requête et explore les onglets **Web / Images / Vidéos / News / Maps**. Sur l'onglet Web, les résultats de la base locale apparaissent en premier (étiquetés "Base locale"), suivis des résultats SearXNG.

---

## ⚙️ Configuration

### Changer le port du serveur

Par défaut le serveur écoute sur `127.0.0.1:8080`. Si ce port est déjà utilisé (`Error: Os { code: 98, kind: AddrInUse }`), modifie `src/main.rs` :

```rust
.bind("127.0.0.1:8080")?
```

en remplaçant `8080` par un port libre, puis relance `cargo run`.

### Changer l'URL de SearXNG

Dans `src/search_engine.rs` :

```rust
searxng_url: "http://localhost:8081".to_string(),
```

### Base de données

L'URL de connexion sqlite est définie dans `src/config.rs` :

```rust
connection_string: "sqlite:./articles.db".to_string(),
```

Un fichier `.env` (chargé via `dotenvy`) peut être utilisé pour surcharger la configuration si tu étends `DbConfig::from_env()`.

---

## 🛠️ Commandes utiles

### SearXNG

```bash
sudo docker logs searxng        # voir les logs
sudo docker restart searxng     # redémarrer
sudo docker stop searxng        # arrêter
sudo docker rm -f searxng       # supprimer le conteneur
sudo docker pull searxng/searxng:latest   # mettre à jour l'image
```

### Galaxy Search

```bash
cargo build            # compiler
cargo run               # compiler + lancer
cargo run --release     # version optimisée (démarrage plus lent, exécution plus rapide)
cargo check              # vérifier la compilation sans build complet
```

---

## 📁 Structure du projet

```
nova/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs           # point d'entrée, routes HTTP, fusion local/distant
│   ├── lib.rs             # déclaration des modules de la librairie
│   ├── config.rs          # configuration (connexion base de données)
│   ├── models.rs          # structures de données partagées
│   ├── nlp.rs              # moteur de recherche TF-IDF (base locale)
│   ├── knowledge_base.rs   # base de connaissances (tags/contenus enrichis)
│   └── search_engine.rs    # client SearXNG (recherche web distante)
├── templates/
│   └── index.html           # interface web (Tera)
├── articles.db               # base sqlite (à initialiser avec schema.sql)
└── schema.sql                 # schéma + données de test
```

---

## 🔧 Fonctionnalités

| Fonctionnalité | Description |
|---|---|
| ✅ Recherche Web | Résultats web via SearXNG |
| ✅ Recherche locale | Base sqlite indexée en TF-IDF, fusionnée avec les résultats web |
| ✅ Images | Grille d'images avec miniatures |
| ✅ Vidéos | Grille de vidéos avec miniatures |
| ✅ News | Articles d'actualité |
| ✅ Maps | Résultats de cartes |
| ✅ Cache mémoire | Recherche instantanée pour une requête déjà vue |
| ✅ Chargement lazy | Bouton "Charger plus" de résultats |
| ✅ Requêtes parallèles | 3 pages SearXNG en parallèle pour la recherche complète |

---

## 🐛 Dépannage

### Port déjà utilisé

```
Error: Os { code: 98, kind: AddrInUse, message: "Address already in use" }
```

Identifie ce qui occupe le port :

```bash
sudo ss -ltnp | grep 8080
# ou
sudo lsof -i :8080
```

Puis tue le processus fautif (remplace `<PID>` par la valeur trouvée) :

```bash
kill -9 <PID>
```

Ou plus direct :

```bash
sudo fuser -k 8080/tcp
```

Si le conflit persiste, change simplement de port dans `src/main.rs` (voir [Configuration](#-configuration)).

### Aucun résultat de la base locale

- Vérifie que `articles.db` n'est pas vide : `ls -la articles.db` (0 octet = base non initialisée).
- Réinitialise avec : `sqlite3 articles.db < schema.sql`.
- Vérifie dans les logs au démarrage que le nombre d'articles chargés n'est pas 0.

### Aucun résultat de SearXNG / erreur "Erreur SearXNG: ..."

- Vérifie que le conteneur tourne : `sudo docker ps | grep searxng`.
- Teste directement : `curl "http://localhost:8081/search?q=test&format=json"`.
- Si la réponse commence par du HTML au lieu de JSON, vérifie que `formats: [html, json]` est bien présent dans `settings.yml`, puis redémarre le conteneur.

### Erreur de compilation `feature 'edition2024' is required`

Ta version de Rust est trop ancienne (typiquement installée via `apt`). Mets à jour via `rustup` :

```bash
rustup update stable
```

### Compilation lente

Utilise le mode release pour l'exécution finale (la compilation initiale est plus longue, mais le binaire est ensuite bien plus rapide) :

```bash
cargo run --release
```

---

## 📜 Licence

Galaxy Search © 2026 — Propulsé par Rust 🦀 & SearXNG
Auteur : Hicham HEE
