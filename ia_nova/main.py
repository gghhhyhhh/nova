"""
main.py
-------
Point d'entrée de l'assistant IA local.

Fonctionnalités :
- discuter en texte libre (mémoire de conversation incluse)
- lire / expliquer / déboguer du code
- exécuter du code Python pour vérifier une solution
- lire le contenu d'un lien web ou d'une vidéo YouTube donné dans le message
- lire une image (OCR + description) si un chemin de fichier image est donné
- mémoriser des faits ("retiens que ...") et les réutiliser plus tard

Lancement :
    python main.py
"""

import os
import re
import subprocess
import sys
import tempfile

from memory import Memory
from llm_engine import LocalLLM
from web_tools import search_web, read_url, read_youtube_transcript, is_url, is_youtube_url
from media_tools import ocr_image, describe_image, transcribe_video_audio
from website_builder import build_website, refine_website
from gaming_tools import get_cheat_codes, get_game_tips, get_full_game_help

IMAGE_EXTENSIONS = (".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif")
VIDEO_EXTENSIONS = (".mp4", ".mov", ".mkv", ".avi")

SYSTEM_PROMPT = (
    "Tu es un assistant IA utile qui répond en français, de façon claire et concise. "
    "Tu peux lire du code, expliquer des erreurs, proposer des corrections, "
    "et t'appuyer sur le contexte fourni (pages web, transcriptions, images, mémoire)."
)


