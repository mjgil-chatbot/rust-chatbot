use anyhow::{bail, Context, Result};
use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub allowed_roots: Vec<PathBuf>,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self> {
        let allowed_roots = match env::var_os("CHATBOT_MEMORY_MCP_ALLOWED_ROOTS") {
            Some(value) if !value.is_empty() => env::split_paths(&value)
                .map(|path| {
                    if !path.is_absolute() {
                        bail!(
                            "CHATBOT_MEMORY_MCP_ALLOWED_ROOTS must contain absolute paths: {}",
                            path.display()
                        );
                    }

                    path.canonicalize()
                        .with_context(|| format!("Cannot resolve allowed root {}", path.display()))
                })
                .collect::<Result<Vec<_>>>()?,
            _ => vec![env::current_dir()
                .context("Cannot resolve current directory for default allowed root")?
                .canonicalize()
                .context("Cannot canonicalize default allowed root")?],
        };

        Ok(Self { allowed_roots })
    }
}
