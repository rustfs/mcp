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

//! Bucket default server-side encryption operations.

mod delete_bucket_encryption;
mod get_bucket_encryption;
mod put_bucket_encryption;

pub use get_bucket_encryption::BucketEncryptionConfig;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default server-side encryption applied to new objects written to a bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServerSideEncryptionByDefault {
    /// `AES256` (SSE-S3), `aws:kms` (SSE-KMS), or `aws:kms:dsse` (dual-layer server-side encryption with KMS).
    #[schemars(
        description = "Server-side encryption algorithm for new objects: 'AES256' (SSE-S3), 'aws:kms' (SSE-KMS), or 'aws:kms:dsse' (dual-layer server-side encryption with KMS); exact spelling, case-sensitive"
    )]
    pub sse_algorithm: String,
    /// KMS key ID, ARN, or alias; only used with the `aws:kms` and `aws:kms:dsse` algorithms.
    #[serde(default)]
    #[schemars(
        description = "Optional KMS key ID, ARN, or alias; only used with the 'aws:kms' and 'aws:kms:dsse' algorithms"
    )]
    pub kms_key_id: Option<String>,
}

impl ServerSideEncryptionByDefault {
    /// Copy the SDK values, keeping an algorithm the SDK does not model as
    /// its raw name.
    fn from_sdk(default: &aws_sdk_s3::types::ServerSideEncryptionByDefault) -> Self {
        Self {
            sse_algorithm: default.sse_algorithm().as_str().to_string(),
            kms_key_id: default.kms_master_key_id().map(|id| id.to_string()),
        }
    }

    /// Build the SDK value; fails when the SDK does not know the algorithm.
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::ServerSideEncryptionByDefault> {
        let algorithm = aws_sdk_s3::types::ServerSideEncryption::try_parse(&self.sse_algorithm)
            .with_context(|| format!("Unknown SSE algorithm '{}'", self.sse_algorithm))?;

        aws_sdk_s3::types::ServerSideEncryptionByDefault::builder()
            .sse_algorithm(algorithm)
            .set_kms_master_key_id(self.kms_key_id.clone())
            .build()
            .context("Failed to build the default server-side encryption")
    }
}

/// One server-side encryption rule: the default encryption plus the optional Bucket Key setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServerSideEncryptionRule {
    /// The default encryption applied to new objects written to the bucket.
    #[schemars(
        description = "Default server-side encryption applied to new objects written to the bucket"
    )]
    pub apply_server_side_encryption_by_default: ServerSideEncryptionByDefault,
    /// Bucket Keys reduce KMS request costs; only valid with the `aws:kms` and `aws:kms:dsse` algorithms.
    #[serde(default)]
    #[schemars(
        description = "Optional Bucket Key setting, which reduces KMS request costs; only valid with the 'aws:kms' and 'aws:kms:dsse' algorithms"
    )]
    pub bucket_key_enabled: Option<bool>,
}

impl ServerSideEncryptionRule {
    /// Copy the SDK rule, dropping members this crate does not model.
    ///
    /// Returns `None` for a rule without a default encryption, which has no
    /// counterpart here.
    fn from_sdk(rule: &aws_sdk_s3::types::ServerSideEncryptionRule) -> Option<Self> {
        Some(Self {
            apply_server_side_encryption_by_default: ServerSideEncryptionByDefault::from_sdk(
                rule.apply_server_side_encryption_by_default()?,
            ),
            bucket_key_enabled: rule.bucket_key_enabled(),
        })
    }

    /// Build the SDK value; fails when the SDK does not know the algorithm.
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::ServerSideEncryptionRule> {
        Ok(aws_sdk_s3::types::ServerSideEncryptionRule::builder()
            .apply_server_side_encryption_by_default(
                self.apply_server_side_encryption_by_default.to_sdk()?,
            )
            .set_bucket_key_enabled(self.bucket_key_enabled)
            .build())
    }

    /// Display name of the Bucket Key setting for tool output.
    pub fn bucket_key_label(&self) -> &'static str {
        match self.bucket_key_enabled {
            Some(true) => "enabled",
            Some(false) => "disabled",
            None => "not set",
        }
    }
}

