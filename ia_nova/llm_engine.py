"""
llm_engine.py
-------------
Connexion à Ollama (modèle IA local)
"""

import json
import urllib.request
import urllib.error


OLLAMA_URL = "http://localhost:11434/api/generate"

# Modèle disponible sur ta machine
DEFAULT_MODEL = "llama3.2:latest"


class LocalLLM:
    def __init__(self, model: str = DEFAULT_MODEL):
        self.model = model

    def is_available(self) -> bool:
        """Vérifie que Ollama fonctionne."""
        try:
            req = urllib.request.Request(
                "http://localhost:11434/api/tags"
            )
            urllib.request.urlopen(req, timeout=2)
            return True

        except (urllib.error.URLError, OSError):
            return False


    def generate(self, prompt: str, system: str = "") -> str:
        """
        Envoie une question au modèle Ollama
        et retourne la réponse.
        """

        if not self.is_available():
            return (
                "[Erreur] Ollama n'est pas lancé.\n"
                "Lance : ollama serve"
            )


        payload = {
            "model": self.model,
            "prompt": prompt,
            "system": system,
            "stream": False
        }


        data = json.dumps(payload).encode("utf-8")


        req = urllib.request.Request(
            OLLAMA_URL,
            data=data,
            headers={
                "Content-Type": "application/json"
            }
        )


        try:
            with urllib.request.urlopen(req, timeout=60) as response:

                result = json.loads(
                    response.read().decode("utf-8")
                )

                return result.get(
                    "response",
                    ""
                ).strip()


        except urllib.error.HTTPError as e:
            return f"[Erreur Ollama HTTP {e.code}] {e.reason}"


        except urllib.error.URLError as e:
            return f"[Erreur connexion Ollama] {e}"


        except json.JSONDecodeError:
            return "[Erreur] Réponse Ollama invalide."
        