const params = new URLSearchParams(window.location.search);

let currentQuery = params.get('q') || "";
let currentCategory = params.get('category') || "";
let currentProjectId = params.get('project') || null;
const flashMessage = "";

let rootQuery = currentQuery;

function categoryToPath(cat) {
    switch (cat) {
        case 'images': return '/search/images';
        case 'videos': return '/search/videos';
        case 'news': return '/search/news';
        case 'map':
        case 'maps': return '/search/maps';
        default: return '/search';
    }
}

function showToast(message) {
    const toast = document.getElementById('toast');
    toast.textContent = message;
    toast.classList.add('show');
    clearTimeout(window._toastTimeout);
    window._toastTimeout = setTimeout(
        () => toast.classList.remove('show'),
        5000
    );
}

if (flashMessage) {
    showToast(flashMessage);
}


// --- Sous-recherche : combine la requête actuelle + l'affinage ---

const subSearchForm = document.getElementById('subSearchForm');

if (subSearchForm) {
    subSearchForm.addEventListener('submit', function (e) {
        const subInput = document.getElementById('subSearchInput');
        const hiddenQuery = document.getElementById('subSearchHiddenQuery');
        const subValue = subInput.value.trim();

        if (!subValue) {
            e.preventDefault();
            return;
        }

        hiddenQuery.value = `${rootQuery} ${subValue}`.trim();
    });
}


// Chargement des projets déplacé entièrement dans le panneau "Mes projets"


// --- Barre de menu du bas ---

const navReseaux = document.getElementById('navReseaux');

if (navReseaux) {
    navReseaux.addEventListener('click', function () {
        window.location.href = '/reseaux';
    });
}


/* =========================================
   ASSISTANT IA NOVA
   ========================================= */

const iaOverlay = document.getElementById('iaOverlay');
const iaPanelClose = document.getElementById('iaPanelClose');
const iaChatForm = document.getElementById('iaChatForm');
const iaMessage = document.getElementById('iaMessage');
const iaMessages = document.getElementById('iaMessages');
const iaWelcome = document.querySelector('.ia-welcome');
const iaSend = document.getElementById('iaSend');

const iaNewChat = document.getElementById('iaNewChat');
const iaNewChatScreen = document.getElementById('iaNewChatScreen');

function showNewIaChat() {
    // Efface les anciens messages
    if (iaMessages) {
        iaMessages.innerHTML = '';
    }

    // Affiche l'écran "Nouveau chat"
    if (iaNewChatScreen) {
        iaNewChatScreen.style.display = 'flex';
    }

    // Réinitialise le champ
    if (iaMessage) {
        iaMessage.value = '';
        iaMessage.style.height = 'auto';
        iaMessage.placeholder = 'Écrire un message...';
    }

    // Réactive l'envoi
    if (iaMessage) {
        iaMessage.disabled = false;
    }

    if (iaSend) {
        iaSend.disabled = false;
    }

    // Remet le focus sur le champ
    if (iaMessage) {
        setTimeout(() => {
            iaMessage.focus();
        }, 100);
    }
}

if (iaNewChat) {
    iaNewChat.addEventListener('click', function () {
        showNewIaChat();
    });
}

const iaCategories = document.querySelectorAll('.ia-category');

let currentIaCategory = 'chat';

iaCategories.forEach(category => {
    category.addEventListener('click', function () {
        currentIaCategory = this.dataset.category;

        if (iaNewChatScreen) {
            iaNewChatScreen.style.display = 'none';
        }

        if (iaMessage) {
            iaMessage.focus();
        }

        console.log(
            'Mode IA sélectionné :',
            currentIaCategory
        );
    });
});

const IA_URL = '/api/ia/chat';


// ---------- Ouvrir l'assistant ----------

const navIA = document.getElementById('navIA');

if (navIA && iaOverlay) {
    navIA.addEventListener('click', function (event) {
        event.preventDefault();

        iaOverlay.classList.add('show');

        if (iaMessage) {
            setTimeout(() => {
                iaMessage.focus();
            }, 100);
        }
    });
}


