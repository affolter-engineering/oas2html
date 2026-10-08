use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

mod renderer;

#[derive(Parser)]
#[command(
    name = "oas2html",
    about = "Convert OpenAPI / Swagger specifications (JSON or YAML) to HTML",
    version
)]
struct Cli {
    /// Input spec file (.json, .yaml, or .yml)
    input: PathBuf,

    /// Write HTML to a file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Override the API title shown in the output
    #[arg(short, long)]
    title: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Try the openapi crate first (Swagger 2.0, handles both JSON and YAML)
    let html = match openapi::from_path(&cli.input) {
        Ok(spec) => renderer::render_swagger(&spec, cli.title.as_deref()),
        Err(_) => {
            // OpenAPI 3.x or unknown: parse as raw JSON/YAML
            let content = std::fs::read_to_string(&cli.input)
                .with_context(|| format!("cannot read {}", cli.input.display()))?;
            let value = parse_raw(&content, &cli.input)?;
            renderer::render_oa3(&value, cli.title.as_deref())
        }
    };

    match &cli.output {
        Some(path) => std::fs::write(path, &html)
            .with_context(|| format!("cannot write {}", path.display()))?,
        None => print!("{html}"),
    }
    Ok(())
}

fn parse_raw(content: &str, path: &PathBuf) -> Result<serde_json::Value> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "yaml" | "yml" => {
            let v: serde_yaml::Value = serde_yaml::from_str(content)
                .context("YAML parse error")?;
            Ok(serde_json::to_value(v)?)
        }
        "json" => serde_json::from_str(content).context("JSON parse error"),
        _ => serde_json::from_str(content).or_else(|_| {
            serde_yaml::from_str::<serde_yaml::Value>(content)
                .context("YAML parse error")
                .and_then(|v| Ok(serde_json::to_value(v)?))
        }),
    }
}
