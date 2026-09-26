"""
media_tools.py
---------------
Donne à l'IA la capacité de "lire" des images et des vidéos, en local :
- OCR (texte présent dans une image)               -> pytesseract
- description visuelle d'une image                  -> modèle local BLIP (transformers)
- transcription audio d'une vidéo                    -> Whisper local (openai-whisper)

Tous ces modèles tournent sur la machine de l'utilisateur, sans API ni
clé. Le premier lancement télécharge les poids du modèle une seule fois.

Installation :
    pip install pytesseract pillow opencv-python transformers torch openai-whisper
    + installer le binaire "tesseract-ocr" sur le système (ex: apt install tesseract-ocr)
"""

from PIL import Image


def ocr_image(path: str) -> str:
    """Extrait le texte visible dans une image."""
    try:
        import pytesseract
    except ImportError:
        return "[Erreur] Installe : pip install pytesseract (+ le binaire tesseract-ocr)"

    try:
        img = Image.open(path)
        text = pytesseract.image_to_string(img, lang="fra+eng")
        return text.strip() or "(aucun texte détecté dans l'image)"
    except Exception as e:
        return f"[Erreur OCR] {e}"


_blip_model = None
_blip_processor = None


def describe_image(path: str) -> str:
    """Décrit le contenu visuel d'une image avec un modèle local (BLIP)."""
    global _blip_model, _blip_processor
    try:
        from transformers import BlipProcessor, BlipForConditionalGeneration
    except ImportError:
        return "[Erreur] Installe : pip install transformers torch"

    try:
        if _blip_model is None:
            _blip_processor = BlipProcessor.from_pretrained(
                "Salesforce/blip-image-captioning-base"
            )
            _blip_model = BlipForConditionalGeneration.from_pretrained(
                "Salesforce/blip-image-captioning-base"
            )
        img = Image.open(path).convert("RGB")
        inputs = _blip_processor(img, return_tensors="pt")
        out = _blip_model.generate(**inputs, max_new_tokens=40)
        caption = _blip_processor.decode(out[0], skip_special_tokens=True)
        return caption
    except Exception as e:
        return f"[Erreur description image] {e}"


def transcribe_video_audio(path: str, model_size: str = "base") -> str:
    """Transcrit l'audio d'un fichier vidéo/audio local avec Whisper (local)."""
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
    