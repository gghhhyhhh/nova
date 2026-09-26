"""
gaming_tools.py
----------------
Donne à l'assistant la capacité de fournir des conseils, astuces et codes
de triche pour des jeux vidéo.

Principe : les codes de triche et soluces sont des informations publiques
(sites spécialisés, wikis de jeux). On les récupère via une recherche web
libre, puis on les fait résumer/organiser par le LLM local pour une
réponse claire.
"""

from llm_engine import LocalLLM
from web_tools import search_web

GAMING_SYSTEM_PROMPT = (
    "Tu es un expert en jeux vidéo. À partir des extraits de recherche web fournis, "
    "tu donnes une réponse organisée et complète en français :\n"
    "- une section 'Codes de triche' avec la liste la plus complète possible "
    "(codes, combinaisons de touches, commandes console), avec la plateforme si connue\n"
    "- une section 'Astuces et conseils' avec des conseils de gameplay utiles\n"
    "Si les résultats de recherche ne contiennent pas de codes fiables pour ce jeu, "
    "dis-le clairement plutôt que d'inventer des codes."
)


def get_cheat_codes(game_name: str, llm: LocalLLM = None) -> str:
    """Recherche et renvoie les codes de triche connus pour un jeu donné."""
    llm = llm or LocalLLM()

    results = search_web(f"codes de triche cheat codes {game_name}", max_results=6)
    results += search_web(f"{game_name} console commands cheats list", max_results=4)

    if not results or all(r.startswith("[Erreur") for r in results):
        return "[Erreur] Impossible de récupérer des résultats de recherche pour ce jeu."

    context = "\n\n".join(results)
    prompt = (
        f"Voici des résultats de recherche web sur les codes de triche du jeu \"{game_name}\" :\n\n"
        f"{context}\n\n"
        f"Donne-moi la liste la plus complète possible des codes de triche pour \"{game_name}\", "
        "organisée par plateforme si pertinent."
    )
    return llm.generate(prompt, GAMING_SYSTEM_PROMPT)


def get_game_tips(game_name: str, topic: str = "", llm: LocalLLM = None) -> str:
    """Recherche et renvoie des conseils/astuces de gameplay pour un jeu donné."""
    llm = llm or LocalLLM()

    query = f"astuces conseils guide {game_name}"
    if topic:
        query += f" {topic}"
    results = search_web(query, max_results=6)

    if not results or all(r.startswith("[Erreur") for r in results):
        return "[Erreur] Impossible de récupérer des résultats de recherche pour ce jeu."

    context = "\n\n".join(results)
    prompt = (
        f"Voici des résultats de recherche web sur le jeu \"{game_name}\""
        f"{' (sujet : ' + topic + ')' if topic else ''} :\n\n"
        f"{context}\n\n"
        f"Donne-moi des conseils et astuces utiles pour progresser dans \"{game_name}\"."
    )
    return llm.generate(prompt, GAMING_SYSTEM_PROMPT)


def get_full_game_help(game_name: str, llm: LocalLLM = None) -> str:
    """Combine codes de triche + conseils en une seule réponse complète."""
    codes = get_cheat_codes(game_name, llm)
    tips = get_game_tips(game_name, llm=llm)
    return f"## Codes de triche pour {game_name}\n{codes}\n\n## Astuces et conseils pour {game_name}\n{tips}"
