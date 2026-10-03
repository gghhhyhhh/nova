use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use lettre::{
    message::Mailbox,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport,
    AsyncTransport,
    Message,
    Tokio1Executor,
};
use rand::Rng;
use serde::Deserialize;
use sqlx::SqlitePool;
use tera::{Context, Tera};

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct VerifyCodeForm {
    pub code: String,
}

#[derive(Deserialize)]
pub struct ForgotPasswordForm {
    pub email: String,
}

#[derive(Deserialize)]
pub struct VerifyResetCodeForm {
    pub code: String,
}

#[derive(Deserialize)]
pub struct ResetPasswordForm {
    pub password: String,
    pub password_confirmation: String,
}

fn render_template(
    tera: &Tera,
    template: &str,
    context: Context,
) -> HttpResponse {
    match tera.render(template, &context) {
        Ok(html) => HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html),

        Err(e) => {
            eprintln!("Erreur Tera ({template}): {e}");

            HttpResponse::InternalServerError()
                .body("Erreur interne du serveur")
        }
    }
}

fn redirect(location: &str) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", location))
        .finish()
}

async fn send_email(
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let smtp_host = std::env::var("SMTP_HOST")
        .map_err(|_| {
            "SMTP_HOST manquant dans .env".to_string()
        })?;

    let smtp_port: u16 = std::env::var("SMTP_PORT")
        .map_err(|_| {
            "SMTP_PORT manquant dans .env".to_string()
        })?
        .parse()
        .map_err(|_| "SMTP_PORT invalide".to_string())?;

    let smtp_username = std::env::var("SMTP_USERNAME")
        .map_err(|_| {
            "SMTP_USERNAME manquant dans .env".to_string()
        })?;

    let smtp_password = std::env::var("SMTP_PASSWORD")
        .map_err(|_| {
            "SMTP_PASSWORD manquant dans .env".to_string()
        })?;

    let smtp_from = std::env::var("SMTP_FROM")
        .map_err(|_| {
            "SMTP_FROM manquant dans .env".to_string()
        })?;

    let from: Mailbox = smtp_from
        .parse()
        .map_err(|e| {
            format!("SMTP_FROM invalide : {e}")
        })?;

    let to_mailbox: Mailbox = to
        .parse()
        .map_err(|e| {
            format!("Adresse email invalide : {e}")
        })?;

    let email = Message::builder()
        .from(from)
        .to(to_mailbox)
        .subject(subject)
        .body(body.to_string())
        .map_err(|e| {
            format!("Erreur création email : {e}")
        })?;

    let credentials =
        Credentials::new(smtp_username, smtp_password);

    let mailer =
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(
            &smtp_host,
        )
        .map_err(|e| {
            format!("Erreur SMTP : {e}")
        })?
        .port(smtp_port)
        .credentials(credentials)
        .build();

    mailer
        .send(email)
        .await
        .map_err(|e| {
            format!("Erreur envoi email : {e}")
        })?;

    Ok(())
}

/* =========================================================
   LOGIN
   ========================================================= */

pub async fn login_page(
    tera: web::Data<Tera>,
) -> impl Responder {
    render_template(
        &tera,
        "login.html",
        Context::new(),
    )
}

pub async fn register_page(
    tera: web::Data<Tera>,
) -> impl Responder {
    render_template(
        &tera,
        "register.html",
        Context::new(),
    )
}

