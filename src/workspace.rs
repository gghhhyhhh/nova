use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use sqlx::FromRow;
use uuid::Uuid;

pub const MAX_PROJECTS_PER_DAY: i64 = 3;
pub const MAX_MEMBERS_PER_PROJECT: i64 = 5;

// ============================================================
// Arbre de branches de recherche
// ============================================================

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SearchNode {
    pub id: String,
    pub project_id: Option<String>,
    pub parent_id: Option<String>,
    pub session_id: String,
    pub query: String,
    pub category: String,
    pub created_at: String,
}

/// Liste tout l'historique de recherche conservé pour cette session
/// (du plus récent au plus ancien, sans limite).
pub async fn list_history_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<SearchNode>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM search_nodes WHERE session_id = ? ORDER BY created_at DESC")
        .bind(session_id)
        .fetch_all(pool)
        .await
}

/// Enregistre une nouvelle étape de recherche (racine si `parent_id` est None),
/// et retourne l'id du nœud créé. Ne garde que les 5 dernières recherches par
/// session : au-delà, les plus anciennes sont supprimées automatiquement.
pub async fn record_search_node(
    pool: &SqlitePool,
    project_id: Option<&str>,
    parent_id: Option<&str>,
    session_id: &str,
    query: &str,
    category: &str,
) -> Result<String, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO search_nodes (id, project_id, parent_id, session_id, query, category) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(parent_id)
    .bind(session_id)
    .bind(query)
    .bind(category)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Remonte la chaîne des ancêtres d'un nœud (du plus ancien au plus récent) :
/// c'est le "fil d'Ariane" de l'arbre de recherche, pour pouvoir revenir en arrière.
pub async fn get_ancestor_chain(
    pool: &SqlitePool,
    node_id: &str,
) -> Result<Vec<SearchNode>, sqlx::Error> {
    let mut chain = Vec::new();
    let mut current_id = Some(node_id.to_string());

    // Sécurité anti-boucle infinie (un arbre ne devrait jamais boucler, mais on se protège)
    for _ in 0..100 {
        let Some(id) = current_id.clone() else { break };
        let node: Option<SearchNode> = sqlx::query_as("SELECT * FROM search_nodes WHERE id = ?")
            .bind(&id)
            .fetch_optional(pool)
            .await?;

        match node {
            Some(n) => {
                current_id = n.parent_id.clone();
                chain.push(n);
            }
            None => break,
        }
    }

    chain.reverse(); // du plus ancien au plus récent
    Ok(chain)
}

