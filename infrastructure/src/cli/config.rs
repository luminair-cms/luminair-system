//! CLI configuration types, deserialization, environment variable ingestion, and validation.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::error::CliError;
use crate::persistence::migration::SafetyPolicy;

// ---------------------------------------------------------------------------
// RunMode & OutputFormat Enums
// ---------------------------------------------------------------------------

/// Execution mode for the CLI command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RunMode {
    /// Executes static and dynamic migrations against the target database.
    #[serde(alias = "migration", alias = "migrate")]
    Migrate,
    /// Connects to DB (or runs offline) and plans migrations without executing them.
    #[serde(alias = "dry-run", alias = "dry_run", alias = "dryrun")]
    DryRun,
    /// Offline validation of configuration files and schema definitions.
    #[default]
    #[serde(alias = "test", alias = "test-config", alias = "validate")]
    Test,
    /// Long-running HTTP REST API service daemon with graceful shutdown.
    #[serde(alias = "service", alias = "serve", alias = "server")]
    Service,
}

/// Output rendering format for CLI outcome reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    /// Human-readable structured text report.
    #[default]
    #[serde(alias = "text")]
    Text,
    /// Machine-readable pretty JSON payload.
    #[serde(alias = "json")]
    Json,
}

/// Identity and role bindings associated with a static service bearer token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTokenConfig {
    /// Bearer token secret.
    pub token: String,
    /// Machine service identifier.
    pub service_id: String,
    /// Role identifiers assigned to the service.
    pub roles: Vec<String>,
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
    /// PostgreSQL connection connection string (optional in test/dry-run modes).
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
    /// HTTP host to bind for service mode.
    pub http_host: String,
    /// HTTP port to bind for service mode.
    pub http_port: u16,
    /// Whether to automatically apply static and dynamic migrations on service startup.
    pub auto_migrate: bool,
    /// Optional OIDC JWT HMAC-SHA256 secret.
    pub oidc_jwt_secret: Option<String>,
    /// Optional OIDC JWT RS256 public key PEM.
    pub oidc_jwt_public_key_pem: Option<String>,
    /// Optional expected OIDC issuer claim.
    pub oidc_issuer: Option<String>,
    /// Optional expected OIDC audience claim.
    pub oidc_audience: Option<String>,
    /// Pre-configured service bearer tokens.
    pub service_tokens: Vec<ServiceTokenConfig>,
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
    pub http_host: Option<String>,
    pub http_port: Option<u16>,
    pub auto_migrate: Option<bool>,
    pub oidc_jwt_secret: Option<String>,
    pub oidc_jwt_public_key_pem: Option<String>,
    pub oidc_issuer: Option<String>,
    pub oidc_audience: Option<String>,
    pub service_tokens: Option<Vec<ServiceTokenConfig>>,
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
            Self::Test => "test",
            Self::Service => "service",
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
        let arg_list: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        let mut config = match extract_config_path(&arg_list, &env_lookup) {
            Some(path) => Self::from_json_file(&path)?,
            None => Self::default(),
        };

        apply_env_overrides(&mut config, &env_lookup)?;
        apply_cli_arg_overrides(&mut config, &arg_list)?;
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

        if (self.mode == RunMode::Migrate || self.mode == RunMode::Service)
            && self.database_url.is_none()
        {
            return Err(CliError::Config(format!(
                "database_url is required for '{}' mode",
                self.mode
            )));
        }

        if self.mode == RunMode::Service && self.http_port == 0 {
            return Err(CliError::Config(
                "http_port must be greater than zero".to_string(),
            ));
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
            "migrate" | "migration" => Ok(Self::Migrate),
            "dry-run" | "dry_run" | "dryrun" => Ok(Self::DryRun),
            "test" | "test-config" | "validate" => Ok(Self::Test),
            "service" | "serve" | "server" => Ok(Self::Service),
            other => Err(CliError::Config(format!(
                "unknown run mode '{other}', expected 'migrate', 'dry-run', 'test', or 'service'"
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
            mode: RunMode::Test,
            schema_dir: PathBuf::from("./config/schemas"),
            database_url: None,
            max_db_connections: 5,
            safety_policy: SafetyPolicy::AdditiveOnly,
            offline: false,
            output_format: OutputFormat::Text,
            output_path: None,
            http_host: "0.0.0.0".to_string(),
            http_port: 8080,
            auto_migrate: false,
            oidc_jwt_secret: None,
            oidc_jwt_public_key_pem: None,
            oidc_issuer: None,
            oidc_audience: None,
            service_tokens: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_config_path<F>(args: &[String], env_lookup: &F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if (arg == "--config" || arg == "-c") && i + 1 < args.len() {
            return Some(PathBuf::from(&args[i + 1]));
        }
        if let Some(val) = arg.strip_prefix("--config=").or_else(|| arg.strip_prefix("-c=")) {
            return Some(PathBuf::from(val));
        }
        i += 1;
    }

    env_lookup("LUMINAIR_CONFIG_FILE").map(PathBuf::from)
}

fn apply_file_dto(config: &mut CliConfig, dto: CliConfigFileDto) {
    if let Some(mode) = dto.mode {
        config.mode = mode;
    }
    if let Some(schema_dir) = dto.schema_dir {
        config.schema_dir = schema_dir;
    }
    if dto.database_url.is_some() {
        config.database_url = dto.database_url;
    }
    if let Some(max_conns) = dto.max_db_connections {
        config.max_db_connections = max_conns;
    }
    if let Some(policy) = dto.safety_policy {
        config.safety_policy = policy;
    }
    if let Some(offline) = dto.offline {
        config.offline = offline;
    }
    if let Some(format) = dto.output_format {
        config.output_format = format;
    }
    if dto.output_path.is_some() {
        config.output_path = dto.output_path;
    }
    if let Some(host) = dto.http_host {
        config.http_host = host;
    }
    if let Some(port) = dto.http_port {
        config.http_port = port;
    }
    if let Some(auto_mig) = dto.auto_migrate {
        config.auto_migrate = auto_mig;
    }
    if dto.oidc_jwt_secret.is_some() {
        config.oidc_jwt_secret = dto.oidc_jwt_secret;
    }
    if dto.oidc_jwt_public_key_pem.is_some() {
        config.oidc_jwt_public_key_pem = dto.oidc_jwt_public_key_pem;
    }
    if dto.oidc_issuer.is_some() {
        config.oidc_issuer = dto.oidc_issuer;
    }
    if dto.oidc_audience.is_some() {
        config.oidc_audience = dto.oidc_audience;
    }
    if let Some(tokens) = dto.service_tokens {
        config.service_tokens = tokens;
    }
}

fn apply_env_overrides<F>(config: &mut CliConfig, env_lookup: &F) -> Result<(), CliError>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(val) = env_lookup("LUMINAIR_MODE") {
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
        config.offline = parse_bool_val(&val)?;
    }
    if let Some(val) = env_lookup("LUMINAIR_OUTPUT_FORMAT") {
        config.output_format = OutputFormat::from_str(&val)?;
    }
    if let Some(val) = env_lookup("LUMINAIR_OUTPUT_PATH") {
        config.output_path = Some(PathBuf::from(val));
    }
    if let Some(val) = env_lookup("LUMINAIR_HTTP_HOST") {
        config.http_host = val;
    }
    if let Some(val) = env_lookup("LUMINAIR_HTTP_PORT") {
        config.http_port = val.parse::<u16>().map_err(|e| {
            CliError::Config(format!("invalid LUMINAIR_HTTP_PORT '{val}': {e}"))
        })?;
    }
    if let Some(val) = env_lookup("LUMINAIR_AUTO_MIGRATE") {
        config.auto_migrate = parse_bool_val(&val)?;
    }
    if let Some(val) = env_lookup("LUMINAIR_OIDC_JWT_SECRET") {
        config.oidc_jwt_secret = Some(val);
    }
    if let Some(val) = env_lookup("LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM") {
        config.oidc_jwt_public_key_pem = Some(val);
    }
    if let Some(val) = env_lookup("LUMINAIR_OIDC_ISSUER") {
        config.oidc_issuer = Some(val);
    }
    if let Some(val) = env_lookup("LUMINAIR_OIDC_AUDIENCE") {
        config.oidc_audience = Some(val);
    }
    if let Some(val) = env_lookup("LUMINAIR_SERVICE_TOKENS") {
        config.service_tokens = parse_service_tokens_env(&val)?;
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

        parse_single_arg(config, args, &mut i)?;
        i += 1;
    }
    Ok(())
}

fn parse_single_arg(
    config: &mut CliConfig,
    args: &[String],
    index: &mut usize,
) -> Result<(), CliError> {
    let arg = &args[*index];
    if arg == "--offline" {
        config.offline = true;
        return Ok(());
    }
    if arg == "--auto-migrate" {
        config.auto_migrate = true;
        return Ok(());
    }
    if is_valued_flag(arg) {
        let val = next_arg_value(args, index, arg)?;
        return apply_flag_value(config, arg, &val);
    }
    parse_inline_arg(config, arg)
}

fn is_valued_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--mode"
            | "-m"
            | "--config"
            | "-c"
            | "--schema-dir"
            | "-s"
            | "--database-url"
            | "-d"
            | "--max-connections"
            | "--policy"
            | "--format"
            | "-f"
            | "--output"
            | "-o"
            | "--host"
            | "--port"
            | "-p"
            | "--oidc-jwt-secret"
            | "--oidc-jwt-public-key"
            | "--oidc-issuer"
            | "--oidc-audience"
    )
}

