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
            window._toastTimeout = setTimeout(() => toast.classList.remove('show'), 5000);
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
        document.getElementById('navReseaux').addEventListener('click', function () {
            window.location.href = '/reseaux';
        });

        const iaOverlay = document.getElementById('iaOverlay');
        const iaIframe = document.getElementById('iaIframe');
        const iaPanelBody = document.getElementById('iaPanelBody');
        const IA_URL = 'http://localhost:3010';

        function showIaFallback() {
            iaPanelBody.innerHTML = `
                <div class="ia-panel-fallback">
                    <div style="font-size:2.5rem;">🤖💤</div>
                    <p><strong>L'assistant IA locale ne répond pas.</strong></p>
                    <p style="opacity:0.75; font-size:0.85rem; max-width:420px;">
                        Ce service tourne à part et doit être lancé manuellement :<br>
                        <code style="background:#101b11; padding:2px 6px; border-radius:4px;">cd ~/ia-locale && docker-compose up -d</code><br>
                        <span style="opacity:0.6;">(ou lance simplement <code style="background:#101b11; padding:2px 6px; border-radius:4px;">./start.sh</code> depuis ~/nova, qui s'en charge automatiquement)</span>
                    </p>
                </div>
            `;
        }

        document.getElementById('navIA').addEventListener('click', function () {
            iaOverlay.classList.add('show');

            // On (re)teste la disponibilité du service à chaque ouverture
            fetch(IA_URL, { mode: 'no-cors' })
                .then(() => {
                    if (iaIframe.getAttribute('src') !== IA_URL) {
                        iaPanelBody.innerHTML = '<iframe id="iaIframe" src="' + IA_URL + '" title="Assistant IA locale"></iframe>';
                    }
                })
                .catch(showIaFallback);

            // Si l'iframe elle-même échoue à charger (timeout de secours)
            setTimeout(() => {
                const frame = document.getElementById('iaIframe');
                if (frame && !frame.dataset.loaded) {
                    // laisse une chance au fetch ci-dessus de trancher ; sinon on ne force rien de plus ici
                }
            }, 3000);
        });

        document.getElementById('iaPanelClose').addEventListener('click', function () {
            iaOverlay.classList.remove('show');
        });

        iaOverlay.addEventListener('click', function (e) {
            if (e.target === iaOverlay) iaOverlay.classList.remove('show');
        });

        document.getElementById('navCarte').addEventListener('click', function () {
            const url = new URL(window.location.origin + '/search/maps');
            if (currentQuery) url.searchParams.set('q', currentQuery);
            if (currentProjectId) url.searchParams.set('project', currentProjectId);
            window.location.href = url.toString();
        });

        const settingsPanel = document.getElementById('settingsPanel');
        document.getElementById('navParametres').addEventListener('click', function () {
            settingsPanel.classList.toggle('show');
        });

        // --- Bouton Web : retour à la page d'accueil ---
        document.getElementById('navWeb').addEventListener('click', function () {
            window.location.href = '/';
        });

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

                const newCardHtml = `<button class="panel-card-new" id="projectsPanelNewBtn" title="Nouveau projet">+</button>`;

                if (!projects.length) {
                    projectsPanelList.innerHTML = newCardHtml +
                        '<div class="panel-list-empty" style="grid-column: 1 / -1;">Aucun projet pour l\'instant.</div>';
                } else {
                    const cards = projects.map(p => {
                        const url = new URL(window.location.origin + '/search');
                        url.searchParams.set('q', currentQuery || '');
                        url.searchParams.set('project', p.id);
                        const date = new Date(p.created_at.replace(' ', 'T') + 'Z').toLocaleDateString('fr-FR');

                        return `<div class="panel-card" style="position:relative; text-decoration:none;">
                            <a href="${url.toString()}" style="text-decoration:none; color:inherit; display:block;">
                                <div class="panel-card-title">📁 ${p.name}</div>
                                <div class="panel-card-meta">Créé le ${date}</div>
                            </a>
                            <div style="position:absolute; top:6px; right:6px; display:flex; gap:4px;">
                                <button class="panel-card-action" data-action="invite" data-id="${p.id}" title="Inviter (lien de groupe)" style="background:none; border:none; cursor:pointer; font-size:0.8rem; opacity:0.7;">👥</button>
                                <button class="panel-card-action" data-action="delete" data-id="${p.id}" title="Supprimer ce projet" style="background:none; border:none; cursor:pointer; font-size:0.8rem; opacity:0.7;">🗑️</button>
                            </div>
                        </div>`;
                    }).join('');

                    projectsPanelList.innerHTML = newCardHtml + cards;
                }

                document.getElementById('projectsPanelNewBtn').addEventListener('click', async function () {
                    const name = prompt('Nom du nouveau projet :');

                    if (!name || !name.trim()) return;

                    try {
                        const res = await fetch('/api/projects', {
                            method: 'POST',
                            headers: {
                                'Content-Type': 'application/json'
                            },
                            body: JSON.stringify({
                                name: name.trim()
                            })
                        });

                        const result = await res.json();

                        showToast(result.message);

                        if (result.success) {
                            await renderProjectsPanel();
                        }
                    } catch (err) {
                        console.error('Erreur création projet:', err);
                        showToast("❌ Erreur réseau lors de la création du projet.");
                    }
                });

                projectsPanelList.querySelectorAll('.panel-card-action').forEach(btn => {
                    btn.addEventListener('click', async function (e) {
                        e.preventDefault();
                        e.stopPropagation();

                        const id = this.dataset.id;

                        if (this.dataset.action === 'invite') {
                            const link = `${window.location.origin}/join/${id}`;

                            navigator.clipboard.writeText(link)
                                .then(() => {
                                    showToast(`🔗 Lien d'invitation copié : ${link}`);
                                })
                                .catch(() => {
                                    showToast(`🔗 Lien d'invitation : ${link}`);
                                });

                        } else if (this.dataset.action === 'delete') {
                            if (!confirm("Supprimer définitivement ce projet ? Cette action est irréversible.")) {
                                return;
                            }

                            try {
                                const res = await fetch(`/api/projects/${id}`, {
                                    method: 'DELETE'
                                });

                                const result = await res.json();

                                showToast(result.message);

                                if (result.success) {
                                    await renderProjectsPanel();
                                }
                            } catch (err) {
                                console.error('Erreur suppression projet:', err);
                                showToast("❌ Erreur réseau lors de la suppression du projet.");
                            }
                        }
                    });
                });

            } catch (err) {
                console.error('Erreur chargement projets:', err);
                projectsPanelList.innerHTML = '<div class="panel-list-empty">Erreur de chargement.</div>';
            }
        }

        document.getElementById('navProjets').addEventListener('click', async function () {
            const isShowing = projectsPanel.classList.contains('show');

            historyPanel.classList.remove('show');
            settingsPanel.classList.remove('show');

            projectsPanel.classList.toggle('show');

            if (isShowing) return;

            await renderProjectsPanel();
        });

        // --- Panneau Historique de recherche ---

        document.getElementById('navHistorique').addEventListener('click', async function () {
            const isShowing = historyPanel.classList.contains('show');

            projectsPanel.classList.remove('show');
            settingsPanel.classList.remove('show');

            historyPanel.classList.toggle('show');

            if (isShowing) return;

            historyPanelList.innerHTML = 'Chargement...';

            try {
                const res = await fetch('/api/history');
                const history = await res.json();

                const newCardHtml = `<a class="panel-card-new" href="/" title="Nouvelle recherche">+</a>`;

                if (!history.length) {
                    historyPanelList.innerHTML = newCardHtml +
                        '<div class="panel-list-empty" style="grid-column: 1 / -1;">Aucune recherche récente.</div>';
                } else {
                    const cards = history.map(node => {
                        const url = new URL(
                            window.location.origin + categoryToPath(node.category)
                        );

                        url.searchParams.set('q', node.query);

                        if (node.parent_id) {
                            url.searchParams.set('parent', node.parent_id);
                        }

                        if (node.project_id) {
                            url.searchParams.set('project', node.project_id);
                        }

                        const date = new Date(
                            node.created_at.replace(' ', 'T') + 'Z'
                        ).toLocaleString('fr-FR', {
                            day: '2-digit',
                            month: '2-digit',
                            hour: '2-digit',
                            minute: '2-digit'
                        });

                        return `<a class="panel-card" href="${url.toString()}">
                            <div class="panel-card-title">🔎 ${node.query}</div>
                            <div class="panel-card-meta">${date}</div>
                        </a>`;
                    }).join('');

                    historyPanelList.innerHTML = newCardHtml + cards;
                }

            } catch (err) {
                console.error('Erreur chargement historique:', err);
                historyPanelList.innerHTML = '<div class="panel-list-empty">Erreur de chargement.</div>';
            }
        });

        document.addEventListener('click', function (e) {
            if (
                !settingsPanel.contains(e.target) &&
                e.target.id !== 'navParametres' &&
                !e.target.closest('#navParametres')
            ) {
                settingsPanel.classList.remove('show');
            }

            if (
                !projectsPanel.contains(e.target) &&
                e.target.id !== 'navProjets' &&
                !e.target.closest('#navProjets')
            ) {
                projectsPanel.classList.remove('show');
            }

            if (
                !historyPanel.contains(e.target) &&
                e.target.id !== 'navHistorique' &&
                !e.target.closest('#navHistorique')
            ) {
                historyPanel.classList.remove('show');
            }
        });

        // --- Chargement lazy des résultats ---
        let loadMorePage = 2;
        const shownUrls = new Set(
            Array.from(document.querySelectorAll('#resultsGrid .result-url')).map(el => el.textContent.trim())
        );

        async function loadMore() {
            const btn = document.getElementById('loadMoreBtn');
            const spinner = document.getElementById('loadingSpinner');
            const grid = document.getElementById('resultsGrid');
            const count = document.getElementById('resultCount');

            btn.disabled = true;
            spinner.style.display = 'block';

            try {
                const response = await fetch(`/api/load-more?q=${encodeURIComponent(currentQuery)}&page=${loadMorePage}`);
                const allResults = await response.json();

                // Sécurité anti-doublon : on ignore toute URL déjà affichée à l'écran
                const newResults = allResults.filter(r => !shownUrls.has(r.url));

                if (allResults.length === 0) {
                    btn.textContent = 'Plus de résultats disponibles';
                    btn.style.display = 'none';
                    return;
                }

                newResults.forEach(result => {
                    shownUrls.add(result.url);
                    const div = document.createElement('div');
                    div.className = 'result';
                    div.innerHTML = `
                        <a href="${result.url}" class="result-title">${result.title}</a>
                        <div class="result-url">${result.url}</div>
                        <div class="result-snippet">${result.snippet}</div>
                    `;
                    grid.appendChild(div);
                });

                loadMorePage += 1;

                const currentCount = parseInt(count.textContent) + newResults.length;
                count.textContent = currentCount;

            } catch (err) {
                console.error('Erreur:', err);
                btn.textContent = 'Erreur de chargement';
            } finally {
                btn.disabled = false;
                spinner.style.display = 'none';
            }
        }
        