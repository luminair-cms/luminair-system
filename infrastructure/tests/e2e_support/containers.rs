//! Container topology orchestration and lifecycle management for E2E testing.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use testcontainers::core::{ContainerRequest, IntoContainerPort, Mount, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use uuid::Uuid;

use super::client::E2eClient;

// ---------------------------------------------------------------------------
// Constants & Types
// ---------------------------------------------------------------------------

const POSTGRES_IMAGE: &str = "postgres";
const POSTGRES_TAG: &str = "18.6-alpine3.24";
const DEX_IMAGE: &str = "ghcr.io/dexidp/dex";
const DEX_TAG: &str = "v2.43.0";
const LUMINAIR_IMAGE: &str = "luminair-cli";
const LUMINAIR_TAG: &str = "local";

/// Complete isolated end-to-end test environment spanning Postgres, Dex, and Luminair.
pub struct E2eEnvironment {
    pub client: E2eClient,
    _postgres: ContainerAsync<GenericImage>,
    _dex: ContainerAsync<GenericImage>,
    _luminair: ContainerAsync<GenericImage>,
    _network_guard: DockerNetworkGuard,
}

/// RAII guard ensuring the isolated Docker network is pruned after container termination.
#[derive(Debug)]
struct DockerNetworkGuard {
    name: String,
}

// ---------------------------------------------------------------------------
// Inherent & Trait Implementations
// ---------------------------------------------------------------------------

impl E2eEnvironment {
    /// Launches the complete container topology on a private Docker bridge network.
    pub async fn start() -> Self {
        let workspace_root = find_workspace_root();
        ensure_luminair_image_built(&workspace_root);

        let network_guard = create_docker_network();
        let net = &network_guard.name;

        let (postgres, pg_alias) = start_postgres_container(net).await;
        let (dex, dex_alias, dex_url) = start_dex_container(net, &workspace_root).await;
        let (luminair, base_url) =
            start_luminair_container(net, &workspace_root, &pg_alias, &dex_alias).await;

        let client = E2eClient::new(base_url, dex_url, "local-dev-admin-token".to_string());
        Self {
            client,
            _postgres: postgres,
            _dex: dex,
            _luminair: luminair,
            _network_guard: network_guard,
        }
    }
}

impl Drop for DockerNetworkGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["network", "rm", "-f", &self.name])
            .output();
    }
}

// ---------------------------------------------------------------------------
// Private Helpers (Top-Down Call Order)
// ---------------------------------------------------------------------------

fn find_workspace_root() -> PathBuf {
    let current = std::env::current_dir().expect("obtain current dir");
    if current.join("Dockerfile").exists() {
        return current;
    }
    if let Some(parent) = current.parent()
        && parent.join("Dockerfile").exists()
    {
        return parent.to_path_buf();
    }
    panic!("Could not locate workspace root containing Dockerfile from {current:?}");
}

fn ensure_luminair_image_built(workspace_root: &Path) {
    if std::env::var("LUMINAIR_SKIP_DOCKER_BUILD").is_ok() {
        return;
    }
    let status = Command::new("docker")
        .args(["build", "-t", "luminair-cli:local", "."])
        .current_dir(workspace_root)
        .status()
        .expect("execute docker build");
    assert!(
        status.success(),
        "docker build failed with status: {status}"
    );
}

fn create_docker_network() -> DockerNetworkGuard {
    let name = format!("luminair-e2e-{}", Uuid::now_v7());
    let status = Command::new("docker")
        .args(["network", "create", &name])
        .status()
        .expect("execute docker network create");
    assert!(
        status.success(),
        "Failed to create docker network {name}"
    );
    DockerNetworkGuard { name }
}

async fn start_postgres_container(
    network_name: &str,
) -> (ContainerAsync<GenericImage>, String) {
    let pg_alias = format!("postgres-{}", Uuid::now_v7().simple());
    let ready_msg = "database system is ready to accept connections";

    let img = GenericImage::new(POSTGRES_IMAGE, POSTGRES_TAG)
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(ready_msg))
        .with_network(network_name)
        .with_container_name(&pg_alias)
        .with_env_var("POSTGRES_USER", "luminair")
        .with_env_var("POSTGRES_PASSWORD", "luminair")
        .with_env_var("POSTGRES_DB", "luminair");

    let container = img.start().await.expect("start postgres container");
    (container, pg_alias)
}