fn apply_flag_value(config: &mut CliConfig, flag: &str, val: &str) -> Result<(), CliError> {
    match flag {
        "--mode" | "-m" => config.mode = RunMode::from_str(val)?,
        "--config" | "-c" => {}
        "--schema-dir" | "-s" => config.schema_dir = PathBuf::from(val),
        "--database-url" | "-d" => config.database_url = Some(val.to_string()),
        "--max-connections" => {
            config.max_db_connections = val.parse::<u32>().map_err(|e| {
                CliError::Config(format!("invalid --max-connections '{val}': {e}"))
            })?;
        }
        "--policy" => config.safety_policy = parse_safety_policy(val)?,
        "--format" | "-f" => config.output_format = OutputFormat::from_str(val)?,
        "--output" | "-o" => config.output_path = Some(PathBuf::from(val)),
        "--host" => config.http_host = val.to_string(),
        "--port" | "-p" => {
            config.http_port = val.parse::<u16>().map_err(|e| {
                CliError::Config(format!("invalid --port '{val}': {e}"))
            })?;
        }
        "--oidc-jwt-secret" => config.oidc_jwt_secret = Some(val.to_string()),
        "--oidc-jwt-public-key" => config.oidc_jwt_public_key_pem = Some(val.to_string()),
        "--oidc-issuer" => config.oidc_issuer = Some(val.to_string()),
        "--oidc-audience" => config.oidc_audience = Some(val.to_string()),
        _ => return Err(CliError::Config(format!("unknown flag '{flag}'"))),
    }
    Ok(())
}

