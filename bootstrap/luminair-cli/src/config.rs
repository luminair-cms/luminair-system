//! CLI configuration types, deserialization, environment variable ingestion, and validation.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::error::CliError;
use luminair_persistence::migration::SafetyPolicy;

// ---------------------------------------------------------------------------
// RunMode & OutputFormat Enums
// ---------------------------------------------------------------------------

/// Execution mode for the CLI command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RunMode {
    /// Offline validation of configuration files and schema definitions.
    #[default]
    Validate,
    /// Connects to DB (or runs offline) and plans migrations without executing them.
    DryRun,
    /// Executes static and dynamic migrations against the target database.
    Migrate,
}

/// Output rendering format for CLI outcome reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Human-readable structured text report.
    #[default]
    Text,
    /// Machine-readable pretty JSON payload.
    Json,
}

// ---------------------------------------------------------------------------
// CliConfig Struct & File DTO
// ---------------------------------------------------------------------------

/// Runtime configuration parsed from CLI flags, environment variables, or config files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConfig {
    /// Desired execution mode.
    pub mode: RunMode,
    /// Root directory containing JSON schemas and system configuration.
    pub schema_dir: PathBuf,
    /// PostgreSQL connection connection string (optional in validate/dry-run modes).
    pub database_url: Option<String>,
    /// Maximum connection pool size.
    pub max_db_connections: u32,
    /// Safety policy governing dynamic DDL generation.
    pub safety_policy: SafetyPolicy,
    /// Whether to force offline operation without attempting DB connectivity.
    pub offline: bool,
    /// Desired output rendering format.
    pub output_format: OutputFormat,
    /// Optional file destination to write the outcome report.
    pub output_path: Option<PathBuf>,
}

/// Partial DTO representing file-based configuration options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CliConfigFileDto {
    pub mode: Option<RunMode>,
    pub schema_dir: Option<PathBuf>,
    pub database_url: Option<String>,
    pub max_db_connections: Option<u32>,
    pub safety_policy: Option<SafetyPolicy>,
    pub offline: Option<bool>,
    pub output_format: Option<OutputFormat>,
    pub output_path: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl RunMode {
    /// Returns the canonical string identifier for the mode.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Migrate => "migrate",
            Self::DryRun => "dry-run",
            Self::Validate => "validate",
        }
    }
}

impl OutputFormat {
    /// Returns the canonical string identifier for the format.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
        }
    }
}

impl CliConfig {
    /// Loads a partial configuration from a JSON file, overlaying values over defaults.
    pub fn from_json_file(path: &Path) -> Result<Self, CliError> {
        let content = std::fs::read_to_string(path).map_err(|source| CliError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let dto = serde_json::from_str::<CliConfigFileDto>(&content).map_err(|source| {
            CliError::JsonConfig {
                path: path.to_path_buf(),
                source,
            }
        })?;

        let mut config = Self::default();
        apply_file_dto(&mut config, dto);
        Ok(config)
    }

    /// Ingests configuration from process environment variables and CLI arguments.
    pub fn from_env_and_args() -> Result<Self, CliError> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        Self::parse_from_args_and_env(args, |key| std::env::var(key).ok())
    }

    /// Ingests configuration purely from an iterator of CLI arguments (deterministic in tests).
    pub fn parse_from_iter<I, S>(args: I) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::parse_from_args_and_env(args, |_| None)
    }

    /// Ingests configuration from arguments and an environment variable lookup closure.
    pub fn parse_from_args_and_env<I, S, F>(args: I, env_lookup: F) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        F: Fn(&str) -> Option<String>,
    {
        let normalized = normalize_cli_args(args);
        let mut config = match extract_config_path(&normalized, &env_lookup) {
            Some(path) => Self::from_json_file(&path)?,
            None => Self::default(),
        };

        apply_env_overrides(&mut config, &env_lookup)?;
        apply_cli_arg_overrides(&mut config, &normalized)?;
        Ok(config)
    }

    /// Validates the configuration semantics and constraints.
    pub fn validate(&self) -> Result<(), CliError> {
        if self.max_db_connections == 0 {
            return Err(CliError::Config(
                "max_db_connections must be greater than zero".to_string(),
            ));
        }

        if !self.schema_dir.exists() {
            return Err(CliError::Config(format!(
                "schema directory '{}' does not exist",
                self.schema_dir.display()
            )));
        }

        if let Some(ref db_url) = self.database_url {
            validate_database_url(db_url)?;
        }

        if self.mode == RunMode::Migrate && self.database_url.is_none() {
            return Err(CliError::Config(format!(
                "database_url is required for '{}' mode",
                self.mode
            )));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl FromStr for RunMode {
    type Err = CliError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "migrate" => Ok(Self::Migrate),
            "dry-run" => Ok(Self::DryRun),
            "validate" => Ok(Self::Validate),
            "serve" => Err(CliError::Config(
                "the 'serve' mode is handled by 'luminair-server'; 'luminair-cli' supports 'validate', 'dry-run', and 'migrate'".to_string(),
            )),
            other => Err(CliError::Config(format!(
                "unknown run mode '{other}', expected 'migrate', 'dry-run', or 'validate'"
            ))),
        }
    }
}

