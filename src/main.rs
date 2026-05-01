mod args;
mod auth;
mod client;
mod output;
mod response;
mod security;

use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use args::Cli;
use clap::Parser;
use client::ImageRequest;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli).await
}

async fn run(cli: Cli) -> Result<()> {
    let prompt = cli.prompt_text().context("failed to read prompt")?;
    let codex_home = auth::resolve_codex_home(cli.codex_home.as_deref())?;
    let model =
        auth::resolve_effective_model(&codex_home, cli.profile.as_deref(), cli.model.as_deref())?;
    let mut request = ImageRequest::from_cli(&cli, prompt, model)?;

    if cli.dry_run {
        print_dry_run(&request)?;
        return Ok(());
    }

    let installation_id = auth::resolve_installation_id(&codex_home)?;
    request.set_codex_identity(installation_id);

    let mut auth_store = auth::AuthStore::load(
        &codex_home,
        cli.auth_store,
        cli.auth_source,
        cli.profile.as_deref(),
    )?;
    let base_url = cli
        .base_url
        .clone()
        .unwrap_or_else(|| auth_store.default_base_url().to_string());
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cli.timeout_secs))
        .user_agent(client::USER_AGENT)
        .build()
        .context("failed to build HTTP client")?;
    let proactive_refresh_error = if auth_store.is_stale_chatgpt_auth() && auth_store.can_refresh()
    {
        auth_store.refresh_chatgpt_token(&http_client).await.err()
    } else {
        None
    };

    let response_value = match client::create_response(
        &http_client,
        &base_url,
        &auth_store,
        &request,
    )
    .await
    {
        Ok(value) => value,
        Err(client::ApiError::Unauthorized { body }) if auth_store.can_refresh() => {
            auth_store
                    .refresh_chatgpt_token(&http_client)
                    .await
                    .with_context(|| {
                        if let Some(err) = proactive_refresh_error.as_ref() {
                            format!(
                                "token refresh failed after unauthorized response: {body}; proactive refresh had already failed: {err}"
                            )
                        } else {
                            format!("token refresh failed after unauthorized response: {body}")
                        }
                    })?;
            client::create_response(&http_client, &base_url, &auth_store, &request).await?
        }
        Err(err) => return Err(err.into()),
    };

    let image = response::extract_first_image(&response_value)?;
    let output_path = output::resolve_output_path(cli.output.as_deref(), request.output_format())?;
    output::write_base64_image(&output_path, &image.result).await?;

    let report = output::GenerationReport {
        response_id: response::response_id(&response_value),
        image_id: image.id,
        revised_prompt: image.revised_prompt,
        output_path,
        format: request.output_format().to_string(),
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("saved: {}", report.output_path.display());
        if let Some(response_id) = report.response_id.as_deref() {
            println!("response_id: {response_id}");
        }
        if let Some(image_id) = report.image_id.as_deref() {
            println!("image_id: {image_id}");
        }
    }

    Ok(())
}

fn print_dry_run(request: &ImageRequest) -> Result<()> {
    let body = request.to_body();
    let preview = serde_json::json!({
        "method": "POST",
        "path": "/responses",
        "auth": "codex credential is read only when dry-run is not set",
        "body": body,
    });
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}
