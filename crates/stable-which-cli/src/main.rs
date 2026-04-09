use serde::Serialize;
use stable_which::candidate::{
    Candidate, PathTag, ScoringPolicy, find_candidates, resolve_stable_path,
};
use std::env;
use std::path::Path;
use std::process;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = "stable-which";

// JSON serialization wrappers

#[derive(Serialize)]
struct CandidateJson {
    path: String,
    canonical: String,
    tags: Vec<serde_json::Value>,
    score: i32,
}

impl CandidateJson {
    fn from_candidate(c: &Candidate, policy: ScoringPolicy) -> Self {
        CandidateJson {
            path: c.path.display().to_string(),
            canonical: c.canonical.display().to_string(),
            tags: c.tags.iter().map(tag_to_json).collect(),
            score: c.score(policy),
        }
    }
}

fn tag_to_json(tag: &PathTag) -> serde_json::Value {
    match tag {
        PathTag::Input => serde_json::Value::String("input".into()),
        PathTag::InPathEnv(order) => serde_json::json!({"in-path-env": order}),
        PathTag::SymlinkTo(target) => {
            serde_json::json!({"symlink-to": target.display().to_string()})
        }
        PathTag::Shim => serde_json::Value::String("shim".into()),
        PathTag::SameCanonical => serde_json::Value::String("same-canonical".into()),
        PathTag::SameContent => serde_json::Value::String("same-content".into()),
        PathTag::Relative => serde_json::Value::String("relative".into()),
        PathTag::NonNormalized => serde_json::Value::String("non-normalized".into()),
        PathTag::DifferentBinary => serde_json::Value::String("different-binary".into()),
        PathTag::ManagedBy(name) => serde_json::json!({"managed-by": name}),
        PathTag::BuildOutput => serde_json::Value::String("build-output".into()),
        PathTag::Ephemeral => serde_json::Value::String("ephemeral".into()),
    }
}

enum OutputFormat {
    Path,
    Json,
}

fn parse_policy(s: &str) -> Result<ScoringPolicy, String> {
    match s {
        "same-binary" => Ok(ScoringPolicy::SameBinary),
        "stable" => Ok(ScoringPolicy::Stable),
        _ => Err(format!(
            "unknown policy: {s} (expected: same-binary, stable)"
        )),
    }
}

fn print_help(to_stderr: bool) {
    let help_text = format!(
        "\
{NAME} {VERSION}
Evaluate binary path stability and find stable PATH candidates.

Usage:
    {NAME} [OPTIONS] <binary>

Arguments:
    <binary>         Path to the binary, or a command name to look up in PATH

Options:
    --all            Show all candidates (default: best candidate only)
    --format <F>     Output format: path (default), json
    --policy <P>     Scoring policy: same-binary (default), stable
    --inspect        Show all candidates as JSON (same as --all --format json)
    --help           Show this help message
    --version        Show version"
    );
    if to_stderr {
        eprint!("{help_text}");
    } else {
        print!("{help_text}");
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_help(true);
        process::exit(1);
    }

    let mut show_all = false;
    let mut format = OutputFormat::Path;
    let mut policy = ScoringPolicy::SameBinary;
    let mut binary_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => {
                print_help(false);
                return Ok(());
            }
            "--version" => {
                println!("{NAME} {VERSION}");
                return Ok(());
            }
            "--all" => show_all = true,
            "--format" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| "--format requires a value (path, json)".to_string())?;
                format = match val.as_str() {
                    "path" => OutputFormat::Path,
                    "json" => OutputFormat::Json,
                    _ => return Err(format!("unknown format: {val} (expected: path, json)")),
                };
            }
            "--policy" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| "--policy requires a value (same-binary, stable)".to_string())?;
                policy = parse_policy(val)?;
            }
            "--inspect" => {
                show_all = true;
                format = OutputFormat::Json;
            }
            _ if args[i].starts_with('-') => {
                return Err(format!("unknown option: {}", args[i]));
            }
            _ => {
                if binary_path.is_some() {
                    return Err("too many arguments".to_string());
                }
                binary_path = Some(args[i].clone());
            }
        }
        i += 1;
    }

    let binary_path = binary_path.ok_or_else(|| {
        print_help(true);
        String::new()
    })?;

    if show_all {
        let candidates =
            find_candidates(Path::new(&binary_path), policy).map_err(|e| e.to_string())?;
        match format {
            OutputFormat::Path => {
                for c in &candidates {
                    println!("{}", c.path.display());
                }
            }
            OutputFormat::Json => {
                let json_candidates: Vec<CandidateJson> = candidates
                    .iter()
                    .map(|c| CandidateJson::from_candidate(c, policy))
                    .collect();
                let json = serde_json::to_string_pretty(&json_candidates)
                    .map_err(|e| format!("JSON serialization error: {e}"))?;
                println!("{json}");
            }
        }
    } else {
        let best =
            resolve_stable_path(Path::new(&binary_path), policy).map_err(|e| e.to_string())?;
        match format {
            OutputFormat::Path => {
                println!("{}", best.path.display());
            }
            OutputFormat::Json => {
                let json_value = CandidateJson::from_candidate(&best, policy);
                let json = serde_json::to_string_pretty(&json_value)
                    .map_err(|e| format!("JSON serialization error: {e}"))?;
                println!("{json}");
            }
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        if !e.is_empty() {
            eprintln!("{NAME}: {e}");
        }
        process::exit(1);
    }
}
