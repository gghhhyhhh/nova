"""
web_tools.py
------------
Donne à l'IA la capacité d'aller chercher des ressources sur Internet,
sans passer par une API payante :
- recherche web (via le package libre `duckduckgo-search`)
- lecture/texte d'une page web (requests + BeautifulSoup)
- transcription de vidéos YouTube (via `youtube-transcript-api`, gratuit)

Installation :
    pip install requests beautifulsoup4 duckduckgo-search youtube-transcript-api
"""

import re
import requests
from bs4 import BeautifulSoup


def search_web(query: str, max_results: int = 5):
    """Recherche des résultats sur le web via DuckDuckGo (gratuit, sans clé)."""
    try:
        from duckduckgo_search import DDGS
    except ImportError:
        return ["[Erreur] Installe le paquet : pip install duckduckgo-search"]

    results = []
    try:
        with DDGS() as ddgs:
            for r in ddgs.text(query, max_results=max_results):
                results.append(f"{r.get('title')} - {r.get('href')}\n{r.get('body')}")
    except Exception as e:
        return [f"[Erreur de recherche] {e}"]
    return results


def read_url(url: str, max_chars: int = 4000) -> str:
    """Télécharge une page web et en extrait le texte lisible."""
    try:
        headers = {"User-Agent": "Mozilla/5.0 (compatible; LocalAI/1.0)"}
        resp = requests.get(url, headers=headers, timeout=10)
        resp.raise_for_status()
        soup = BeautifulSoup(resp.text, "html.parser")

        for tag in soup(["script", "style", "nav", "footer", "header"]):
            tag.decompose()

        text = " ".join(soup.get_text(separator=" ").split())
        return text[:max_chars]
    except requests.RequestException as e:
        return f"[Erreur] Impossible de lire l'URL : {e}"


def _extract_youtube_id(url: str):
    match = re.search(r"(?:v=|youtu\.be/|embed/)([A-Za-z0-9_-]{11})", url)
    return match.group(1) if match else None


def read_youtube_transcript(url: str, languages=("fr", "en")) -> str:
    """Récupère la transcription texte d'une vidéo YouTube (gratuit, sans clé)."""
    try:
        from youtube_transcript_api import YouTubeTranscriptApi
    except ImportError:
        return "[Erreur] Installe le paquet : pip install youtube-transcript-api"

    video_id = _extract_youtube_id(url)
    if not video_id:
        return "[Erreur] URL YouTube non reconnue."

    try:
        transcript = YouTubeTranscriptApi.get_transcript(video_id, languages=list(languages))
        text = " ".join(chunk["text"] for chunk in transcript)
        return text
    except Exception as e:
        return f"[Erreur] Impossible de récupérer la transcription : {e}"


def is_url(text: str) -> bool:
    return bool(re.match(r"^https?://", text.strip()))


def is_youtube_url(text: str) -> bool:
    return "youtube.com" in text or "youtu.be" in text
