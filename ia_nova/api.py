import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from ia_complete import Assistant


HOST = "127.0.0.1"
PORT = 3020

assistant = Assistant()


class IAHandler(BaseHTTPRequestHandler):

    def send_json(self, status: int, data: dict):
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")

        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Access-Control-Allow-Methods", "POST, OPTIONS")
        self.end_headers()

        self.wfile.write(body)

    def do_OPTIONS(self):
        self.send_json(200, {"ok": True})

    def do_GET(self):
        if self.path == "/":
            self.send_json(200, {
                "ok": True,
                "service": "ia_nova",
                "message": "Assistant IA NOVA disponible"
            })
            return

        if self.path == "/health":
            self.send_json(200, {
                "ok": assistant.llm.is_available()
            })
            return

        self.send_json(404, {
            "ok": False,
            "error": "Route inconnue"
        })

    def do_POST(self):
        if self.path != "/chat":
            self.send_json(404, {
                "ok": False,
                "error": "Route inconnue"
            })
            return

        try:
            content_length = int(self.headers.get("Content-Length", "0"))
            raw_body = self.rfile.read(content_length)
            data = json.loads(raw_body.decode("utf-8"))

            message = str(data.get("message", "")).strip()

            if not message:
                self.send_json(400, {
                    "ok": False,
                    "error": "Le message est obligatoire"
                })
                return

            response = assistant.handle(message)

            self.send_json(200, {
                "ok": True,
                "response": response
            })

        except json.JSONDecodeError:
            self.send_json(400, {
                "ok": False,
                "error": "JSON invalide"
            })

        except Exception as e:
            print(f"[IA] Erreur : {e}", flush=True)

            self.send_json(500, {
                "ok": False,
                "error": "Erreur interne de l'assistant"
            })

    def log_message(self, format, *args):
        print(f"[IA] {format % args}", flush=True)


def main():
    server = ThreadingHTTPServer((HOST, PORT), IAHandler)

    print(f"🤖 ia_nova API démarrée sur http://{HOST}:{PORT}", flush=True)

    if assistant.llm.is_available():
        print(
            f"✅ Ollama disponible — modèle : {assistant.llm.model}",
            flush=True
        )
    else:
        print(
            "⚠️ Ollama n'est pas disponible.",
            flush=True
        )

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