pub async fn register(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    session: Session,
    form: web::Form<RegisterForm>,
) -> impl Responder {
    let username = form.username.trim();
    let email = form.email.trim().to_lowercase();
    let password = form.password.as_str();

    let mut context = Context::new();

    if username.len() < 3 {
        context.insert(
            "error",
            "Le nom d'utilisateur doit contenir au moins 3 caractères.",
        );

        return render_template(
            &tera,
            "register.html",
            context,
        );
    }

    if password.len() < 8 {
        context.insert(
            "error",
            "Le mot de passe doit contenir au moins 8 caractères.",
        );

        return render_template(
            &tera,
            "register.html",
            context,
        );
    }

    if !email.contains('@') {
        context.insert(
            "error",
            "Adresse email invalide.",
        );

        return render_template(
            &tera,
            "register.html",
            context,
        );
    }

    let password_hash =
        match crate::auth::hash_password(password) {
            Ok(hash) => hash,

            Err(e) => {
                eprintln!("{e}");

                context.insert(
                    "error",
                    "Impossible de sécuriser le mot de passe.",
                );

                return render_template(
                    &tera,
                    "register.html",
                    context,
                );
            }
        };

    let result = sqlx::query(
        "INSERT INTO users (
            email,
            username,
            password_hash
        )
        VALUES (?, ?, ?)",
    )
    .bind(&email)
    .bind(username)
    .bind(password_hash)
    .execute(pool.get_ref())
    .await;

    let user_id = match result {
        Ok(result) => result.last_insert_rowid(),

        Err(e) => {
            let message =
                if e.to_string().contains("UNIQUE") {
                    "Cet email ou ce nom d'utilisateur est déjà utilisé."
                } else {
                    eprintln!(
                        "Erreur création utilisateur : {e}"
                    );

                    "Impossible de créer le compte."
                };

            context.insert(
                "error",
                message,
            );

            return render_template(
                &tera,
                "register.html",
                context,
            );
        }
    };

    let _ = session.insert(
        "user_id",
        user_id,
    );

    session.remove(
        "pending_user_id",
    );

    session.remove(
        "password_reset_user_id",
    );

    session.remove(
        "password_reset_verified",
    );

    redirect("/")
}

pub async fn login(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    session: Session,
    form: web::Form<LoginForm>,
) -> impl Responder {
    let email = form.email.trim().to_lowercase();

    let user =
        sqlx::query_as::<_, (i64, String, String)>(
            "SELECT id, username, password_hash
             FROM users
             WHERE email = ?",
        )
        .bind(&email)
        .fetch_optional(pool.get_ref())
        .await;

    let Some((user_id, _username, password_hash)) =
        (match user {
            Ok(user) => user,

            Err(e) => {
                eprintln!(
                    "Erreur récupération utilisateur : {e}"
                );

                return HttpResponse::InternalServerError()
                    .body("Erreur serveur");
            }
        })
    else {
        let mut context = Context::new();

        context.insert(
            "error",
            "Email ou mot de passe incorrect.",
        );

        return render_template(
            &tera,
            "login.html",
            context,
        );
    };

    if !verify_password(
        &form.password,
        &password_hash,
    ) {
        let mut context = Context::new();

        context.insert(
            "error",
            "Email ou mot de passe incorrect.",
        );

        return render_template(
            &tera,
            "login.html",
            context,
        );
    }

    let _ = session.insert(
        "user_id",
        user_id,
    );

    session.remove(
        "pending_user_id",
    );

    session.remove(
        "password_reset_user_id",
    );

    session.remove(
        "password_reset_verified",
    );

    redirect("/")
}

/* =========================================================
   CODE DE VÉRIFICATION LOGIN
   ========================================================= */

pub async fn verify_code_page(
    tera: web::Data<Tera>,
) -> impl Responder {
    render_template(
        &tera,
        "verify-code.html",
        Context::new(),
    )
}

