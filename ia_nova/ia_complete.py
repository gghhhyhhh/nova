"""
ia_complete.py
================
UNE SEULE IA, un seul fichier, qui regroupe TOUT :

  1. Discussion en texte libre avec MÉMOIRE persistante
  2. Lecture, explication et exécution de CODE
  3. Lecture de LIENS WEB et de VIDÉOS YOUTUBE
  4. Lecture d'IMAGES (OCR + description) et de VIDÉOS locales (transcription)
  5. Recherche sur INTERNET
  6. Construction et modification de SITES WEB (HTML/CSS/JS)
  7. Conseils et CODES DE TRICHE pour les jeux vidéo

Fonctionne sans API payante : le "cerveau" est un modèle de langage local
via Ollama (gratuit, https://ollama.com). Les autres capacités utilisent
des bibliothèques Python gratuites (installées à la demande).

INSTALLATION MINIMALE (chat + mémoire + code) :
    1. Installer Ollama : https://ollama.com/download
    2. ollama pull llama3.2
    3. pip install requests beautifulsoup4

INSTALLATION COMPLÈTE (toutes les fonctionnalités) :
    pip install -r requirements.txt
    (voir la liste des paquets optionnels plus bas dans ce fichier)

LANCEMENT :
    python ia_complete.py
"""

import os
import re
import sys
import json
import time
import subprocess
import tempfile
import urllib.request
import urllib.error
from typing import Dict, List, Any

# ============================================================
# Bibliothèques toujours nécessaires (légères, à installer une fois)
# ============================================================
try:
    import requests
    from bs4 import BeautifulSoup
except ImportError:
    requests = None
    BeautifulSoup = None

# ============================================================
# 0. CONFIGURATION
# ============================================================
OLLAMA_URL = "http://localhost:11434/api/generate"
OLLAMA_TAGS_URL = "http://localhost:11434/api/tags"
DEFAULT_MODEL = "llama3.2:latest"
MEMORY_PATH = "memory_store.json"
SITE_DIR = "site_genere"

IMAGE_EXTENSIONS = (".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif")
VIDEO_EXTENSIONS = (".mp4", ".mov", ".mkv", ".avi")

SYSTEM_PROMPT = (
    "Tu es un assistant IA utile qui répond en français, de façon claire et concise. "
    "Tu peux lire du code, expliquer des erreurs, proposer des corrections, "
    "et t'appuyer sur le contexte fourni (pages web, transcriptions, images, mémoire)."
)

GAMING_SYSTEM_PROMPT = (
    "Tu es un expert en jeux vidéo. À partir des extraits de recherche web fournis, "
    "tu donnes une réponse organisée et complète en français :\n"
    "- une section 'Codes de triche' avec la liste la plus complète possible "
    "(codes, combinaisons de touches, commandes console), avec la plateforme si connue\n"
    "- une section 'Astuces et conseils' avec des conseils de gameplay utiles\n"
    "Si les résultats de recherche ne contiennent pas de codes fiables pour ce jeu, "
    "dis-le clairement plutôt que d'inventer des codes."
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
    "- Le CSS doit produire un design propre et moderne.\n"
    "- Le JS ne doit contenir que du code fonctionnel.\n"
    "- Ne mets aucun texte d'explication en dehors des blocs de fichiers."
)

FILE_MARKER_RE = re.compile(
    r"===\s*FILE:\s*(?P<name>[\w./-]+)\s*===\s*```(?:\w+)?\n(?P<content>.*?)```",
    re.DOTALL,
)


# ============================================================
# 1. MÉMOIRE PERSISTANTE
# ============================================================
class Memory:
    def __init__(self, path: str = MEMORY_PATH):
        self.path = path
        self.data: Dict[str, Any] = {"conversations": [], "facts": []}
        self._load()

    def _load(self):
        if os.path.exists(self.path):
            try:
                with open(self.path, "r", encoding="utf-8") as f:
                    self.data = json.load(f)
            except (json.JSONDecodeError, IOError):
                self.data = {"conversations": [], "facts": []}

    def _save(self):
        with open(self.path, "w", encoding="utf-8") as f:
            json.dump(self.data, f, ensure_ascii=False, indent=2)

    def add_message(self, role: str, content: str):
        self.data["conversations"].append({
            "role": role, "content": content, "timestamp": time.time()
        })
        self.data["conversations"] = self.data["conversations"][-200:]
        self._save()

    def get_recent_context(self, n: int = 10) -> List[Dict[str, str]]:
        return self.data["conversations"][-n:]

    def remember_fact(self, fact: str):
        self.data["facts"].append({"fact": fact, "timestamp": time.time()})
        self._save()

    def search_facts(self, query: str) -> List[str]:
        query_words = set(query.lower().split())
        return [e["fact"] for e in self.data["facts"]
                if query_words & set(e["fact"].lower().split())]

    def all_facts(self) -> List[str]:
        return [e["fact"] for e in self.data["facts"]]


