use anyhow::Result;
use api_server::{app, state::AppState};

#[tokio::main]
async fn main() -> Result<()> {
    let state = AppState::new();
    let router = app(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, router).await?;

    Ok(())
}
