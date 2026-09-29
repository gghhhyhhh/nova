document.addEventListener("DOMContentLoaded", () => {
    // Afficher / masquer les mots de passe
    document.querySelectorAll(".password-toggle").forEach((button) => {
        button.addEventListener("click", () => {
            const input = document.getElementById(button.dataset.target);

            if (!input) {
                return;
            }

            const visible = input.type === "text";

            input.type = visible ? "password" : "text";
            button.textContent = visible ? "Afficher" : "Masquer";
        });
    });

    // Évite les doubles clics sur les formulaires
    document.querySelectorAll(".auth-form").forEach((form) => {
        form.addEventListener("submit", () => {
            const button = form.querySelector("button[type='submit']");

            if (!button) {
                return;
            }

            button.disabled = true;
            button.dataset.originalText = button.textContent;
            button.textContent = "Chargement...";
        });
    });

    // Code de vérification : uniquement des chiffres
    const codeInput = document.querySelector(".code-input");

    if (codeInput) {
        codeInput.addEventListener("input", () => {
            codeInput.value = codeInput.value
                .replace(/\D/g, "")
                .slice(0, 6);
        });

        codeInput.focus();
    }

    // Empêche plusieurs clics sur "Renvoyer le code"
    const resendForm = document.querySelector(".resend-form");

    if (resendForm) {
        resendForm.addEventListener("submit", () => {
            const button = resendForm.querySelector("button");

            if (!button) {
                return;
            }

            button.disabled = true;
            button.textContent = "Envoi...";
        });
    }
});