# ============================================================
# 2. MOTEUR LLM LOCAL (Ollama, sans API)
# ============================================================
class LocalLLM:
    def __init__(self, model: str = DEFAULT_MODEL):
        self.model = model

    def is_available(self) -> bool:
        try:
            urllib.request.urlopen(urllib.request.Request(OLLAMA_TAGS_URL), timeout=2)
            return True
        except (urllib.error.URLError, OSError):
            return False

    def generate(self, prompt: str, system: str = "") -> str:
        if not self.is_available():
            return (
                "[Erreur] Ollama n'est pas détecté sur cette machine.\n"
                "Installe-le depuis https://ollama.com puis lance :\n"
                f"    ollama pull {self.model}\n"
                "Ensuite relance ce script."
            )
        payload = {"model": self.model, "prompt": prompt, "system": system, "stream": False}
        data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(OLLAMA_URL, data=data,
                                      headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                result = json.loads(resp.read().decode("utf-8"))
                return result.get("response", "").strip()
        except urllib.error.URLError as e:
            return f"[Erreur de connexion au modèle local] {e}"
        except json.JSONDecodeError:
            return "[Erreur] Réponse du modèle illisible."


# ============================================================
# 3. OUTILS WEB (recherche, pages, YouTube)
# ============================================================
def search_web(query: str, max_results: int = 5) -> List[str]:
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
    if requests is None or BeautifulSoup is None:
        return "[Erreur] Installe : pip install requests beautifulsoup4"
    try:
        headers = {"User-Agent": "Mozilla/5.0 (compatible; LocalAI/1.0)"}
        resp = requests.get(url, headers=headers, timeout=10)
        resp.raise_for_status()
        soup = BeautifulSoup(resp.text, "html.parser")
        for tag in soup(["script", "style", "nav", "footer", "header"]):
            tag.decompose()
        text = " ".join(soup.get_text(separator=" ").split())
        return text[:max_chars]
    except Exception as e:
        return f"[Erreur] Impossible de lire l'URL : {e}"


def _extract_youtube_id(url: str):
    match = re.search(r"(?:v=|youtu\.be/|embed/)([A-Za-z0-9_-]{11})", url)
    return match.group(1) if match else None


def read_youtube_transcript(url: str, languages=("fr", "en")) -> str:
    try:
        from youtube_transcript_api import YouTubeTranscriptApi
    except ImportError:
        return "[Erreur] Installe le paquet : pip install youtube-transcript-api"
    video_id = _extract_youtube_id(url)
    if not video_id:
        return "[Erreur] URL YouTube non reconnue."
    try:
        transcript = YouTubeTranscriptApi.get_transcript(video_id, languages=list(languages))
        return " ".join(chunk["text"] for chunk in transcript)
    except Exception as e:
        return f"[Erreur] Impossible de récupérer la transcription : {e}"


def is_url(text: str) -> bool:
    return bool(re.match(r"^https?://", text.strip()))


def is_youtube_url(text: str) -> bool:
    return "youtube.com" in text or "youtu.be" in text


# ============================================================
# 4. OUTILS IMAGES / VIDÉOS (OCR, description, transcription)
# ============================================================
_blip_model = None
_blip_processor = None


def ocr_image(path: str) -> str:
    try:
        import pytesseract
        from PIL import Image
    except ImportError:
        return "[Erreur] Installe : pip install pytesseract pillow (+ binaire tesseract-ocr)"
    try:
        img = Image.open(path)
        text = pytesseract.image_to_string(img, lang="fra+eng")
        return text.strip() or "(aucun texte détecté dans l'image)"
    except Exception as e:
        return f"[Erreur OCR] {e}"


def describe_image(path: str) -> str:
    global _blip_model, _blip_processor
    try:
        from transformers import BlipProcessor, BlipForConditionalGeneration
        from PIL import Image
    except ImportError:
        return "[Erreur] Installe : pip install transformers torch pillow"
    try:
        if _blip_model is None:
            _blip_processor = BlipProcessor.from_pretrained("Salesforce/blip-image-captioning-base")
            _blip_model = BlipForConditionalGeneration.from_pretrained("Salesforce/blip-image-captioning-base")
        img = Image.open(path).convert("RGB")
        inputs = _blip_processor(img, return_tensors="pt")
        out = _blip_model.generate(**inputs, max_new_tokens=40)
        return _blip_processor.decode(out[0], skip_special_tokens=True)
    except Exception as e:
        return f"[Erreur description image] {e}"


def transcribe_video_audio(path: str, model_size: str = "base") -> str:
    try:
        import whisper
    except ImportError:
        return "[Erreur] Installe : pip install openai-whisper (+ ffmpeg sur le système)"
    try:
        model = whisper.load_model(model_size)
        result = model.transcribe(path)
        return result.get("text", "").strip()
    except Exception as e:
        return f"[Erreur transcription vidéo] {e}"


# ============================================================
# 5. CONSTRUCTION DE SITES WEB
# ============================================================
def parse_files(llm_response: str) -> Dict[str, str]:
    files = {}
    for match in FILE_MARKER_RE.finditer(llm_response):
        files[match.group("name").strip()] = match.group("content").strip("\n")
    return files


def save_files(files: Dict[str, str], project_dir: str) -> None:
    os.makedirs(project_dir, exist_ok=True)
    for name, content in files.items():
        full_path = os.path.join(project_dir, name)
        os.makedirs(os.path.dirname(full_path) or ".", exist_ok=True)
        with open(full_path, "w", encoding="utf-8") as f:
            f.write(content)


def build_website(description: str, llm: LocalLLM, project_dir: str = SITE_DIR) -> str:
    if not llm.is_available():
        return "[Erreur] Ollama n'est pas disponible. Installe-le pour générer un site."
    prompt = (
        f"Construis un site web qui correspond à cette demande :\n\"{description}\"\n\n"
        "Respecte strictement le format de fichiers demandé dans les instructions système."
    )
    response = llm.generate(prompt, BUILD_SYSTEM_PROMPT)
    files = parse_files(response)
    if not files:
        return "[Erreur] Le modèle n'a pas renvoyé de fichiers au format attendu.\n\n" + response[:1500]
    save_files(files, project_dir)
    file_list = "\n".join(f"- {name}" for name in files)
    return (f"Site généré avec succès dans le dossier '{project_dir}/' :\n{file_list}\n\n"
            f"Ouvre '{os.path.join(project_dir, 'index.html')}' dans un navigateur pour le voir.")


def refine_website(instruction: str, llm: LocalLLM, project_dir: str = SITE_DIR) -> str:
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
        "Renvoie TOUS les fichiers mis à jour (même ceux non modifiés)."
    )
    response = llm.generate(prompt, BUILD_SYSTEM_PROMPT)
    files = parse_files(response)
    if not files:
        return "[Erreur] Le modèle n'a pas renvoyé de fichiers valides pour la modification."
    save_files(files, project_dir)
    file_list = "\n".join(f"- {name}" for name in files)
    return f"Site mis à jour dans '{project_dir}/' :\n{file_list}"


