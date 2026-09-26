"""
memory.py
----------
Gère la mémoire persistante de l'IA :
- historique des conversations
- "faits" que l'utilisateur demande de retenir
- recherche simple par mots-clés dans les souvenirs

Stockage : simple fichier JSON local (pas de base de données externe,
pas d'API, tout reste sur la machine de l'utilisateur).
"""

import json
import os
import time
from typing import List, Dict, Any


class Memory:
    def __init__(self, path: str = "memory_store.json"):
        self.path = path
        self.data: Dict[str, Any] = {"conversations": [], "facts": []}
        self._load()

    # ---------- persistance ----------
    def _load(self):
        if os.path.exists(self.path):
            try:
                with open(self.path, "r", encoding="utf-8") as f:
                    self.data = json.load(f)
            except (json.JSONDecodeError, IOError):
                # fichier corrompu ou vide -> on repart de zéro sans planter
                self.data = {"conversations": [], "facts": []}

    def _save(self):
        with open(self.path, "w", encoding="utf-8") as f:
            json.dump(self.data, f, ensure_ascii=False, indent=2)

    # ---------- historique de conversation ----------
    def add_message(self, role: str, content: str):
        self.data["conversations"].append({
            "role": role,
            "content": content,
            "timestamp": time.time()
        })
        # on garde les 200 derniers messages pour ne pas grossir indéfiniment
        self.data["conversations"] = self.data["conversations"][-200:]
        self._save()

    def get_recent_context(self, n: int = 10) -> List[Dict[str, str]]:
        return self.data["conversations"][-n:]

    # ---------- faits mémorisés explicitement ----------
    def remember_fact(self, fact: str):
        self.data["facts"].append({"fact": fact, "timestamp": time.time()})
        self._save()

    def search_facts(self, query: str) -> List[str]:
        query_words = set(query.lower().split())
        results = []
        for entry in self.data["facts"]:
            fact_words = set(entry["fact"].lower().split())
            if query_words & fact_words:
                results.append(entry["fact"])
        return results

    def all_facts(self) -> List[str]:
        return [e["fact"] for e in self.data["facts"]]
    