pub async fn resend_code(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    session: Session,
) -> impl Responder {
    let pending_user_id =
        match session.get::<i64>("pending_user_id") {
            Ok(Some(id)) => id,

            _ => {
                return redirect("/login");
            }
        };

    let user =
        sqlx::query_as::<_, (String, String, i64)>(
            "SELECT email, username, resend_count
             FROM users
             JOIN login_verification_codes
               ON login_verification_codes.user_id = users.id
             WHERE users.id = ?
             ORDER BY login_verification_codes.id DESC
             LIMIT 1",
        )
        .bind(pending_user_id)
        .fetch_optional(pool.get_ref())
        .await;

    let Some((email, username, resend_count)) =
        (match user {
            Ok(user) => user,

            Err(e) => {
                eprintln!(
                    "Erreur récupération utilisateur : {e}"
                );

                let mut context = Context::new();

                context.insert(
                    "error",
                    "Erreur interne du serveur.",
                );

                return render_template(
                    &tera,
                    "verify-code.html",
                    context,
                );
            }
        })
    else {
        return redirect("/login");
    };

    if resend_count >= 5 {
        let mut context = Context::new();

        context.insert(
            "error",
            "Vous avez atteint la limite de 5 renvois de code.",
        );

        context.insert(
            "resend_count",
            &resend_count,
        );

        return render_template(
            &tera,
            "verify-code.html",
            context,
        );
    }

    let new_code =
        format!(
            "{:06}",
            rand::thread_rng()
                .gen_range(0..1_000_000)
        );

    let new_count = resend_count + 1;

    if let Err(e) = sqlx::query(
        "UPDATE login_verification_codes
         SET code = ?,
             expires_at = datetime('now', '+10 minutes'),
             resend_count = ?
         WHERE user_id = ?",
    )
    .bind(&new_code)
    .bind(new_count)
    .bind(pending_user_id)
    .execute(pool.get_ref())
    .await
    {
        eprintln!(
            "Erreur création nouveau code : {e}"
        );

        let mut context = Context::new();

        context.insert(
            "error",
            "Impossible de générer un nouveau code.",
        );

        context.insert(
            "resend_count",
            &resend_count,
        );

        return render_template(
            &tera,
            "verify-code.html",
            context,
        );
    }

    let body = format!(
        "Bonjour {},\n\n\
         Voici votre nouveau code de vérification NOVA : {}\n\n\
         Ce code est valable pendant 10 minutes.\n\n\
         Renvois utilisés : {}/5.\n\n\
         Si vous n'êtes pas à l'origine de cette demande, ignorez cet email.",
        username,
        new_code,
        new_count
    );

    if let Err(e) =
        send_email(
            &email,
            "Nouveau code de vérification NOVA",
            &body,
        )
        .await
    {
        eprintln!(
            "Erreur envoi nouveau code : {e}"
        );

        let mut context = Context::new();

        context.insert(
            "error",
            "Impossible d'envoyer le nouveau code.",
        );

        context.insert(
            "resend_count",
            &resend_count,
        );

        return render_template(
            &tera,
            "verify-code.html",
            context,
        );
    }

    let mut context = Context::new();

    context.insert(
        "success",
        "Un nouveau code vient de vous être envoyé.",
    );

    context.insert(
        "resend_count",
        &new_count,
    );

    render_template(
        &tera,
        "verify-code.html",
        context,
    )
}

pub async fn verify_code(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    session: Session,
    form: web::Form<VerifyCodeForm>,
) -> impl Responder {
    let pending_user_id =
        match session.get::<i64>("pending_user_id") {
            Ok(Some(id)) => id,

            _ => {
                return redirect("/login");
            }
        };

    let code = form.code.trim();

    let result =
        sqlx::query_as::<_, (i64,)>(
            "SELECT id
             FROM login_verification_codes
             WHERE user_id = ?
               AND code = ?
               AND expires_at > CURRENT_TIMESTAMP
             ORDER BY id DESC
             LIMIT 1",
        )
        .bind(pending_user_id)
        .bind(code)
        .fetch_optional(pool.get_ref())
        .await;

    match result {
        Ok(Some(_)) => {
            let _ = sqlx::query(
                "DELETE FROM login_verification_codes
                 WHERE user_id = ?",
            )
            .bind(pending_user_id)
            .execute(pool.get_ref())
            .await;

            let _ = session.insert(
                "user_id",
                pending_user_id,
            );

            session.remove(
                "pending_user_id",
            );

            redirect("/")
        }

        Ok(None) => {
            let mut context = Context::new();

            context.insert(
                "error",
                "Code incorrect ou expiré.",
            );

            render_template(
                &tera,
                "verify-code.html",
                context,
            )
        }

        Err(e) => {
            eprintln!(
                "Erreur vérification code : {e}"
            );

            let mut context = Context::new();

            context.insert(
                "error",
                "Erreur interne du serveur.",
            );

            render_template(
                &tera,
                "verify-code.html",
                context,
            )
        }
    }
}

/* =========================================================
   MOT DE PASSE OUBLIÉ
   ========================================================= */

pub async fn forgot_password_page(
    tera: web::Data<Tera>,
) -> impl Responder {
    render_template(
        &tera,
        "forgot-password.html",
        Context::new(),
    )
}