/// Liste les branches "sœurs" d'un nœud : les autres recherches faites à partir
/// du même parent PAR LA MÊME SESSION (permet de choisir une autre piste déjà
/// explorée, sans mélanger l'historique d'autres sessions/navigateurs).
/// Limité aux 5 plus récentes.
pub async fn get_sibling_branches(
    pool: &SqlitePool,
    parent_id: Option<&str>,
    exclude_node_id: &str,
    session_id: &str,
) -> Result<Vec<SearchNode>, sqlx::Error> {
    let nodes: Vec<SearchNode> = match parent_id {
        Some(pid) => {
            sqlx::query_as(
                "SELECT * FROM search_nodes
                 WHERE parent_id = ? AND id != ? AND session_id = ?
                 ORDER BY created_at DESC
                 LIMIT 5",
            )
            .bind(pid)
            .bind(exclude_node_id)
            .bind(session_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT * FROM search_nodes
                 WHERE parent_id IS NULL AND id != ? AND session_id = ?
                 ORDER BY created_at DESC
                 LIMIT 5",
            )
            .bind(exclude_node_id)
            .bind(session_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(nodes)
}

// ============================================================
// Projets
// ============================================================

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub owner_session: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ProjectActionResult {
    pub success: bool,
    pub message: String,
    pub project: Option<Project>,
}

/// Crée un projet pour cette session, en respectant le quota gratuit journalier.
/// Si le quota est dépassé, renvoie un message d'avertissement (pas d'erreur bloquante silencieuse).
pub async fn create_project(
    pool: &SqlitePool,
    owner_session: &str,
    name: &str,
) -> Result<ProjectActionResult, sqlx::Error> {
    let count_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM projects WHERE owner_session = ? AND date(created_at) = date('now')",
    )
    .bind(owner_session)
    .fetch_one(pool)
    .await?;

    if count_today >= MAX_PROJECTS_PER_DAY {
        return Ok(ProjectActionResult {
            success: false,
            message: format!(
                "⚠️ Limite atteinte : {} projets créés aujourd'hui (max {} en version gratuite). Passe à la version premium pour créer des projets illimités.",
                count_today, MAX_PROJECTS_PER_DAY
            ),
            project: None,
        });
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO projects (id, name, owner_session) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(owner_session)
        .execute(pool)
        .await?;

    // Le créateur devient automatiquement le premier membre du groupe
    sqlx::query("INSERT INTO project_members (project_id, session_id) VALUES (?, ?)")
        .bind(&id)
        .bind(owner_session)
        .execute(pool)
        .await?;

    let project: Project = sqlx::query_as("SELECT * FROM projects WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await?;

    Ok(ProjectActionResult {
        success: true,
        message: format!(
            "✅ Projet « {} » créé ({}/{} aujourd'hui).",
            name,
            count_today + 1,
            MAX_PROJECTS_PER_DAY
        ),
        project: Some(project),
    })
}

/// Liste les projets où cette session est membre (créateur ou invité).
pub async fn list_projects_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<Project>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.* FROM projects p
         INNER JOIN project_members pm ON pm.project_id = p.id
         WHERE pm.session_id = ?
         ORDER BY p.created_at DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
}

// ============================================================
// Groupes (membres d'un projet)
// ============================================================

#[derive(Debug, Serialize)]
pub struct JoinResult {
    pub success: bool,
    pub message: String,
    pub project: Option<Project>,
}

/// Ajoute la session courante comme membre du projet, en respectant le quota
/// gratuit de membres par groupe.
pub async fn join_project(
    pool: &SqlitePool,
    project_id: &str,
    session_id: &str,
) -> Result<JoinResult, sqlx::Error> {
    let project: Option<Project> = sqlx::query_as("SELECT * FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(pool)
        .await?;

    let Some(project) = project else {
        return Ok(JoinResult {
            success: false,
            message: "❌ Ce projet n'existe pas (lien invalide ou expiré).".to_string(),
            project: None,
        });
    };

    // Déjà membre ? On ne compte pas deux fois.
    let already_member: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_members WHERE project_id = ? AND session_id = ?",
    )
    .bind(project_id)
    .bind(session_id)
    .fetch_one(pool)
    .await?;

    if already_member > 0 {
        return Ok(JoinResult {
            success: true,
            message: format!("Tu es déjà membre du projet « {} ».", project.name),
            project: Some(project),
        });
    }

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_members WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(pool)
            .await?;

    if member_count >= MAX_MEMBERS_PER_PROJECT {
        return Ok(JoinResult {
            success: false,
            message: format!(
                "⚠️ Limite atteinte : ce groupe compte déjà {} membres (max {} en version gratuite). Passe à la version premium pour des groupes illimités.",
                member_count, MAX_MEMBERS_PER_PROJECT
            ),
            project: Some(project),
        });
    }

    sqlx::query("INSERT INTO project_members (project_id, session_id) VALUES (?, ?)")
        .bind(project_id)
        .bind(session_id)
        .execute(pool)
        .await?;

    Ok(JoinResult {
        success: true,
        message: format!(
            "✅ Tu as rejoint « {} » ({}/{} membres).",
            project.name,
            member_count + 1,
            MAX_MEMBERS_PER_PROJECT
        ),
        project: Some(project),
    })
}

pub async fn count_members(pool: &SqlitePool, project_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM project_members WHERE project_id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectPayload {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub success: bool,
    pub message: String,
}

/// Supprime un projet, uniquement si la session courante en est le créateur.
/// Supprime aussi les membres et l'historique de recherche liés au projet.
pub async fn delete_project(
    pool: &SqlitePool,
    project_id: &str,
    session_id: &str,
) -> Result<DeleteResult, sqlx::Error> {
    let project: Option<Project> = sqlx::query_as("SELECT * FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(pool)
        .await?;

    let Some(project) = project else {
        return Ok(DeleteResult {
            success: false,
            message: "❌ Ce projet n'existe pas (déjà supprimé ?).".to_string(),
        });
    };

    if project.owner_session != session_id {
        return Ok(DeleteResult {
            success: false,
            message: "⛔ Seul le créateur du projet peut le supprimer.".to_string(),
        });
    }

    sqlx::query("DELETE FROM search_nodes WHERE project_id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM project_members WHERE project_id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;

    Ok(DeleteResult {
        success: true,
        message: format!("🗑️ Projet « {} » supprimé.", project.name),
    })
}