fn parse_inline_arg(config: &mut CliConfig, arg: &str) -> Result<(), CliError> {
    if let Some(val) = arg.strip_prefix("--mode=").or_else(|| arg.strip_prefix("-m=")) {
        config.mode = RunMode::from_str(val)?;
    } else if let Some(val) = arg.strip_prefix("--schema-dir=").or_else(|| arg.strip_prefix("-s=")) {
        config.schema_dir = PathBuf::from(val);
    } else if let Some(val) = arg.strip_prefix("--database-url=").or_else(|| arg.strip_prefix("-d=")) {
        config.database_url = Some(val.to_string());
    } else if let Some(val) = arg.strip_prefix("--max-connections=") {
        config.max_db_connections = val.parse::<u32>().map_err(|e| {
            CliError::Config(format!("invalid --max-connections '{val}': {e}"))
        })?;
    } else if let Some(val) = arg.strip_prefix("--policy=") {
        config.safety_policy = parse_safety_policy(val)?;
    } else if let Some(val) = arg.strip_prefix("--format=").or_else(|| arg.strip_prefix("-f=")) {
        config.output_format = OutputFormat::from_str(val)?;
    } else if let Some(val) = arg.strip_prefix("--output=").or_else(|| arg.strip_prefix("-o=")) {
        config.output_path = Some(PathBuf::from(val));
    } else if let Some(val) = arg.strip_prefix("--host=") {
        config.http_host = val.to_string();
    } else if let Some(val) = arg.strip_prefix("--port=").or_else(|| arg.strip_prefix("-p=")) {
        config.http_port = val.parse::<u16>().map_err(|e| {
            CliError::Config(format!("invalid --port '{val}': {e}"))
        })?;
    } else if let Some(val) = arg.strip_prefix("--oidc-jwt-secret=") {
        config.oidc_jwt_secret = Some(val.to_string());
    } else if let Some(val) = arg.strip_prefix("--oidc-jwt-public-key=") {
        config.oidc_jwt_public_key_pem = Some(val.to_string());
    } else if let Some(val) = arg.strip_prefix("--oidc-issuer=") {
        config.oidc_issuer = Some(val.to_string());
    } else if let Some(val) = arg.strip_prefix("--oidc-audience=") {
        config.oidc_audience = Some(val.to_string());
    } else if arg.strip_prefix("--config=").or_else(|| arg.strip_prefix("-c=")).is_some() {
        // Handled during pre-scan
    } else {
        return Err(CliError::Config(format!("unknown CLI argument '{arg}'")));
    }
    Ok(())
}