async fn start_dex_container(
    network_name: &str,
    workspace_root: &Path,
) -> (ContainerAsync<GenericImage>, String, String) {
    let dex_alias = format!("dex-{}", Uuid::now_v7().simple());
    let dex_config = normalize_mount_path(&workspace_root.join("config/dex.yaml"));

    let img = GenericImage::new(DEX_IMAGE, DEX_TAG)
        .with_exposed_port(5556.tcp())
        .with_mount(Mount::bind_mount(dex_config, "/etc/dex/config.yaml"))
        .with_cmd(["dex", "serve", "/etc/dex/config.yaml"])
        .with_network(network_name)
        .with_container_name(&dex_alias);

    let container = img.start().await.expect("start dex container");
    let host_port = container
        .get_host_port_ipv4(5556.tcp())
        .await
        .expect("query dex mapped port");

    let dex_host_url = format!("http://127.0.0.1:{host_port}/dex");
    let health_url = format!("{dex_host_url}/healthz");
    wait_for_http_ok(&health_url, Duration::from_secs(30)).await;

    (container, dex_alias, dex_host_url)
}

async fn start_luminair_container(
    network_name: &str,
    workspace_root: &Path,
    pg_alias: &str,
    dex_alias: &str,
) -> (ContainerAsync<GenericImage>, String) {
    let luminair_alias = format!("luminair-{}", Uuid::now_v7().simple());
    let img = build_luminair_image(
        workspace_root,
        network_name,
        pg_alias,
        dex_alias,
        &luminair_alias,
    );

    let container = img.start().await.expect("start luminair container");
    let host_port = container
        .get_host_port_ipv4(8080.tcp())
        .await
        .expect("query luminair mapped port");

    let host_url = format!("http://127.0.0.1:{host_port}");
    let ready_url = format!("{host_url}/ready");
    wait_for_luminair_ready(&container, &ready_url, Duration::from_secs(45)).await;

    (container, host_url)
}

async fn wait_for_http_ok(url: &str, timeout: Duration) {
    let client = reqwest::Client::new();
    let start = Instant::now();

    while start.elapsed() < timeout {
        if let Ok(resp) = client.get(url).send().await
            && resp.status().is_success()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("Endpoint {url} did not return 200 OK within {timeout:?}");
}

fn build_luminair_image(
    workspace_root: &Path,
    network_name: &str,
    pg_alias: &str,
    dex_alias: &str,
    luminair_alias: &str,
) -> ContainerRequest<GenericImage> {
    let config_dir = normalize_mount_path(&workspace_root.join("config"));
    let db_url = format!("postgres://luminair:luminair@{pg_alias}:5432/luminair");
    let jwks_url = format!("http://{dex_alias}:5556/dex/keys");

    GenericImage::new(LUMINAIR_IMAGE, LUMINAIR_TAG)
        .with_exposed_port(8080.tcp())
        .with_mount(Mount::bind_mount(config_dir, "/app/config"))
        .with_network(network_name)
        .with_container_name(luminair_alias)
        .with_env_var("LUMINAIR_DATABASE_URL", db_url)
        .with_env_var("LUMINAIR_HTTP_HOST", "0.0.0.0")
        .with_env_var("LUMINAIR_HTTP_PORT", "8080")
        .with_env_var("LUMINAIR_AUTO_MIGRATE", "true")
        .with_env_var("LUMINAIR_OIDC_ISSUER", "http://localhost:5556/dex")
        .with_env_var("LUMINAIR_OIDC_AUDIENCE", "luminair-admin-spa")
        .with_env_var("LUMINAIR_OIDC_JWKS_URL", jwks_url)
        .with_env_var(
            "LUMINAIR_SERVICE_TOKENS",
            "local-dev-admin-token:ci-agent:admin",
        )
        .with_cmd([
            "--config",
            "/app/config/cli-config.json",
            "--mode",
            "serve",
            "--auto-migrate",
        ])
}

async fn wait_for_luminair_ready(
    container: &ContainerAsync<GenericImage>,
    ready_url: &str,
    timeout: Duration,
) {
    let client = reqwest::Client::new();
    let start = Instant::now();

    while start.elapsed() < timeout {
        if let Ok(resp) = client.get(ready_url).send().await
            && resp.status().is_success()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let stdout =
        String::from_utf8_lossy(&container.stdout_to_vec().await.unwrap_or_default()).to_string();
    let stderr =
        String::from_utf8_lossy(&container.stderr_to_vec().await.unwrap_or_default()).to_string();
    panic!(
        "Luminair container did not become ready at {ready_url} within {timeout:?}.\nStdout:\n{stdout}\nStderr:\n{stderr}"
    );
}

fn normalize_mount_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let cleaned = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    cleaned.replace('\\', "/")
}