pub async fn forgot_password(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    session: Session,
    form: web::Form<ForgotPasswordForm>,
) -> impl Responder {
    let email = form.email.trim().to_lowercase();

    let user =
        sqlx::query_as::<_, (i64, String)>(
            "SELECT id, username
             FROM users
             WHERE email = ?",
        )
        .bind(&email)
        .fetch_optional(pool.get_ref())
        .await;

    if let Ok(Some((user_id, username))) = user {
        let code =
            format!(
                "{:06}",
                rand::thread_rng()
                    .gen_range(0..1_000_000)
            );

        let _ = sqlx::query(
            "DELETE FROM login_verification_codes
             WHERE user_id = ?",
        )
        .bind(user_id)
        .execute(pool.get_ref())
        .await;

        let insert = sqlx::query(
            "INSERT INTO login_verification_codes
             (
                 user_id,
                 code,
                 expires_at,
                 resend_count
             )
             VALUES (
                 ?,
                 ?,
                 datetime('now', '+10 minutes'),
                 0
             )",
        )
        .bind(user_id)
        .bind(&code)
        .execute(pool.get_ref())
        .await;

        if insert.is_ok() {
            let body = format!(
                "Bonjour {},\n\n\
                 Une demande de réinitialisation de mot de passe \
                 a été effectuée pour votre compte NOVA.\n\n\
                 Votre code de vérification est : {}\n\n\
                 Ce code est valable pendant 10 minutes.\n\n\
                 Si vous n'êtes pas à l'origine de cette demande, \
                 ignorez cet email.",
                username,
                code
            );

            if let Err(e) =
                send_email(
                    &email,
                    "Code de réinitialisation NOVA",
                    &body,
                )
                .await
            {
                eprintln!(
                    "Erreur envoi code reset password : {e}"
                );
            } else {
                let _ = session.insert(
                    "password_reset_user_id",
                    user_id,
                );

                session.remove(
                    "password_reset_verified",
                );

                return redirect(
                    "/verify-reset-code",
                );
            }
        }
    }

    let mut context = Context::new();

    context.insert(
        "message",
        "Si cette adresse correspond à un compte NOVA, \
         un code de vérification vient d'être envoyé.",
    );

    render_template(
        &tera,
        "forgot-password.html",
        context,
    )
}

/* =========================================================
   VÉRIFICATION DU CODE DE RÉINITIALISATION
   ========================================================= */

pub async fn verify_reset_code_page(
    tera: web::Data<Tera>,
    session: Session,
) -> impl Responder {
    match session.get::<i64>(
        "password_reset_user_id",
    ) {
        Ok(Some(_)) => {
            render_template(
                &tera,
                "verify-reset-code.html",
                Context::new(),
            )
        }

        _ => {
            redirect("/forgot-password")
        }
    }
}

pub async fn verify_reset_code(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    session: Session,
    form: web::Form<VerifyResetCodeForm>,
) -> impl Responder {
    let user_id =
        match session.get::<i64>(
            "password_reset_user_id",
        ) {
            Ok(Some(id)) => id,

            _ => {
                return redirect(
                    "/forgot-password",
                );
            }
        };

    let code = form.code.trim();

    if code.len() != 6
        || !code
            .chars()
            .all(|character| {
                character.is_ascii_digit()
            })
    {
        let mut context = Context::new();

        context.insert(
            "error",
            "Le code doit contenir exactement 6 chiffres.",
        );

        return render_template(
            &tera,
            "verify-reset-code.html",
            context,
        );
    }

    let result =
        sqlx::query_as::<_, (i64,)>(
            "SELECT id
             FROM login_verification_codes
             WHERE user_id = ?
               AND code = ?
               AND expires_at > CURRENT_TIMESTAMP
             ORDER BY id DESC
             LIMIT 1",
        )
        .bind(user_id)
        .bind(code)
        .fetch_optional(pool.get_ref())
        .await;

    match result {
        Ok(Some(_)) => {
            /*
             * Le code est supprimé immédiatement.
             * Il ne pourra plus être utilisé.
             */
            let _ = sqlx::query(
                "DELETE FROM login_verification_codes
                 WHERE user_id = ?",
            )
            .bind(user_id)
            .execute(pool.get_ref())
            .await;

            let _ = session.insert(
                "password_reset_verified",
                true,
            );

            redirect("/reset-password")
        }

        Ok(None) => {
            let mut context = Context::new();

            context.insert(
                "error",
                "Code incorrect ou expiré.",
            );

            render_template(
                &tera,
                "verify-reset-code.html",
                context,
            )
        }

        Err(e) => {
            eprintln!(
                "Erreur vérification code reset : {e}"
            );

            let mut context = Context::new();

            context.insert(
                "error",
                "Erreur interne du serveur.",
            );

            render_template(
                &tera,
                "verify-reset-code.html",
                context,
            )
        }
    }
}