# ============================================================
# 6. JEUX VIDÉO : CONSEILS ET CODES DE TRICHE
# ============================================================
def get_cheat_codes(game_name: str, llm: LocalLLM) -> str:
    results = search_web(f"codes de triche cheat codes {game_name}", max_results=6)
    results += search_web(f"{game_name} console commands cheats list", max_results=4)
    if not results or all(r.startswith("[Erreur") for r in results):
        return "[Erreur] Impossible de récupérer des résultats de recherche pour ce jeu."
    context = "\n\n".join(results)
    prompt = (
        f"Voici des résultats de recherche web sur les codes de triche du jeu \"{game_name}\" :\n\n"
        f"{context}\n\nDonne-moi la liste la plus complète possible des codes de triche pour "
        f"\"{game_name}\", organisée par plateforme si pertinent."
    )
    return llm.generate(prompt, GAMING_SYSTEM_PROMPT)


def get_game_tips(game_name: str, llm: LocalLLM, topic: str = "") -> str:
    query = f"astuces conseils guide {game_name}" + (f" {topic}" if topic else "")
    results = search_web(query, max_results=6)
    if not results or all(r.startswith("[Erreur") for r in results):
        return "[Erreur] Impossible de récupérer des résultats de recherche pour ce jeu."
    context = "\n\n".join(results)
    prompt = (
        f"Voici des résultats de recherche web sur le jeu \"{game_name}\" :\n\n{context}\n\n"
        f"Donne-moi des conseils et astuces utiles pour progresser dans \"{game_name}\"."
    )
    return llm.generate(prompt, GAMING_SYSTEM_PROMPT)


def get_full_game_help(game_name: str, llm: LocalLLM) -> str:
    codes = get_cheat_codes(game_name, llm)
    tips = get_game_tips(game_name, llm)
    return f"## Codes de triche pour {game_name}\n{codes}\n\n## Astuces et conseils pour {game_name}\n{tips}"