impl fmt::Display for RunMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for OutputFormat {
    type Err = CliError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(CliError::Config(format!(
                "unknown output format '{other}', expected 'text' or 'json'"
            ))),
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            mode: RunMode::Validate,
            schema_dir: PathBuf::from("./config/schemas"),
            database_url: None,
            max_db_connections: 5,
            safety_policy: SafetyPolicy::AdditiveOnly,
            offline: false,
            output_format: OutputFormat::Text,
            output_path: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Private Helpers & Normalized Argument Processing
// ---------------------------------------------------------------------------

/// Normalizes CLI tokens by splitting `--flag=value` into `["--flag", "value"]`.
fn normalize_cli_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut normalized = Vec::new();
    for item in args {
        let s = item.as_ref();
        if s.starts_with('-')
            && let Some((k, v)) = s.split_once('=')
        {
            normalized.push(k.to_string());
            normalized.push(v.to_string());
        } else {
            normalized.push(s.to_string());
        }
    }
    normalized
}

fn extract_config_path<F>(args: &[String], env_lookup: &F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "--config" || args[i] == "-c") && i + 1 < args.len() {
            return Some(PathBuf::from(&args[i + 1]));
        }
        i += 1;
    }
    env_lookup("LUMINAIR_CONFIG_FILE").map(PathBuf::from)
}

fn apply_file_dto(config: &mut CliConfig, dto: CliConfigFileDto) {
    if let Some(v) = dto.mode {
        config.mode = v;
    }
    if let Some(v) = dto.schema_dir {
        config.schema_dir = v;
    }
    if dto.database_url.is_some() {
        config.database_url = dto.database_url;
    }
    if let Some(v) = dto.max_db_connections {
        config.max_db_connections = v;
    }
    if let Some(v) = dto.safety_policy {
        config.safety_policy = v;
    }
    if let Some(v) = dto.offline {
        config.offline = v;
    }
    if let Some(v) = dto.output_format {
        config.output_format = v;
    }
    if dto.output_path.is_some() {
        config.output_path = dto.output_path;
    }
}

fn apply_env_overrides<F>(config: &mut CliConfig, env_lookup: &F) -> Result<(), CliError>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(val) = env_lookup("LUMINAIR_CLI_MODE").or_else(|| env_lookup("LUMINAIR_MODE")) {
        config.mode = RunMode::from_str(&val)?;
    }
    if let Some(val) = env_lookup("LUMINAIR_SCHEMA_DIR") {
        config.schema_dir = PathBuf::from(val);
    }
    if let Some(val) = env_lookup("LUMINAIR_DATABASE_URL").or_else(|| env_lookup("DATABASE_URL")) {
        config.database_url = Some(val);
    }
    if let Some(val) = env_lookup("LUMINAIR_MAX_DB_CONNECTIONS") {
        config.max_db_connections = val.parse::<u32>().map_err(|e| {
            CliError::Config(format!("invalid LUMINAIR_MAX_DB_CONNECTIONS '{val}': {e}"))
        })?;
    }
    if let Some(val) = env_lookup("LUMINAIR_SAFETY_POLICY") {
        config.safety_policy = parse_safety_policy(&val)?;
    }
    if let Some(val) = env_lookup("LUMINAIR_OFFLINE") {
        config.offline = parse_bool(&val)?;
    }
    if let Some(val) = env_lookup("LUMINAIR_OUTPUT_FORMAT") {
        config.output_format = OutputFormat::from_str(&val)?;
    }
    if let Some(val) = env_lookup("LUMINAIR_OUTPUT_PATH") {
        config.output_path = Some(PathBuf::from(val));
    }
    Ok(())
}