/// Reject a rule S3 cannot apply: an algorithm other than the three it
/// accepts for a bucket default, or a Bucket Key combined with an algorithm
/// it does not apply to.
pub fn validate_encryption_rule(rule: &ServerSideEncryptionRule) -> Result<()> {
    let default = &rule.apply_server_side_encryption_by_default;

    // S3 compares the name exactly, so 'aes256' is not 'AES256'.
    if !matches!(
        default.sse_algorithm.as_str(),
        "AES256" | "aws:kms" | "aws:kms:dsse"
    ) {
        anyhow::bail!(
            "Unsupported SSE algorithm '{}': use exactly 'AES256', 'aws:kms', or 'aws:kms:dsse' (case-sensitive)",
            default.sse_algorithm
        );
    }

    if default.sse_algorithm == "AES256" && rule.bucket_key_enabled == Some(true) {
        anyhow::bail!(
            "bucket_key_enabled must not be true with the 'AES256' algorithm: Bucket Keys only apply to 'aws:kms' and 'aws:kms:dsse'"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encryption_rule(
        sse_algorithm: &str,
        kms_key_id: Option<&str>,
        bucket_key_enabled: Option<bool>,
    ) -> ServerSideEncryptionRule {
        ServerSideEncryptionRule {
            apply_server_side_encryption_by_default: ServerSideEncryptionByDefault {
                sse_algorithm: sse_algorithm.to_string(),
                kms_key_id: kms_key_id.map(|id| id.to_string()),
            },
            bucket_key_enabled,
        }
    }

    #[test]
    fn test_validate_encryption_rule_accepts_supported_rules() {
        for sse_algorithm in ["AES256", "aws:kms", "aws:kms:dsse"] {
            assert!(
                validate_encryption_rule(&encryption_rule(sse_algorithm, None, None)).is_ok(),
                "{sse_algorithm} without a Bucket Key"
            );
            assert!(
                validate_encryption_rule(&encryption_rule(sse_algorithm, None, Some(false)))
                    .is_ok(),
                "{sse_algorithm} with the Bucket Key disabled"
            );
        }

        let kms_key = "arn:aws:kms:us-east-1:111122223333:key/abc";
        assert!(
            validate_encryption_rule(&encryption_rule("aws:kms", Some(kms_key), Some(true)))
                .is_ok()
        );
        assert!(
            validate_encryption_rule(&encryption_rule(
                "aws:kms:dsse",
                Some("alias/my-key"),
                Some(true)
            ))
            .is_ok()
        );
    }

    #[test]
    fn test_validate_encryption_rule_rejects_unsupported_algorithms() {
        for sse_algorithm in [
            "aes256",
            "AES-256",
            "aws:KMS",
            "aws:kms:dsse ",
            "",
            "aws:backup",
        ] {
            let error = validate_encryption_rule(&encryption_rule(sse_algorithm, None, None))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(&format!("'{sse_algorithm}'")),
                "the message of {error:?} should name the algorithm {sse_algorithm:?}"
            );
            assert!(
                error.contains("'AES256', 'aws:kms', or 'aws:kms:dsse'"),
                "the message of {error:?} should list the accepted algorithms"
            );
        }
    }

    #[test]
    fn test_validate_encryption_rule_rejects_bucket_key_with_aes256() {
        let error = validate_encryption_rule(&encryption_rule("AES256", None, Some(true)))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("bucket_key_enabled") && error.contains("'AES256'"),
            "unexpected message: {error:?}"
        );

        // A disabled or unset Bucket Key is fine next to AES256.
        assert!(validate_encryption_rule(&encryption_rule("AES256", None, Some(false))).is_ok());
        assert!(validate_encryption_rule(&encryption_rule("AES256", None, None)).is_ok());
    }

    #[test]
    fn test_encryption_rule_serialization() {
        let original = encryption_rule(
            "aws:kms",
            Some("arn:aws:kms:us-east-1:111122223333:key/abc"),
            Some(true),
        );

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ServerSideEncryptionRule = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized, original);
        assert_eq!(
            deserialized
                .apply_server_side_encryption_by_default
                .sse_algorithm,
            "aws:kms"
        );
        assert_eq!(
            deserialized
                .apply_server_side_encryption_by_default
                .kms_key_id,
            Some("arn:aws:kms:us-east-1:111122223333:key/abc".to_string())
        );
        assert_eq!(deserialized.bucket_key_enabled, Some(true));
    }

    #[test]
    fn test_encryption_rule_optional_members_default_to_none() {
        let minimal: ServerSideEncryptionRule = serde_json::from_str(
            r#"{"apply_server_side_encryption_by_default": {"sse_algorithm": "AES256"}}"#,
        )
        .unwrap();

        assert_eq!(minimal, encryption_rule("AES256", None, None));
    }

    #[test]
    fn test_encryption_rule_sdk_round_trip() {
        for sse_algorithm in ["AES256", "aws:kms", "aws:kms:dsse"] {
            let original = encryption_rule(sse_algorithm, Some("alias/my-key"), Some(true));
            assert_eq!(
                ServerSideEncryptionRule::from_sdk(&original.to_sdk().unwrap()),
                Some(original.clone()),
                "{sse_algorithm} should survive the round trip"
            );
        }

        // Options that are unset must stay unset instead of becoming defaults.
        let aes_only = encryption_rule("AES256", None, None);
        let sdk = aes_only.to_sdk().unwrap();
        assert_eq!(sdk.bucket_key_enabled(), None);
        let sdk_default = sdk.apply_server_side_encryption_by_default().unwrap();
        assert_eq!(sdk_default.sse_algorithm().as_str(), "AES256");
        assert_eq!(sdk_default.kms_master_key_id(), None);
        assert_eq!(ServerSideEncryptionRule::from_sdk(&sdk), Some(aes_only));
    }

    #[test]
    fn test_encryption_types_from_sdk_drops_unmodelled_values() {
        // An algorithm the SDK does not model arrives as its `Unknown`
        // variant; keep the raw name instead of failing.
        let future = ServerSideEncryptionByDefault::from_sdk(
            &aws_sdk_s3::types::ServerSideEncryptionByDefault::builder()
                .sse_algorithm(aws_sdk_s3::types::ServerSideEncryption::from(
                    "future-algorithm",
                ))
                .kms_master_key_id("alias/my-key")
                .build()
                .unwrap(),
        );
        assert_eq!(future.sse_algorithm, "future-algorithm");
        assert_eq!(future.kms_key_id.as_deref(), Some("alias/my-key"));

        // A rule carrying members this crate does not model keeps the
        // modelled ones.
        let blocked = aws_sdk_s3::types::BlockedEncryptionTypes::builder()
            .encryption_type(aws_sdk_s3::types::EncryptionType::SseC)
            .build();
        let rule = ServerSideEncryptionRule::from_sdk(
            &aws_sdk_s3::types::ServerSideEncryptionRule::builder()
                .apply_server_side_encryption_by_default(
                    aws_sdk_s3::types::ServerSideEncryptionByDefault::builder()
                        .sse_algorithm(aws_sdk_s3::types::ServerSideEncryption::Aes256)
                        .build()
                        .unwrap(),
                )
                .bucket_key_enabled(false)
                .blocked_encryption_types(blocked)
                .build(),
        );
        assert_eq!(rule, Some(encryption_rule("AES256", None, Some(false))));

        // A rule without a default encryption block cannot be represented.
        let without_default = aws_sdk_s3::types::ServerSideEncryptionRule::builder()
            .bucket_key_enabled(true)
            .build();
        assert_eq!(ServerSideEncryptionRule::from_sdk(&without_default), None);
    }

    #[test]
    fn test_encryption_rule_to_sdk_rejects_unknown_algorithm() {
        let error = encryption_rule("aes256", None, None).to_sdk().unwrap_err();
        assert_eq!(error.to_string(), "Unknown SSE algorithm 'aes256'");
    }

    #[test]
    fn test_bucket_key_label() {
        assert_eq!(
            encryption_rule("aws:kms", None, Some(true)).bucket_key_label(),
            "enabled"
        );
        assert_eq!(
            encryption_rule("aws:kms", None, Some(false)).bucket_key_label(),
            "disabled"
        );
        assert_eq!(
            encryption_rule("AES256", None, None).bucket_key_label(),
            "not set"
        );
    }
}
