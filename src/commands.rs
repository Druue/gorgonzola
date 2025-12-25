use sqlx::{Pool, Sqlite};

use crate::BoxErr;

type Context<'a> = poise::Context<'a, Data, Box<dyn std::error::Error + Send + Sync>>;

pub struct Data {
    pub pool: Pool<Sqlite>,
}

#[poise::command(slash_command)]
pub async fn register(ctx: Context<'_>, civ_username: String) -> Result<(), BoxErr> {
    ctx.defer_ephemeral().await?;

    let pool = &ctx.data().pool;

    let mut tx = pool
        .begin()
        .await
        .inspect_err(|_e| println!("Failed to acquire connection to db"))?;

    let author_id = ctx.author().id.to_string();

    let result = sqlx::query!(
        r#"
            INSERT INTO user ( discord_id )
            SELECT ?1
            WHERE NOT EXISTS ( SELECT * FROM user WHERE discord_id = ?1 )
        "#,
        author_id,
    )
    .execute(&mut *tx)
    .await;

    if let Err(e) = result {
        println!("Failed to insert user mapping");

        tx.rollback().await?;
        return Err(Box::new(e));
    }

    let result = sqlx::query!(
        r#"
            INSERT INTO civ_discord_user_map ( discord_id, civ_user_name )
            SELECT ?1, ?2
            WHERE NOT EXISTS ( SELECT * FROM civ_discord_user_map WHERE discord_id = ?1 AND civ_user_name = ?2 )
        "#,
        author_id,
        civ_username,
    ).execute(&mut *tx).await;

    if let Err(e) = result {
        println!("Failed to insert user mapping");

        tx.rollback().await?;
        return Err(Box::new(e));
    }

    tx.commit().await.inspect_err(|e| {
        println!("failed to commit transaction for registering a new user; {e}")
    })?;

    ctx.say("Successfully registered your Civ username.")
        .await?;
    Ok(())
}