// ---------- Fermer l'assistant ----------

if (iaPanelClose && iaOverlay) {
    iaPanelClose.addEventListener('click', function () {
        iaOverlay.classList.remove('show');
    });
}


// ---------- Fermer en cliquant autour du panneau ----------

if (iaOverlay) {
    iaOverlay.addEventListener('click', function (event) {
        if (event.target === iaOverlay) {
            iaOverlay.classList.remove('show');
        }
    });
}


// ---------- Fermer avec Échap ----------

document.addEventListener('keydown', function (event) {
    if (
        event.key === 'Escape' &&
        iaOverlay &&
        iaOverlay.classList.contains('show')
    ) {
        iaOverlay.classList.remove('show');
    }
});


// ---------- Ajouter un message ----------

function addIaMessage(content, type) {
    if (!iaMessages) {
        return;
    }

    const message = document.createElement('div');

    message.className = `ia-message ${type}`;

    const messageContent = document.createElement('div');

    messageContent.className = 'ia-message-content';

    messageContent.textContent = content;

    message.appendChild(messageContent);

    iaMessages.appendChild(message);

    const iaChat = document.getElementById('iaPanelBody');

    if (iaChat) {
        iaChat.scrollTop = iaChat.scrollHeight;
    }
}


// ---------- Envoyer un message ----------

if (iaChatForm) {
    iaChatForm.addEventListener('submit', async function (event) {
        event.preventDefault();

        if (!iaMessage || !iaSend) {
            return;
        }

        const message = iaMessage.value.trim();

        if (!message) {
            return;
        }

        // Afficher le message utilisateur
        addIaMessage(message, 'user');

        if (iaNewChatScreen) {
            iaNewChatScreen.style.display = 'none';
        }

        // Masquer le message de bienvenue
        if (iaWelcome) {
            iaWelcome.style.display = 'none';
        }

        // Vider le champ
        iaMessage.value = '';

        // Bloquer pendant la requête
        iaMessage.disabled = true;
        iaSend.disabled = true;

        // Message de chargement
        const loading = document.createElement('div');

        loading.className = 'ia-message assistant';

        loading.innerHTML = `
            <div class="ia-message-content">
                Réflexion...
            </div>
        `;

        iaMessages.appendChild(loading);

        const iaChat = document.getElementById('iaPanelBody');

        if (iaChat) {
            iaChat.scrollTop = iaChat.scrollHeight;
        }

        try {
            const response = await fetch(IA_URL, {
                method: 'POST',

                headers: {
                    'Content-Type': 'application/json'
                },

                credentials: 'same-origin',

                body: JSON.stringify({
                    message: message,
                    category: currentIaCategory
                })
            });

            const data = await response.json();

            // Supprimer "Réflexion..."
            loading.remove();

            if (!response.ok) {
                addIaMessage(
                    data.error || 'Une erreur est survenue.',
                    'assistant'
                );

                return;
            }

            if (data.response) {
                addIaMessage(
                    data.response,
                    'assistant'
                );
            } else {
                addIaMessage(
                    'L’assistant n’a pas retourné de réponse.',
                    'assistant'
                );
            }

        } catch (error) {
            console.error(
                'Erreur assistant IA :',
                error
            );

            loading.remove();

            addIaMessage(
                'Impossible de contacter l’assistant IA.',
                'assistant'
            );

        } finally {
            iaMessage.disabled = false;
            iaSend.disabled = false;

            iaMessage.focus();
        }
    });
}


// ---------- Entrée = envoyer ----------

if (iaMessage) {
    iaMessage.addEventListener('keydown', function (event) {

        if (
            event.key === 'Enter' &&
            !event.shiftKey
        ) {
            event.preventDefault();

            if (iaChatForm) {
                iaChatForm.requestSubmit();
            }
        }
    });
}


// ---------- Hauteur automatique du textarea ----------