fn apply_cli_arg_overrides(config: &mut CliConfig, args: &[String]) -> Result<(), CliError> {
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if i == 0 && !arg.starts_with('-') && RunMode::from_str(arg).is_err() && args.len() > 1 {
            i += 1;
            continue;
        }

        if !arg.starts_with('-')
            && let Ok(mode) = RunMode::from_str(arg)
        {
            config.mode = mode;
            i += 1;
            continue;
        }

        match arg.as_str() {
            "--offline" => {
                config.offline = true;
                i += 1;
                continue;
            }
            "--config" | "-c" => {
                if i + 1 >= args.len() {
                    return Err(CliError::Config(format!("flag '{arg}' requires a value")));
                }
                i += 2;
                continue;
            }
            _ => {}
        }

        if i + 1 >= args.len() {
            return Err(CliError::Config(format!("flag '{arg}' requires a value")));
        }
        let val = &args[i + 1];
        match arg.as_str() {
            "--mode" | "-m" => config.mode = RunMode::from_str(val)?,
            "--schema-dir" | "-s" => config.schema_dir = PathBuf::from(val),
            "--database-url" | "-d" => config.database_url = Some(val.clone()),
            "--max-connections" | "--max-db-connections" => {
                config.max_db_connections = val
                    .parse::<u32>()
                    .map_err(|e| CliError::Config(format!("invalid {arg} '{val}': {e}")))?;
            }
            "--policy" | "--safety-policy" => config.safety_policy = parse_safety_policy(val)?,
            "--format" | "-f" => config.output_format = OutputFormat::from_str(val)?,
            "--output" | "--output-path" | "-o" => config.output_path = Some(PathBuf::from(val)),
            unknown => {
                return Err(CliError::Config(format!(
                    "unknown CLI argument '{unknown}'"
                )));
            }
        }
        i += 2;
    }
    Ok(())
}

fn parse_safety_policy(s: &str) -> Result<SafetyPolicy, CliError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "additive" | "additiveonly" | "additive-only" | "additive_only" => {
            Ok(SafetyPolicy::AdditiveOnly)
        }
        "destructive" | "allowdestructive" | "allow-destructive" | "allow_destructive" => {
            Ok(SafetyPolicy::AllowDestructive)
        }
        other => Err(CliError::Config(format!(
            "unknown safety policy '{other}', expected 'additive' or 'destructive'"
        ))),
    }
}

fn parse_bool(s: &str) -> Result<bool, CliError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(CliError::Config(format!(
            "invalid boolean value '{other}', expected 'true' or 'false'"
        ))),
    }
}

fn validate_database_url(url: &str) -> Result<(), CliError> {
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err(CliError::Config(format!(
            "database_url must start with postgres:// or postgresql://, got '{url}'"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert_eq!(config.mode, RunMode::Validate);
        assert_eq!(config.max_db_connections, 5);
        assert_eq!(config.safety_policy, SafetyPolicy::AdditiveOnly);
        assert!(!config.offline);
        assert_eq!(config.output_format, OutputFormat::Text);
    }

    #[test]
    fn test_parse_cli_args_mode_and_offline() {
        let args = ["--mode", "dry-run", "--offline"];
        let config = CliConfig::parse_from_iter(args).unwrap();
        assert_eq!(config.mode, RunMode::DryRun);
        assert!(config.offline);
    }

    #[test]
    fn test_parse_cli_positional_mode() {
        let args = ["migrate", "--database-url", "postgres://localhost/test"];
        let config = CliConfig::parse_from_iter(args).unwrap();
        assert_eq!(config.mode, RunMode::Migrate);
        assert_eq!(
            config.database_url,
            Some("postgres://localhost/test".to_string())
        );
    }

    #[test]
    fn test_parse_env_overrides() {
        let config = CliConfig::parse_from_args_and_env(Vec::<String>::new(), |k| match k {
            "LUMINAIR_CLI_MODE" => Some("dry-run".to_string()),
            "LUMINAIR_OFFLINE" => Some("true".to_string()),
            "LUMINAIR_OUTPUT_FORMAT" => Some("json".to_string()),
            _ => None,
        })
        .unwrap();

        assert_eq!(config.mode, RunMode::DryRun);
        assert!(config.offline);
        assert_eq!(config.output_format, OutputFormat::Json);
    }

    #[test]
    fn test_validate_rejects_zero_connections() {
        let config = CliConfig {
            max_db_connections: 0,
            ..CliConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_migrate_requires_database_url() {
        let config = CliConfig {
            schema_dir: PathBuf::from("."),
            mode: RunMode::Migrate,
            database_url: None,
            ..CliConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("database_url is required"));
    }

    #[test]
    fn test_serve_mode_gives_helpful_error() {
        let result = RunMode::from_str("serve");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("luminair-server"));
    }
}
