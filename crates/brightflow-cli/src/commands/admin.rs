//! `create-admin`: seed the first user.
//!
//! Deliberately keeps the brightflow_api AuthDb/hash_password imports — this
//! is server admin tooling and must hash exactly the way the server verifies,
//! so sharing the server's own crate is the point, not a shortcut.
//!
//! The password normally comes from an interactive prompt. `BRIGHTFLOW_ADMIN_
//! PASSWORD` overrides it for automation (provisioning, the test-template
//! builder) — there is no other way in, since the API exposes no signup route.
//! An env var is visible to anything that can read the process environment, so
//! it is for throwaway and provisioning credentials, not an operator's own.

use anyhow::Result;

/// Env var supplying the password non-interactively (see module doc).
const PASSWORD_ENV: &str = "BRIGHTFLOW_ADMIN_PASSWORD";

/// The password, from the environment when set, otherwise prompted twice.
fn read_password() -> Result<String> {
    if let Ok(from_env) = std::env::var(PASSWORD_ENV) {
        if from_env.is_empty() {
            anyhow::bail!("{PASSWORD_ENV} is set but empty");
        }
        return Ok(from_env);
    }

    let password = rpassword::read_password_from_tty(Some("Password: "))?;
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }
    let confirm = rpassword::read_password_from_tty(Some("Confirm password: "))?;
    if password != confirm {
        anyhow::bail!("Passwords do not match");
    }
    Ok(password)
}

pub(crate) async fn handle_create_admin(email: &str, name: &str, database_url: &str) -> Result<()> {
    // Ensure data directory exists
    if let Some(parent) = brightflow_core::sqlite_db_path(database_url)
        .as_deref()
        .and_then(std::path::Path::parent)
    {
        std::fs::create_dir_all(parent)?;
    }

    let db = brightflow_api::auth::AuthDb::new(database_url).await?;

    // Check if user already exists
    if let Some(_existing) = db.get_user_by_email(email).await? {
        anyhow::bail!("User with email '{email}' already exists");
    }

    let password = read_password()?;

    let hash = brightflow_api::auth::hash_password(&password)?;
    let user = db.create_user(email, name, &hash).await?;

    println!("Admin user created:");
    println!("  ID:    {}", user.id);
    println!("  Email: {}", user.email);
    println!("  Name:  {}", user.display_name);

    Ok(())
}
