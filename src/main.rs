//! Desktop Chat Application
//!
//! A desktop GUI application for chatting with multiple model CLIs.
//!
//! ## Architecture
//!
//! The application is organized into the following modules:
//! - `app` - Application state and UI components
//!   - `state` - Main AppState struct and business logic
//!   - `ui` - User interface rendering
//! - `provider_bridge` - Shared provider launch/runtime helpers
//! - `worker` - Detached worker lifecycle and spool integration
//! - `config` - Configuration constants
//! - `data` - Data models and persistence
//! - `markdown` - Markdown rendering with table support

use std::sync::Arc;
use eframe::egui;
pub use chatbot_lib::config;
pub use chatbot_lib::data;
pub use chatbot_lib::debug;
pub use chatbot_lib::memory;
pub use chatbot_lib::memory_job;
pub use chatbot_lib::model;
pub use chatbot_lib::omniforge;
pub use chatbot_lib::provider_bridge;
pub use chatbot_lib::storage;
pub use chatbot_lib::stream;
pub use chatbot_lib::summary;
pub use chatbot_lib::sync;
pub use chatbot_lib::worker;
pub use chatbot_lib::workspace;
#[cfg(test)]
pub(crate) use chatbot_lib::TEST_ENV_LOCK;
mod app;
mod common;
mod document;
mod forge_loop;
mod inline;
mod markdown;
mod socket;
use model::ModelProvider;
use rust_debug::tracing::{error, info};
fn apply_default_debug_env() {
    set_env_default("CLAUDE_LOG_STDERR", "1");
}
fn set_env_default(name: &str, value: &str) {
    if std::env::var_os(name).is_none() {
        std::env::set_var(name, value);
    }
}
struct CliArgs {
    provider: ModelProvider,
    instance: Option<u32>,
    window_geometry: WindowGeometry,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WindowGeometry {
    x: Option<i32>,
    y: Option<i32>,
    width: Option<f32>,
    height: Option<f32>,
}
impl WindowGeometry {
    fn apply_to_viewport(self, mut viewport: egui::ViewportBuilder) -> egui::ViewportBuilder {
        if let (Some(x), Some(y)) = (self.x, self.y) {
            viewport = viewport.with_position([x as f32, y as f32]);
        }

        viewport.with_inner_size([
            self.width.unwrap_or(config::WINDOW_WIDTH),
            self.height.unwrap_or(config::WINDOW_HEIGHT),
        ])
    }}
fn parse_provider_flag(arg: &str) -> Option<ModelProvider> {
    let Some(stripped) = arg.strip_prefix("--") else {
        return None;
    };

    if let Some((flag, value)) = stripped.split_once('=') {
        match flag {
            "provider" | "backend" => ModelProvider::from_launch_alias(value),
            _ => None,
        }
    } else {
        ModelProvider::from_launch_alias(stripped)
    }
}
fn parse_args() -> CliArgs {
    parse_args_from_with_default(std::env::args(), configured_default_provider())
}
#[cfg(test)]
fn parse_args_from<I>(args: I) -> CliArgs
where
    I: IntoIterator<Item = String>,
{
    parse_args_from_with_default(args, ModelProvider::Codex)
}
fn parse_args_from_with_default<I>(args: I, default_provider: ModelProvider) -> CliArgs
where
    I: IntoIterator<Item = String>,
{
    let mut provider = default_provider;
    let mut instance: Option<u32> = None;
    let mut window_geometry = WindowGeometry::default();
    let mut args = args.into_iter().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--provider" | "--backend" => {
                if let Some(value) = args.next() {
                    if let Some(parsed) = ModelProvider::from_launch_alias(&value) {
                        provider = parsed;
                    } else {
                        provider = default_provider;
                        eprintln!(
                            "Unknown provider '{}'. Using default ({}).",
                            value,
                            default_provider.as_str()
                        );
                    }
                } else {
                    provider = default_provider;
                    eprintln!(
                        "Missing value for --provider/--backend. Using default ({}).",
                        default_provider.as_str()
                    );
                }
            }
            "--instance" => {
                if let Some(value) = args.next() {
                    if let Ok(n) = value.parse::<u32>() {
                        instance = Some(n);
                    } else {
                        eprintln!("Invalid instance number '{}'. Ignoring.", value);
                    }
                } else {
                    eprintln!("Missing value for --instance.");
                }
            }
            "--window-x" => {
                parse_i32_flag_value(args.next(), "--window-x")
                    .map(|value| window_geometry.x = Some(value));
            }
            "--window-y" => {
                parse_i32_flag_value(args.next(), "--window-y")
                    .map(|value| window_geometry.y = Some(value));
            }
            "--window-width" => {
                parse_positive_f32_flag_value(args.next(), "--window-width")
                    .map(|value| window_geometry.width = Some(value));
            }
            "--window-height" => {
                parse_positive_f32_flag_value(args.next(), "--window-height")
                    .map(|value| window_geometry.height = Some(value));
            }
            "--help" | "-h" => {
                print_provider_help();
                std::process::exit(0);
            }
            _ => {
                if let Some(parsed) = parse_provider_flag(&arg) {
                    provider = parsed;
                }
            }
        }
    }

    CliArgs {
        provider,
        instance,
        window_geometry,
    }
}
fn configured_default_provider() -> ModelProvider {
    resolve_default_provider(&config::config().provider.default_provider)
}
fn resolve_default_provider(provider_name: &str) -> ModelProvider {
    if let Some(provider) = ModelProvider::from_launch_alias(provider_name) {
        provider
    } else {
        eprintln!(
            "Invalid configured default provider '{}'. Falling back to codex.",
            provider_name
        );
        ModelProvider::Codex
    }
}
fn parse_i32_flag_value(value: Option<String>, flag_name: &str) -> Option<i32> {
    let Some(value) = value else {
        eprintln!("Missing value for {}.", flag_name);
        return None;
    };

    match value.parse::<i32>() {
        Ok(parsed) => Some(parsed),
        Err(_) => {
            eprintln!("Invalid value '{}' for {}. Ignoring.", value, flag_name);
            None
        }
    }
}
fn parse_positive_f32_flag_value(value: Option<String>, flag_name: &str) -> Option<f32> {
    let Some(value) = value else {
        eprintln!("Missing value for {}.", flag_name);
        return None;
    };

    match value.parse::<f32>() {
        Ok(parsed) if parsed.is_finite() && parsed > 0.0 => Some(parsed),
        _ => {
            eprintln!("Invalid value '{}' for {}. Ignoring.", value, flag_name);
            None
        }
    }
}
fn print_provider_help() {
    println!(
        "Desktop Chat\n\n\
Usage: rust-chatbot [--minimax | --codex | --geminiforge | --geminicliforge | --minimaxforge | --kimi | --provider <name>]\n\n\
Options:\n\
  --minimax                Use MiniMax M2.7\n\
  --codex                  Use Codex\n\
  --geminiforge            Use GeminiForge\n\
  --geminicliforge         Use GeminiCliForge\n\
  --minimaxforge           Use MiniMaxForge\n\
  --kimi                   Use Kimi\n\
  --provider, --backend    Select provider: minimax | codex | geminiforge | geminicliforge | minimaxforge | kimi\n\
  Startup default comes from [provider].default_provider in ~/.rust-chatbot/config.toml and falls back to codex.\n\
  --instance <n>           Instance number (used for window/app identity and side panel)\n\
  --window-x <n>           Initial outer window X position\n\
  --window-y <n>           Initial outer window Y position\n\
  --window-width <n>       Initial window width\n\
  --window-height <n>      Initial window height\n\
  -h, --help               Show this help message\n"
    );
}
fn icon_data(provider: ModelProvider) -> Arc<egui::IconData> {
    let png_bytes: &[u8] = match provider {
        ModelProvider::Claude => include_bytes!("../assets/anthropic_logo.png"),
        ModelProvider::MiniMax => include_bytes!("../assets/minimax_logo.png"),
        ModelProvider::Qwen => include_bytes!("../assets/qwen_logo.png"),
        ModelProvider::Codex => include_bytes!("../assets/openai_logo.png"),
        ModelProvider::Nvidia => include_bytes!("../assets/openai_logo.png"), // Reuse for now
        ModelProvider::Gemini => include_bytes!("../assets/gemini_logo.png"),
        ModelProvider::OpenRouter => include_bytes!("../assets/openai_logo.png"),
        ModelProvider::OmniForge => include_bytes!("../assets/gemini_logo.png"),
        ModelProvider::GeminiForge => include_bytes!("../assets/gemini_logo.png"),
        ModelProvider::GeminiCliForge => include_bytes!("../assets/gemini_logo.png"),
        ModelProvider::MiniMaxForge => include_bytes!("../assets/minimax_logo.png"),
        ModelProvider::Kimi => include_bytes!("../assets/qwen_logo.png"), // Reuse for now
        ModelProvider::OpenCode => include_bytes!("../assets/openai_logo.png"),
        ModelProvider::Hermes => include_bytes!("../assets/openai_logo.png"),
        ModelProvider::Forge => include_bytes!("../assets/openai_logo.png"),
        ModelProvider::Claw => include_bytes!("../assets/openai_logo.png"),
    };
    let img = image::load_from_memory(png_bytes)
        .expect("failed to load provider logo")
        .into_rgba8();
    let (width, height) = img.dimensions();
    Arc::new(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}
#[cfg(target_os = "linux")]
fn reexec_with_provider_argv0(provider: ModelProvider, instance: Option<u32>) {
    use std::ffi::OsString;
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    const ARG0_ENV: &str = "DESKTOP_CHAT_ARGV0_APPLIED";

    if std::env::var_os(ARG0_ENV).is_some() {
        return;
    }

    let desired_arg0 = OsString::from(config::window_app_id(provider, instance));
    let current_arg0 = std::env::args_os().next();
    let current_name = current_arg0
        .as_deref()
        .and_then(|arg0| Path::new(arg0).file_name());

    if current_name == Some(desired_arg0.as_os_str()) {
        return;
    }

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            error!(error=%err, "failed to resolve current executable for re-exec");
            return;
        }
    };
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let err = Command::new(exe)
        .args(args)
        .arg0(&desired_arg0)
        .env(ARG0_ENV, "1")
        .exec();
    error!(error=%err, "failed to re-exec process with provider window class");
}
#[cfg(not(target_os = "linux"))]
fn reexec_with_provider_argv0(_provider: ModelProvider, _instance: Option<u32>) {}
fn main() {}
