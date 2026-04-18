//! `rcc` launcher wrapper.
//!
//! Usage:
//!   rcc [codex|minimax|geminiforge|geminicliforge|minimaxforge|kimi] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!   rcc minimax [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!   rcc codex [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!   rcc geminiforge [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!   rcc geminicliforge [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!   rcc minimaxforge [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!   rcc kimi [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!
//! Also supports the older flag form:
//!   rcc [--codex|--minimax|--geminiforge|--geminicliforge|--minimaxforge|--kimi] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]
//!   rcc [--provider=<name>|--backend=<name>] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]

use std::path::Path;
use std::process::Command;

use chatbot_lib::model::ModelProvider;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

struct CliArgs {
    provider: &'static str,
    forwarded: Vec<String>,
    show_help: bool,
}

fn print_usage() {
    eprintln!(
        "Usage: rcc [codex|minimax|geminiforge|geminicliforge|minimaxforge|kimi] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]\n\
         \n\
         Also accepts: rcc [codex|minimax|geminiforge|geminicliforge|minimaxforge|kimi] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]\n\
         Also accepts: rcc [--codex|--minimax|--geminiforge|--geminicliforge|--minimaxforge|--kimi] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]\n\
         Also accepts: rcc [--provider=<name>|--backend=<name>] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]\n"
    );
}

fn parse_args_from<I>(args: I) -> CliArgs
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().skip(1);
    let mut provider: Option<&'static str> = None;
    let mut forwarded: Vec<String> = Vec::new();

    while let Some(arg) = args.next() {
        if matches!(arg.as_str(), "--help" | "-h") {
            return CliArgs {
                provider: "codex",
                forwarded,
                show_help: true,
            };
        }

        if matches!(arg.as_str(), "--provider" | "--backend") {
            let Some(value) = args.next() else {
                forwarded.push(arg);
                continue;
            };
            if let Some(parsed_provider) = ModelProvider::from_launch_alias(&value) {
                provider = Some(parsed_provider.as_str());
            } else {
                forwarded.push(arg);
                forwarded.push(value);
            }
            continue;
        }

        if let Some(stripped) = arg.strip_prefix("--") {
            if let Some((flag, value)) = stripped.split_once('=') {
                if matches!(flag, "provider" | "backend") {
                    if let Some(parsed_provider) = ModelProvider::from_launch_alias(value) {
                        provider = Some(parsed_provider.as_str());
                    } else {
                        forwarded.push(arg);
                    }
                } else {
                    forwarded.push(arg);
                }
            } else if let Some(parsed_provider) = ModelProvider::from_launch_alias(stripped) {
                provider = Some(parsed_provider.as_str());
            } else {
                forwarded.push(arg);
            }
            continue;
        }

        if let Some(parsed_provider) = ModelProvider::from_launch_alias(arg.as_str()) {
            provider = Some(parsed_provider.as_str());
        } else {
            forwarded.push(arg);
        };
    }

    CliArgs {
        provider: provider.unwrap_or("codex"),
        forwarded,
        show_help: false,
    }
}

fn main() {
    chatbot_lib::process_limits::apply_default_process_memory_limit("rcc");
    chatbot_lib::ensure_path_symlink();

    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("Failed to locate current executable: {}", err);
            std::process::exit(1);
        }
    };
    let resolved_exe = current_exe
        .canonicalize()
        .unwrap_or_else(|_| current_exe.clone());
    let self_dir = resolved_exe.parent().unwrap_or(Path::new("."));
    let ctl = self_dir.join("chatbot-ctl");

    if !ctl.exists() {
        eprintln!(
            "Launcher could not find sibling 'chatbot-ctl' next to {}",
            resolved_exe.display()
        );
        std::process::exit(1);
    }

    let cli = parse_args_from(std::env::args());
    if cli.show_help {
        print_usage();
        return;
    }
    let provider = cli.provider;
    let forwarded = cli.forwarded;

    #[cfg(unix)]
    {
        let err = Command::new(&ctl)
            .arg("launch")
            .arg("--provider")
            .arg(provider)
            .args(&forwarded)
            .exec();
        eprintln!("Failed to exec {}: {}", ctl.display(), err);
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    {
        match Command::new(&ctl)
            .arg("launch")
            .arg("--provider")
            .arg(provider)
            .args(&forwarded)
            .status()
        {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(err) => {
                eprintln!("Failed to launch {}: {}", ctl.display(), err);
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_supported_provider_names() {
        for provider in ModelProvider::LAUNCHABLE {
            let parsed = parse_args_from(vec!["rcc".to_string(), provider.as_str().to_string()]);
            assert_eq!(parsed.provider, provider.as_str());
            assert!(parsed.forwarded.is_empty());
        }
    }

    #[test]
    fn parse_args_defaults_to_codex_when_provider_is_omitted() {
        let parsed = parse_args_from(vec!["rcc".to_string()]);
        assert_eq!(parsed.provider, ModelProvider::Codex.as_str());
        assert!(parsed.forwarded.is_empty());
        assert!(!parsed.show_help);
    }

    #[test]
    fn parse_args_accepts_provider_flags_and_aliases() {
        for provider in ModelProvider::LAUNCHABLE {
            let parsed =
                parse_args_from(vec!["rcc".to_string(), format!("--{}", provider.as_str())]);
            assert_eq!(parsed.provider, provider.as_str());
        }
    }

    #[test]
    fn parse_args_accepts_provider_equals_flags() {
        for provider in ModelProvider::LAUNCHABLE {
            let parsed = parse_args_from(vec![
                "rcc".to_string(),
                format!("--provider={}", provider.as_str()),
            ]);
            assert_eq!(parsed.provider, provider.as_str());
        }
    }

    #[test]
    fn parse_args_accepts_provider_with_explicit_value() {
        for provider in ModelProvider::LAUNCHABLE {
            let parsed = parse_args_from(vec![
                "rcc".to_string(),
                "--provider".to_string(),
                provider.as_str().to_string(),
            ]);
            assert_eq!(parsed.provider, provider.as_str());
        }
    }

    #[test]
    fn parse_args_leaves_disabled_providers_as_forwarded_args() {
        let parsed = parse_args_from(vec!["rcc".to_string(), "claude".to_string()]);
        assert_eq!(parsed.provider, ModelProvider::Codex.as_str());
        assert_eq!(parsed.forwarded, vec!["claude".to_string()]);
    }
}
