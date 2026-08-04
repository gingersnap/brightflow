//! `create-admin`: seed the first user.
//!
//! Deliberately keeps the brightflow_api AuthDb/hash_password imports — this
//! is server admin tooling and must hash exactly the way the server verifies,
//! so sharing the server's own crate is the point, not a shortcut.

use anyhow::Result;

pub(crate) async fn handle_create_admin(email: &str, name: &str, database_url: &str) -> Result<()> {
    // Ensure data directory exists
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        let db_path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let db = brightflow_api::auth::AuthDb::new(database_url).await?;

    // Check if user already exists
    if let Some(_existing) = db.get_user_by_email(email).await? {
        anyhow::bail!("User with email '{email}' already exists");
    }

    // Prompt for password
    let password = rpassword::read_password_from_tty(Some("Password: "))?;
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }
    let confirm = rpassword::read_password_from_tty(Some("Confirm password: "))?;
    if password != confirm {
        anyhow::bail!("Passwords do not match");
    }

    let hash = brightflow_api::auth::hash_password(&password)?;
    let user = db.create_user(email, name, &hash).await?;

    println!("Admin user created:");
    println!("  ID:    {}", user.id);
    println!("  Email: {}", user.email);
    println!("  Name:  {}", user.display_name);

    Ok(())
}