/* =========================================================
   NOUVEAU MOT DE PASSE
   ========================================================= */

pub async fn reset_password_page(
    tera: web::Data<Tera>,
    session: Session,
) -> impl Responder {
    match session.get::<i64>(
        "password_reset_user_id",
    ) {
        Ok(Some(_)) => {}

        _ => {
            return redirect(
                "/forgot-password",
            );
        }
    }

    match session.get::<bool>(
        "password_reset_verified",
    ) {
        Ok(Some(true)) => {}

        _ => {
            return redirect(
                "/verify-reset-code",
            );
        }
    }

    render_template(
        &tera,
        "reset-password.html",
        Context::new(),
    )
}

pub async fn reset_password(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    session: Session,
    form: web::Form<ResetPasswordForm>,
) -> impl Responder {
    let user_id =
        match session.get::<i64>(
            "password_reset_user_id",
        ) {
            Ok(Some(id)) => id,

            _ => {
                return redirect(
                    "/forgot-password",
                );
            }
        };

    match session.get::<bool>(
        "password_reset_verified",
    ) {
        Ok(Some(true)) => {}

        _ => {
            return redirect(
                "/verify-reset-code",
            );
        }
    }

    if form.password.len() < 8 {
        let mut context = Context::new();

        context.insert(
            "error",
            "Le mot de passe doit contenir au moins 8 caractères.",
        );

        return render_template(
            &tera,
            "reset-password.html",
            context,
        );
    }

    if form.password
        != form.password_confirmation
    {
        let mut context = Context::new();

        context.insert(
            "error",
            "Les deux mots de passe ne correspondent pas.",
        );

        return render_template(
            &tera,
            "reset-password.html",
            context,
        );
    }

    let password_hash =
        match hash_password(
            &form.password,
        ) {
            Ok(hash) => hash,

            Err(e) => {
                eprintln!(
                    "Erreur hash mot de passe : {e}"
                );

                return HttpResponse::InternalServerError()
                    .body(
                        "Impossible de sécuriser le nouveau mot de passe.",
                    );
            }
        };

    if let Err(e) = sqlx::query(
        "UPDATE users
         SET password_hash = ?,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?",
    )
    .bind(password_hash)
    .bind(user_id)
    .execute(pool.get_ref())
    .await
    {
        eprintln!(
            "Erreur mise à jour mot de passe : {e}"
        );

        return HttpResponse::InternalServerError()
            .body(
                "Erreur interne du serveur.",
            );
    }

    /*
     * Le parcours de récupération est terminé.
     */
    session.remove(
        "password_reset_user_id",
    );

    session.remove(
        "password_reset_verified",
    );

    redirect("/login")
}

/* =========================================================
   LOGOUT
   ========================================================= */

pub async fn logout(
    session: Session,
) -> impl Responder {
    session.remove("user_id");
    session.remove("pending_user_id");

    session.remove(
        "password_reset_user_id",
    );

    session.remove(
        "password_reset_verified",
    );

    redirect("/")
}

/* =========================================================
   HASH MOT DE PASSE
   ========================================================= */

pub fn hash_password(
    password: &str,
) -> Result<String, String> {
    use argon2::{
        password_hash::{
            rand_core::OsRng,
            PasswordHasher,
            SaltString,
        },
        Argon2,
    };

    let salt =
        SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(
            password.as_bytes(),
            &salt,
        )
        .map(|hash| hash.to_string())
        .map_err(|e| {
            format!(
                "Erreur lors du hash du mot de passe : {e}"
            )
        })
}

/* =========================================================
   VÉRIFICATION MOT DE PASSE
   ========================================================= */

pub fn verify_password(
    password: &str,
    password_hash: &str,
) -> bool {
    use argon2::{
        password_hash::{
            PasswordHash,
            PasswordVerifier,
        },
        Argon2,
    };

    let parsed_hash =
        match PasswordHash::new(
            password_hash,
        ) {
            Ok(hash) => hash,
            Err(_) => return false,
        };

    Argon2::default()
        .verify_password(
            password.as_bytes(),
            &parsed_hash,
        )
        .is_ok()
}