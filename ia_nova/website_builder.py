"""
website_builder.py
-------------------
Permet à l'assistant de CONSTRUIRE un site web (HTML/CSS/JS) à partir
d'une simple description, en s'appuyant sur le LLM local (Ollama).

Principe :
1. On demande au LLM de générer plusieurs fichiers, chacun délimité par
   un marqueur "=== FILE: nom_du_fichier ===" suivi d'un bloc de code.
2. On parse cette réponse et on écrit chaque fichier sur le disque,
   dans un dossier de projet dédié.

Aucune API externe : uniquement le modèle local + écriture de fichiers.
"""

import os
import re
from typing import Dict

from llm_engine import LocalLLM

FILE_MARKER_RE = re.compile(
    r"===\s*FILE:\s*(?P<name>[\w./-]+)\s*===\s*```(?:\w+)?\n(?P<content>.*?)```",
    re.DOTALL,
)

BUILD_SYSTEM_PROMPT = (
    "Tu es un développeur web expert. Quand on te demande de construire un site, "
    "tu réponds UNIQUEMENT avec les fichiers nécessaires, chacun présenté exactement "
    "sous cette forme (répète ce format pour chaque fichier, sans rien ajouter avant/après) :\n\n"
    "=== FILE: index.html ===\n"
    "```html\n"
    "<!-- contenu complet du fichier -->\n"
    "```\n\n"
    "=== FILE: style.css ===\n"
    "```css\n"
    "/* contenu complet du fichier */\n"
    "```\n\n"
    "=== FILE: script.js ===\n"
    "```javascript\n"
    "// contenu complet du fichier\n"
    "```\n\n"
    "Règles :\n"
    "- Toujours inclure au minimum index.html, style.css et script.js.\n"
    "- Le HTML doit être complet et valide (doctype, head, body).\n"
    "- Le CSS doit produire un design propre et moderne (pas de style minimal par défaut).\n"
    "- Le JS ne doit contenir que du code fonctionnel, sans commentaire superflu.\n"
    "- Ne mets aucun texte d'explication en dehors des blocs de fichiers."
)


def parse_files(llm_response: str) -> Dict[str, str]:
    """Extrait un dictionnaire {nom_de_fichier: contenu} depuis la réponse du LLM."""
    files = {}
    for match in FILE_MARKER_RE.finditer(llm_response):
        name = match.group("name").strip()
        content = match.group("content").strip("\n")
        files[name] = content
    return files


def save_files(files: Dict[str, str], project_dir: str) -> None:
    os.makedirs(project_dir, exist_ok=True)
    for name, content in files.items():
        full_path = os.path.join(project_dir, name)
        os.makedirs(os.path.dirname(full_path) or ".", exist_ok=True)
        with open(full_path, "w", encoding="utf-8") as f:
            f.write(content)


def build_website(description: str, project_dir: str = "site_genere", llm: LocalLLM = None) -> str:
    """
    Génère un site web complet à partir d'une description en langage naturel.
    Retourne un message récapitulatif (fichiers créés + emplacement).
    """
    llm = llm or LocalLLM()

    if not llm.is_available():
        return (
            "[Erreur] Le modèle local (Ollama) n'est pas disponible. "
            "Installe-le et fais 'ollama pull llama3.2' avant de générer un site."
        )

    prompt = (
        f"Construis un site web qui correspond à cette demande :\n\"{description}\"\n\n"
        "Respecte strictement le format de fichiers demandé dans les instructions système."
    )

    response = llm.generate(prompt, BUILD_SYSTEM_PROMPT)
    files = parse_files(response)

    if not files:
        return (
            "[Erreur] Le modèle n'a pas renvoyé de fichiers au format attendu. "
            "Réponse brute :\n\n" + response[:1500]
        )

    save_files(files, project_dir)

    file_list = "\n".join(f"- {name}" for name in files)
    index_path = os.path.join(project_dir, "index.html")
    return (
        f"Site généré avec succès dans le dossier '{project_dir}/' :\n{file_list}\n\n"
        f"Ouvre '{index_path}' dans un navigateur pour le voir."
    )


def refine_website(instruction: str, project_dir: str = "site_genere", llm: LocalLLM = None) -> str:
    """
    Modifie un site déjà généré, en donnant au LLM le contenu actuel des
    fichiers + l'instruction de modification, puis en réécrivant les fichiers.
    """
    llm = llm or LocalLLM()

    if not os.path.isdir(project_dir):
        return f"[Erreur] Aucun projet trouvé dans '{project_dir}'. Génère d'abord un site."

    current_files = {}
    for root, _, filenames in os.walk(project_dir):
        for fname in filenames:
            path = os.path.join(root, fname)
            rel = os.path.relpath(path, project_dir)
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                current_files[rel] = f.read()

    current_blob = "\n\n".join(
        f"=== FILE: {name} ===\n```\n{content}\n```" for name, content in current_files.items()
    )

    prompt = (
        f"Voici les fichiers actuels du site :\n\n{current_blob}\n\n"
        f"Applique cette modification : \"{instruction}\"\n"
        "Renvoie TOUS les fichiers mis à jour (même ceux non modifiés), "
        "dans le même format que les instructions système."
    )

    response = llm.generate(prompt, BUILD_SYSTEM_PROMPT)
    files = parse_files(response)

    if not files:
        return "[Erreur] Le modèle n'a pas renvoyé de fichiers valides pour la modification."

    save_files(files, project_dir)
    file_list = "\n".join(f"- {name}" for name in files)
    return f"Site mis à jour dans '{project_dir}/' :\n{file_list}"
