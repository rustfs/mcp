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
mod read_mode;
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
        description = "Create a new S3 bucket with the specified name",
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

        match self.s3_client.create_bucket(&req.bucket_name).await {
            Ok(_) => {
                format!("Successfully created bucket: {}", req.bucket_name)
            }
            Err(e) => {
                format!("Failed to create bucket '{}': {:?}", req.bucket_name, e)
            }
        }
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {e}"
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                    "Failed to list S3 buckets: {e}\n\nPossible causes:\n\
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
                                 **Error:** {e}"
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
                     **Error:** {}\n\n\
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
}

/// Tags sorted by key, so tool output does not depend on hash order.
fn sorted_tags(tags: &std::collections::HashMap<String, String>) -> Vec<(&String, &String)> {
    let mut sorted: Vec<(&String, &String)> = tags.iter().collect();
    sorted.sort();
    sorted
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
    fn test_tools_that_only_read_are_annotated_read_only() {
        for name in ["list_buckets", "list_objects", "head_object", "list_parts"] {
            // Read-only tools carry no destructive or idempotent hint: those
            // are only meaningful for tools that modify their environment.
            assert_eq!(hints(name), (Some(true), None, None), "{name}");
        }
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
    fn test_presigned_url_is_not_annotated_read_only() {
        // The tool changes nothing on S3, but it mints a credential-bearing
        // URL, so it must not be auto-approved like a read-only tool.
        assert_eq!(hints("presigned_url"), (None, Some(false), Some(true)));
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
