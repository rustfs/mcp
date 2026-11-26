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

//! Server access logging of buckets.

mod delete_bucket_logging;
mod get_bucket_logging;
mod put_bucket_logging;

pub use get_bucket_logging::BucketLoggingConfig;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Server access logging of a bucket: every request to it is recorded as
/// an object in a target bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoggingConfiguration {
    #[schemars(
        description = "Bucket the access logs are written to, e.g. 'my-logs'; it should exist and allow the logging service to write to it"
    )]
    pub target_bucket: String,
    /// Prefix of the keys the logs are stored under, e.g. `logs/`; absent
    /// for the root of the target bucket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional prefix of the keys the logs are stored under, e.g. 'logs/' (default: the root of the target bucket)"
    )]
    pub target_prefix: Option<String>,
}

impl LoggingConfiguration {
    /// The lines describing the configuration for tool output.
    pub fn summary(&self) -> String {
        let mut text = format!("**Log Target Bucket:** {}\n", self.target_bucket);
        if let Some(prefix) = &self.target_prefix {
            text.push_str(&format!("**Log Key Prefix:** {prefix}\n"));
        }
        text
    }
}

/// Reject a logging configuration S3 cannot apply: no target bucket, or an
/// empty prefix.
pub fn validate_logging_configuration(configuration: &LoggingConfiguration) -> Result<()> {
    if configuration.target_bucket.trim().is_empty() {
        anyhow::bail!("The log target bucket must not be empty");
    }
    if configuration.target_prefix.as_deref() == Some("") {
        anyhow::bail!(
            "The log key prefix must not be empty; omit it to store the logs in the root of the target bucket"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration() -> LoggingConfiguration {
        LoggingConfiguration {
            target_bucket: "my-logs".to_string(),
            target_prefix: Some("logs/".to_string()),
        }
    }

    #[test]
    fn test_logging_configuration_serialization() {
        let original = configuration();
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            json,
            r#"{"target_bucket":"my-logs","target_prefix":"logs/"}"#
        );
        let deserialized: LoggingConfiguration = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);

        // A prefix left out and a prefix sent as null mean the same.
        let deserialized: LoggingConfiguration =
            serde_json::from_str(r#"{"target_bucket": "my-logs", "target_prefix": null}"#).unwrap();
        assert_eq!(deserialized.target_prefix, None);
    }

    #[test]
    fn test_logging_configuration_requires_a_target_bucket() {
        assert!(
            serde_json::from_str::<LoggingConfiguration>(r#"{"target_prefix": "logs/"}"#).is_err()
        );
    }

    #[test]
    fn test_validate_logging_configuration() {
        assert!(validate_logging_configuration(&configuration()).is_ok());
        assert!(
            validate_logging_configuration(&LoggingConfiguration {
                target_bucket: "my-logs".to_string(),
                target_prefix: None,
            })
            .is_ok()
        );

        for target_bucket in ["", "   "] {
            let invalid = LoggingConfiguration {
                target_bucket: target_bucket.to_string(),
                target_prefix: None,
            };
            let error = validate_logging_configuration(&invalid).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("target bucket must not be empty"),
                "{error}"
            );
        }

        let invalid = LoggingConfiguration {
            target_bucket: "my-logs".to_string(),
            target_prefix: Some(String::new()),
        };
        let error = validate_logging_configuration(&invalid).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("log key prefix must not be empty"),
            "{error}"
        );
    }

    #[test]
    fn test_logging_configuration_summary() {
        assert_eq!(
            configuration().summary(),
            "**Log Target Bucket:** my-logs\n**Log Key Prefix:** logs/\n"
        );
        assert_eq!(
            LoggingConfiguration {
                target_bucket: "my-logs".to_string(),
                target_prefix: None,
            }
            .summary(),
            "**Log Target Bucket:** my-logs\n"
        );
    }
}
