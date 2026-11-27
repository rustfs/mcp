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

mod download_mode;
mod format_error;
mod lifecycle;
mod object_lock;
mod read_mode;
mod replication;
mod requests;
mod shell_quote;

// Re-export the request types so external paths (`rustfs_mcp::server::*`)
// stay stable across the split.
pub use requests::*;

use anyhow::Result;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerConfig},
    service::{NotificationContext, RequestContext},
    tool, tool_handler, tool_router,
};
use tracing::{debug, error, info};

use crate::config::Config;
use crate::s3_client::{
    CopyObjectOptions, ListObjectsOptions, PresignMethod, S3Client, UploadFileOptions,
};
use shell_quote::shell_single_quote;

#[derive(Debug, Clone)]
pub struct RustfsMcpServer {
    s3_client: S3Client,
    _config: Config,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl RustfsMcpServer {
    /// Build a server instance from validated configuration.
    pub async fn new(config: Config) -> Result<Self> {
        info!("Creating RustFS MCP Server");

        let s3_client = S3Client::new(&config).await?;

        Ok(Self {
            s3_client,
            _config: config,
            tool_router: Self::tool_router(),
        })
    }

    #[tool(
        description = "Create a new S3 bucket with the specified name, optionally with Object Lock enabled (which also turns on versioning)",
        // Destructive because the call is not guaranteed to target a new
        // bucket: in us-east-1 S3 answers CreateBucket for an existing
        // bucket owned by the caller with success and resets its ACLs.
        annotations(destructive_hint = true)
    )]
    pub async fn create_bucket(&self, Parameters(req): Parameters<CreateBucketRequest>) -> String {
        info!(
            "Executing create_bucket tool for bucket: {}",
            req.bucket_name
        );

        // Object Lock can only be enabled when the bucket is created, so
        // the choice has to be made here and not by a later configuration
        // call.
        let object_lock_enabled = req.object_lock_enabled == Some(true);
        let result = if object_lock_enabled {
            self.s3_client
                .create_bucket_with_object_lock(&req.bucket_name)
                .await
        } else {
            self.s3_client.create_bucket(&req.bucket_name).await
        };

        match result {
            Ok(_) => {
                let mut result_text = format!("Successfully created bucket: {}", req.bucket_name);
                if object_lock_enabled {
                    result_text.push_str(
                        "\n\nℹ️ **Note:** The bucket was created with Object Lock enabled and versioning turned on. Object Lock cannot be turned off for this bucket: set a default retention with put_bucket_lock_configuration, or retain individual object versions with put_object_retention.",
                    );
                }
                result_text
            }
            Err(e) => {
                format!("Failed to create bucket '{}': {:?}", req.bucket_name, e)
            }
        }
    }

    #[tool(
        description = "Get the default server-side encryption configuration of an S3 bucket: the SSE algorithm (AES256, aws:kms, or aws:kms:dsse), the KMS key, and the Bucket Key setting of each rule",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_encryption(
        &self,
        Parameters(req): Parameters<GetBucketEncryptionRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_encryption tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_encryption(&req.bucket_name).await {
            Ok(result) => {
                debug!(
                    "Bucket '{}' has {} default encryption rule(s)",
                    result.bucket,
                    result.rules.len()
                );

                let mut result_text = format!(
                    "✅ **Bucket encryption retrieved!**\n\n\
                     **Bucket:** {}\n",
                    result.bucket
                );

                if result.rules.is_empty() {
                    result_text.push_str(
                        "\nℹ️ **Note:** No default encryption is configured for this bucket.",
                    );
                }

                for (index, rule) in result.rules.iter().enumerate() {
                    result_text.push_str(&format!(
                        "\n**Rule {}:**\n{}",
                        index + 1,
                        Self::format_encryption_rule(rule)
                    ));
                }

                info!("get_bucket_encryption tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get encryption configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the encryption configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set the default server-side encryption of an S3 bucket by replacing its configuration with one rule: AES256 (SSE-S3), aws:kms (SSE-KMS), or aws:kms:dsse (dual-layer), with an optional KMS key and Bucket Key setting. Applies to new objects only; existing objects are not re-encrypted",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_encryption(
        &self,
        Parameters(req): Parameters<PutBucketEncryptionRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_encryption tool for bucket '{}': {}",
            req.bucket_name, req.sse_algorithm
        );

        let rule = crate::s3_client::ServerSideEncryptionRule {
            apply_server_side_encryption_by_default:
                crate::s3_client::ServerSideEncryptionByDefault {
                    sse_algorithm: req.sse_algorithm.clone(),
                    kms_key_id: req.kms_key_id.clone(),
                },
            bucket_key_enabled: req.bucket_key_enabled,
        };

        // Rendered up front because the data layer takes ownership of the rule.
        let rule_text = Self::format_encryption_rule(&rule);

        match self
            .s3_client
            .put_bucket_encryption(&req.bucket_name, rule)
            .await
        {
            Ok(()) => {
                info!(
                    "put_bucket_encryption tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket encryption updated!**\n\n\
                     **Bucket:** {}\n\n\
                     {}\n\
                     ℹ️ **Note:** The rule applies to new objects only; existing objects are not re-encrypted.",
                    req.bucket_name, rule_text
                )
            }
            Err(e) => {
                error!(
                    "Failed to set encryption for bucket '{}' to {}: {:?}",
                    req.bucket_name, req.sse_algorithm, e
                );

                format!(
                    "❌ **Failed to update the encryption configuration of S3 bucket '{}'**\n\n\
                     **Requested SSE Algorithm:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • SSE-KMS was requested but the storage backend has no KMS key configured\n\
                     • The SSE algorithm is not exactly 'AES256', 'aws:kms', or 'aws:kms:dsse', or a Bucket Key was combined with an algorithm that does not support it ('AES256' or 'aws:kms:dsse')\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to change the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Spell the SSE algorithm exactly: 'AES256', 'aws:kms', or 'aws:kms:dsse' (case-sensitive)\n\
                     3. Use a Bucket Key only together with 'aws:kms'\n\
                     4. For 'aws:kms' and 'aws:kms:dsse', confirm the storage backend has KMS configured and the KMS key exists\n\
                     5. Check your AWS credentials and permissions",
                    req.bucket_name, req.sse_algorithm, e
                )
            }
        }
    }

    #[tool(
        description = "Remove the default server-side encryption configuration of an S3 bucket; subsequent writes then follow the storage backend's default encryption behavior (on Amazon S3 the default resets to SSE-S3)",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_encryption(
        &self,
        Parameters(req): Parameters<DeleteBucketEncryptionRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_encryption tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .delete_bucket_encryption(&req.bucket_name)
            .await
        {
            Ok(()) => {
                info!(
                    "delete_bucket_encryption tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket encryption removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** The configured default rule was removed, and new objects are encrypted \
                     according to the backend's default encryption behavior — on Amazon S3, removing the \
                     configuration resets the default to SSE-S3. \
                     Existing objects keep the encryption they were stored with.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to delete encryption configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the encryption configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to change the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    /// The lines describing one encryption rule; the get and put tools print
    /// a rule the same way.
    fn format_encryption_rule(rule: &crate::s3_client::ServerSideEncryptionRule) -> String {
        let default = &rule.apply_server_side_encryption_by_default;
        let mut text = format!("**SSE Algorithm:** {}\n", default.sse_algorithm);
        if let Some(ref kms_key_id) = default.kms_key_id {
            text.push_str(&format!("**KMS Key ID:** {kms_key_id}\n"));
        }
        text.push_str(&format!("**Bucket Key:** {}\n", rule.bucket_key_label()));
        text
    }

    #[tool(
        description = "Delete an existing S3 bucket with the specified name",
        annotations(destructive_hint = true)
    )]
    pub async fn delete_bucket(&self, Parameters(req): Parameters<DeleteBucketRequest>) -> String {
        info!(
            "Executing delete_bucket tool for bucket: {}",
            req.bucket_name
        );

        // check if bucket is empty, if not, can not delete bucket directly.
        let object_result = match self
            .s3_client
            .list_objects_v2(&req.bucket_name, ListObjectsOptions::default())
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!(
                    "Failed to list objects in bucket '{}': {:?}",
                    req.bucket_name, e
                );
                return format!(
                    "Failed to list objects in bucket '{}': {:?}",
                    req.bucket_name, e
                );
            }
        };

        if !object_result.objects.is_empty() {
            error!("Bucket '{}' is not empty", req.bucket_name);
            return format!(
                "Failed to delete bucket '{}': bucket is not empty",
                req.bucket_name
            );
        }

        // delete the bucket.
        match self.s3_client.delete_bucket(&req.bucket_name).await {
            Ok(_) => {
                format!("Successfully deleted bucket: {}", req.bucket_name)
            }
            Err(e) => {
                format!("Failed to delete bucket '{}': {:?}", req.bucket_name, e)
            }
        }
    }

    #[tool(
        description = "Check that an S3 bucket exists and is accessible with the configured credentials, and report its region when the storage backend provides it",
        annotations(read_only_hint = true)
    )]
    pub async fn head_bucket(&self, Parameters(req): Parameters<HeadBucketRequest>) -> String {
        info!("Executing head_bucket tool for bucket: {}", req.bucket_name);

        match self.s3_client.head_bucket(&req.bucket_name).await {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Bucket found!**\n\n\
                     **Bucket:** {}\n",
                    result.bucket
                );

                if let Some(ref region) = result.region {
                    result_text.push_str(&format!("**Region:** {region}\n"));
                }

                info!("head_bucket tool executed successfully");
                result_text
            }
            Err(e) => {
                error!("Bucket check failed for '{}': {:?}", req.bucket_name, e);

                format!(
                    "❌ **Bucket check failed**\n\n\
                     **Bucket:** {}\n\
                     **Error:** {:#}\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket name against list_buckets\n\
                     2. Check that your AWS credentials may access the bucket\n\
                     3. Verify the endpoint and region configuration",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the region an S3 bucket lives in (buckets created outside us-east-1 must be addressed through their own region)",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_location(
        &self,
        Parameters(req): Parameters<GetBucketLocationRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_location tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_location(&req.bucket_name).await {
            Ok(result) => {
                info!("get_bucket_location tool executed successfully");
                format!(
                    "✅ **Bucket location retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Region:** {}",
                    result.bucket, result.region
                )
            }
            Err(e) => {
                error!(
                    "Failed to get location for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the location of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the resource policy of an S3 bucket as a JSON document (who may access the bucket and its objects)",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_policy(
        &self,
        Parameters(req): Parameters<GetBucketPolicyRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_policy tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_policy(&req.bucket_name).await {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Bucket policy retrieved!**\n\n\
                     **Bucket:** {}\n",
                    result.bucket
                );

                match result.policy {
                    Some(policy) => {
                        // Policies round-trip through S3 as JSON text; pretty-
                        // print them unless the document is not valid JSON.
                        let pretty = serde_json::from_str::<serde_json::Value>(&policy)
                            .ok()
                            .and_then(|value| serde_json::to_string_pretty(&value).ok())
                            .unwrap_or(policy);
                        result_text.push_str(&format!(
                            "\n**Policy:**\n```json\n{pretty}\n```\n"
                        ));
                    }
                    None => result_text.push_str(
                        "\nℹ️ **Note:** No bucket policy is set for this bucket; access is governed by the credentials and any remaining access controls such as bucket or object ACLs.\n",
                    ),
                }

                info!("get_bucket_policy tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get policy for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the policy of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set the resource policy of an S3 bucket from a JSON policy document (Version + Statement array), replacing the current policy. A policy can expose a bucket or its objects to other accounts or to everyone - review it before applying",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_policy(
        &self,
        Parameters(req): Parameters<PutBucketPolicyRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_policy tool for bucket: {}",
            req.bucket_name
        );

        // The policy arrives as a JSON object so callers do not have to
        // escape a JSON document into a string; S3 takes it as JSON text.
        let statements = match req.policy.get("Statement") {
            Some(serde_json::Value::Array(statements)) => statements.len().to_string(),
            Some(_) => "1".to_string(),
            None => "none".to_string(),
        };
        let policy = match serde_json::to_string(&req.policy) {
            Ok(policy) => policy,
            Err(e) => {
                error!("Failed to serialize the bucket policy: {:?}", e);
                return format!(
                    "❌ **Failed to serialize the bucket policy**\n\n\
                     **Error:** {e:#}"
                );
            }
        };

        match self
            .s3_client
            .put_bucket_policy(&req.bucket_name, &policy)
            .await
        {
            Ok(()) => {
                info!("put_bucket_policy tool executed successfully");
                format!(
                    "✅ **Bucket policy updated!**\n\n\
                     **Bucket:** {}\n\
                     **Statements:** {}\n\n\
                     ⚠️ **Note:** The policy takes effect immediately. Statements with a wildcard \
                     principal (\"*\") make the bucket or its objects public — use get_bucket_policy to review it.",
                    req.bucket_name, statements
                )
            }
            Err(e) => {
                error!(
                    "Failed to set policy for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to set the policy of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • The policy is malformed, or its resources do not name this bucket\n\
                     • AWS credentials lack permission to write bucket policies\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check that \"Resource\" entries use this bucket's ARN\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Remove the resource policy from an S3 bucket; access then falls back to the bucket's remaining access controls such as bucket or object ACLs",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_policy(
        &self,
        Parameters(req): Parameters<DeleteBucketPolicyRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_policy tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.delete_bucket_policy(&req.bucket_name).await {
            Ok(()) => {
                info!("delete_bucket_policy tool executed successfully");
                format!(
                    "✅ **Bucket policy removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** Access to this bucket is now governed by its remaining access \
                     controls such as bucket or object ACLs; public ACL grants are not revoked.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to remove policy for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the policy of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to write bucket policies\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get who pays for the requests to an S3 bucket: the bucket owner (the S3 default) or the requester",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_request_payment(
        &self,
        Parameters(req): Parameters<GetBucketRequestPaymentRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_request_payment tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .get_bucket_request_payment(&req.bucket_name)
            .await
        {
            Ok(result) => {
                info!("get_bucket_request_payment tool executed successfully");
                let requester_pays = result.payer == crate::s3_client::BucketPayer::Requester;
                format!(
                    "✅ **Bucket request payment retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Payer:** {}\n\n\
                     ℹ️ **Note:** {}",
                    result.bucket,
                    serde_json::to_string(&result.payer)
                        .unwrap_or_else(|_| result.payer.label().to_string())
                        .trim_matches('"'),
                    if requester_pays {
                        "Requests to this bucket cost the callers: they must authenticate and \
                         cover the request and data-transfer charges, and anonymous access to \
                         the bucket is disabled while requester pays is set."
                    } else {
                        "The bucket owner pays for the requests to this bucket, including the \
                         ones made anonymously (set payer to \"Requester\" to charge callers)."
                    }
                )
            }
            Err(e) => {
                error!(
                    "Failed to get request payment configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the request payment configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set who pays for the requests to an S3 bucket: the bucket owner (the S3 default), or the requester, who must then authenticate and cover the request and data-transfer costs. Requester pays disables anonymous access to the bucket while it is set",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_request_payment(
        &self,
        Parameters(req): Parameters<PutBucketRequestPaymentRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_request_payment tool for bucket '{}': payer {}",
            req.bucket_name,
            req.payer.label()
        );

        match self
            .s3_client
            .put_bucket_request_payment(&req.bucket_name, req.payer)
            .await
        {
            Ok(()) => {
                info!("put_bucket_request_payment tool executed successfully");
                let requester_pays = req.payer == crate::s3_client::BucketPayer::Requester;
                format!(
                    "✅ **Bucket request payment updated!**\n\n\
                     **Bucket:** {}\n\
                     **Payer:** {}\n\n\
                     ℹ️ **Note:** {}",
                    req.bucket_name,
                    serde_json::to_string(&req.payer)
                        .unwrap_or_else(|_| req.payer.label().to_string())
                        .trim_matches('"'),
                    if requester_pays {
                        "Requests to this bucket now cost the callers: they must authenticate \
                         and cover the request and data-transfer charges, and anonymous access \
                         to the bucket is disabled while requester pays is set."
                    } else {
                        "The bucket owner again pays for the requests to this bucket, \
                         including the ones made anonymously."
                    }
                )
            }
            Err(e) => {
                error!(
                    "Failed to set request payment configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to set the request payment configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Read the current setting with get_bucket_request_payment\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "List all S3 buckets accessible with the configured credentials",
        annotations(read_only_hint = true)
    )]
    pub async fn list_buckets(&self) -> String {
        info!("Executing list_buckets tool");

        match self.s3_client.list_buckets().await {
            Ok(buckets) => {
                debug!("Successfully retrieved {} buckets", buckets.len());

                if buckets.is_empty() {
                    return "No S3 buckets found. The AWS credentials may not have access to any buckets, or no buckets exist in this account.".to_string();
                }

                let mut result_text = format!("Found {} S3 bucket(s):\n\n", buckets.len());

                for (index, bucket) in buckets.iter().enumerate() {
                    result_text.push_str(&format!("{}. **{}**", index + 1, bucket.name));

                    if let Some(ref creation_date) = bucket.creation_date {
                        result_text.push_str(&format!("\n   - Created: {creation_date}"));
                    }
                    result_text.push_str("\n\n");
                }

                result_text.push_str("---\n");
                result_text.push_str(&format!("Total buckets: {}\n", buckets.len()));
                result_text.push_str(
                    "Note: Only buckets accessible with the current AWS credentials are shown.",
                );

                info!("list_buckets tool executed successfully");
                result_text
            }
            Err(e) => {
                error!("Failed to list buckets: {:?}", e);

                format!(
                    "Failed to list S3 buckets: {e:#}\n\nPossible causes:\n\
                     • AWS credentials are not set or invalid\n\
                     • Network connectivity issues\n\
                     • AWS region is not set correctly\n\
                     • Insufficient permissions to list buckets\n\
                     • Custom endpoint is misconfigured\n\n\
                     Please verify your AWS configuration and try again."
                )
            }
        }
    }

    #[tool(
        description = "Get the lifecycle rules of an S3 bucket: when objects expire, move to another storage class, or get cleaned up. Returns a readable summary plus the rules as JSON, which can be edited and sent back through put_bucket_lifecycle",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_lifecycle(
        &self,
        Parameters(req): Parameters<GetBucketLifecycleRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_lifecycle tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_lifecycle(&req.bucket_name).await {
            Ok(result) => {
                info!("get_bucket_lifecycle tool executed successfully");

                if result.rules.is_empty() {
                    return format!(
                        "✅ **Bucket lifecycle retrieved!**\n\n\
                         **Bucket:** {}\n\n\
                         ℹ️ **Note:** No lifecycle configuration is set for this bucket; objects are kept until they are deleted by hand.",
                        result.bucket
                    );
                }

                let rules_json = match serde_json::to_string_pretty(&result.rules) {
                    Ok(json) => json,
                    Err(e) => {
                        error!(
                            "Failed to render the lifecycle rules of bucket '{}': {:?}",
                            req.bucket_name, e
                        );
                        return format!(
                            "❌ **Failed to render the lifecycle rules as JSON**\n\n\
                                 **Error:** {e:#}"
                        );
                    }
                };

                format!(
                    "✅ **Bucket lifecycle retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Rules:** {}\n\n{}\n\
                     **Rules as JSON** (edit them and pass the result to put_bucket_lifecycle):\n```json\n{}\n```\n",
                    result.bucket,
                    result.rules.len(),
                    lifecycle::summarize_rules(&result.rules),
                    rules_json
                )
            }
            Err(e) => {
                error!(
                    "Failed to get lifecycle configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the lifecycle configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set the lifecycle rules of an S3 bucket from a list of rules, replacing the current configuration. Rules can expire objects, move them to another storage class, expire or transition noncurrent versions, and discard incomplete multipart uploads. Expiration permanently deletes data",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_lifecycle(
        &self,
        Parameters(req): Parameters<PutBucketLifecycleRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_lifecycle tool for bucket '{}': {} rule(s)",
            req.bucket_name,
            req.rules.len()
        );

        match self
            .s3_client
            .put_bucket_lifecycle(&req.bucket_name, &req.rules)
            .await
        {
            Ok(()) => {
                info!("put_bucket_lifecycle tool executed successfully");
                format!(
                    "✅ **Bucket lifecycle updated!**\n\n\
                     **Bucket:** {}\n\
                     **Rules:** {}\n\n{}\n\
                     ⚠️ **Note:** The rules take effect immediately. Expiration actions permanently \
                     delete objects and cannot be undone.",
                    req.bucket_name,
                    req.rules.len(),
                    lifecycle::summarize_rules(&req.rules)
                )
            }
            Err(e) => {
                error!(
                    "Failed to set lifecycle configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to set the lifecycle configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • A rule is malformed, e.g. an expiration with both days and date\n\
                     • A transition names a storage class the storage backend does not offer\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration\n\n\
                     **Troubleshooting steps:**\n\
                     1. Read the current rules with get_bucket_lifecycle and compare them\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Ensure the bucket name is correct",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Remove every lifecycle rule from an S3 bucket; objects are then kept until they are deleted by hand",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_lifecycle(
        &self,
        Parameters(req): Parameters<DeleteBucketLifecycleRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_lifecycle tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .delete_bucket_lifecycle(&req.bucket_name)
            .await
        {
            Ok(()) => {
                info!("delete_bucket_lifecycle tool executed successfully");
                format!(
                    "✅ **Bucket lifecycle removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** Objects in this bucket are now kept until they are deleted by hand.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to remove lifecycle configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the lifecycle configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the replication configuration of an S3 bucket: the role used for copying (an IAM role on AWS S3; the ARN of a replication target configured on the server on MinIO and RustFS), and the rules deciding which object versions are copied to which destination bucket. Returns a readable summary plus the rules as JSON, which can be edited and sent back through put_bucket_replication",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_replication(
        &self,
        Parameters(req): Parameters<GetBucketReplicationRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_replication tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .get_bucket_replication(&req.bucket_name)
            .await
        {
            Ok(result) => {
                info!("get_bucket_replication tool executed successfully");

                if result.rules.is_empty() {
                    return format!(
                        "✅ **Bucket replication retrieved!**\n\n\
                         **Bucket:** {}\n\n\
                         ℹ️ **Note:** No replication configuration is set for this bucket; object versions stay in this bucket only.",
                        result.bucket
                    );
                }

                let rules_json = match serde_json::to_string_pretty(&result.rules) {
                    Ok(json) => json,
                    Err(e) => {
                        error!(
                            "Failed to render the replication rules of bucket '{}': {:?}",
                            req.bucket_name, e
                        );
                        return format!(
                            "❌ **Failed to render the replication rules as JSON**\n\n\
                                 **Error:** {e:#}"
                        );
                    }
                };

                format!(
                    "✅ **Bucket replication retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Role:** {}\n\
                     **Rules:** {}\n\n{}\n{}\
                     **Rules as JSON** (edit them and pass the result to put_bucket_replication):\n```json\n{}\n```\n",
                    result.bucket,
                    result.role.as_deref().unwrap_or("(none)"),
                    result.rules.len(),
                    replication::summarize_rules(&result.rules),
                    replication::unmodeled_settings_warning(&result.unmodeled_settings),
                    rules_json
                )
            }
            Err(e) => {
                error!(
                    "Failed to get replication configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the replication configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set the replication configuration of an S3 bucket from a role and a list of rules, replacing the current configuration. Rules copy object versions to another bucket, selected by prefix and tags. On AWS S3 the role is an IAM role; on MinIO and RustFS it is the ARN of a replication target configured on the server. The source bucket needs versioning enabled (put_bucket_versioning), and rules only copy new object versions unless they enable existing_object_replication",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_replication(
        &self,
        Parameters(req): Parameters<PutBucketReplicationRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_replication tool for bucket '{}': {} rule(s)",
            req.bucket_name,
            req.rules.len()
        );

        match self
            .s3_client
            .put_bucket_replication(&req.bucket_name, &req.role, &req.rules)
            .await
        {
            Ok(()) => {
                info!("put_bucket_replication tool executed successfully");
                format!(
                    "✅ **Bucket replication updated!**\n\n\
                     **Bucket:** {}\n\
                     **Role:** {}\n\
                     **Rules:** {}\n\n{}\n\
                     ⚠️ **Note:** Replication starts copying new object versions immediately. \
                     Versions that already exist are only copied by rules with existing_object_replication on, \
                     and the destination buckets keep growing until their lifecycle rules expire the copies.",
                    req.bucket_name,
                    req.role,
                    req.rules.len(),
                    replication::summarize_rules(&req.rules)
                )
            }
            Err(e) => {
                error!(
                    "Failed to set replication configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to set the replication configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • A rule is malformed: a destination equal to the source bucket, a priority used twice (a rule without one counts as 0), or delete_marker_replication on a rule that selects by a single tag\n\
                     • The bucket does not have versioning enabled, which MinIO and RustFS require before replication can be configured\n\
                     • On MinIO and RustFS, the role does not name a replication target configured on the server (they answer InvalidRequest: stale target, or replication target configuration not found)\n\
                     • The configuration names a field or a storage class the storage backend does not support: RustFS keeps STANDARD copies only and refuses metrics, replication time control, access control translation, encryption configuration, and the destination account\n\
                     • The current configuration holds settings this tool does not model, so the bucket was refused\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration\n\n\
                     **Troubleshooting steps:**\n\
                     1. Read the current rules with get_bucket_replication and compare them\n\
                     2. Check the versioning state of the source bucket with get_bucket_versioning\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Remove every replication rule written through S3 from a bucket, then read the configuration back to report what the storage backend kept: on RustFS, the rules a site replication manages stay in effect. Copies that already reached the destination buckets stay there",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_replication(
        &self,
        Parameters(req): Parameters<DeleteBucketReplicationRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_replication tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .delete_bucket_replication(&req.bucket_name)
            .await
        {
            Ok(removal) => {
                info!("delete_bucket_replication tool executed successfully");
                replication::removal_text(&removal, &req.bucket_name)
            }
            Err(e) => {
                error!(
                    "Failed to remove replication configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the replication configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the server access logging configuration of an S3 bucket: the log target bucket requests are recorded to, and the key prefix they are stored under",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_logging(
        &self,
        Parameters(req): Parameters<GetBucketLoggingRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_logging tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_logging(&req.bucket_name).await {
            Ok(result) => {
                let Some(configuration) = result.configuration else {
                    info!(
                        "get_bucket_logging tool executed successfully: access logging is off for bucket '{}'",
                        req.bucket_name
                    );
                    return format!(
                        "✅ **Bucket logging configuration retrieved!**\n\n\
                         **Bucket:** {}\n\n\
                         ℹ️ **Note:** Access logging is not enabled for this bucket, so requests to it are not recorded. Enable it with put_bucket_logging.",
                        result.bucket
                    );
                };

                info!(
                    "get_bucket_logging tool executed successfully for bucket '{}'",
                    req.bucket_name
                );

                // Settings this tool does not model are named, so a caller
                // knows put_bucket_logging would drop them.
                let warning = if result.unshown_settings.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\n⚠️ **Warning:** The configuration also holds settings this tool does not show: {}. put_bucket_logging replaces the whole configuration and would drop them.",
                        result.unshown_settings.join(", ")
                    )
                };

                format!(
                    "✅ **Bucket logging configuration retrieved!**\n\n\
                     **Bucket:** {}\n\
                     {}\n\
                     {}\
                     ℹ️ **Note:** Requests to this bucket are recorded as objects in the log target bucket. On AWS, the target bucket needs a bucket policy that lets the S3 logging service write to it (put_bucket_policy). Storage backends differ in whether they deliver the logs: RustFS 1.0.6 stores this configuration without writing log objects.",
                    result.bucket,
                    configuration.summary(),
                    warning
                )
            }
            Err(e) => {
                error!(
                    "Failed to get the logging configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the logging configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration (s3:GetBucketLogging)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Enable server access logging for an S3 bucket: its requests are then recorded as objects in a log target bucket, under an optional key prefix (storage backends differ in whether they deliver the logs; RustFS 1.0.6 stores the configuration without writing log objects)",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_logging(
        &self,
        Parameters(req): Parameters<PutBucketLoggingRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_logging tool for bucket '{}' into bucket '{}'",
            req.bucket_name, req.target_bucket
        );

        let configuration = crate::s3_client::LoggingConfiguration {
            target_bucket: req.target_bucket,
            target_prefix: req.target_prefix,
        };

        match self
            .s3_client
            .put_bucket_logging(&req.bucket_name, &configuration)
            .await
        {
            Ok(()) => {
                info!(
                    "put_bucket_logging tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket logging configuration updated!**\n\n\
                     **Bucket:** {}\n\
                     {}\n\
                     ℹ️ **Note:** Requests to this bucket are recorded as objects in the log target bucket, which keeps growing until the logs are removed or expired (put_bucket_lifecycle on the target bucket can expire them). On AWS, the target bucket needs a bucket policy that lets the S3 logging service write to it (put_bucket_policy). Storage backends differ in whether they deliver the logs: RustFS 1.0.6 stores this configuration without writing log objects.",
                    req.bucket_name,
                    configuration.summary()
                )
            }
            Err(e) => {
                error!(
                    "Failed to set the logging configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to update the logging configuration of S3 bucket '{}'**\n\n\
                     **Log Target Bucket:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • No log target bucket was given\n\
                     • The log target bucket does not exist, or lies in another region (on AWS it must be in the same region as the source bucket)\n\
                     • The log target bucket does not allow the logging service to write to it (on AWS through its bucket policy)\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration (s3:PutBucketLogging)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket and the log target bucket exist using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, configuration.target_bucket, e
                )
            }
        }
    }

    #[tool(
        description = "Disable server access logging for an S3 bucket; requests to it are then no longer recorded. Logs that were already written stay in the log target bucket",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_logging(
        &self,
        Parameters(req): Parameters<DeleteBucketLoggingRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_logging tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.delete_bucket_logging(&req.bucket_name).await {
            Ok(()) => {
                info!(
                    "delete_bucket_logging tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket logging configuration removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** Requests to this bucket are no longer recorded. Logs that were already written stay in the log target bucket.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to remove the logging configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the logging configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration (s3:PutBucketLogging)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "List objects in a specific S3 bucket with optional prefix filtering, directory-style browsing (delimiter), and pagination (max_keys + continuation token)",
        annotations(read_only_hint = true)
    )]
    pub async fn list_objects(&self, Parameters(req): Parameters<ListObjectsRequest>) -> String {
        info!(
            "Executing list_objects tool for bucket: {}",
            req.bucket_name
        );

        // The S3 ListObjectsV2 API accepts at most 1000 keys per page.
        let max_keys = req.max_keys.map(|keys| keys.clamp(1, 1000));

        let options = ListObjectsOptions {
            prefix: req.prefix.clone(),
            delimiter: req.delimiter.clone(),
            max_keys,
            continuation_token: req.continuation_token.clone(),
            start_after: req.start_after.clone(),
        };

        match self
            .s3_client
            .list_objects_v2(&req.bucket_name, options)
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully retrieved {} objects and {} common prefixes from bucket '{}'",
                    result.objects.len(),
                    result.common_prefixes.len(),
                    req.bucket_name
                );

                if result.objects.is_empty() && result.common_prefixes.is_empty() {
                    let prefix_msg = req
                        .prefix
                        .as_ref()
                        .map(|p| format!(" with prefix '{p}'"))
                        .unwrap_or_default();
                    return format!(
                        "No objects found in bucket '{}'{prefix_msg}. The bucket may be empty or the prefix may not match any objects.",
                        req.bucket_name
                    );
                }

                let mut result_text = format!(
                    "Found {} object(s) in bucket **{}**",
                    result.key_count, req.bucket_name
                );

                if let Some(ref p) = req.prefix {
                    result_text.push_str(&format!(" with prefix '{p}'"));
                }
                result_text.push_str(":\n\n");

                if !result.common_prefixes.is_empty() {
                    result_text.push_str("**Directories:**\n");
                    for (index, prefix) in result.common_prefixes.iter().enumerate() {
                        result_text.push_str(&format!("{}. 📁 {prefix}\n", index + 1));
                    }
                    result_text.push('\n');
                }

                if !result.objects.is_empty() {
                    result_text.push_str("**Objects:**\n");
                    for (index, obj) in result.objects.iter().enumerate() {
                        result_text.push_str(&format!("{}. **{}**\n", index + 1, obj.key));

                        if let Some(size) = obj.size {
                            result_text.push_str(&format!("   - Size: {size} bytes\n"));
                        }

                        if let Some(ref last_modified) = obj.last_modified {
                            result_text.push_str(&format!("   - Last Modified: {last_modified}\n"));
                        }

                        if let Some(ref etag) = obj.etag {
                            result_text.push_str(&format!("   - ETag: {etag}\n"));
                        }

                        if let Some(ref storage_class) = obj.storage_class {
                            result_text.push_str(&format!("   - Storage Class: {storage_class}\n"));
                        }

                        result_text.push('\n');
                    }
                }

                if result.is_truncated {
                    result_text.push_str("**Note:** Results are truncated. ");
                    if let Some(ref token) = result.next_continuation_token {
                        result_text.push_str(&format!(
                            "Call this tool again with continuation_token=\"{token}\" to fetch the next page.\n"
                        ));
                    }
                    result_text.push('\n');
                }

                result_text.push_str("---\n");
                result_text.push_str(&format!(
                    "Total: {} object(s), {} directory/ies",
                    result.objects.len(),
                    result.common_prefixes.len()
                ));

                if let Some(max_keys) = result.max_keys {
                    result_text.push_str(&format!(", Max keys: {max_keys}"));
                }

                info!(
                    "list_objects tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to list objects in bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "Failed to list objects in S3 bucket '{}': {}\n\nPossible causes:\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to list objects in this bucket\n\
                     • Network connectivity issues\n\
                     • Custom endpoint is misconfigured\n\
                     • Bucket name contains invalid characters\n\n\
                     Please verify the bucket name, your AWS configuration, and permissions.",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the versioning configuration of an S3 bucket: enabled, suspended, or never enabled",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_versioning(
        &self,
        Parameters(req): Parameters<GetBucketVersioningRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_versioning tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_versioning(&req.bucket_name).await {
            Ok(result) => {
                let (status, explanation) = match result.status {
                    Some(status) => (status.label(), status.effect()),
                    None => (
                        "Never enabled",
                        "Only the latest copy of each object is kept; overwrites and deletes are permanent.",
                    ),
                };

                info!("get_bucket_versioning tool executed successfully");
                format!(
                    "✅ **Bucket versioning retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Versioning:** {status}\n\n\
                     ℹ️ **Note:** {explanation}",
                    result.bucket
                )
            }
            Err(e) => {
                error!(
                    "Failed to get versioning configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the versioning configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Enable or suspend versioning for an S3 bucket: 'enabled' keeps every version of every object, 'suspended' stops creating new versions while keeping the existing ones",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_versioning(
        &self,
        Parameters(req): Parameters<PutBucketVersioningRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_versioning tool for bucket '{}': {}",
            req.bucket_name,
            req.status.as_str()
        );

        match self
            .s3_client
            .put_bucket_versioning(&req.bucket_name, req.status)
            .await
        {
            Ok(()) => {
                info!("put_bucket_versioning tool executed successfully");
                format!(
                    "✅ **Bucket versioning updated!**\n\n\
                     **Bucket:** {}\n\
                     **Versioning:** {}\n\n\
                     ℹ️ **Note:** {}",
                    req.bucket_name,
                    req.status.label(),
                    req.status.effect()
                )
            }
            Err(e) => {
                error!(
                    "Failed to set versioning for bucket '{}' to {}: {:?}",
                    req.bucket_name,
                    req.status.as_str(),
                    e
                );

                format!(
                    "❌ **Failed to update versioning for S3 bucket '{}'**\n\n\
                     **Requested State:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to change the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name,
                    req.status.label(),
                    e
                )
            }
        }
    }

    #[tool(
        description = "List every version and delete marker of the objects in an S3 bucket, newest first per key, with optional prefix filtering, directory-style browsing (delimiter), and pagination (max_keys + key_marker / version_id_marker). Returns the version IDs used by get_object, copy_object, and delete_object_versions",
        annotations(read_only_hint = true)
    )]
    pub async fn list_object_versions(
        &self,
        Parameters(req): Parameters<ListObjectVersionsRequest>,
    ) -> String {
        info!(
            "Executing list_object_versions tool for bucket: {}",
            req.bucket_name
        );

        // The S3 ListObjectVersions API accepts at most 1000 entries per page.
        let max_keys = req.max_keys.map(|keys| keys.clamp(1, 1000));

        let options = crate::s3_client::ListObjectVersionsOptions {
            prefix: req.prefix.clone(),
            delimiter: req.delimiter.clone(),
            max_keys,
            key_marker: req.key_marker.clone(),
            version_id_marker: req.version_id_marker.clone(),
        };

        match self
            .s3_client
            .list_object_versions(&req.bucket_name, options)
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully retrieved {} versions/delete markers and {} common prefixes from bucket '{}'",
                    result.entries.len(),
                    result.common_prefixes.len(),
                    req.bucket_name
                );

                if result.entries.is_empty() && result.common_prefixes.is_empty() {
                    let prefix_msg = req
                        .prefix
                        .as_ref()
                        .map(|p| format!(" with prefix '{p}'"))
                        .unwrap_or_default();
                    return format!(
                        "No object versions found in bucket '{}'{prefix_msg}. The bucket may be empty or the prefix may not match any keys.",
                        req.bucket_name
                    );
                }

                let mut result_text = format!(
                    "Found {} version(s)/delete marker(s) in bucket **{}**",
                    result.entries.len(),
                    req.bucket_name
                );

                if let Some(ref p) = req.prefix {
                    result_text.push_str(&format!(" with prefix '{p}'"));
                }
                result_text.push_str(":\n\n");

                if !result.common_prefixes.is_empty() {
                    result_text.push_str("**Directories:**\n");
                    for (index, prefix) in result.common_prefixes.iter().enumerate() {
                        result_text.push_str(&format!("{}. 📁 {prefix}\n", index + 1));
                    }
                    result_text.push('\n');
                }

                for (index, entry) in result.entries.iter().enumerate() {
                    let kind = if entry.is_delete_marker {
                        "delete marker"
                    } else {
                        "version"
                    };
                    let latest = if entry.is_latest { " — latest" } else { "" };
                    result_text.push_str(&format!(
                        "{}. **{}** — {kind}{latest}\n",
                        index + 1,
                        entry.key
                    ));

                    if let Some(ref version_id) = entry.version_id {
                        result_text.push_str(&format!("   - Version ID: `{version_id}`\n"));
                    }
                    if let Some(ref last_modified) = entry.last_modified {
                        result_text.push_str(&format!("   - Last Modified: {last_modified}\n"));
                    }
                    if let Some(size) = entry.size {
                        result_text.push_str(&format!("   - Size: {size} bytes\n"));
                    }
                    if let Some(ref etag) = entry.etag {
                        result_text.push_str(&format!("   - ETag: {etag}\n"));
                    }
                    if let Some(ref storage_class) = entry.storage_class {
                        result_text.push_str(&format!("   - Storage Class: {storage_class}\n"));
                    }
                    result_text.push('\n');
                }

                if result.is_truncated {
                    result_text.push_str("**Note:** Results are truncated. ");
                    match (
                        result.next_key_marker.as_ref(),
                        result.next_version_id_marker.as_ref(),
                    ) {
                        (Some(key_marker), Some(version_id_marker))
                            if !version_id_marker.is_empty() =>
                        {
                            result_text.push_str(&format!(
                                "Call this tool again with key_marker=\"{key_marker}\" and version_id_marker=\"{version_id_marker}\" to fetch the next page.\n"
                            ));
                        }
                        (Some(key_marker), _) => {
                            // A truncated page that ends on a common prefix
                            // (delimiter listings) carries no usable version
                            // marker; the key marker alone continues the
                            // listing.
                            result_text.push_str(&format!(
                                "Call this tool again with key_marker=\"{key_marker}\" and no version_id_marker to fetch the next page.\n"
                            ));
                        }
                        _ => result_text.push_str(
                            "Call this tool again with a later key_marker to fetch the next page.\n",
                        ),
                    }
                }

                info!(
                    "list_object_versions tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to list object versions in bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "Failed to list object versions in S3 bucket '{}': {}\n\nPossible causes:\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to list object versions in this bucket\n\
                     • Network connectivity issues\n\
                     • Custom endpoint is misconfigured\n\n\
                     Please verify the bucket name, your AWS configuration, and permissions.",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Permanently delete specific object versions or delete markers (up to 1000) from an S3 bucket in a single batch request, reporting per-version failures. Collect the version IDs with list_object_versions. This operation cannot be undone",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_object_versions(
        &self,
        Parameters(req): Parameters<DeleteObjectVersionsRequest>,
    ) -> String {
        info!(
            "Executing delete_object_versions tool for {} version(s) in bucket: {}",
            req.versions.len(),
            req.bucket_name
        );

        if req.versions.is_empty() {
            return "No object versions provided. Please specify at least one key + version ID pair to delete."
                .to_string();
        }

        if req.versions.len() > 1000 {
            return format!(
                "❌ Too many object versions ({}). The S3 DeleteObjects API allows at most \
                 1000 identifiers per request; split the list into smaller batches.",
                req.versions.len()
            );
        }

        match self
            .s3_client
            .delete_object_versions(&req.bucket_name, &req.versions)
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Version delete completed!**\n\n\
                     **Bucket:** {}\n\
                     **Deleted:** {} version(s)\n\
                     **Failed:** {} version(s)\n",
                    result.bucket,
                    result.deleted.len(),
                    result.errors.len()
                );

                if !result.deleted.is_empty() {
                    result_text.push_str("\n**Deleted versions:**\n");
                    for deleted in &result.deleted {
                        match deleted.version_id {
                            // Not every S3 implementation flags a removed
                            // delete marker, so stay neutral when unflagged.
                            Some(ref version_id) if deleted.delete_marker => {
                                result_text.push_str(&format!(
                                    "• {} (delete marker `{version_id}`)\n",
                                    deleted.key
                                ));
                            }
                            Some(ref version_id) => {
                                result_text
                                    .push_str(&format!("• {} (`{version_id}`)\n", deleted.key));
                            }
                            None => result_text.push_str(&format!("• {}\n", deleted.key)),
                        }
                    }
                }

                if !result.errors.is_empty() {
                    result_text.push_str("\n**Failed versions:**\n");
                    for err in &result.errors {
                        match err.version_id {
                            Some(ref version_id) => result_text.push_str(&format!(
                                "• `{}` (version `{version_id}`): {}\n",
                                err.key, err.message
                            )),
                            None => {
                                result_text.push_str(&format!("• `{}`: {}\n", err.key, err.message))
                            }
                        }
                    }
                }

                result_text
                    .push_str("\n⚠️ **Note:** This operation is permanent and cannot be undone.");

                info!(
                    "delete_object_versions tool executed successfully: {} deleted, {} errors",
                    result.deleted.len(),
                    result.errors.len()
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to batch delete object versions in bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to batch delete object versions from S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • One or more version IDs are invalid or were already deleted\n\
                     • AWS credentials lack permissions to delete these versions\n\
                     • Bucket does not exist or is not accessible\n\
                     • More than 1000 versions were supplied\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the version IDs using list_object_versions\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Reduce the batch size to 1000 versions or fewer\n\
                     4. Retry — batch deletes are not atomic and partial failures are reported per version",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the Object Lock configuration of an S3 bucket: whether Object Lock is enabled and which retention period (GOVERNANCE or COMPLIANCE, in days or years) new object versions receive by default",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_lock_configuration(
        &self,
        Parameters(req): Parameters<GetBucketLockConfigurationRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_lock_configuration tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .get_bucket_lock_configuration(&req.bucket_name)
            .await
        {
            Ok(result) => {
                // `None` means S3 reported that the bucket has no Object
                // Lock configuration, not that the answer was unreadable.
                let Some(configuration) = result.configuration else {
                    info!(
                        "get_bucket_lock_configuration tool executed successfully: Object Lock is not enabled for bucket '{}'",
                        req.bucket_name
                    );
                    return format!(
                        "✅ **Bucket Object Lock configuration retrieved!**\n\n\
                         **Bucket:** {}\n\n\
                         ℹ️ **Note:** Object Lock is not enabled for this bucket. It can only be enabled when a bucket is created: create the bucket with object_lock_enabled set to true, which also turns on versioning.",
                        result.bucket
                    );
                };

                let mut result_text = format!(
                    "✅ **Bucket Object Lock configuration retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Object Lock:** {}\n",
                    result.bucket,
                    if configuration.object_lock_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );

                match configuration.default_retention {
                    Some(retention) => {
                        result_text.push_str(&format!(
                            "**Default Retention:** {}\n\n",
                            retention.label()
                        ));
                        result_text.push_str(&format!(
                            "ℹ️ **Note:** Every new object version of this bucket is retained for {}. {} The retention belongs to each version separately: overwriting or deleting an object leaves the earlier versions locked until their retention expires.",
                            retention.period_label(),
                            retention.mode.effect()
                        ));
                    }
                    None => result_text.push_str(
                        "\nℹ️ **Note:** Object Lock is enabled, but new object versions are not retained by default. Retain a version with put_object_retention, set a default here with put_bucket_lock_configuration, or hold a version with put_object_legal_hold.",
                    ),
                }

                info!(
                    "get_bucket_lock_configuration tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the Object Lock configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the Object Lock configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration (s3:GetBucketObjectLockConfiguration)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set or remove the default retention period that every new object version of an S3 bucket receives. To set it, pass mode (GOVERNANCE: users with the s3:BypassGovernanceRetention permission can shorten it; COMPLIANCE: cannot be shortened or removed until it expires) together with exactly one of days or years; omit all three to remove the default retention. The bucket must have Object Lock enabled, which is verified before the update (create a bucket with object_lock_enabled to turn it on)",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_lock_configuration(
        &self,
        Parameters(req): Parameters<PutBucketLockConfigurationRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_lock_configuration tool for bucket: {}",
            req.bucket_name
        );

        // Checked before anything is sent: the three fields either describe
        // one complete retention or ask to remove it.
        let outcome = match crate::s3_client::DefaultRetention::from_optional_fields(
            req.mode, req.days, req.years,
        ) {
            Err(e) => Err(e),
            Ok(default_retention) => self
                .s3_client
                .put_bucket_lock_configuration(&req.bucket_name, default_retention.as_ref())
                .await
                .map(|()| default_retention),
        };

        match outcome {
            Ok(Some(default_retention)) => {
                info!(
                    "put_bucket_lock_configuration tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket Object Lock configuration updated!**\n\n\
                     **Bucket:** {}\n\
                     **Default Retention:** {}\n\n\
                     ⚠️ **Note:** Every object version written to this bucket from now on is retained for {}. {} Object Lock cannot be turned off for this bucket afterwards.",
                    req.bucket_name,
                    default_retention.label(),
                    default_retention.period_label(),
                    default_retention.mode.effect()
                )
            }
            Ok(None) => {
                info!(
                    "put_bucket_lock_configuration tool executed successfully for bucket '{}': default retention removed",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket default retention removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** New object versions of this bucket are no longer retained by default. Object Lock stays enabled for the bucket, and versions that already carry a retention period keep it.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to set the Object Lock configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to set the Object Lock configuration of S3 bucket '{}'**\n\n\
                     **Requested Default Retention:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • The fields do not describe one retention: pass mode together with exactly one of days or years, or omit all three\n\
                     • The bucket does not have Object Lock enabled (verified before the update: on AWS the request would irreversibly enable Object Lock and permanently enable versioning)\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration (s3:PutBucketObjectLockConfiguration)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Read the current state with get_bucket_lock_configuration\n\
                     2. To set a retention, pass mode and exactly one of days or years, with a value of at least 1\n\
                     3. To remove the default retention, omit mode, days, and years\n\
                     4. To lock a bucket without Object Lock, create a new bucket with object_lock_enabled set to true\n\
                     5. Check your AWS credentials and permissions",
                    req.bucket_name,
                    crate::s3_client::DefaultRetention::from_optional_fields(
                        req.mode, req.days, req.years
                    )
                    .ok()
                    .flatten()
                    .map(|retention| retention.label())
                    .unwrap_or_else(|| "none".to_string()),
                    e
                )
            }
        }
    }

    #[tool(
        description = "Get the retention period of an S3 object version: the mode (GOVERNANCE or COMPLIANCE) and the moment until which the version cannot be deleted or overwritten",
        annotations(read_only_hint = true)
    )]
    pub async fn get_object_retention(
        &self,
        Parameters(req): Parameters<GetObjectRetentionRequest>,
    ) -> String {
        info!(
            "Executing get_object_retention tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .get_object_retention(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Object retention retrieved!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    result.bucket, result.key
                );

                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                match result.retention {
                    Some(retention) => {
                        result_text.push_str(&format!(
                            "**Retention Mode:** {}\n\
                             **Retain Until:** {}\n",
                            retention.mode.as_str(),
                            retention.retain_until
                        ));

                        if retention.is_active() {
                            result_text.push_str(&format!(
                                "\nℹ️ **Note:** Until then this version cannot be deleted or overwritten. {}",
                                retention.mode.effect()
                            ));
                        } else {
                            result_text.push_str(
                                "\nℹ️ **Note:** The retention period has ended: this version can be deleted or overwritten unless a legal hold is still on (get_object_legal_hold).",
                            );
                        }
                    }
                    None => result_text.push_str(&format!(
                        "\nℹ️ **Note:** {}",
                        object_lock::unset_retention_note(
                            result
                                .unset_reason
                                .unwrap_or(crate::s3_client::UnsetLockReason::NotSet),
                            &result.bucket
                        )
                    )),
                }

                info!("get_object_retention tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the retention of s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to get the retention of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • The bucket does not have Object Lock enabled\n\
                     • AWS credentials lack permission to read object retention (s3:GetObjectRetention)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Verify the bucket has Object Lock enabled using get_bucket_lock_configuration\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Set the retention period of an S3 object version until a given ISO 8601 moment: GOVERNANCE (users with the s3:BypassGovernanceRetention permission can shorten it) or COMPLIANCE (cannot be shortened or removed until it expires). Until then the version cannot be deleted or overwritten",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_object_retention(
        &self,
        Parameters(req): Parameters<PutObjectRetentionRequest>,
    ) -> String {
        info!(
            "Executing put_object_retention tool: {} on s3://{}/{}",
            req.mode.as_str(),
            req.bucket_name,
            req.object_key
        );

        let retention = crate::s3_client::ObjectRetention {
            mode: req.mode,
            retain_until: req.retain_until.clone(),
        };

        match self
            .s3_client
            .put_object_retention(
                &req.bucket_name,
                &req.object_key,
                req.version_id.as_deref(),
                &retention,
                req.bypass_governance_retention,
            )
            .await
        {
            Ok(()) => {
                let mut result_text = format!(
                    "✅ **Object retention updated!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    req.bucket_name, req.object_key
                );

                if let Some(ref version_id) = req.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }
                result_text.push_str(&format!(
                    "**Retention Mode:** {}\n\
                     **Retain Until:** {}\n\n\
                     ⚠️ **Note:** Until then this version cannot be deleted or overwritten. {}",
                    retention.mode.as_str(),
                    retention.retain_until,
                    retention.mode.effect()
                ));

                if req.bypass_governance_retention {
                    result_text.push_str(
                        "\n💡 **Note:** The request was allowed to override an existing GOVERNANCE retention (bypass_governance_retention).",
                    );
                }

                info!("put_object_retention tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to set the retention of s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to set the retention of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • retain_until is not an ISO 8601 timestamp (e.g. \"2030-01-01T00:00:00Z\") or does not lie in the future\n\
                     • Object (or the requested version) does not exist\n\
                     • The bucket does not have Object Lock enabled\n\
                     • The version already carries a retention that this call would shorten: a GOVERNANCE retention needs bypass_governance_retention, and a COMPLIANCE retention cannot be changed until it expires\n\
                     • AWS credentials lack permission to write object retention (s3:PutObjectRetention)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Read the current retention with get_object_retention\n\
                     2. Pass retain_until as an ISO 8601 UTC timestamp that lies in the future\n\
                     3. Verify the bucket has Object Lock enabled using get_bucket_lock_configuration\n\
                     4. To shorten a GOVERNANCE retention, set bypass_governance_retention to true\n\
                     5. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Get the legal hold status of an S3 object version: ON means the version cannot be deleted or overwritten until the hold is removed",
        annotations(read_only_hint = true)
    )]
    pub async fn get_object_legal_hold(
        &self,
        Parameters(req): Parameters<GetObjectLegalHoldRequest>,
    ) -> String {
        info!(
            "Executing get_object_legal_hold tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .get_object_legal_hold(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Object legal hold retrieved!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    result.bucket, result.key
                );

                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                match result.status {
                    Some(status) => {
                        result_text.push_str(&format!("**Legal Hold:** {}\n", status.as_str()));
                        result_text.push_str(&format!("\nℹ️ **Note:** {}", status.effect()));
                    }
                    None => result_text.push_str(&format!(
                        "\n**Legal Hold:** OFF\n\nℹ️ **Note:** {}",
                        object_lock::unset_legal_hold_note(
                            result
                                .unset_reason
                                .unwrap_or(crate::s3_client::UnsetLockReason::NotSet),
                            &result.bucket
                        )
                    )),
                }

                info!("get_object_legal_hold tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the legal hold of s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to get the legal hold of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • The bucket does not have Object Lock enabled\n\
                     • AWS credentials lack permission to read legal holds (s3:GetObjectLegalHold)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Verify the bucket has Object Lock enabled using get_bucket_lock_configuration\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Set the legal hold of an S3 object version: ON stops the version from being deleted or overwritten until the hold is removed, OFF removes the hold",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_object_legal_hold(
        &self,
        Parameters(req): Parameters<PutObjectLegalHoldRequest>,
    ) -> String {
        info!(
            "Executing put_object_legal_hold tool: {} on s3://{}/{}",
            req.status.as_str(),
            req.bucket_name,
            req.object_key
        );

        match self
            .s3_client
            .put_object_legal_hold(
                &req.bucket_name,
                &req.object_key,
                req.version_id.as_deref(),
                req.status,
            )
            .await
        {
            Ok(()) => {
                let mut result_text = format!(
                    "✅ **Object legal hold updated!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    req.bucket_name, req.object_key
                );

                if let Some(ref version_id) = req.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }
                result_text.push_str(&format!("**Legal Hold:** {}\n", req.status.as_str()));

                if req.status == crate::s3_client::ObjectLockLegalHoldStatus::On {
                    result_text.push_str(
                        "\n⚠️ **Note:** While the legal hold is on, this version cannot be deleted or overwritten. Remove it by calling this tool with status OFF.",
                    );
                } else {
                    result_text.push_str(
                        "\nℹ️ **Note:** The legal hold was removed; this version can now be deleted or overwritten, unless a retention period still applies to it.",
                    );
                }

                info!("put_object_legal_hold tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to set the legal hold of s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to set the legal hold of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • The bucket does not have Object Lock enabled\n\
                     • AWS credentials lack permission to write legal holds (s3:PutObjectLegalHold)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Verify the bucket has Object Lock enabled using get_bucket_lock_configuration\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Get/download an object from an S3 bucket - supports read mode for text files, download mode for all files, byte-range reads (range), and conditional reads (if_modified_since / if_unmodified_since)",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn get_object(&self, Parameters(req): Parameters<GetObjectRequest>) -> String {
        info!(
            "Executing get_object tool: s3://{}/{} (mode: {:?})",
            req.bucket_name, req.object_key, req.mode
        );

        match req.mode {
            GetObjectMode::Read => self.handle_read_mode(req).await,
            GetObjectMode::Download => self.handle_download_mode(req).await,
        }
    }

    #[tool(
        description = "Upload a local file to an S3 bucket",
        annotations(destructive_hint = true)
    )]
    pub async fn upload_file(&self, Parameters(req): Parameters<UploadFileRequest>) -> String {
        info!(
            "Executing upload_file tool: '{}' -> s3://{}/{}",
            req.local_file_path, req.bucket_name, req.object_key
        );

        let options = UploadFileOptions {
            content_type: req.content_type.clone(),
            storage_class: req.storage_class.clone(),
            cache_control: req.cache_control.clone(),
            metadata: req.metadata.clone(),
            server_side_encryption: req.server_side_encryption.clone(),
            content_disposition: req.content_disposition.clone(),
            content_encoding: req.content_encoding.clone(),
            content_language: req.content_language.clone(),
        };

        match self
            .s3_client
            .upload_file(
                &req.local_file_path,
                &req.bucket_name,
                &req.object_key,
                options,
            )
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully uploaded file '{}' to s3://{}/{} ({} bytes)",
                    req.local_file_path, req.bucket_name, req.object_key, result.file_size
                );

                let mut result_text = format!(
                    "✅ **File uploaded successfully!**\n\n\
                     **Local File:** {}\n\
                     **S3 Location:** s3://{}/{}\n\
                     **File Size:** {} bytes ({:.2} MB)\n\
                     **Content Type:** {}\n\
                     **ETag:** {}\n",
                    req.local_file_path,
                    result.bucket,
                    result.key,
                    result.file_size,
                    result.file_size as f64 / 1_048_576.0,
                    result.content_type,
                    result.etag
                );

                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                result_text.push_str("\n---\n");
                result_text.push_str("**Upload Summary:**\n");
                result_text.push_str(&format!("• Source: {}\n", req.local_file_path));
                result_text.push_str(&format!("• Destination: {}\n", result.location));
                result_text.push_str(&format!("• Size: {} bytes\n", result.file_size));
                result_text.push_str(&format!("• Type: {}\n", result.content_type));

                if result.file_size > 5 * 1024 * 1024 {
                    result_text.push_str("\n💡 **Note:** Large file uploaded successfully. For files larger than 100MB, consider the multipart workflow instead: initiate_multipart_upload → upload_part → complete_multipart_upload.");
                }

                info!(
                    "upload_file tool executed successfully: {} bytes uploaded to s3://{}/{}",
                    result.file_size, req.bucket_name, req.object_key
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to upload file '{}' to s3://{}/{}: {:?}",
                    req.local_file_path, req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to upload file '{}' to S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Local file does not exist or is not readable\n\
                     • AWS credentials lack permissions to upload to this bucket\n\
                     • S3 bucket does not exist or is not accessible\n\
                     • Network connectivity issues\n\
                     • File path contains invalid characters or is too long\n\
                     • Insufficient disk space or memory\n\
                     • Custom endpoint is misconfigured\n\
                     • File is locked by another process\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the local file exists and is readable\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Ensure the bucket name is correct and accessible\n\
                     4. Try with a smaller file to test connectivity\n\
                     5. Check the file path for special characters\n\n\
                     **File:** {}\n\
                     **Bucket:** {}\n\
                     **Object Key:** {}",
                    req.local_file_path,
                    req.bucket_name,
                    e,
                    req.local_file_path,
                    req.bucket_name,
                    req.object_key
                )
            }
        }
    }

    #[tool(
        description = "Upload inline text content as an S3 object without a local file - creates or replaces the object; suited for notes, JSON, and code up to 5 MB (use upload_file for larger files or binary data)",
        annotations(destructive_hint = true)
    )]
    pub async fn upload_content(
        &self,
        Parameters(req): Parameters<UploadContentRequest>,
    ) -> String {
        info!(
            "Executing upload_content tool: {} bytes -> s3://{}/{}",
            req.content.len(),
            req.bucket_name,
            req.object_key
        );

        let options = UploadFileOptions {
            content_type: req.content_type.clone(),
            storage_class: req.storage_class.clone(),
            cache_control: req.cache_control.clone(),
            metadata: req.metadata.clone(),
            server_side_encryption: req.server_side_encryption.clone(),
            content_disposition: req.content_disposition.clone(),
            content_encoding: req.content_encoding.clone(),
            content_language: req.content_language.clone(),
        };

        match self
            .s3_client
            .upload_content(&req.content, &req.bucket_name, &req.object_key, options)
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully uploaded {} bytes of content to s3://{}/{}",
                    result.file_size, result.bucket, result.key
                );

                let mut result_text = format!(
                    "✅ **Content uploaded successfully!**\n\n\
                     **S3 Location:** {}\n\
                     **Size:** {} bytes\n\
                     **Content Type:** {}\n\
                     **ETag:** {}\n",
                    result.location, result.file_size, result.content_type, result.etag
                );

                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                result_text.push_str("\n---\n");
                result_text.push_str("**Upload Summary:**\n");
                result_text.push_str(&format!("• Destination: {}\n", result.location));
                result_text.push_str(&format!("• Size: {} bytes\n", result.file_size));
                result_text.push_str(&format!("• Type: {}\n", result.content_type));

                info!(
                    "upload_content tool executed successfully: {} bytes uploaded to s3://{}/{}",
                    result.file_size, req.bucket_name, req.object_key
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to upload content to s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to upload content to S3 bucket '{}'**\n\n\
                     **Object Key:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Content exceeds the 5 MB limit for inline uploads\n\
                     • AWS credentials lack permissions to upload to this bucket\n\
                     • S3 bucket does not exist or is not accessible\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. For content larger than 5 MB, write it to a local file and use upload_file\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Ensure the bucket name is correct and accessible",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Permanently delete a single object from an S3 bucket (optionally a specific version). This operation cannot be undone",
        annotations(destructive_hint = true)
    )]
    pub async fn delete_object(&self, Parameters(req): Parameters<DeleteObjectRequest>) -> String {
        info!(
            "Executing delete_object tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .delete_object(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(_) => {
                let mut result_text = format!(
                    "✅ **Object deleted successfully!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    req.bucket_name, req.object_key
                );

                if let Some(ref version_id) = req.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                result_text
                    .push_str("\n⚠️ **Note:** This operation is permanent and cannot be undone.");

                info!("delete_object tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to delete object s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to delete object from S3 bucket '{}'**\n\n\
                     **Object Key:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object does not exist in the specified bucket\n\
                     • AWS credentials lack permissions to delete this object\n\
                     • Bucket does not exist or is not accessible\n\
                     • Version ID is invalid (for versioned objects)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using list_objects or head_object\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Ensure the bucket name and object key are correct\n\
                     4. Check if the bucket has versioning enabled",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Permanently delete multiple objects (up to 1000) from an S3 bucket in a single batch request, reporting per-key failures. This operation cannot be undone",
        annotations(destructive_hint = true)
    )]
    pub async fn delete_objects(
        &self,
        Parameters(req): Parameters<DeleteObjectsRequest>,
    ) -> String {
        info!(
            "Executing delete_objects tool for {} object(s) in bucket: {}",
            req.object_keys.len(),
            req.bucket_name
        );

        if req.object_keys.is_empty() {
            return "No object keys provided. Please specify at least one object key to delete."
                .to_string();
        }

        if req.object_keys.len() > 1000 {
            return format!(
                "❌ Too many object keys ({}). The S3 DeleteObjects API allows at most \
                 1000 keys per request; split the list into smaller batches.",
                req.object_keys.len()
            );
        }

        match self
            .s3_client
            .delete_objects(&req.bucket_name, &req.object_keys)
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Batch delete completed!**\n\n\
                     **Bucket:** {}\n\
                     **Deleted:** {} object(s)\n\
                     **Failed:** {} object(s)\n",
                    result.bucket,
                    result.deleted.len(),
                    result.errors.len()
                );

                if !result.deleted.is_empty() {
                    result_text.push_str("\n**Deleted objects:**\n");
                    for key in &result.deleted {
                        result_text.push_str(&format!("• {key}\n"));
                    }
                }

                if !result.errors.is_empty() {
                    result_text.push_str("\n**Failed objects:**\n");
                    for err in &result.errors {
                        result_text.push_str(&format!("• `{}`: {}\n", err.key, err.message));
                    }
                }

                result_text
                    .push_str("\n⚠️ **Note:** This operation is permanent and cannot be undone.");

                info!(
                    "delete_objects tool executed successfully: {} deleted, {} errors",
                    result.deleted.len(),
                    result.errors.len()
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to batch delete objects in bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to batch delete objects from S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • AWS credentials lack permissions to delete these objects\n\
                     • Bucket does not exist or is not accessible\n\
                     • More than 1000 object keys were supplied\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the objects exist using list_objects\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Reduce the batch size to 1000 keys or fewer\n\
                     4. Retry — batch deletes are not atomic and partial failures are reported per key",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the access control list (ACL) of an S3 bucket: its owner and the permissions granted to accounts, everyone, or authenticated users",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_acl(&self, Parameters(req): Parameters<GetBucketAclRequest>) -> String {
        info!(
            "Executing get_bucket_acl tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_acl(&req.bucket_name).await {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Bucket ACL retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Owner:** {}\n\
                     **Grants:**\n{}\n",
                    result.bucket,
                    acl_owner_line(&result.policy),
                    acl_grant_lines(&result.policy)
                );

                if !result.policy.grantees_are_identified() {
                    result_text.push_str(ACL_UNREPORTED_NOTE);
                }

                info!("get_bucket_acl tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the ACL of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the ACL of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket ACL (s3:GetBucketAcl)\n\
                     • The storage backend does not support ACLs\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Apply a canned ACL to an S3 bucket: 'private' (only the owner has access), 'public-read' (everyone may read), 'public-read-write' (everyone may read and write), or 'authenticated-read' (any authenticated account may read). Replaces the whole ACL and reads it back to confirm that the storage backend applied it",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_acl(&self, Parameters(req): Parameters<PutBucketAclRequest>) -> String {
        info!(
            "Executing put_bucket_acl tool for bucket '{}': {}",
            req.bucket_name,
            req.acl.as_str()
        );

        match self
            .s3_client
            .put_bucket_acl(&req.bucket_name, req.acl)
            .await
        {
            Ok(result) => acl_update_text(
                result,
                &format!("**Bucket:** {}", req.bucket_name),
                "put_bucket_policy",
            ),
            Err(e) => {
                error!(
                    "Failed to set the ACL of bucket '{}' to '{}': {:?}",
                    req.bucket_name,
                    req.acl.as_str(),
                    e
                );

                format!(
                    "❌ **Failed to set the ACL of S3 bucket '{}'**\n\n\
                     **Requested ACL:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • The ACL is not defined for buckets: bucket-owner-read and bucket-owner-full-control apply to objects only\n\
                     • The storage backend does not support ACLs (it answers NotImplemented)\n\
                     • Access to the bucket is managed by its bucket policy or public access block settings instead of ACLs\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to write the bucket ACL (s3:PutBucketAcl)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. For access that a canned ACL cannot express, set a bucket policy with put_bucket_policy\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name,
                    req.acl.as_str(),
                    e
                )
            }
        }
    }

    #[tool(
        description = "Get the access control list (ACL) of an S3 object version: its owner and the permissions granted to accounts, everyone, or authenticated users",
        annotations(read_only_hint = true)
    )]
    pub async fn get_object_acl(&self, Parameters(req): Parameters<GetObjectAclRequest>) -> String {
        info!(
            "Executing get_object_acl tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .get_object_acl(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Object ACL retrieved!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    result.bucket, result.key
                );

                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }
                result_text.push_str(&format!(
                    "**Owner:** {}\n\
                     **Grants:**\n{}\n",
                    acl_owner_line(&result.policy),
                    acl_grant_lines(&result.policy)
                ));

                if !result.policy.grantees_are_identified() {
                    result_text.push_str(ACL_UNREPORTED_NOTE);
                }

                info!("get_object_acl tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the ACL of s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to get the ACL of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • AWS credentials lack permission to read the object ACL (s3:GetObjectAcl)\n\
                     • The storage backend does not support ACLs\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Apply a canned ACL to an S3 object version: 'private' (only the owner has access), 'public-read' (everyone may read), 'public-read-write' (everyone may read and write), 'authenticated-read' (any authenticated account may read), 'bucket-owner-read', or 'bucket-owner-full-control'. Replaces the whole ACL and reads it back to confirm that the storage backend applied it",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_object_acl(&self, Parameters(req): Parameters<PutObjectAclRequest>) -> String {
        info!(
            "Executing put_object_acl tool: {} on s3://{}/{}",
            req.acl.as_str(),
            req.bucket_name,
            req.object_key
        );

        let target = format!(
            "**S3 Location:** s3://{}/{}\n{}",
            req.bucket_name,
            req.object_key,
            req.version_id
                .as_ref()
                .map(|version_id| format!("**Version ID:** {version_id}\n"))
                .unwrap_or_default()
        );

        match self
            .s3_client
            .put_object_acl(
                &req.bucket_name,
                &req.object_key,
                req.version_id.as_deref(),
                req.acl,
            )
            .await
        {
            Ok(result) => acl_update_text(result, &target, "put_bucket_policy"),
            Err(e) => {
                error!(
                    "Failed to set the ACL of s3://{}/{} to '{}': {:?}",
                    req.bucket_name,
                    req.object_key,
                    req.acl.as_str(),
                    e
                );

                format!(
                    "❌ **Failed to set the ACL of object s3://{}/{}**\n\n\
                     **Requested ACL:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • The storage backend does not support ACLs (it answers NotImplemented)\n\
                     • Access to the bucket is managed by its bucket policy or public access block settings instead of ACLs\n\
                     • AWS credentials lack permission to write the object ACL (s3:PutObjectAcl)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. For access that a canned ACL cannot express, set a bucket policy with put_bucket_policy\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name,
                    req.object_key,
                    req.acl.as_str(),
                    e
                )
            }
        }
    }

    #[tool(
        description = "Copy an object server-side within the same bucket or across buckets. Content type and cache control overrides are applied on top of the source object's preserved headers and user metadata; storage class can be changed without affecting metadata",
        annotations(destructive_hint = true)
    )]
    pub async fn copy_object(&self, Parameters(req): Parameters<CopyObjectRequest>) -> String {
        info!(
            "Executing copy_object tool: s3://{}/{} -> s3://{}/{}",
            req.source_bucket, req.source_key, req.destination_bucket, req.destination_key
        );

        let options = CopyObjectOptions {
            content_type: req.content_type.clone(),
            storage_class: req.storage_class.clone(),
            cache_control: req.cache_control.clone(),
            source_version_id: req.source_version_id.clone(),
        };

        match self
            .s3_client
            .copy_object(
                &req.source_bucket,
                &req.source_key,
                &req.destination_bucket,
                &req.destination_key,
                options,
            )
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Object copied successfully!**\n\n\
                     **Source:** s3://{}/{}\n\
                     **Destination:** s3://{}/{}\n",
                    result.source_bucket,
                    result.source_key,
                    result.destination_bucket,
                    result.destination_key
                );

                if let Some(ref version_id) = req.source_version_id {
                    result_text.push_str(&format!("**Source Version ID:** {version_id}\n"));
                }
                if let Some(ref etag) = result.etag {
                    result_text.push_str(&format!("**ETag:** {etag}\n"));
                }
                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }
                if let Some(ref content_type) = result.content_type {
                    result_text.push_str(&format!("**Content Type:** {content_type}\n"));
                }
                if let Some(ref storage_class) = result.storage_class {
                    result_text.push_str(&format!("**Storage Class:** {storage_class}\n"));
                }

                info!("copy_object tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to copy s3://{}/{} to s3://{}/{}: {:?}",
                    req.source_bucket,
                    req.source_key,
                    req.destination_bucket,
                    req.destination_key,
                    e
                );

                format!(
                    "❌ **Failed to copy object**\n\n\
                     **Source:** s3://{}/{}\n\
                     **Destination:** s3://{}/{}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Source object (or the requested source version) does not exist\n\
                     • AWS credentials lack permissions to read the source or write the destination\n\
                     • Destination bucket does not exist or is not accessible\n\
                     • Source and destination are in different regions (not supported by the storage backend)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the source object exists using head_object\n\
                     2. Check your AWS credentials and permissions for both buckets\n\
                     3. Ensure the destination bucket exists and is writable\n\
                     4. Verify the source and destination are served by the same region/endpoint",
                    req.source_bucket,
                    req.source_key,
                    req.destination_bucket,
                    req.destination_key,
                    e
                )
            }
        }
    }

    #[tool(
        description = "Retrieve metadata (size, ETag, content type, storage class, version, user metadata) for an S3 object without downloading its content",
        annotations(read_only_hint = true)
    )]
    pub async fn head_object(&self, Parameters(req): Parameters<HeadObjectRequest>) -> String {
        info!(
            "Executing head_object tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .head_object(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Object metadata retrieved!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    result.bucket, result.key
                );

                if let Some(size) = result.size {
                    result_text.push_str(&format!(
                        "**Size:** {size} bytes ({:.2} MB)\n",
                        size as f64 / 1_048_576.0
                    ));
                }
                if let Some(ref last_modified) = result.last_modified {
                    result_text.push_str(&format!("**Last Modified:** {last_modified}\n"));
                }
                if let Some(ref etag) = result.etag {
                    result_text.push_str(&format!("**ETag:** {etag}\n"));
                }
                if let Some(ref content_type) = result.content_type {
                    result_text.push_str(&format!("**Content Type:** {content_type}\n"));
                }
                if let Some(ref cache_control) = result.cache_control {
                    result_text.push_str(&format!("**Cache Control:** {cache_control}\n"));
                }
                if let Some(ref content_encoding) = result.content_encoding {
                    result_text.push_str(&format!("**Content Encoding:** {content_encoding}\n"));
                }
                if let Some(ref content_disposition) = result.content_disposition {
                    result_text
                        .push_str(&format!("**Content Disposition:** {content_disposition}\n"));
                }
                if let Some(ref content_language) = result.content_language {
                    result_text.push_str(&format!("**Content Language:** {content_language}\n"));
                }
                if let Some(ref storage_class) = result.storage_class {
                    result_text.push_str(&format!("**Storage Class:** {storage_class}\n"));
                }
                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                // Object Lock settings belong to the version, so they are
                // only reported when the storage backend sent them along.
                if let Some(ref mode) = result.object_lock_mode {
                    result_text.push_str(&format!("**Object Lock Retention:** {}", mode.as_str()));
                    if let Some(ref retain_until) = result.object_lock_retain_until {
                        result_text.push_str(&format!(" until {retain_until}"));
                    }
                    result_text.push('\n');
                }
                if let Some(ref status) = result.object_lock_legal_hold {
                    result_text.push_str(&format!("**Legal Hold:** {}\n", status.as_str()));
                }

                if !result.user_metadata.is_empty() {
                    result_text.push_str("\n**User Metadata:**\n");
                    for (key, value) in &result.user_metadata {
                        result_text.push_str(&format!("• {key}: {value}\n"));
                    }
                }

                info!("head_object tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to fetch metadata for s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to fetch object metadata from S3 bucket '{}'**\n\n\
                     **Object Key:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object does not exist in the specified bucket\n\
                     • AWS credentials lack permissions to read this object's metadata\n\
                     • Bucket does not exist or is not accessible\n\
                     • Version ID is invalid (for versioned objects)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using list_objects\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Ensure the bucket name and object key are correct\n\
                     4. Check if the bucket has versioning enabled",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Get the attributes of an S3 object version without downloading its content: ETag, the checksums it was stored with, the parts it was assembled from, storage class, and size",
        annotations(read_only_hint = true)
    )]
    pub async fn get_object_attributes(
        &self,
        Parameters(req): Parameters<GetObjectAttributesRequest>,
    ) -> String {
        info!(
            "Executing get_object_attributes tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .get_object_attributes(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Object attributes retrieved!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    result.bucket, result.key
                );

                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }
                if let Some(size) = result.size {
                    result_text.push_str(&format!(
                        "**Size:** {size} bytes ({:.2} MB)\n",
                        size as f64 / 1_048_576.0
                    ));
                }
                if let Some(ref last_modified) = result.last_modified {
                    result_text.push_str(&format!("**Last Modified:** {last_modified}\n"));
                }
                if let Some(ref etag) = result.etag {
                    result_text.push_str(&format!("**ETag:** {etag}\n"));
                }
                if let Some(ref storage_class) = result.storage_class {
                    result_text.push_str(&format!("**Storage Class:** {storage_class}\n"));
                }

                match result.checksums.as_slice() {
                    [] => {
                        result_text.push_str("**Checksum:** none recorded by the storage backend\n")
                    }
                    checksums => {
                        // A COMPOSITE checksum is one object-level value
                        // computed from the per-part checksums; a
                        // FULL_OBJECT checksum covers the object directly.
                        let scope = match result.checksum_type.as_deref() {
                            Some("COMPOSITE") => {
                                " (composite of the parts; validates the whole object)"
                            }
                            Some("FULL_OBJECT") => " (covers the whole object)",
                            _ => "",
                        };
                        result_text.push_str("**Checksums:**\n");
                        for checksum in checksums {
                            result_text.push_str(&format!(
                                "• {}: `{}`{scope}\n",
                                checksum.algorithm, checksum.value
                            ));
                        }
                    }
                }

                if result.parts.is_empty() {
                    // General-purpose S3 buckets report no parts for
                    // multipart objects without an additional checksum, so
                    // an empty list says nothing about the upload history.
                    result_text.push_str("**Parts:** none reported by the storage backend\n");
                } else {
                    result_text.push_str(&format!("**Parts:** {}", result.parts.len()));
                    if let Some(total) = result.total_parts_count {
                        result_text.push_str(&format!(" (total: {total})"));
                    }
                    result_text.push_str(":\n");
                    for part in &result.parts {
                        let number = part.part_number.map_or("-".to_string(), |n| n.to_string());
                        let size = part.size.map_or("size not reported".to_string(), |size| {
                            format!("{size} bytes")
                        });
                        result_text.push_str(&format!("• Part {number}: {size}\n"));
                    }
                }

                if result.delete_marker {
                    result_text.push_str(
                        "\nℹ️ **Note:** The current version of this key is a delete marker; pass a version_id to read the attributes of an earlier version.\n",
                    );
                }

                info!("get_object_attributes tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the attributes of s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to get the attributes of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • The storage backend does not support reading object attributes\n\
                     • AWS credentials lack permission to read object attributes: without a version ID S3 requires s3:GetObject together with s3:GetObjectAttributes, and with a version ID it requires s3:GetObjectVersion together with s3:GetObjectVersionAttributes\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Get the tags (key-value pairs) of an S3 object, or of a specific version of it",
        annotations(read_only_hint = true)
    )]
    pub async fn get_object_tagging(
        &self,
        Parameters(req): Parameters<GetObjectTaggingRequest>,
    ) -> String {
        info!(
            "Executing get_object_tagging tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .get_object_tagging(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Object tags retrieved!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    result.bucket, result.key
                );

                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                if result.tags.is_empty() {
                    result_text.push_str("\nℹ️ **Note:** This object has no tags.\n");
                } else {
                    result_text.push_str("\n**Tags:**\n");
                    for (key, value) in &result.tags {
                        result_text.push_str(&format!("• {key}: {value}\n"));
                    }
                }

                info!("get_object_tagging tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get tags for s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to get the tags of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • AWS credentials lack permission to read object tags\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Set the tags (key-value pairs) of an S3 object, or of a specific version of it. The given tags replace every existing tag of that object version",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_object_tagging(
        &self,
        Parameters(req): Parameters<PutObjectTaggingRequest>,
    ) -> String {
        info!(
            "Executing put_object_tagging tool: {} tag(s) on s3://{}/{}",
            req.tags.len(),
            req.bucket_name,
            req.object_key
        );

        match self
            .s3_client
            .put_object_tagging(
                &req.bucket_name,
                &req.object_key,
                req.version_id.as_deref(),
                &req.tags,
            )
            .await
        {
            Ok(()) => {
                let mut result_text = format!(
                    "✅ **Object tags updated!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    req.bucket_name, req.object_key
                );

                if let Some(ref version_id) = req.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                result_text.push_str("\n**Tags:**\n");
                for (key, value) in sorted_tags(&req.tags) {
                    result_text.push_str(&format!("• {key}: {value}\n"));
                }

                result_text.push_str(
                    "\n⚠️ **Note:** The previous tag set was replaced. Tags belong to a single object version; other versions keep their own tags.",
                );

                info!("put_object_tagging tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to set tags on s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to set the tags of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • AWS credentials lack permission to write object tags\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Remove every tag from an S3 object, or from a specific version of it",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_object_tagging(
        &self,
        Parameters(req): Parameters<DeleteObjectTaggingRequest>,
    ) -> String {
        info!(
            "Executing delete_object_tagging tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .delete_object_tagging(&req.bucket_name, &req.object_key, req.version_id.as_deref())
            .await
        {
            Ok(()) => {
                let mut result_text = format!(
                    "✅ **Object tags removed!**\n\n\
                     **S3 Location:** s3://{}/{}\n",
                    req.bucket_name, req.object_key
                );

                if let Some(ref version_id) = req.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                info!("delete_object_tagging tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to remove tags from s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to remove the tags of object s3://{}/{}**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Object (or the requested version) does not exist\n\
                     • AWS credentials lack permission to write object tags\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using head_object\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Get the tags (key-value pairs) of an S3 bucket",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_tagging(
        &self,
        Parameters(req): Parameters<GetBucketTaggingRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_tagging tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_tagging(&req.bucket_name).await {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Bucket tags retrieved!**\n\n\
                     **Bucket:** {}\n",
                    result.bucket
                );

                if result.tags.is_empty() {
                    result_text.push_str("\nℹ️ **Note:** This bucket has no tags.\n");
                } else {
                    result_text.push_str("\n**Tags:**\n");
                    for (key, value) in &result.tags {
                        result_text.push_str(&format!("• {key}: {value}\n"));
                    }
                }

                info!("get_bucket_tagging tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get tags for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the tags of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read bucket tags\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set the tags (key-value pairs) of an S3 bucket. The given tags replace every existing tag of the bucket",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_tagging(
        &self,
        Parameters(req): Parameters<PutBucketTaggingRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_tagging tool: {} tag(s) on bucket '{}'",
            req.tags.len(),
            req.bucket_name
        );

        match self
            .s3_client
            .put_bucket_tagging(&req.bucket_name, &req.tags)
            .await
        {
            Ok(()) => {
                let mut result_text = format!(
                    "✅ **Bucket tags updated!**\n\n\
                     **Bucket:** {}\n",
                    req.bucket_name
                );

                result_text.push_str("\n**Tags:**\n");
                for (key, value) in sorted_tags(&req.tags) {
                    result_text.push_str(&format!("• {key}: {value}\n"));
                }

                result_text.push_str("\n⚠️ **Note:** The previous tag set was replaced.");

                info!("put_bucket_tagging tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to set tags on bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to set the tags of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to write bucket tags\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Remove every tag from an S3 bucket",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_tagging(
        &self,
        Parameters(req): Parameters<DeleteBucketTaggingRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_tagging tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.delete_bucket_tagging(&req.bucket_name).await {
            Ok(()) => {
                info!("delete_bucket_tagging tool executed successfully");
                format!(
                    "✅ **Bucket tags removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** Every tag was removed from this bucket.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to remove tags from bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the tags of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to write bucket tags\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Generate a presigned URL for an S3 object: 'get' URLs grant temporary download access, 'put' URLs grant temporary upload access, without embedding AWS credentials in the client",
        annotations(destructive_hint = false, idempotent_hint = true)
    )]
    pub async fn presigned_url(&self, Parameters(req): Parameters<PresignUrlRequest>) -> String {
        // Clamp to the SigV4 limits as documented in the schema
        // (1-604800 seconds, 7-day maximum).
        let expires_in_secs = req.expires_in_secs.clamp(1, 604800);

        info!(
            "Executing presigned_url tool: {:?} s3://{}/{} (expires in {}s)",
            req.method, req.bucket_name, req.object_key, expires_in_secs
        );

        match self
            .s3_client
            .presigned_url(
                &req.bucket_name,
                &req.object_key,
                req.method,
                expires_in_secs,
            )
            .await
        {
            Ok(result) => {
                let expiry_hint = if result.expires_in_secs % 3600 == 0 {
                    format!("{} hour(s)", result.expires_in_secs / 3600)
                } else {
                    format!("{} minute(s)", result.expires_in_secs / 60)
                };

                let mut result_text = format!(
                    "✅ **Presigned URL generated!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Method:** {}\n\
                     **Valid For:** {} seconds (~{})\n\n\
                     **URL:**\n```\n{}\n```\n",
                    result.bucket,
                    result.key,
                    result.method,
                    result.expires_in_secs,
                    expiry_hint,
                    result.url
                );

                if req.expires_in_secs != expires_in_secs {
                    result_text.push_str(&format!(
                        "\n💡 **Note:** the requested expiry of {} seconds was clamped to the allowed range of 1-604800 seconds.",
                        req.expires_in_secs
                    ));
                }

                match req.method {
                    PresignMethod::Get => {
                        let suggested_name = result.key.rsplit('/').next().unwrap_or("download");
                        result_text.push_str(&format!(
                            "\n**Download example:**\n```\ncurl -o {} \"<url>\"\n```",
                            shell_single_quote(suggested_name)
                        ));
                    }
                    PresignMethod::Put => {
                        result_text.push_str(
                            "\n**Upload example:**\n```\ncurl -X PUT --upload-file '<local-file>' \"<url>\"\n```",
                        );
                    }
                }

                result_text.push_str(
                    "\n\n⚠️ **Note:** Anyone holding this URL can perform the operation until it expires — share it carefully.",
                );

                info!("presigned_url tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to generate presigned URL for s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to generate presigned URL for S3 bucket '{}'**\n\n\
                     **Object Key:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • AWS credentials are missing or invalid (presigning signs with the configured credentials)\n\
                     • The configured region does not match the bucket\n\
                     • expires_in_secs is outside the 1-604800 seconds range\n\
                     • Custom endpoint is misconfigured\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify your AWS credentials are set and valid\n\
                     2. Ensure the region and endpoint configuration are correct\n\
                     3. Use an expires_in_secs value between 1 and 604800",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Get the cross-origin resource sharing (CORS) rules of an S3 bucket: the origins, HTTP methods, and headers that browsers on other origins may use",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_cors(
        &self,
        Parameters(req): Parameters<GetBucketCorsRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_cors tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_cors(&req.bucket_name).await {
            Ok(result) => {
                if result.rules.is_empty() {
                    // A bucket without a CORS configuration is in its default
                    // state, not in an error state; say so instead of failing.
                    info!(
                        "get_bucket_cors tool executed successfully: bucket '{}' has no CORS configuration",
                        req.bucket_name
                    );
                    return format!(
                        "✅ **Bucket CORS configuration retrieved!**\n\n\
                         **Bucket:** {}\n\n\
                         ℹ️ **Note:** No CORS configuration is set for this bucket; browser cross-origin requests from other origins are not allowed.",
                        result.bucket
                    );
                }

                let mut result_text = format!(
                    "✅ **Bucket CORS configuration retrieved!**\n\n\
                     **Bucket:** {}\n\n",
                    result.bucket
                );

                for (index, rule) in result.rules.iter().enumerate() {
                    let number = index + 1;
                    result_text.push_str(&format!("{number}. **Rule {number}**\n"));
                    if let Some(ref id) = rule.id {
                        result_text.push_str(&format!("   - ID: `{id}`\n"));
                    }
                    result_text.push_str(&format!(
                        "   - Allowed Methods: {}\n",
                        rule.allowed_methods.join(", ")
                    ));
                    result_text.push_str(&format!(
                        "   - Allowed Origins: {}\n",
                        rule.allowed_origins.join(", ")
                    ));
                    if !rule.allowed_headers.is_empty() {
                        result_text.push_str(&format!(
                            "   - Allowed Headers: {}\n",
                            rule.allowed_headers.join(", ")
                        ));
                    }
                    if !rule.expose_headers.is_empty() {
                        result_text.push_str(&format!(
                            "   - Expose Headers: {}\n",
                            rule.expose_headers.join(", ")
                        ));
                    }
                    if let Some(max_age_seconds) = rule.max_age_seconds {
                        result_text.push_str(&format!("   - Max Age: {max_age_seconds} seconds\n"));
                    }
                    result_text.push('\n');
                }

                info!(
                    "get_bucket_cors tool executed successfully for bucket '{}': {} rule(s)",
                    req.bucket_name,
                    result.rules.len()
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the CORS configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the CORS configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to read the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Replace the cross-origin resource sharing (CORS) configuration of an S3 bucket with the given rules, which decide the origins, HTTP methods, and headers browsers on other origins may use. The rules replace the whole configuration, so send every rule that should apply",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_cors(
        &self,
        Parameters(req): Parameters<PutBucketCorsRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_cors tool for bucket '{}' with {} rule(s)",
            req.bucket_name,
            req.rules.len()
        );

        match self
            .s3_client
            .put_bucket_cors(&req.bucket_name, &req.rules)
            .await
        {
            Ok(()) => {
                let mut result_text = format!(
                    "✅ **Bucket CORS configuration updated!**\n\n\
                     **Bucket:** {}\n\
                     **Rules:** {}\n\n\
                     **Rules applied:**\n",
                    req.bucket_name,
                    req.rules.len()
                );

                for (index, rule) in req.rules.iter().enumerate() {
                    result_text.push_str(&format!(
                        "• {}: methods {}; origins {}\n",
                        rule.label(index),
                        rule.allowed_methods.join(", "),
                        rule.allowed_origins.join(", ")
                    ));
                }

                info!(
                    "put_bucket_cors tool executed successfully for bucket '{}': {} rule(s) set",
                    req.bucket_name,
                    req.rules.len()
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to set the CORS configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to update the CORS configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • The rule list is empty (remove the configuration with delete_bucket_cors instead)\n\
                     • A rule has no allowed methods, an unsupported method (only GET, PUT, POST, DELETE, HEAD), or no allowed origins\n\
                     • A rule's max_age_seconds is outside the allowed range of 0-86400\n\
                     • AWS credentials lack permissions to change the bucket configuration\n\
                     • Bucket does not exist or is not accessible\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Correct the rule named in the error; each rule needs at least one allowed method and one allowed origin\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Verify the bucket exists using list_buckets",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Remove the cross-origin resource sharing (CORS) configuration of an S3 bucket: afterwards browsers on other origins are no longer allowed to make cross-origin requests to it",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_cors(
        &self,
        Parameters(req): Parameters<DeleteBucketCorsRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_cors tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.delete_bucket_cors(&req.bucket_name).await {
            Ok(()) => {
                info!("delete_bucket_cors tool executed successfully");

                format!(
                    "✅ **Bucket CORS configuration removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ⚠️ **Note:** Browser cross-origin requests to this bucket are no longer allowed; pages served from other origins cannot access it until put_bucket_cors sets a new configuration.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to delete the CORS configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the CORS configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to change the bucket configuration\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Get the notification configuration of an S3 bucket: which object events (created, removed, ...) are delivered to which Lambda functions, queues, or topics, optionally limited to keys with a given prefix or suffix. Returns a readable summary plus the rules as JSON, which can be edited and sent back through put_bucket_notification",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_notification(
        &self,
        Parameters(req): Parameters<GetBucketNotificationRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_notification tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .get_bucket_notification(&req.bucket_name)
            .await
        {
            Ok(result) => {
                if result.rules.is_empty() {
                    info!(
                        "get_bucket_notification tool executed successfully: bucket '{}' delivers no notifications",
                        req.bucket_name
                    );
                    return format!(
                        "✅ **Bucket notification configuration retrieved!**\n\n\
                         **Bucket:** {}\n\n\
                         ℹ️ **Note:** No notifications are configured for this bucket: object events are not delivered anywhere. Add rules with put_bucket_notification.",
                        result.bucket
                    );
                }

                let rules_json = match serde_json::to_string_pretty(&result.rules) {
                    Ok(json) => json,
                    Err(e) => {
                        error!(
                            "Failed to render the notification rules of bucket '{}': {:?}",
                            req.bucket_name, e
                        );
                        return format!(
                            "❌ **Failed to render the notification rules as JSON**\n\n\
                             **Error:** {e:#}"
                        );
                    }
                };

                let mut result_text = format!(
                    "✅ **Bucket notification configuration retrieved!**\n\n\
                     **Bucket:** {}\n\
                     **Notification Rules:** {}\n\n",
                    result.bucket,
                    result.rules.len()
                );

                for (index, rule) in result.rules.iter().enumerate() {
                    let number = index + 1;
                    result_text.push_str(&format!(
                        "{number}. **{}** → {}\n",
                        rule.label(index),
                        rule.destination.label()
                    ));
                    result_text.push_str(&format!("   - Target ARN: `{}`\n", rule.arn));
                    result_text.push_str(&format!("   - Events: {}\n", rule.events.join(", ")));
                    if let Some(ref prefix) = rule.prefix {
                        result_text.push_str(&format!("   - Key Prefix: {prefix}\n"));
                    }
                    if let Some(ref suffix) = rule.suffix {
                        result_text.push_str(&format!("   - Key Suffix: {suffix}\n"));
                    }
                    result_text.push('\n');
                }

                result_text.push_str(&format!(
                    "**Rules as JSON** (edit them and pass the result to put_bucket_notification):\n```json\n{rules_json}\n```\n"
                ));

                info!(
                    "get_bucket_notification tool executed successfully for bucket '{}': {} rule(s)",
                    req.bucket_name,
                    result.rules.len()
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to get the notification configuration for bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the notification configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration (s3:GetBucketNotification)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Replace the notification configuration of an S3 bucket with the given rules, which decide which object events are delivered to which destinations. The rules replace the whole configuration, so send every rule that should apply. Each rule needs a destination (lambda, queue, or topic), the ARN of the target, and at least one S3 event; an optional prefix and suffix limit it to matching keys",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_notification(
        &self,
        Parameters(req): Parameters<PutBucketNotificationRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_notification tool for bucket '{}' with {} rule(s)",
            req.bucket_name,
            req.rules.len()
        );

        match self
            .s3_client
            .put_bucket_notification(&req.bucket_name, &req.rules)
            .await
        {
            Ok(()) => {
                let mut result_text = format!(
                    "✅ **Bucket notification configuration updated!**\n\n\
                     **Bucket:** {}\n\
                     **Notification Rules:** {}\n\n\
                     **Rules applied:**\n",
                    req.bucket_name,
                    req.rules.len()
                );

                for (index, rule) in req.rules.iter().enumerate() {
                    result_text.push_str(&format!("• {}: {}\n", rule.label(index), rule.summary()));
                }

                result_text.push_str(
                    "\n⚠️ **Note:** The rules take effect immediately and replace the previous configuration. From now on every matching object event is sent to these destinations.",
                );

                info!(
                    "put_bucket_notification tool executed successfully for bucket '{}': {} rule(s) set",
                    req.bucket_name,
                    req.rules.len()
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to set the notification configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to update the notification configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • A rule is malformed: an ARN that does not start with arn:, no events, an event name that is not an S3 event, or a missing destination\n\
                     • The storage backend has no notification target configured yet (RustFS answers \"No notify targets configured\" until a webhook, Kafka, or other target is defined in its environment)\n\
                     • An ARN does not name a target of the storage backend: RustFS expects arn:rustfs:sqs::<id>:<name>, AWS the ARN of a Lambda function, SQS queue, or SNS topic\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to write the notification configuration (s3:PutBucketNotification)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Read the current rules with get_bucket_notification and compare them\n\
                     2. Check every rule: a destination, an ARN, and at least one event such as s3:ObjectCreated:*\n\
                     3. On RustFS, make sure a notification target is configured and use its ARN (arn:rustfs:sqs::<id>:<name>)\n\
                     4. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Remove every notification rule from an S3 bucket; object events of the bucket are then no longer delivered anywhere",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_notification(
        &self,
        Parameters(req): Parameters<DeleteBucketNotificationRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_notification tool for bucket: {}",
            req.bucket_name
        );

        match self
            .s3_client
            .delete_bucket_notification(&req.bucket_name)
            .await
        {
            Ok(()) => {
                info!(
                    "delete_bucket_notification tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket notification configuration removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** Object events of this bucket are no longer delivered anywhere. Notifications can be set again with put_bucket_notification.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to remove the notification configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the notification configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • The storage backend has no notification target configured yet (RustFS rejects every notification request, including an empty one, until a target is defined in its environment)\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to write the notification configuration (s3:PutBucketNotification)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Initiate a multipart upload for a large file and return an upload ID used by upload_part, list_parts, complete_multipart_upload, and abort_multipart_upload",
        annotations(destructive_hint = false)
    )]
    pub async fn initiate_multipart_upload(
        &self,
        Parameters(req): Parameters<InitiateMultipartUploadRequest>,
    ) -> String {
        info!(
            "Executing initiate_multipart_upload tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        let options = UploadFileOptions {
            content_type: req.content_type.clone(),
            storage_class: req.storage_class.clone(),
            cache_control: req.cache_control.clone(),
            metadata: req.metadata.clone(),
            ..UploadFileOptions::default()
        };

        match self
            .s3_client
            .initiate_multipart_upload(&req.bucket_name, &req.object_key, options)
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Multipart upload initiated!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Upload ID:** `{}`\n\n\
                     **Next steps:**\n\
                     1. Split the large file into part files (5 MB minimum per part, except the last)\n\
                     2. Call upload_part for each part with this upload ID and a part number (1-10000)\n\
                     3. Record each part's ETag\n\
                     4. Call complete_multipart_upload with all collected parts",
                    result.bucket, result.key, result.upload_id
                );

                if let Some(ref ct) = req.content_type {
                    result_text.push_str(&format!("\n\n**Content Type:** {ct}"));
                }

                info!("initiate_multipart_upload tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to initiate multipart upload for s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to initiate multipart upload for S3 bucket '{}'**\n\n\
                     **Object Key:** {}\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • AWS credentials lack permissions to write to this bucket\n\
                     • Bucket does not exist or is not accessible\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Check your AWS credentials and permissions\n\
                     2. Ensure the bucket name and object key are correct\n\
                     3. Verify the destination bucket is writable",
                    req.bucket_name, req.object_key, e
                )
            }
        }
    }

    #[tool(
        description = "Upload a local file as a single multipart part (part numbers 1-10000); record the returned ETag for complete_multipart_upload",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn upload_part(&self, Parameters(req): Parameters<UploadPartRequest>) -> String {
        info!(
            "Executing upload_part tool: part {} from '{}' -> s3://{}/{}",
            req.part_number, req.local_file_path, req.bucket_name, req.object_key
        );

        if !(1..=10000).contains(&req.part_number) {
            return format!(
                "❌ **Invalid part_number: {}**\n\n\
                 Part numbers must be between 1 and 10000.",
                req.part_number
            );
        }

        match self
            .s3_client
            .upload_part_from_file(
                &req.local_file_path,
                &req.bucket_name,
                &req.object_key,
                &req.upload_id,
                req.part_number,
            )
            .await
        {
            Ok(result) => {
                format!(
                    "✅ **Part {} uploaded successfully!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Part Number:** {}\n\
                     **Part Size:** {} bytes ({:.2} MB)\n\
                     **Part ETag:** `{}`\n\n\
                     💡 **Record this ETag** — you need it (paired with part number {}) for complete_multipart_upload.\n\
                     You can also call list_parts at any time to see all uploaded parts.",
                    result.part_number,
                    req.bucket_name,
                    req.object_key,
                    result.part_number,
                    result.size,
                    result.size as f64 / 1_048_576.0,
                    result.etag,
                    result.part_number
                )
            }
            Err(e) => {
                error!(
                    "Failed to upload part {} for s3://{}/{}: {:?}",
                    req.part_number, req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to upload part {} to S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Upload ID is invalid, expired, or was aborted\n\
                     • Local file does not exist or is not readable\n\
                     • Part number is out of the 1-10000 range\n\
                     • Parts must be at least 5 MB each (except the last part)\n\
                     • AWS credentials lack permissions to upload\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the upload ID with list_parts\n\
                     2. Verify the local file exists and is readable\n\
                     3. Ensure each part (except the last) is at least 5 MB\n\
                     4. Check your AWS credentials and permissions",
                    req.part_number, req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Complete a multipart upload by assembling the uploaded parts (part number + ETag pairs); parts are sorted by part number automatically",
        annotations(destructive_hint = true)
    )]
    pub async fn complete_multipart_upload(
        &self,
        Parameters(req): Parameters<CompleteMultipartUploadRequest>,
    ) -> String {
        info!(
            "Executing complete_multipart_upload tool: {} part(s) for s3://{}/{}",
            req.parts.len(),
            req.bucket_name,
            req.object_key
        );

        if req.parts.is_empty() {
            return "❌ No parts provided. Collect part ETags with upload_part or list_parts first."
                .to_string();
        }

        let parts = req
            .parts
            .iter()
            .map(|part| (part.part_number, part.etag.clone()))
            .collect();

        match self
            .s3_client
            .complete_multipart_upload(&req.bucket_name, &req.object_key, &req.upload_id, parts)
            .await
        {
            Ok(result) => {
                let mut result_text = format!(
                    "✅ **Multipart upload completed!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Parts Assembled:** {}\n",
                    result.bucket,
                    result.key,
                    req.parts.len()
                );

                if let Some(ref location) = result.location {
                    result_text.push_str(&format!("**Location:** {location}\n"));
                }
                if let Some(ref etag) = result.etag {
                    result_text.push_str(&format!("**ETag:** {etag}\n"));
                }
                if let Some(ref version_id) = result.version_id {
                    result_text.push_str(&format!("**Version ID:** {version_id}\n"));
                }

                info!("complete_multipart_upload tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to complete multipart upload for s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to complete multipart upload for S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Upload ID is invalid, expired, or was aborted\n\
                     • One or more part ETags are wrong or belong to a different part number\n\
                     • A part is missing from the list\n\
                     • Parts must be at least 5 MB each (except the last part)\n\n\
                     **Troubleshooting steps:**\n\
                     1. Call list_parts to verify the uploaded parts and their ETags\n\
                     2. Ensure every part number/ETag pair matches an uploaded part\n\
                     3. Ensure all parts except the last are at least 5 MB",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Abort a multipart upload and permanently discard all parts uploaded so far",
        annotations(destructive_hint = true)
    )]
    pub async fn abort_multipart_upload(
        &self,
        Parameters(req): Parameters<AbortMultipartUploadRequest>,
    ) -> String {
        info!(
            "Executing abort_multipart_upload tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .abort_multipart_upload(&req.bucket_name, &req.object_key, &req.upload_id)
            .await
        {
            Ok(_) => {
                format!(
                    "✅ **Multipart upload aborted!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Upload ID:** `{}`\n\n\
                     ⚠️ **Note:** All parts uploaded under this upload ID have been permanently discarded.",
                    req.bucket_name, req.object_key, req.upload_id
                )
            }
            Err(e) => {
                error!(
                    "Failed to abort multipart upload for s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to abort multipart upload for S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Upload ID is invalid or was already completed or aborted\n\
                     • AWS credentials lack permissions for this operation\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the upload ID with list_parts\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "List the parts uploaded so far for a multipart upload, including each part's ETag for complete_multipart_upload",
        annotations(read_only_hint = true)
    )]
    pub async fn list_parts(&self, Parameters(req): Parameters<ListPartsRequest>) -> String {
        info!(
            "Executing list_parts tool: s3://{}/{}",
            req.bucket_name, req.object_key
        );

        match self
            .s3_client
            .list_parts(&req.bucket_name, &req.object_key, &req.upload_id)
            .await
        {
            Ok(parts) => {
                if parts.is_empty() {
                    return format!(
                        "No parts uploaded yet for upload ID `{}` on s3://{}/{}.",
                        req.upload_id, req.bucket_name, req.object_key
                    );
                }

                let mut result_text = format!(
                    "Found {} uploaded part(s) for s3://{}/{}:\n\n",
                    parts.len(),
                    req.bucket_name,
                    req.object_key
                );

                for part in &parts {
                    result_text.push_str(&format!("**Part {}**\n", part.part_number));
                    if let Some(ref etag) = part.etag {
                        result_text.push_str(&format!("   - ETag: {etag}\n"));
                    }
                    if let Some(size) = part.size {
                        result_text.push_str(&format!("   - Size: {size} bytes\n"));
                    }
                    if let Some(ref last_modified) = part.last_modified {
                        result_text.push_str(&format!("   - Last Modified: {last_modified}\n"));
                    }
                    result_text.push('\n');
                }

                result_text.push_str(
                    "💡 Use these part numbers and ETags with complete_multipart_upload to assemble the object.",
                );

                info!("list_parts tool executed successfully");
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to list parts for s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );

                format!(
                    "❌ **Failed to list parts for S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Upload ID is invalid, expired, or was aborted or completed\n\
                     • AWS credentials lack permissions for this operation\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the upload ID returned by initiate_multipart_upload\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "List the in-progress multipart uploads of a bucket with optional prefix filtering, directory-style browsing (delimiter), and pagination (max_uploads + key_marker / upload_id_marker). Each entry carries the upload ID used by upload_part, upload_part_copy, list_parts, complete_multipart_upload, and abort_multipart_upload; unfinished uploads keep consuming storage until they are completed or aborted",
        annotations(read_only_hint = true)
    )]
    pub async fn list_multipart_uploads(
        &self,
        Parameters(req): Parameters<ListMultipartUploadsRequest>,
    ) -> String {
        info!(
            "Executing list_multipart_uploads tool for bucket: {}",
            req.bucket_name
        );

        // The S3 ListMultipartUploads API accepts at most 1000 uploads per page.
        let max_uploads = req.max_uploads.map(|uploads| uploads.clamp(1, 1000));

        let options = crate::s3_client::ListMultipartUploadsOptions {
            prefix: req.prefix.clone(),
            delimiter: req.delimiter.clone(),
            max_uploads,
            key_marker: req.key_marker.clone(),
            upload_id_marker: req.upload_id_marker.clone(),
        };

        match self
            .s3_client
            .list_multipart_uploads(&req.bucket_name, options)
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully retrieved {} multipart upload(s) and {} common prefixes from bucket '{}'",
                    result.uploads.len(),
                    result.common_prefixes.len(),
                    req.bucket_name
                );

                if result.uploads.is_empty() && result.common_prefixes.is_empty() {
                    return format!(
                        "No in-progress multipart uploads found in bucket '{}'.",
                        req.bucket_name
                    );
                }

                let mut result_text = format!(
                    "Found {} in-progress multipart upload(s) in bucket **{}**",
                    result.uploads.len(),
                    req.bucket_name
                );

                if let Some(ref p) = req.prefix {
                    result_text.push_str(&format!(" with prefix '{p}'"));
                }
                result_text.push_str(":\n\n");

                if !result.common_prefixes.is_empty() {
                    result_text.push_str("**Directories:**\n");
                    for (index, prefix) in result.common_prefixes.iter().enumerate() {
                        result_text.push_str(&format!("{}. 📁 {prefix}\n", index + 1));
                    }
                    result_text.push('\n');
                }

                for (index, upload) in result.uploads.iter().enumerate() {
                    result_text.push_str(&format!("{}. **{}**\n", index + 1, upload.key));
                    result_text.push_str(&format!("   - Upload ID: `{}`\n", upload.upload_id));

                    if let Some(ref initiated) = upload.initiated {
                        result_text.push_str(&format!("   - Initiated: {initiated}\n"));
                    }

                    if let Some(ref storage_class) = upload.storage_class {
                        result_text.push_str(&format!("   - Storage Class: {storage_class}\n"));
                    }
                    result_text.push('\n');
                }

                if result.is_truncated {
                    result_text.push_str("**Note:** Results are truncated. ");
                    match (
                        result.next_key_marker.as_ref(),
                        result.next_upload_id_marker.as_ref(),
                    ) {
                        (Some(key_marker), Some(upload_id_marker)) => {
                            result_text.push_str(&format!(
                                "Call this tool again with key_marker=\"{key_marker}\" and upload_id_marker=\"{upload_id_marker}\" to fetch the next page.\n"
                            ));
                        }
                        (Some(key_marker), None) => {
                            result_text.push_str(&format!(
                                "Call this tool again with key_marker=\"{key_marker}\" to fetch the next page.\n"
                            ));
                        }
                        (None, Some(upload_id_marker)) => {
                            result_text.push_str(&format!(
                                "Call this tool again with upload_id_marker=\"{upload_id_marker}\" to fetch the next page.\n"
                            ));
                        }
                        (None, None) => result_text.push_str(
                            "Call this tool again with a later key_marker to fetch the next page.\n",
                        ),
                    }
                }

                result_text.push('\n');
                result_text.push_str(
                    "💡 **Note:** Unfinished multipart uploads keep consuming storage. Cancel the ones you no longer need with abort_multipart_upload.",
                );

                info!(
                    "list_multipart_uploads tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to list multipart uploads in bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to list multipart uploads for S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permissions to list multipart uploads in this bucket\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Upload one part of a multipart upload by copying an existing object server-side, without downloading it: either the whole source object or an inclusive byte range of it (range \"<start>-<end>\"). Assembles large objects from data already in S3. Each part except the last must be at least 5 MB",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn upload_part_copy(
        &self,
        Parameters(req): Parameters<UploadPartCopyRequest>,
    ) -> String {
        info!(
            "Executing upload_part_copy tool: part {} from s3://{}/{} -> s3://{}/{}",
            req.part_number, req.source_bucket, req.source_key, req.bucket_name, req.object_key
        );

        if !(1..=10000).contains(&req.part_number) {
            return format!(
                "❌ **Invalid part_number: {}**\n\n\
                 Part numbers must be between 1 and 10000.",
                req.part_number
            );
        }

        let range = match req.range.as_deref().map(crate::s3_client::CopyRange::parse) {
            Some(Ok(range)) => Some(range),
            Some(Err(e)) => {
                error!(
                    "Invalid range '{}' for part {} of s3://{}/{}",
                    req.range.clone().unwrap_or_default(),
                    req.part_number,
                    req.bucket_name,
                    req.object_key
                );

                return format!(
                    "❌ **Failed to copy part {} to S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • The range is not written as \"<start>-<end>\" (zero-based, inclusive byte offsets of the source object)\n\
                     • The start offset is greater than the end offset\n\n\
                     **Troubleshooting steps:**\n\
                     1. Pass range as \"<start>-<end>\", e.g. \"0-5242879\" for the first 5 MB of the source object\n\
                     2. Omit range to copy the whole source object as this part\n\
                     3. Check the source object's size with head_object to choose valid offsets",
                    req.part_number, req.bucket_name, e
                );
            }
            None => None,
        };

        match self
            .s3_client
            .upload_part_copy(
                &req.bucket_name,
                &req.object_key,
                &req.upload_id,
                req.part_number,
                &req.source_bucket,
                &req.source_key,
                req.source_version_id.as_deref(),
                range,
            )
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully copied part {} of s3://{}/{} from s3://{}/{} (ETag {})",
                    result.part_number,
                    req.bucket_name,
                    req.object_key,
                    req.source_bucket,
                    req.source_key,
                    result.etag
                );

                let mut result_text = format!(
                    "✅ **Part {} uploaded successfully (server-side copy)!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Part Number:** {}\n",
                    result.part_number, req.bucket_name, req.object_key, result.part_number
                );

                if let Some(size) = result.size {
                    result_text.push_str(&format!(
                        "**Part Size:** {size} bytes ({:.2} MB)\n",
                        size as f64 / 1_048_576.0
                    ));
                }

                if let Some(ref last_modified) = result.last_modified {
                    result_text.push_str(&format!("**Last Modified:** {last_modified}\n"));
                }

                result_text.push_str(&format!(
                    "**Copied From:** s3://{}/{}",
                    req.source_bucket, req.source_key
                ));
                if let Some(ref version_id) = req.source_version_id {
                    result_text.push_str(&format!(" (version {version_id})"));
                }
                result_text.push('\n');

                if let Some(range) = range {
                    result_text.push_str(&format!("**Source Range:** {}\n", range.to_header()));
                }

                result_text.push_str(&format!(
                    "**Part ETag:** `{}`\n\n\
                     💡 **Record this ETag** — you need it (paired with part number {}) for complete_multipart_upload.\n\
                     You can also call list_parts at any time to see all uploaded parts.",
                    result.etag, result.part_number
                ));

                info!(
                    "upload_part_copy tool executed successfully: part {} of s3://{}/{}",
                    result.part_number, req.bucket_name, req.object_key
                );
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to copy part {} for s3://{}/{} from s3://{}/{}: {:?}",
                    req.part_number,
                    req.bucket_name,
                    req.object_key,
                    req.source_bucket,
                    req.source_key,
                    e
                );

                format!(
                    "❌ **Failed to copy part {} to S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Upload ID is invalid, expired, or was aborted\n\
                     • Source object (or the requested source version) does not exist\n\
                     • The part is smaller than 5 MB but is not the last part (all other parts must be at least 5 MB)\n\
                     • The range lies outside the source object\n\
                     • Source bucket is in a different region (not supported by the storage backend)\n\
                     • AWS credentials lack permissions to read the source or upload to this bucket\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the upload ID with list_parts\n\
                     2. Verify the source object exists using head_object (pass its version ID if one was given)\n\
                     3. Keep every part except the last at 5 MB or more; smaller parts are rejected by complete_multipart_upload\n\
                     4. Ensure the source and destination buckets are served by the same region/endpoint\n\
                     5. Check your AWS credentials and permissions",
                    req.part_number, req.bucket_name, e
                )
            }
        }
    }
    #[tool(
        description = "Get the website configuration of an S3 bucket: the index document served for a request ending with '/', the error document, a redirect of all requests, and routing rules. Returns a readable summary plus the configuration as JSON, which can be edited and sent back through put_bucket_website",
        annotations(read_only_hint = true)
    )]
    pub async fn get_bucket_website(
        &self,
        Parameters(req): Parameters<GetBucketWebsiteRequest>,
    ) -> String {
        info!(
            "Executing get_bucket_website tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.get_bucket_website(&req.bucket_name).await {
            Ok(result) => {
                let Some(configuration) = result.configuration else {
                    info!(
                        "get_bucket_website tool executed successfully: bucket '{}' has no website configuration",
                        req.bucket_name
                    );
                    return format!(
                        "✅ **Bucket website configuration retrieved!**\n\n\
                         **Bucket:** {}\n\n\
                         ℹ️ **Note:** No website configuration is set for this bucket, so its objects are not served as a website. Set one with put_bucket_website.",
                        result.bucket
                    );
                };

                let json = match serde_json::to_string_pretty(&configuration) {
                    Ok(json) => json,
                    Err(e) => {
                        error!(
                            "Failed to render the website configuration of bucket '{}': {:?}",
                            req.bucket_name, e
                        );
                        return format!(
                            "❌ **Failed to render the website configuration as JSON**\n\n\
                             **Error:** {e:#}"
                        );
                    }
                };

                info!(
                    "get_bucket_website tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket website configuration retrieved!**\n\n\
                     **Bucket:** {}\n\
                     {}\n\
                     {}\n\
                     **Website configuration as JSON** (edit it and pass it to put_bucket_website):\n```json\n{json}\n```\n",
                    result.bucket,
                    configuration.summary(),
                    Self::website_routing_lines(&configuration.routing_rules)
                )
            }
            Err(e) => {
                error!(
                    "Failed to get the website configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to get the website configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to read the bucket configuration (s3:GetBucketWebsite)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Set the website configuration of an S3 bucket, replacing the current one: the index document served for a request ending with '/', an optional error document, and optional routing rules - or a redirect of every request to another host. Visitors can only be served objects that a bucket policy lets everyone read",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn put_bucket_website(
        &self,
        Parameters(req): Parameters<PutBucketWebsiteRequest>,
    ) -> String {
        info!(
            "Executing put_bucket_website tool for bucket: {}",
            req.bucket_name
        );

        let configuration = crate::s3_client::WebsiteConfiguration {
            index_document_suffix: req.index_document_suffix,
            error_document_key: req.error_document_key,
            redirect_all_requests_to: req.redirect_all_requests_to,
            routing_rules: req.routing_rules,
        };

        match self
            .s3_client
            .put_bucket_website(&req.bucket_name, &configuration)
            .await
        {
            Ok(()) => {
                info!(
                    "put_bucket_website tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket website configuration updated!**\n\n\
                     **Bucket:** {}\n\
                     {}\n\
                     {}\n\
                     ℹ️ **Note:** The configuration replaces the previous one. The storage backend serves the website on its website endpoint, and only for objects that a bucket policy lets everyone read (put_bucket_policy); its S3 endpoint keeps answering S3 requests.",
                    req.bucket_name,
                    configuration.summary(),
                    Self::website_routing_lines(&configuration.routing_rules)
                )
            }
            Err(e) => {
                error!(
                    "Failed to set the website configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to update the website configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • No index document was given and no redirect of all requests\n\
                     • The index document suffix is a path instead of a single name such as 'index.html'\n\
                     • A redirect of all requests was combined with documents or routing rules\n\
                     • A routing rule lacks its key prefix or the prefix it redirects to\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to change the bucket configuration (s3:PutBucketWebsite)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Read the current configuration with get_bucket_website and compare it\n\
                     2. Pass index_document_suffix (e.g. index.html), or redirect_all_requests_to on its own\n\
                     3. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    #[tool(
        description = "Delete the website configuration of an S3 bucket; its objects are then no longer served as a website. The objects themselves stay in place",
        annotations(destructive_hint = true, idempotent_hint = true)
    )]
    pub async fn delete_bucket_website(
        &self,
        Parameters(req): Parameters<DeleteBucketWebsiteRequest>,
    ) -> String {
        info!(
            "Executing delete_bucket_website tool for bucket: {}",
            req.bucket_name
        );

        match self.s3_client.delete_bucket_website(&req.bucket_name).await {
            Ok(()) => {
                info!(
                    "delete_bucket_website tool executed successfully for bucket '{}'",
                    req.bucket_name
                );
                format!(
                    "✅ **Bucket website configuration removed!**\n\n\
                     **Bucket:** {}\n\n\
                     ℹ️ **Note:** The objects of this bucket are no longer served as a website. The objects themselves stay in place.",
                    req.bucket_name
                )
            }
            Err(e) => {
                error!(
                    "Failed to delete the website configuration of bucket '{}': {:?}",
                    req.bucket_name, e
                );

                format!(
                    "❌ **Failed to remove the website configuration of S3 bucket '{}'**\n\n\
                     **Error:** {:#}\n\n\
                     **Possible causes:**\n\
                     • Bucket does not exist or is not accessible\n\
                     • AWS credentials lack permission to delete the website configuration (s3:DeleteBucketWebsite)\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the bucket exists using list_buckets\n\
                     2. Check your AWS credentials and permissions",
                    req.bucket_name, e
                )
            }
        }
    }

    /// The routing rules of a website configuration for tool output.
    fn website_routing_lines(rules: &[crate::s3_client::RoutingRule]) -> String {
        if rules.is_empty() {
            return String::new();
        }
        let mut text = format!("**Routing Rules:** {}\n", rules.len());
        for rule in rules {
            text.push_str(&format!(
                "• {} → {}\n",
                rule.key_prefix_equals, rule.replace_key_prefix_with
            ));
        }
        text
    }
}

/// Tags sorted by key, so tool output does not depend on hash order.
fn sorted_tags(tags: &std::collections::HashMap<String, String>) -> Vec<(&String, &String)> {
    let mut sorted: Vec<(&String, &String)> = tags.iter().collect();
    sorted.sort();
    sorted
}

/// Said when a storage backend reports ACL grants without naming who
/// receives them.
const ACL_UNREPORTED_NOTE: &str = "\nℹ️ **Note:** The storage backend does not report who receives these permissions, so the ACL cannot be inspected here. Some S3-compatible servers accept ACL requests without applying them (RustFS 1.0.6 does); manage public access with a bucket policy there (put_bucket_policy).";

/// The owner of an access control policy for tool output.
fn acl_owner_line(policy: &crate::s3_client::AccessControlPolicy) -> String {
    match &policy.owner {
        Some(owner) => match &owner.display_name {
            Some(name) if !owner.id.is_empty() => format!("{name} ({})", owner.id),
            _ => owner.id.clone(),
        },
        None => "not reported by the storage backend".to_string(),
    }
}

/// The grants of an access control policy, one per line and sorted, so tool
/// output does not depend on the order the storage backend lists them in.
fn acl_grant_lines(policy: &crate::s3_client::AccessControlPolicy) -> String {
    let mut lines: Vec<String> = policy
        .grants
        .iter()
        .map(|grant| {
            format!(
                "• {} — {}",
                grant.permission.as_str(),
                grant.grantee.label()
            )
        })
        .collect();
    lines.sort();
    if lines.is_empty() {
        lines.push("• No grants reported by the storage backend".to_string());
    }
    lines.join("\n")
}

/// The outcome of a canned ACL that was set: what was asked for, and what
/// the storage backend reports afterwards. `target` names the bucket or
/// object; `policy_tool` is the tool that manages access when ACLs are not
/// applied.
fn acl_update_text(result: crate::s3_client::AclUpdate, target: &str, policy_tool: &str) -> String {
    let acl = result.requested;
    let public_warning = match acl {
        crate::s3_client::CannedAcl::PublicReadWrite => Some(
            "⚠️ **Warning:** Anyone on the internet can read, overwrite, and delete the resource.",
        ),
        crate::s3_client::CannedAcl::PublicRead => {
            Some("⚠️ **Warning:** Anyone on the internet can read the resource.")
        }
        _ => None,
    };

    if result.confirmed {
        let policy = result
            .read_back
            .as_ref()
            .expect("a confirmed update carries its policy");
        let mut result_text = format!(
            "✅ **ACL updated!**\n\n\
             {}\n\
             **ACL:** {}\n\
             **Owner:** {}\n\
             **Grants:**\n{}\n",
            target.trim_end(),
            acl.as_str(),
            acl_owner_line(policy),
            acl_grant_lines(policy)
        );
        result_text.push_str(&format!("\nℹ️ **Note:** {}", acl.effect()));
        if let Some(warning) = public_warning {
            result_text.push_str(&format!("\n{warning}"));
        }
        info!(
            "put ACL tool executed successfully: '{}' was confirmed by reading it back",
            acl.as_str()
        );
        return result_text;
    }

    // The request was answered with success, but the policy read back does
    // not show the ACL: say so instead of claiming it was applied.
    let grants = match &result.read_back {
        Some(policy) => format!(
            "**Owner:** {}\n\
             **Grants:**\n{}\n",
            acl_owner_line(policy),
            acl_grant_lines(policy)
        ),
        None => {
            "**The storage backend did not answer the follow-up read of the ACL.**\n".to_string()
        }
    };
    let mut result_text = format!(
        "⚠️ **ACL request accepted, but its effect is not confirmed**\n\n\
         {}\n\
         **Requested ACL:** {}\n\n\
         {}\n\
         ℹ️ **Note:** {} The storage backend does not report this access after the request. Some S3-compatible servers accept ACL requests without applying them (RustFS 1.0.6 does); use a bucket policy ({policy_tool}) to manage public access there.",
        target.trim_end(),
        acl.as_str(),
        grants,
        acl.effect()
    );
    if let Some(warning) = public_warning {
        result_text.push_str(&format!("\n{warning}"));
    }
    tracing::warn!(
        "put ACL tool: '{}' was accepted, but reading the ACL back does not show it",
        acl.as_str()
    );
    result_text
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RustfsMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "RustFS MCP Server providing S3 operations through Model Context Protocol",
            )
            .with_server_info(Implementation::new(
                "rustfs-mcp-server",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_protocol_version(ProtocolVersion::LATEST)
    }

    async fn ping(&self, _ctx: RequestContext<RoleServer>) -> Result<(), ErrorData> {
        info!("Received ping request");
        Ok(())
    }

    async fn on_initialized(&self, _ctx: NotificationContext<RoleServer>) {
        info!("Client initialized successfully");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hints of a registered tool, in `(read_only, destructive, idempotent)` order.
    fn hints(tool_name: &str) -> (Option<bool>, Option<bool>, Option<bool>) {
        let router = RustfsMcpServer::tool_router();
        let tool = router
            .get(tool_name)
            .unwrap_or_else(|| panic!("tool `{tool_name}` is not registered"));
        let annotations = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("tool `{tool_name}` declares no annotations"));
        (
            annotations.read_only_hint,
            annotations.destructive_hint,
            annotations.idempotent_hint,
        )
    }

    #[test]
    fn test_object_attributes_is_annotated_read_only() {
        assert_eq!(hints("get_object_attributes"), (Some(true), None, None));
    }

    #[test]
    fn test_tools_that_only_read_are_annotated_read_only() {
        for name in ["list_buckets", "list_objects", "head_object", "list_parts"] {
            // Read-only tools carry no destructive or idempotent hint: those
            // are only meaningful for tools that modify their environment.
            assert_eq!(hints(name), (Some(true), None, None), "{name}");
        }
    }

    #[test]
    fn test_object_lock_setters_are_annotated_destructive_and_idempotent() {
        // Every setter replaces the whole setting of one bucket or version,
        // so repeating a call converges on the same state.
        for name in [
            "put_bucket_lock_configuration",
            "put_object_retention",
            "put_object_legal_hold",
        ] {
            assert_eq!(hints(name), (None, Some(true), Some(true)), "{name}");
        }
    }

    #[test]
    fn test_object_lock_getters_are_annotated_read_only() {
        for name in [
            "get_bucket_lock_configuration",
            "get_object_retention",
            "get_object_legal_hold",
        ] {
            assert_eq!(hints(name), (Some(true), None, None), "{name}");
        }
    }

    #[test]
    fn test_replication_getter_is_annotated_read_only() {
        assert_eq!(hints("get_bucket_replication"), (Some(true), None, None));
    }

    #[test]
    fn test_replication_setters_are_annotated_destructive_and_idempotent() {
        // Both setters replace the whole configuration of one bucket, so
        // repeating a call converges on the same state.
        for name in ["put_bucket_replication", "delete_bucket_replication"] {
            assert_eq!(hints(name), (None, Some(true), Some(true)), "{name}");
        }
    }

    #[test]
    fn test_request_payment_getter_is_annotated_read_only() {
        assert_eq!(
            hints("get_bucket_request_payment"),
            (Some(true), None, None)
        );
    }

    #[test]
    fn test_request_payment_setter_is_annotated_destructive_and_idempotent() {
        // Setting a payer replaces the whole configuration of one bucket, so
        // repeating a call converges on the same state.
        assert_eq!(
            hints("put_bucket_request_payment"),
            (None, Some(true), Some(true))
        );
    }

    #[test]
    fn test_tools_that_only_add_are_annotated_non_destructive() {
        // Each call creates something new, so it is not idempotent.
        assert_eq!(
            hints("initiate_multipart_upload"),
            (None, Some(false), None)
        );
    }

    #[test]
    fn test_create_bucket_is_annotated_destructive() {
        // CreateBucket is not guaranteed to target a new bucket: in
        // us-east-1 S3 answers the call for an existing bucket owned by
        // the caller with success and resets its ACLs, so it may modify
        // existing access controls.
        assert_eq!(hints("create_bucket"), (None, Some(true), None));
    }

    #[test]
    fn test_notification_setters_are_annotated_destructive_and_idempotent() {
        // Both calls replace the whole configuration of a bucket, so
        // repeating them converges on the same state.
        for name in ["put_bucket_notification", "delete_bucket_notification"] {
            assert_eq!(hints(name), (None, Some(true), Some(true)), "{name}");
        }
    }

    #[test]
    fn test_notification_getter_is_annotated_read_only() {
        assert_eq!(hints("get_bucket_notification"), (Some(true), None, None));
    }

    #[test]
    fn test_tools_that_overwrite_or_delete_are_annotated_destructive() {
        // Repeating these calls converges on the same state no matter the
        // bucket versioning: re-uploading a part replaces the same part,
        // and download mode rewrites the same local file.
        for name in ["upload_part", "get_object"] {
            assert_eq!(hints(name), (None, Some(true), Some(true)), "{name}");
        }

        // Repeating these calls fails because the resource is already gone.
        for name in [
            "delete_bucket",
            "complete_multipart_upload",
            "abort_multipart_upload",
        ] {
            assert_eq!(hints(name), (None, Some(true), None), "{name}");
        }

        // These must not claim idempotency: with bucket versioning enabled,
        // a repeated upload or copy adds another object version and a
        // repeated plain delete adds another delete marker, so retries do
        // have additional effects.
        for name in [
            "upload_file",
            "delete_object",
            "delete_objects",
            "copy_object",
        ] {
            assert_eq!(hints(name), (None, Some(true), None), "{name}");
        }
    }

    #[test]
    fn test_acl_setters_are_annotated_destructive_and_idempotent() {
        // Applying the same canned ACL twice leaves the same policy behind.
        for name in ["put_bucket_acl", "put_object_acl"] {
            assert_eq!(hints(name), (None, Some(true), Some(true)), "{name}");
        }
    }

    fn policy(grants: Vec<crate::s3_client::AclGrant>) -> crate::s3_client::AccessControlPolicy {
        crate::s3_client::AccessControlPolicy {
            owner: Some(crate::s3_client::AclOwner {
                id: "owner-id".to_string(),
                display_name: Some("rustfs".to_string()),
            }),
            grants,
        }
    }

    fn grant(
        grantee: crate::s3_client::AclGrantee,
        permission: crate::s3_client::AclPermission,
    ) -> crate::s3_client::AclGrant {
        crate::s3_client::AclGrant {
            grantee,
            permission,
        }
    }

    #[test]
    fn test_acl_grant_lines_are_sorted_and_name_the_grantees() {
        let grants = acl_grant_lines(&policy(vec![
            grant(
                crate::s3_client::AclGrantee::Group {
                    uri: "http://acs.amazonaws.com/groups/global/AllUsers".to_string(),
                },
                crate::s3_client::AclPermission::Read,
            ),
            grant(
                crate::s3_client::AclGrantee::User {
                    id: "owner-id".to_string(),
                    display_name: Some("rustfs".to_string()),
                },
                crate::s3_client::AclPermission::FullControl,
            ),
        ]));

        assert_eq!(
            grants,
            "• FULL_CONTROL — rustfs (owner-id)\n• READ — Everyone"
        );
        assert_eq!(
            acl_grant_lines(&policy(Vec::new())),
            "• No grants reported by the storage backend"
        );
        assert_eq!(acl_owner_line(&policy(Vec::new())), "rustfs (owner-id)");
        assert_eq!(
            acl_owner_line(&crate::s3_client::AccessControlPolicy {
                owner: None,
                grants: Vec::new()
            }),
            "not reported by the storage backend"
        );
    }

    #[test]
    fn test_acl_update_text_reports_an_unconfirmed_acl_as_such() {
        // RustFS 1.0.6 answers ACL requests with success but reports a grant
        // whose grantee carries no ID afterwards.
        let unconfirmed = acl_update_text(
            crate::s3_client::AclUpdate {
                requested: crate::s3_client::CannedAcl::PublicRead,
                confirmed: false,
                read_back: Some(policy(vec![grant(
                    crate::s3_client::AclGrantee::User {
                        id: String::new(),
                        display_name: None,
                    },
                    crate::s3_client::AclPermission::FullControl,
                )])),
            },
            "**Bucket:** b\n",
            "put_bucket_policy",
        );
        assert!(
            unconfirmed.starts_with("⚠️ **ACL request accepted, but its effect is not confirmed**")
        );
        assert!(unconfirmed.contains("**Bucket:** b\n"));
        assert!(unconfirmed.contains("**Requested ACL:** public-read"));
        assert!(unconfirmed.contains("• FULL_CONTROL — not reported by the storage backend"));
        assert!(unconfirmed.contains("RustFS 1.0.6"));
        assert!(unconfirmed.contains("put_bucket_policy"));
        assert!(unconfirmed.contains("Anyone on the internet can read"));
        // A successful tool answer must not claim the update succeeded.
        assert!(!unconfirmed.contains("✅"));

        // A failed read-back is reported in place of the grants.
        let unreadable = acl_update_text(
            crate::s3_client::AclUpdate {
                requested: crate::s3_client::CannedAcl::Private,
                confirmed: false,
                read_back: None,
            },
            "**Bucket:** b\n",
            "put_bucket_policy",
        );
        assert!(unreadable.contains("did not answer the follow-up read"));
    }

    #[test]
    fn test_acl_update_text_reports_a_confirmed_acl_with_its_grants() {
        let confirmed = acl_update_text(
            crate::s3_client::AclUpdate {
                requested: crate::s3_client::CannedAcl::PublicRead,
                confirmed: true,
                read_back: Some(policy(vec![
                    grant(
                        crate::s3_client::AclGrantee::User {
                            id: "owner-id".to_string(),
                            display_name: Some("rustfs".to_string()),
                        },
                        crate::s3_client::AclPermission::FullControl,
                    ),
                    grant(
                        crate::s3_client::AclGrantee::Group {
                            uri: "http://acs.amazonaws.com/groups/global/AllUsers".to_string(),
                        },
                        crate::s3_client::AclPermission::Read,
                    ),
                ])),
            },
            "**S3 Location:** s3://b/k\n**Version ID:** v1\n",
            "put_bucket_policy",
        );
        assert!(confirmed.starts_with("✅ **ACL updated!**"));
        assert!(confirmed.contains("**S3 Location:** s3://b/k\n**Version ID:** v1"));
        assert!(confirmed.contains("**ACL:** public-read"));
        assert!(confirmed.contains("• READ — Everyone"));
        assert!(confirmed.contains("Anyone on the internet can read"));
        assert!(!confirmed.contains("not confirmed"));
    }

    #[test]
    fn test_acl_getters_are_annotated_read_only() {
        for name in ["get_bucket_acl", "get_object_acl"] {
            assert_eq!(hints(name), (Some(true), None, None), "{name}");
        }
    }

    #[test]
    fn test_presigned_url_is_not_annotated_read_only() {
        // The tool changes nothing on S3, but it mints a credential-bearing
        // URL, so it must not be auto-approved like a read-only tool.
        assert_eq!(hints("presigned_url"), (None, Some(false), Some(true)));
    }

    #[test]
    fn test_website_setters_are_annotated_destructive_and_idempotent() {
        // Both calls replace or remove the whole configuration of a bucket,
        // so repeating them converges on the same state.
        for name in ["put_bucket_website", "delete_bucket_website"] {
            assert_eq!(hints(name), (None, Some(true), Some(true)), "{name}");
        }
    }

    #[test]
    fn test_website_routing_lines() {
        assert_eq!(RustfsMcpServer::website_routing_lines(&[]), "");
        let rules = [
            crate::s3_client::RoutingRule {
                key_prefix_equals: "docs/".to_string(),
                replace_key_prefix_with: "documents/".to_string(),
            },
            crate::s3_client::RoutingRule {
                key_prefix_equals: "img/".to_string(),
                replace_key_prefix_with: "images/".to_string(),
            },
        ];
        assert_eq!(
            RustfsMcpServer::website_routing_lines(&rules),
            "**Routing Rules:** 2\n• docs/ → documents/\n• img/ → images/\n"
        );
    }

    #[test]
    fn test_website_getter_is_annotated_read_only() {
        assert_eq!(hints("get_bucket_website"), (Some(true), None, None));
    }

    #[test]
    fn test_every_tool_declares_annotations() {
        for tool in RustfsMcpServer::tool_router().list_all() {
            assert!(
                tool.annotations.is_some(),
                "tool `{}` must declare annotations(...)",
                tool.name
            );
        }
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = Config {
            access_key_id: Some("test_key".to_string()),
            secret_access_key: Some("test_secret".to_string()),
            endpoint_url: Some("http://127.0.0.1:9000".to_string()),
            force_path_style: true,
            ..Config::default()
        };

        let result = RustfsMcpServer::new(config).await;
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_logging_setters_are_annotated_destructive_and_idempotent() {
        // Enabling the same logging twice and disabling it twice both leave
        // the same configuration behind.
        for name in ["put_bucket_logging", "delete_bucket_logging"] {
            assert_eq!(hints(name), (None, Some(true), Some(true)), "{name}");
        }
    }

    #[test]
    fn test_logging_getter_is_annotated_read_only() {
        assert_eq!(hints("get_bucket_logging"), (Some(true), None, None));
    }

    #[test]
    fn test_sorted_tags_orders_by_key() {
        let tags: std::collections::HashMap<String, String> =
            [("team", "storage"), ("cost-center", "42"), ("env", "dev")]
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();

        let sorted = sorted_tags(&tags);

        assert_eq!(
            sorted,
            [
                (&"cost-center".to_string(), &"42".to_string()),
                (&"env".to_string(), &"dev".to_string()),
                (&"team".to_string(), &"storage".to_string()),
            ]
        );
        assert!(sorted_tags(&std::collections::HashMap::new()).is_empty());
    }
}