fn parse_service_tokens_env(val: &str) -> Result<Vec<ServiceTokenConfig>, CliError> {
    if val.trim().starts_with('[') {
        return serde_json::from_str(val).map_err(CliError::Serialization);
    }
    let mut entries = Vec::new();
    for item in val.split(';') {
        let parts: Vec<&str> = item.split(':').collect();
        if parts.len() >= 3 {
            let token = parts[0].trim().to_string();
            let service_id = parts[1].trim().to_string();
            let roles = parts[2]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            entries.push(ServiceTokenConfig {
                token,
                service_id,
                roles,
            });
        }
    }
    Ok(entries)
}

fn next_arg_value(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String, CliError> {
    if *index + 1 >= args.len() {
        return Err(CliError::Config(format!("flag '{flag}' requires a value")));
    }
    *index += 1;
    Ok(args[*index].clone())
}

fn parse_safety_policy(val: &str) -> Result<SafetyPolicy, CliError> {
    match val.trim().to_ascii_lowercase().as_str() {
        "additive" | "additive-only" | "additive_only" => Ok(SafetyPolicy::AdditiveOnly),
        "destructive" | "allow-destructive" | "allow_destructive" => {
            Ok(SafetyPolicy::AllowDestructive)
        }
        other => Err(CliError::Config(format!(
            "unknown safety policy '{other}', expected 'additive' or 'destructive'"
        ))),
    }
}

fn parse_bool_val(val: &str) -> Result<bool, CliError> {
    match val.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(CliError::Config(format!(
            "invalid boolean value '{other}', expected 'true' or 'false'"
        ))),
    }
}