if (iaMessage) {
    iaMessage.addEventListener('input', function () {

        this.style.height = 'auto';

        this.style.height = Math.min(
            this.scrollHeight,
            150
        ) + 'px';
    });
}


// --- Carte ---

const navCarte = document.getElementById('navCarte');

if (navCarte) {
    navCarte.addEventListener('click', function () {
        const url = new URL(
            window.location.origin + '/search/maps'
        );

        if (currentQuery) {
            url.searchParams.set('q', currentQuery);
        }

        if (currentProjectId) {
            url.searchParams.set('project', currentProjectId);
        }

        window.location.href = url.toString();
    });
}


// --- Paramètres ---

const settingsPanel = document.getElementById('settingsPanel');
const navParametres = document.getElementById('navParametres');

if (navParametres && settingsPanel) {
    navParametres.addEventListener('click', function () {
        settingsPanel.classList.toggle('show');
    });
}


// --- Bouton Web : retour à la page d'accueil ---

const navWeb = document.getElementById('navWeb');

if (navWeb) {
    navWeb.addEventListener('click', function () {
        window.location.href = '/';
    });
}


// --- Panneau Mes projets ---

const projectsPanel = document.getElementById('projectsPanel');
const projectsPanelList = document.getElementById('projectsPanelList');


// --- Panneau Historique de recherche ---

const historyPanel = document.getElementById('historyPanel');
const historyPanelList = document.getElementById('historyPanelList');


async function renderProjectsPanel() {
    projectsPanelList.innerHTML = 'Chargement...';

    try {
        const res = await fetch('/api/projects');
        const projects = await res.json();

        const newCardHtml = `
            <button
                class="panel-card-new"
                id="projectsPanelNewBtn"
                title="Nouveau projet"
            >
                +
            </button>
        `;

        if (!projects.length) {
            projectsPanelList.innerHTML =
                newCardHtml +
                '<div class="panel-list-empty" style="grid-column: 1 / -1;">Aucun projet pour l\'instant.</div>';
        } else {
            const cards = projects.map(p => {
                const url = new URL(
                    window.location.origin + '/search'
                );

                url.searchParams.set(
                    'q',
                    currentQuery || ''
                );

                url.searchParams.set(
                    'project',
                    p.id
                );

                const date = new Date(
                    p.created_at.replace(' ', 'T') + 'Z'
                ).toLocaleDateString('fr-FR');

                return `
                    <div
                        class="panel-card"
                        style="position:relative; text-decoration:none;"
                    >
                        <a
                            href="${url.toString()}"
                            style="text-decoration:none; color:inherit; display:block;"
                        >
                            <div class="panel-card-title">
                                📁 ${p.name}
                            </div>

                            <div class="panel-card-meta">
                                Créé le ${date}
                            </div>
                        </a>

                        <div
                            style="
                                position:absolute;
                                top:6px;
                                right:6px;
                                display:flex;
                                gap:4px;
                            "
                        >
                            <button
                                class="panel-card-action"
                                data-action="invite"
                                data-id="${p.id}"
                                title="Inviter (lien de groupe)"
                                style="
                                    background:none;
                                    border:none;
                                    cursor:pointer;
                                    font-size:0.8rem;
                                    opacity:0.7;
                                "
                            >
                                👥
                            </button>

                            <button
                                class="panel-card-action"
                                data-action="delete"
                                data-id="${p.id}"
                                title="Supprimer ce projet"
                                style="
                                    background:none;
                                    border:none;
                                    cursor:pointer;
                                    font-size:0.8rem;
                                    opacity:0.7;
                                "
                            >
                                🗑️
                            </button>
                        </div>
                    </div>
                `;
            }).join('');

            projectsPanelList.innerHTML =
                newCardHtml + cards;
        }

        const projectsPanelNewBtn =
            document.getElementById(
                'projectsPanelNewBtn'
            );

        if (projectsPanelNewBtn) {
            projectsPanelNewBtn.addEventListener(
                'click',
                async function () {
                    const name = prompt(
                        'Nom du nouveau projet :'
                    );

                    if (!name || !name.trim()) {
                        return;
                    }

                    try {
                        const res = await fetch(
                            '/api/projects',
                            {
                                method: 'POST',

                                headers: {
                                    'Content-Type':
                                        'application/json'
                                },

                                body: JSON.stringify({
                                    name: name.trim()
                                })
                            }
                        );

                        const result = await res.json();

                        showToast(result.message);

                        if (result.success) {
                            await renderProjectsPanel();
                        }

                    } catch (err) {
                        console.error(
                            'Erreur création projet:',
                            err
                        );

                        showToast(
                            "❌ Erreur réseau lors de la création du projet."
                        );
                    }
                }
            );
        }

        projectsPanelList
            .querySelectorAll('.panel-card-action')
            .forEach(btn => {
                btn.addEventListener(
                    'click',
                    async function (e) {
                        e.preventDefault();
                        e.stopPropagation();

                        const id = this.dataset.id;

                        if (
                            this.dataset.action ===
                            'invite'
                        ) {
                            const link =
                                `${window.location.origin}/join/${id}`;

                            navigator.clipboard
                                .writeText(link)
                                .then(() => {
                                    showToast(
                                        `🔗 Lien d'invitation copié : ${link}`
                                    );
                                })
                                .catch(() => {
                                    showToast(
                                        `🔗 Lien d'invitation : ${link}`
                                    );
                                });

                        } else if (
                            this.dataset.action ===
                            'delete'
                        ) {
                            if (
                                !confirm(
                                    "Supprimer définitivement ce projet ? Cette action est irréversible."
                                )
                            ) {
                                return;
                            }

                            try {
                                const res =
                                    await fetch(
                                        `/api/projects/${id}`,
                                        {
                                            method:
                                                'DELETE'
                                        }
                                    );

                                const result =
                                    await res.json();

                                showToast(
                                    result.message
                                );

                                if (result.success) {
                                    await renderProjectsPanel();
                                }

                            } catch (err) {
                                console.error(
                                    'Erreur suppression projet:',
                                    err
                                );

                                showToast(
                                    "❌ Erreur réseau lors de la suppression du projet."
                                );
                            }
                        }
                    }
                );
            });

    } catch (err) {
        console.error(
            'Erreur chargement projets:',
            err
        );

        projectsPanelList.innerHTML =
            '<div class="panel-list-empty">Erreur de chargement.</div>';
    }
}


