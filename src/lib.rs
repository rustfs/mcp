// Copyright 2024 RustFS Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(docsrs, feature(doc_cfg))]
//! RustFS MCP server library.
//!
//! This crate exposes a Model Context Protocol (MCP) server that provides
//! S3-compatible storage operations, plus reusable configuration and client
//! primitives for embedding into other Rust applications.
//!
//! # Optional features
//!
//! - `io-uring`: Enables Tokio's `io-uring` integration. This requires building
//!   with `RUSTFLAGS="--cfg tokio_unstable"`.

#[cfg(all(feature = "io-uring", not(tokio_unstable)))]
compile_error!(
    "feature `io-uring` requires cfg `tokio_unstable` (set RUSTFLAGS/RUSTDOCFLAGS to \"--cfg tokio_unstable\")"
);

/// Command-line and environment configuration parsing/validation.
pub mod config;
/// S3 client wrapper and operation/result data types.
pub mod s3_client;
/// MCP server and tool handlers.
pub mod server;

/// Runtime configuration for the server.
pub use config::Config;
/// Bucket metadata and async S3 client wrapper.
pub use s3_client::{BucketInfo, S3Client};
/// MCP service implementation.
pub use server::RustfsMcpServer;

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use tokio::io::{stdin, stdout};
use tracing::info;

/// Start the MCP server with an explicit [`Config`].
pub async fn run_server_with_config(config: Config) -> Result<()> {
    info!("Starting RustFS MCP Server with provided configuration");

    config
        .validate()
        .context("Configuration validation failed")?;

    let server = RustfsMcpServer::new(config).await?;

    info!("Running MCP server with stdio transport");

    // Run the server with stdio
    server
        .serve((stdin(), stdout()))
        .await
        .context("Failed to serve MCP server")?
        .waiting()
        .await
        .context("Error while waiting for server shutdown")?;

    Ok(())
}

/// Start the MCP server with [`Config::default`].
pub async fn run_server() -> Result<()> {
    info!("Starting RustFS MCP Server with default configuration");

    let config = Config::default();
    run_server_with_config(config).await
}

/// Validate required AWS credentials in environment variables.
///
/// This function is kept for backward compatibility.
pub fn validate_environment() -> Result<()> {
    use std::env;

    if env::var("AWS_ACCESS_KEY_ID").is_err() {
        anyhow::bail!("AWS_ACCESS_KEY_ID environment variable is required");
    }

    if env::var("AWS_SECRET_ACCESS_KEY").is_err() {
        anyhow::bail!("AWS_SECRET_ACCESS_KEY environment variable is required");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = Config {
            access_key_id: Some("test_key".to_string()),
            secret_access_key: Some("test_secret".to_string()),
            ..Config::default()
        };

        assert!(config.validate().is_ok());
        assert_eq!(config.access_key_id(), "test_key");
        assert_eq!(config.secret_access_key(), "test_secret");
    }

    #[tokio::test]
    async fn test_run_server_with_invalid_config() {
        let config = Config::default();

        let result = run_server_with_config(config).await;
        assert!(result.is_err());
    }
}
