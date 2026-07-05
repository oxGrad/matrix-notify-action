mod config;
mod matrix;
mod output;
mod render;

use config::Config;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        let msg = e.to_string();
        eprintln!("Error: {}", msg);
        let _ = output::write_error(&msg);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let rendered = render::render(&config.message, &config.format);
    let client = matrix::build_client(&config).await?;
    let result = matrix::send_message(&client, &config, &rendered).await;
    matrix::maybe_logout(&client, &config).await;
    output::write_event_id(&result?)?;
    Ok(())
}