class Assistant:
    def __init__(self):
        self.memory = Memory()
        self.llm = LocalLLM()

    # ---------- détection d'intention ----------
    def looks_like_code(self, text: str) -> bool:
        code_markers = ["def ", "import ", "class ", "{", "}", ";", "print(", "SELECT ", "function "]
        return any(m in text for m in code_markers) or "```" in text

    def extract_code_block(self, text: str) -> str:
        match = re.search(r"```(?:python)?\n(.*?)```", text, re.DOTALL)
        return match.group(1) if match else text

    def run_python_code(self, code: str) -> str:
        """Exécute du code Python fourni par l'utilisateur, dans un processus
        isolé avec un timeout, pour l'aider à résoudre / tester un problème."""
        with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as f:
            f.write(code)
            path = f.name
        try:
            result = subprocess.run(
                [sys.executable, path],
                capture_output=True, text=True, timeout=10
            )
            output = result.stdout
            if result.stderr:
                output += "\n[stderr]\n" + result.stderr
            return output.strip() or "(le code s'est exécuté sans sortie)"
        except subprocess.TimeoutExpired:
            return "[Erreur] Le code a dépassé le temps limite (10s)."
        finally:
            os.remove(path)

    # ---------- construction du contexte pour le LLM ----------
    def build_context(self, extra: str = "") -> str:
        history = self.memory.get_recent_context(8)
        history_text = "\n".join(f"{m['role']}: {m['content']}" for m in history)
        facts = self.memory.all_facts()
        facts_text = "\n".join(f"- {f}" for f in facts) if facts else "(aucun)"
        return (
            f"Faits mémorisés :\n{facts_text}\n\n"
            f"Historique récent :\n{history_text}\n\n"
            f"{extra}"
        )

    # ---------- traitement d'un message utilisateur ----------
    def handle(self, user_input: str) -> str:
        self.memory.add_message("user", user_input)

        # 1. mémoriser un fait explicite
        if user_input.lower().startswith(("retiens que", "souviens-toi que")):
            fact = user_input.split("que", 1)[1].strip()
            self.memory.remember_fact(fact)
            return f"D'accord, je retiens : {fact}"

        # 2. construction / modification d'un site web
        lower_input = user_input.lower()
        if any(k in lower_input for k in ["construis un site", "crée un site", "génère un site", "fais-moi un site"]):
            description = re.sub(
                r"^(construis un site( web)?|crée un site( web)?|génère un site( web)?|fais-moi un site( web)?)\s*",
                "", user_input, flags=re.IGNORECASE
            )
            result = build_website(description.strip() or user_input)
            self.memory.add_message("assistant", result)
            return result

        if any(k in lower_input for k in ["modifie le site", "change le site", "mets à jour le site"]):
            instruction = re.sub(
                r"^(modifie le site|change le site|mets à jour le site)\s*",
                "", user_input, flags=re.IGNORECASE
            )
            result = refine_website(instruction.strip() or user_input)
            self.memory.add_message("assistant", result)
            return result

        # 3. conseils / codes de triche pour un jeu vidéo
        cheat_match = re.search(
            r"(?:codes?( de triche)?|cheat codes?)\s+(?:pour|de|du|sur)\s+(.+)",
            user_input, flags=re.IGNORECASE
        )
        tips_match = re.search(
            r"(?:astuces?|conseils?|soluce)\s+(?:pour|sur|du jeu)\s+(.+)",
            user_input, flags=re.IGNORECASE
        )
        full_help_match = re.search(
            r"aide[- ]moi sur le jeu\s+(.+)", user_input, flags=re.IGNORECASE
        )

        if full_help_match:
            game = full_help_match.group(1).strip(" ?.")
            result = get_full_game_help(game)
            self.memory.add_message("assistant", result)
            return result

        if cheat_match:
            game = cheat_match.group(2).strip(" ?.")
            result = get_cheat_codes(game)
            self.memory.add_message("assistant", result)
            return result

        if tips_match:
            game = tips_match.group(1).strip(" ?.")
            result = get_game_tips(game)
            self.memory.add_message("assistant", result)
            return result

        # 4. recherche web explicite
        if user_input.lower().startswith(("cherche sur internet", "recherche", "trouve sur le web")):
            query = re.sub(r"^(cherche sur internet|recherche|trouve sur le web)\s*", "",
                            user_input, flags=re.IGNORECASE)
            results = search_web(query)
            extra = "Résultats de recherche web :\n" + "\n\n".join(results)
            prompt = self.build_context(extra) + f"\n\nRésume ces résultats pour répondre à : {query}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 5. lien YouTube
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

        # 6. chemin vers une image locale
        image_path = next((w for w in words if w.lower().endswith(IMAGE_EXTENSIONS)), None)
        if image_path and os.path.exists(image_path):
            text_in_image = ocr_image(image_path)
            caption = describe_image(image_path)
            extra = f"Description de l'image : {caption}\nTexte détecté dans l'image : {text_in_image}"
            prompt = self.build_context(extra) + f"\n\nQuestion de l'utilisateur : {user_input}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 7. chemin vers une vidéo locale
        video_path = next((w for w in words if w.lower().endswith(VIDEO_EXTENSIONS)), None)
        if video_path and os.path.exists(video_path):
            transcript = transcribe_video_audio(video_path)
            extra = f"Transcription audio de la vidéo :\n{transcript}"
            prompt = self.build_context(extra) + f"\n\nQuestion de l'utilisateur : {user_input}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 8. demande d'exécution de code ("exécute", "lance ce code", "teste ce code")
        if any(k in user_input.lower() for k in ["exécute ce code", "lance ce code", "teste ce code"]):
            code = self.extract_code_block(user_input)
            result = self.run_python_code(code)
            extra = f"Résultat de l'exécution du code :\n{result}"
            prompt = self.build_context(extra) + "\n\nExplique ce résultat à l'utilisateur, et propose une correction si besoin."
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 9. code à analyser/résoudre (sans forcément l'exécuter)
        if self.looks_like_code(user_input):
            extra = "L'utilisateur partage du code à analyser ou corriger."
            prompt = self.build_context(extra) + f"\n\n{user_input}"
            answer = self.llm.generate(prompt, SYSTEM_PROMPT)
            self.memory.add_message("assistant", answer)
            return answer

        # 10. discussion générale
        prompt = self.build_context() + f"\n\nUtilisateur : {user_input}"
        answer = self.llm.generate(prompt, SYSTEM_PROMPT)
        self.memory.add_message("assistant", answer)
        return answer


def main():
    assistant = Assistant()
    print("=== Assistant IA local (tape 'quit' pour sortir) ===")
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
    