fn validate_database_url(raw: &str) -> Result<(), CliError> {
    let (scheme, rest) = raw.split_once("://").ok_or_else(|| {
        CliError::Config(format!(
            "invalid database URL '{raw}': missing '://' scheme separator"
        ))
    })?;

    if scheme != "postgres" && scheme != "postgresql" {
        return Err(CliError::Config(format!(
            "invalid database URL '{raw}': scheme must be 'postgres' or 'postgresql', found '{scheme}'"
        )));
    }

    if rest.trim().is_empty() {
        return Err(CliError::Config(format!(
            "invalid database URL '{raw}': host authority is empty"
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
    fn test_run_mode_from_str() {
        assert_eq!(RunMode::from_str("migrate").unwrap(), RunMode::Migrate);
        assert_eq!(RunMode::from_str("migration").unwrap(), RunMode::Migrate);
        assert_eq!(RunMode::from_str("dry-run").unwrap(), RunMode::DryRun);
        assert_eq!(RunMode::from_str("dry_run").unwrap(), RunMode::DryRun);
        assert_eq!(RunMode::from_str("dryrun").unwrap(), RunMode::DryRun);
        assert_eq!(RunMode::from_str("test").unwrap(), RunMode::Test);
        assert_eq!(RunMode::from_str("test-config").unwrap(), RunMode::Test);
        assert_eq!(RunMode::from_str("validate").unwrap(), RunMode::Test);
        assert!(RunMode::from_str("invalid").is_err());
    }

    #[test]
    fn test_run_mode_display() {
        assert_eq!(RunMode::Migrate.to_string(), "migrate");
        assert_eq!(RunMode::DryRun.to_string(), "dry-run");
        assert_eq!(RunMode::Test.to_string(), "test");
    }

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(OutputFormat::from_str("text").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::from_str("json").unwrap(), OutputFormat::Json);
        assert!(OutputFormat::from_str("xml").is_err());
    }

    #[test]
    fn test_output_format_display() {
        assert_eq!(OutputFormat::Text.to_string(), "text");
        assert_eq!(OutputFormat::Json.to_string(), "json");
    }

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert_eq!(config.mode, RunMode::Test);
        assert_eq!(config.schema_dir, PathBuf::from("./config/schemas"));
        assert_eq!(config.database_url, None);
        assert_eq!(config.max_db_connections, 5);
        assert_eq!(config.safety_policy, SafetyPolicy::AdditiveOnly);
        assert!(!config.offline);
        assert_eq!(config.output_format, OutputFormat::Text);
        assert_eq!(config.output_path, None);
    }

    #[test]
    fn test_parse_from_iter_positional_mode() {
        let config = CliConfig::parse_from_iter(["migrate", "--offline"]).unwrap();
        assert_eq!(config.mode, RunMode::Migrate);
        assert!(config.offline);
    }

    #[test]
    fn test_parse_from_iter_flags() {
        let args = [
            "--mode",
            "dry-run",
            "--schema-dir",
            "/tmp/schemas",
            "--database-url",
            "postgres://localhost/test",
            "--max-connections",
            "10",
            "--policy",
            "destructive",
            "--format",
            "json",
            "--output",
            "/tmp/out.json",
        ];
        let config = CliConfig::parse_from_iter(args).unwrap();
        assert_eq!(config.mode, RunMode::DryRun);
        assert_eq!(config.schema_dir, PathBuf::from("/tmp/schemas"));
        assert_eq!(
            config.database_url,
            Some("postgres://localhost/test".to_string())
        );
        assert_eq!(config.max_db_connections, 10);
        assert_eq!(config.safety_policy, SafetyPolicy::AllowDestructive);
        assert_eq!(config.output_format, OutputFormat::Json);
        assert_eq!(config.output_path, Some(PathBuf::from("/tmp/out.json")));
    }

    #[test]
    fn test_parse_from_args_and_env() {
        let env_map = |k: &str| match k {
            "LUMINAIR_MODE" => Some("dry-run".to_string()),
            "DATABASE_URL" => Some("postgresql://user:pass@host/db".to_string()),
            "LUMINAIR_MAX_DB_CONNECTIONS" => Some("8".to_string()),
            "LUMINAIR_OFFLINE" => Some("true".to_string()),
            _ => None,
        };
        let config = CliConfig::parse_from_args_and_env(Vec::<String>::new(), env_map).unwrap();
        assert_eq!(config.mode, RunMode::DryRun);
        assert_eq!(
            config.database_url,
            Some("postgresql://user:pass@host/db".to_string())
        );
        assert_eq!(config.max_db_connections, 8);
        assert!(config.offline);
    }

    #[test]
    fn test_validate_connections_zero() {
        let config = CliConfig {
            max_db_connections: 0,
            ..CliConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_database_url_scheme() {
        let mut config = CliConfig {
            schema_dir: PathBuf::from("."),
            database_url: Some("mysql://localhost/test".to_string()),
            ..CliConfig::default()
        };
        assert!(config.validate().is_err());

        config.database_url = Some("postgres://localhost/test".to_string());
        assert!(config.validate().is_ok());

        config.database_url = Some("postgresql://localhost/test".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_from_json_file() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_luminair_config.json");
        let json_content = r#"{
            "mode": "migrate",
            "maxDbConnections": 15,
            "safetyPolicy": "allowDestructive",
            "outputFormat": "json"
        }"#;
        std::fs::write(&file_path, json_content).unwrap();

        let config = CliConfig::from_json_file(&file_path).unwrap();
        assert_eq!(config.mode, RunMode::Migrate);
        assert_eq!(config.max_db_connections, 15);
        assert_eq!(config.safety_policy, SafetyPolicy::AllowDestructive);
        assert_eq!(config.output_format, OutputFormat::Json);
        assert_eq!(config.schema_dir, PathBuf::from("./config/schemas"));

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_parse_from_iter_with_config_file() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_luminair_override.json");
        let json_content = r#"{
            "mode": "test",
            "maxDbConnections": 20
        }"#;
        std::fs::write(&file_path, json_content).unwrap();

        let args = [
            "--config",
            file_path.to_str().unwrap(),
            "--mode",
            "dry-run",
        ];
        let config = CliConfig::parse_from_iter(args).unwrap();
        // --mode flag overrides the mode in the config file
        assert_eq!(config.mode, RunMode::DryRun);
        // maxDbConnections comes from the config file
        assert_eq!(config.max_db_connections, 20);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_validate_migrate_requires_db_url() {
        let mut config = CliConfig {
            schema_dir: PathBuf::from("."),
            mode: RunMode::Migrate,
            database_url: None,
            ..CliConfig::default()
        };
        assert!(config.validate().is_err());

        config.database_url = Some("postgres://localhost/test".to_string());
        assert!(config.validate().is_ok());
    }
}