const navProjets = document.getElementById('navProjets');

if (navProjets) {
    navProjets.addEventListener(
        'click',
        async function () {
            const isShowing =
                projectsPanel.classList.contains(
                    'show'
                );

            historyPanel.classList.remove('show');
            settingsPanel.classList.remove('show');

            projectsPanel.classList.toggle('show');

            if (isShowing) {
                return;
            }

            await renderProjectsPanel();
        }
    );
}


// --- Panneau Historique de recherche ---

const navHistorique =
    document.getElementById('navHistorique');

if (navHistorique) {
    navHistorique.addEventListener(
        'click',
        async function () {
            const isShowing =
                historyPanel.classList.contains(
                    'show'
                );

            projectsPanel.classList.remove('show');
            settingsPanel.classList.remove('show');

            historyPanel.classList.toggle('show');

            if (isShowing) {
                return;
            }

            historyPanelList.innerHTML =
                'Chargement...';

            try {
                const res = await fetch(
                    '/api/history'
                );

                const history = await res.json();

                const newCardHtml = `
                    <a
                        class="panel-card-new"
                        href="/"
                        title="Nouvelle recherche"
                    >
                        +
                    </a>
                `;

                if (!history.length) {
                    historyPanelList.innerHTML =
                        newCardHtml +
                        '<div class="panel-list-empty" style="grid-column: 1 / -1;">Aucune recherche récente.</div>';
                } else {
                    const cards = history.map(
                        node => {
                            const url =
                                new URL(
                                    window.location.origin +
                                    categoryToPath(
                                        node.category
                                    )
                                );

                            url.searchParams.set(
                                'q',
                                node.query
                            );

                            if (node.parent_id) {
                                url.searchParams.set(
                                    'parent',
                                    node.parent_id
                                );
                            }

                            if (node.project_id) {
                                url.searchParams.set(
                                    'project',
                                    node.project_id
                                );
                            }

                            const date =
                                new Date(
                                    node.created_at.replace(
                                        ' ',
                                        'T'
                                    ) + 'Z'
                                ).toLocaleString(
                                    'fr-FR',
                                    {
                                        day: '2-digit',
                                        month: '2-digit',
                                        hour: '2-digit',
                                        minute: '2-digit'
                                    }
                                );

                            return `
                                <a
                                    class="panel-card"
                                    href="${url.toString()}"
                                >
                                    <div class="panel-card-title">
                                        🔎 ${node.query}
                                    </div>

                                    <div class="panel-card-meta">
                                        ${date}
                                    </div>
                                </a>
                            `;
                        }
                    ).join('');

                    historyPanelList.innerHTML =
                        newCardHtml + cards;
                }

            } catch (err) {
                console.error(
                    'Erreur chargement historique:',
                    err
                );

                historyPanelList.innerHTML =
                    '<div class="panel-list-empty">Erreur de chargement.</div>';
            }
        }
    );
}