# ============================================================
# 7. ASSISTANT PRINCIPAL : routage de toutes les capacités
# ============================================================
class Assistant:
    def __init__(self):
        self.memory = Memory()
        self.llm = LocalLLM()
        self.external_history = None

    def looks_like_code(self, text: str) -> bool:
        markers = ["def ", "import ", "class ", "{", "}", ";", "print(", "SELECT ", "function "]
        return any(m in text for m in markers) or "```" in text

    def extract_code_block(self, text: str) -> str:
        match = re.search(r"```(?:python)?\n(.*?)```", text, re.DOTALL)
        return match.group(1) if match else text

    def run_python_code(self, code: str) -> str:
        with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as f:
            f.write(code)
            path = f.name
        try:
            result = subprocess.run(
                [sys.executable, path],
                capture_output=True,
                text=True,
                timeout=10
            )
            output = result.stdout
            if result.stderr:
                output += "\n[stderr]\n" + result.stderr
            return output.strip() or "(le code s'est exécuté sans sortie)"
        except subprocess.TimeoutExpired:
            return "[Erreur] Le code a dépassé le temps limite (10s)."
        finally:
            os.remove(path)

    def build_context(
        self,
        extra: str = "",
        history: List[Dict[str, str]] | None = None
    ) -> str:
        if history is not None:
            current_history = history
            facts = []
        elif self.external_history is not None:
            current_history = self.external_history
            facts = []
        else:
            current_history = self.memory.get_recent_context(8)
            facts = self.memory.all_facts()

        history_text = "\n".join(
            f"{m['role']}: {m['content']}"
            for m in current_history
        )

        facts_text = (
            "\n".join(f"- {f}" for f in facts)
            if facts
            else "(aucun)"
        )

        return (
            f"Faits mémorisés:\n{facts_text}\n\n"
            f"Historique récent:\n{history_text}\n\n"
            f"{extra}"
        )

    def handle(
        self,
        user_input: str,
        category: str = "chat",
        history: List[Dict[str, str]] | None = None
    ) -> str:

        self.external_history = history

        if history is None:
            self.memory.add_message("user", user_input)
        
        external_history = history is not None

        if not external_history:
            self.memory.add_message("user", user_input)

        lower_input = user_input.lower()

        category_prompts = {
            "chat": (
                "Tu es en mode Discussion. "
                "Réponds naturellement et clairement à l'utilisateur."
            ),

            "code": (
                "Tu es en mode Code. "
                "Concentre-toi sur la programmation, l'explication, "
                "la correction et l'amélioration du code. "
                "Donne des exemples concrets lorsque c'est utile."
            ),

            "image": (
                "Tu es en mode Générateur d'image. "
                "Aide l'utilisateur à concevoir précisément une image, "
                "en décrivant le sujet, le style, la composition, "
                "la lumière et les détails importants."
            ),

            "search": (
                "Tu es en mode Recherche. "
                "Analyse précisément la demande et privilégie les "
                "informations vérifiables et structurées."
            ),

            "study": (
                "Tu es en mode Étude. "
                "Explique progressivement les notions, avec des exemples "
                "simples et adaptés à l'apprentissage."
            ),

            "writing": (
                "Tu es en mode Rédaction. "
                "Aide à écrire, reformuler, corriger et améliorer les textes "
                "en respectant le style demandé."
            ),
        }

        category_instruction = category_prompts.get(
            category,
            category_prompts["chat"]
        )

        # 1. mémoriser un fait explicite
        if lower_input.startswith(("retiens que", "souviens-toi que")):
            fact = user_input.split("que", 1)[1].strip()
            self.memory.remember_fact(fact)
            return f"D'accord, je retiens : {fact}"

        # 2. construction / modification d'un site web
        if any(k in lower_input for k in ["construis un site", "crée un site", "génère un site", "fais-moi un site"]):
            description = re.sub(
                r"^(construis un site( web)?|crée un site( web)?|génère un site( web)?|fais-moi un site( web)?)\s*",
                "", user_input, flags=re.IGNORECASE)
            result = build_website(description.strip() or user_input, self.llm)
            self.memory.add_message("assistant", result)
            return result

        if any(k in lower_input for k in ["modifie le site", "change le site", "mets à jour le site"]):
            instruction = re.sub(
                r"^(modifie le site|change le site|mets à jour le site)\s*",
                "", user_input, flags=re.IGNORECASE)
            result = refine_website(instruction.strip() or user_input, self.llm)
            self.memory.add_message("assistant", result)
            return result

        # 3. conseils / codes de triche pour un jeu vidéo
        cheat_match = re.search(r"(?:codes?( de triche)?|cheat codes?)\s+(?:pour|de|du|sur)\s+(.+)",
                                 user_input, flags=re.IGNORECASE)
        tips_match = re.search(r"(?:astuces?|conseils?|soluce)\s+(?:pour|sur|du jeu)\s+(.+)",
                                user_input, flags=re.IGNORECASE)
        full_help_match = re.search(r"aide[- ]moi sur le jeu\s+(.+)", user_input, flags=re.IGNORECASE)

        if full_help_match:
            result = get_full_game_help(full_help_match.group(1).strip(" ?."), self.llm)
            self.memory.add_message("assistant", result)
            return result
        if cheat_match:
            result = get_cheat_codes(cheat_match.group(2).strip(" ?."), self.llm)
            self.memory.add_message("assistant", result)
            return result
        if tips_match:
            result = get_game_tips(tips_match.group(1).strip(" ?."), self.llm)
            self.memory.add_message("assistant", result)
            return result

        # 4. recherche web explicite
        if lower_input.startswith(("cherche sur internet", "recherche", "trouve sur le web")):
            query = re.sub(r"^(cherche sur internet|recherche|trouve sur le web)\s*", "",
                            user_input, flags=re.IGNORECASE)
            results = search_web(query)
            extra = "Résultats de recherche web :\n" + "\n\n".join(results)
            prompt = self.build_context(extra) + f"\n\nRésume ces résultats pour répondre à : {query}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 5. lien web ou YouTube
        words = user_input.split()
        url_candidates = [w for w in words if is_url(w)]
        if url_candidates:
            url = url_candidates[0]
            if is_youtube_url(url):
                content = read_youtube_transcript(url)
                label = "Transcription de la vidéo YouTube"
            else:
                content = read_url(url)
                label = "Contenu de la page web"
            extra = f"{label} ({url}) :\n{content}"
            prompt = self.build_context(extra) + f"\n\nQuestion de l'utilisateur : {user_input}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 6. image locale
        image_path = next((w for w in words if w.lower().endswith(IMAGE_EXTENSIONS)), None)
        if image_path and os.path.exists(image_path):
            text_in_image = ocr_image(image_path)
            caption = describe_image(image_path)
            extra = f"Description de l'image : {caption}\nTexte détecté dans l'image : {text_in_image}"
            prompt = self.build_context(extra) + f"\n\nQuestion de l'utilisateur : {user_input}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 7. vidéo locale
        video_path = next((w for w in words if w.lower().endswith(VIDEO_EXTENSIONS)), None)
        if video_path and os.path.exists(video_path):
            transcript = transcribe_video_audio(video_path)
            extra = f"Transcription audio de la vidéo :\n{transcript}"
            prompt = self.build_context(extra) + f"\n\nQuestion de l'utilisateur : {user_input}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 8. exécution de code
        if any(k in lower_input for k in ["exécute ce code", "lance ce code", "teste ce code"]):
            code = self.extract_code_block(user_input)
            result = self.run_python_code(code)
            extra = f"Résultat de l'exécution du code :\n{result}"
            prompt = self.build_context(extra) + "\n\nExplique ce résultat à l'utilisateur, et propose une correction si besoin."
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 9. code à analyser/corriger
        if self.looks_like_code(user_input):
            extra = "L'utilisateur partage du code à analyser ou corriger."
            prompt = self.build_context(extra) + f"\n\n{user_input}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

            # 10. discussion générale
        prompt = (
            self.build_context()
            + f"\n\nMode sélectionné : {category_instruction}"
            + f"\n\nUtilisateur : {user_input}"
        )

        answer = self.llm.generate(prompt, SYSTEM_PROMPT)
        self.memory.add_message("assistant", answer)
        return answer


# ============================================================
# 8. BOUCLE PRINCIPALE
# ============================================================
def main():
    assistant = Assistant()
    print("=== IA locale complète (tape 'quit' pour sortir) ===")
    if not assistant.llm.is_available():
        print(
            "\n⚠️  Ollama n'est pas détecté. Installe-le (https://ollama.com), "
            "fais 'ollama pull llama3.2', puis relance ce script.\n"
        )

    while True:
        try:
            user_input = input("\nVous > ").strip()
        except (EOFError, KeyboardInterrupt):
            break
        if user_input.lower() in ("quit", "exit", "quitter"):
            break
        if not user_input:
            continue
        response = assistant.handle(user_input)
        print(f"\nIA > {response}")


if __name__ == "__main__":
    main()
    