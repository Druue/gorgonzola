mod commands;

use std::{env, error::Error, net::SocketAddr};

use axum::{
    Json,
    body::Body,
    extract::{self, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use poise::serenity_prelude::{ExecuteWebhook, Http, Webhook};
use serde::Deserialize;

use axum::routing::post;
use sqlx::{
    Pool, Postgres,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tokio::signal;

pub type BoxErr = Box<dyn Error + Send + Sync + 'static>;

#[derive(Clone)]
pub struct ServerState {
    pub webhook_url: String,
    pub pool: Pool<Postgres>,
}

#[tokio::main]
async fn main() -> Result<(), BoxErr> {
    println!("Starting up...");

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install ring as rustls crypto provider");

    println!("Connecting to database...");

    let db_port =
        &env::var("DATABASE_PORT").inspect_err(|_e| println!("failed to load database port"))?;

    let db_port: u16 = db_port
        .parse()
        .inspect_err(|_e| println!("failed to parse database port"))?;

    let pg_options = PgConnectOptions::new()
        .host(
            &env::var("DATABASE_HOST")
                .inspect_err(|_e| println!("failed to load database host"))?,
        )
        .port(db_port)
        .database(
            &env::var("DATABASE_NAME")
                .inspect_err(|_e| println!("failed to load database name"))?,
        )
        .username(
            &env::var("DATABASE_USERNAME")
                .inspect_err(|_e| println!("failed to load database user"))?,
        )
        .password(
            &env::var("DATABASE_PASSWORD")
                .inspect_err(|_e| println!("failed to load database host"))?,
        );

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pg_options)
        .await
        .inspect_err(|e| println!("Failed to create connection pool -- {}", e.to_string()))?;

    println!("Setting up poise");
    let opts = poise::FrameworkOptions {
        commands: vec![
            commands::register(),
            // commands::register(),
        ],
        skip_checks_for_owners: false,
        ..Default::default()
    };

    let data = commands::Data { pool: pool.clone() };
    let framework = poise::Framework::builder()
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", _ready.user.name);

                let shard_manager = framework.shard_manager().clone();
                tokio::spawn(async move {
                    shutdown_signal().await;

                    println!("Stopping client...");
                    shard_manager.shutdown_all().await;
                });

                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                Ok(data)
            })
        })
        .options(opts)
        .build();

    let discord_token =
        env::var("DISCORD_TOKEN").inspect_err(|_e| println!("failed to load discord token"))?;

    let discord_client = async {
        println!("Starting Discord client...");
        let intents = poise::serenity_prelude::GatewayIntents::non_privileged()
            | poise::serenity_prelude::GatewayIntents::MESSAGE_CONTENT;

        let mut client = poise::serenity_prelude::ClientBuilder::new(discord_token, intents)
            .framework(framework)
            .await?;

        client.start().await
    };

    // let discord_client = build_discord_client(&discord_token);

    let webhook_url = env::var("WEBHOOK_URL")?;
    let router = router(ServerState { webhook_url, pool });

    let server = async {
        let port = 3000;

        println!("Starting Server on port {port}...");
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        println!("Serving through axum");
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                shutdown_signal().await;
            })
            .await
    };

    let (_server, _discord) = tokio::join!(server, discord_client);

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to start SIGINT handler");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to start SIGTERM handler")
            .recv()
            .await;
    };

    tokio::select! {
        () = ctrl_c => (),
        () = terminate => (),
    }
}

#[derive(Deserialize, Debug)]
pub struct CivCloudHook {
    #[serde(rename = "value1")]
    pub game_name: String,
    #[serde(rename = "value2")]
    pub player_name: String,
    #[serde(rename = "value3")]
    pub turn_number: String,
}

/// Here we want to take the webhook payload we recieve from Civ 6's Play by Cloud feature
/// and convert it into something more reasonable for a Discord webhook.
///
/// Discord expects webhook data to be in the form of:
/// ```json
/// { "content":"..." }
/// ```
pub async fn handle_civ_webhook(
    State(state): extract::State<ServerState>,
    Json(payload): extract::Json<CivCloudHook>,
) -> Response<Body> {
    let pool = state.pool;
    let player_name = &payload.player_name.to_lowercase();
    let discord_tag: Result<Option<String>, sqlx::Error> = sqlx::query_scalar(
        r#"
        SELECT discord_id
        FROM civ_discord_user_map
        where civ_user_name = $1
    "#,
    )
    .bind(&player_name)
    .fetch_one(&pool)
    .await;

    let discord_tag = match discord_tag {
        Ok(Some(discord_id)) => format!("<@{discord_id}>"),
        Err(_) | Ok(None) => player_name.to_owned(),
    };

    let msg = format!(
        "Hey {discord_tag}, it's time to take your turn in {}! Game is currently on turn {}",
        payload.game_name, payload.turn_number
    );

    let http = Http::new("");
    let webhook = Webhook::from_url(&http, &state.webhook_url)
        .await
        .expect("Replace the webhook with your own");

    let builder = ExecuteWebhook::new().content(msg).username("Gorgonzola");

    webhook
        .execute(&http, false, builder)
        .await
        .expect("Could not execute webhook.");

    StatusCode::OK.into_response()
}

pub fn router(state: ServerState) -> axum::Router {
    axum::Router::new()
        .route("/webhooks", post(handle_civ_webhook))
        .with_state(state)
}