// --- Fermeture des panneaux ---

document.addEventListener(
    'click',
    function (e) {

        if (
            settingsPanel &&
            !settingsPanel.contains(e.target) &&
            e.target.id !== 'navParametres' &&
            !e.target.closest('#navParametres')
        ) {
            settingsPanel.classList.remove('show');
        }

        if (
            projectsPanel &&
            !projectsPanel.contains(e.target) &&
            e.target.id !== 'navProjets' &&
            !e.target.closest('#navProjets')
        ) {
            projectsPanel.classList.remove('show');
        }

        if (
            historyPanel &&
            !historyPanel.contains(e.target) &&
            e.target.id !== 'navHistorique' &&
            !e.target.closest('#navHistorique')
        ) {
            historyPanel.classList.remove('show');
        }
    }
);


// --- Chargement lazy des résultats ---

let loadMorePage = 2;

const shownUrls = new Set(
    Array.from(
        document.querySelectorAll(
            '#resultsGrid .result-url'
        )
    ).map(
        el => el.textContent.trim()
    )
);


async function loadMore() {
    const btn =
        document.getElementById(
            'loadMoreBtn'
        );

    const spinner =
        document.getElementById(
            'loadingSpinner'
        );

    const grid =
        document.getElementById(
            'resultsGrid'
        );

    const count =
        document.getElementById(
            'resultCount'
        );

    btn.disabled = true;

    spinner.style.display = 'block';

    try {
        const response =
            await fetch(
                `/api/load-more?q=${encodeURIComponent(
                    currentQuery
                )}&page=${loadMorePage}`
            );

        const allResults =
            await response.json();

        // Sécurité anti-doublon :
        // on ignore toute URL déjà affichée à l'écran
        const newResults =
            allResults.filter(
                r => !shownUrls.has(r.url)
            );

        if (allResults.length === 0) {
            btn.textContent =
                'Plus de résultats disponibles';

            btn.style.display = 'none';

            return;
        }

        newResults.forEach(result => {
            shownUrls.add(result.url);

            const div =
                document.createElement('div');

            div.className = 'result';

            div.innerHTML = `
                <a
                    href="${result.url}"
                    class="result-title"
                >
                    ${result.title}
                </a>

                <div class="result-url">
                    ${result.url}
                </div>

                <div class="result-snippet">
                    ${result.snippet}
                </div>
            `;

            grid.appendChild(div);
        });

        loadMorePage += 1;

        const currentCount =
            parseInt(
                count.textContent
            ) + newResults.length;

        count.textContent =
            currentCount;

    } catch (err) {
        console.error(
            'Erreur:',
            err
        );

        btn.textContent =
            'Erreur de chargement';

    } finally {
        btn.disabled = false;

        spinner.style.display =
            'none';
    }
}