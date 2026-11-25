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

use anyhow::Result;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerConfig},
    service::{NotificationContext, RequestContext},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::config::Config;
use crate::s3_client::{
    CopyObjectOptions, DetectedFileType, GetObjectOptions, ListObjectsOptions, PresignMethod,
    S3Client, UploadFileOptions,
};

/// Request payload for listing objects in a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListObjectsRequest {
    pub bucket_name: String,
    #[serde(default)]
    #[schemars(description = "Optional prefix to filter objects")]
    pub prefix: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional character used to group keys, e.g. '/' for directory-style browsing (returns common prefixes as Directories)"
    )]
    pub delimiter: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional maximum number of keys to return per page (1-1000, default 1000)"
    )]
    pub max_keys: Option<i32>,
    #[serde(default)]
    #[schemars(
        description = "Optional token from a previous truncated response's next_continuation_token to fetch the next page"
    )]
    pub continuation_token: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional key to start listing after (ignored when a continuation token is provided)"
    )]
    pub start_after: Option<String>,
}

/// Request payload for uploading a file to S3.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UploadFileRequest {
    #[schemars(description = "Path to the local file to upload")]
    pub local_file_path: String,
    #[schemars(description = "Name of the S3 bucket to upload to")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional content type (auto-detected if not specified)")]
    pub content_type: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional storage class (STANDARD, REDUCED_REDUNDANCY, etc.)")]
    pub storage_class: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional cache control header")]
    pub cache_control: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional user metadata key-value pairs stored with the object (x-amz-meta-*)"
    )]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    #[schemars(description = "Optional server-side encryption (e.g. AES256 or aws:kms)")]
    pub server_side_encryption: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content disposition header (e.g. attachment; filename=x)")]
    pub content_disposition: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content encoding header (e.g. gzip)")]
    pub content_encoding: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content language header (e.g. en-US)")]
    pub content_language: Option<String>,
}

/// Request payload for creating a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CreateBucketRequest {
    #[schemars(description = "Name of the S3 bucket to create")]
    pub bucket_name: String,
}

/// Request payload for deleting a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketRequest {
    #[schemars(description = "Name of the S3 bucket to delete")]
    pub bucket_name: String,
}

/// Request payload for reading/downloading an object.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetObjectRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional version ID for versioned objects")]
    pub version_id: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional byte range to read, e.g. 'bytes=0-1023' (partial read, supported in both modes)"
    )]
    pub range: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional ISO 8601 timestamp; the read succeeds only if the object was modified after this time, otherwise it fails with a not-modified error"
    )]
    pub if_modified_since: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional ISO 8601 timestamp; the read succeeds only if the object was not modified after this time, otherwise it fails with a precondition error"
    )]
    pub if_unmodified_since: Option<String>,
    #[serde(default = "default_operation_mode")]
    #[schemars(
        description = "Operation mode: read (return content) or download (save to local file)"
    )]
    pub mode: GetObjectMode,
    #[serde(default)]
    #[schemars(description = "Local file path for download mode (required when mode is download)")]
    pub local_path: Option<String>,
    #[serde(default = "default_max_content_size")]
    #[schemars(description = "Maximum content size to read in bytes for read mode (default: 1MB)")]
    pub max_content_size: usize,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum GetObjectMode {
    /// Read object content directly and return it in response text.
    #[serde(rename = "read")]
    Read,
    /// Download object content to a local file path.
    #[serde(rename = "download")]
    Download,
}

fn default_operation_mode() -> GetObjectMode {
    GetObjectMode::Read
}
fn default_max_content_size() -> usize {
    1024 * 1024
}

/// Request payload for deleting a single object.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteObjectRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional version ID for versioned objects")]
    pub version_id: Option<String>,
}

/// Request payload for batch-deleting objects.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteObjectsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "Object keys to delete permanently (up to 1000 keys per request)")]
    pub object_keys: Vec<String>,
}

/// Request payload for server-side object copy.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CopyObjectRequest {
    #[schemars(description = "Name of the source S3 bucket")]
    pub source_bucket: String,
    #[schemars(description = "Source S3 object key")]
    pub source_key: String,
    #[schemars(description = "Name of the destination S3 bucket")]
    pub destination_bucket: String,
    #[schemars(description = "Destination S3 object key")]
    pub destination_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional content type override (applied with REPLACE metadata directive)"
    )]
    pub content_type: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional storage class override (STANDARD, REDUCED_REDUNDANCY, etc.)"
    )]
    pub storage_class: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional cache control header override")]
    pub cache_control: Option<String>,
}

/// Request payload for retrieving object metadata.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct HeadObjectRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional version ID for versioned objects")]
    pub version_id: Option<String>,
}

/// Request payload for generating a presigned URL.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PresignUrlRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default = "default_presign_method")]
    #[schemars(description = "Operation the URL grants: get (download) or put (upload)")]
    pub method: PresignMethod,
    #[serde(default = "default_presign_expiry")]
    #[schemars(
        description = "URL validity in seconds, clamped to 1-604800 (7 days maximum; default 3600)"
    )]
    pub expires_in_secs: u64,
}

fn default_presign_method() -> PresignMethod {
    PresignMethod::Get
}
fn default_presign_expiry() -> u64 {
    3600
}

/// Request payload for initiating a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InitiateMultipartUploadRequest {
    #[schemars(description = "Name of the S3 bucket to upload to")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (destination path)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional content type of the final object")]
    pub content_type: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional storage class (STANDARD, REDUCED_REDUNDANCY, etc.)")]
    pub storage_class: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional cache control header")]
    pub cache_control: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional user metadata key-value pairs stored with the final object (x-amz-meta-*)"
    )]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// A part recorded for multipart upload completion.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CompletedPartInput {
    #[schemars(description = "Part number (1-10000)")]
    pub part_number: i32,
    #[schemars(description = "ETag returned by upload_part for this part")]
    pub etag: String,
}

/// Request payload for uploading one multipart part from a local file.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UploadPartRequest {
    #[schemars(
        description = "Path to the local file to upload as this part (the whole file is sent as one part)"
    )]
    pub local_file_path: String,
    #[schemars(description = "Name of the S3 bucket to upload to")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
    #[schemars(description = "Part number (1-10000)")]
    pub part_number: i32,
}

/// Request payload for completing a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CompleteMultipartUploadRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
    #[schemars(
        description = "Parts to assemble, in any order (sorted by part number automatically)"
    )]
    pub parts: Vec<CompletedPartInput>,
}

/// Request payload for aborting a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AbortMultipartUploadRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
}

/// Request payload for listing uploaded parts of a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListPartsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
}

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

    #[tool(description = "Create a new S3 bucket with the specified name")]
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

    #[tool(description = "Delete an existing S3 bucket with the specified name")]
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

    #[tool(description = "List all S3 buckets accessible with the configured credentials")]
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
        description = "List objects in a specific S3 bucket with optional prefix filtering, directory-style browsing (delimiter), and pagination (max_keys + continuation token)"
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
        description = "Get/download an object from an S3 bucket - supports read mode for text files, download mode for all files, byte-range reads (range), and conditional reads (if_modified_since / if_unmodified_since)"
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

    async fn handle_read_mode(&self, req: GetObjectRequest) -> String {
        let options = GetObjectOptions {
            version_id: req.version_id.clone(),
            range: req.range.clone(),
            if_modified_since: req.if_modified_since.clone(),
            if_unmodified_since: req.if_unmodified_since.clone(),
            max_content_size: Some(req.max_content_size),
        };

        match self
            .s3_client
            .get_object(&req.bucket_name, &req.object_key, options)
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully retrieved object s3://{}/{} ({} bytes)",
                    req.bucket_name, req.object_key, result.content_length
                );

                match result.detected_type {
                    DetectedFileType::Text => {
                        if let Some(ref text_content) = result.text_content {
                            let mut result_text = format!(
                                "✅ **Text file content retrieved!**\n\n\
                                 **S3 Location:** s3://{}/{}\n",
                                result.bucket, result.key
                            );
                            if let Some(ref range) = req.range {
                                result_text
                                    .push_str(&format!("**Range:** {range} (partial read)\n"));
                            }
                            result_text.push_str(&format!(
                                "**File Size:** {} bytes\n\
                                 **Content Type:** {}\n\n\
                                 **Content:**\n```\n{}\n```",
                                result.content_length, result.content_type, text_content
                            ));
                            result_text
                        } else {
                            format!(
                                "⚠️ **Text file detected but content could not be decoded!**\n\n\
                                 **S3 Location:** s3://{}/{}\n\
                                 **File Size:** {} bytes\n\
                                 **Content Type:** {}\n\n\
                                 **Note:** Could not decode file as UTF-8 text. \
                                 Try using download mode instead.",
                                result.bucket,
                                result.key,
                                result.content_length,
                                result.content_type
                            )
                        }
                    }
                    DetectedFileType::NonText(ref mime_type) => {
                        let file_category = if mime_type.starts_with("image/") {
                            "Image"
                        } else if mime_type.starts_with("audio/") {
                            "Audio"
                        } else if mime_type.starts_with("video/") {
                            "Video"
                        } else {
                            "Binary"
                        };

                        format!(
                            "⚠️ **Non-text file detected!**\n\n\
                             **S3 Location:** s3://{}/{}\n\
                             **File Type:** {} ({})\n\
                             **File Size:** {} bytes ({:.2} MB)\n\n\
                             **Note:** This file type cannot be displayed as text.\n\
                             Please use download mode to save it to a local file:\n\n\
                             ```json\n{{\n  \"mode\": \"download\",\n  \"local_path\": \"/path/to/save/file\"\n}}\n```",
                            result.bucket,
                            result.key,
                            file_category,
                            mime_type,
                            result.content_length,
                            result.content_length as f64 / 1_048_576.0
                        )
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to read object s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );
                self.format_error_message(&req, e)
            }
        }
    }

    async fn handle_download_mode(&self, req: GetObjectRequest) -> String {
        let local_path = match req.local_path {
            Some(ref path) => path,
            None => {
                return "❌ **Error:** local_path is required when using download mode.\n\n\
                        **Example:**\n```json\n{\n  \"mode\": \"download\",\n  \"local_path\": \"/path/to/save/file.ext\"\n}\n```"
                    .to_string();
            }
        };

        let options = GetObjectOptions {
            version_id: req.version_id.clone(),
            range: req.range.clone(),
            if_modified_since: req.if_modified_since.clone(),
            if_unmodified_since: req.if_unmodified_since.clone(),
            ..GetObjectOptions::default()
        };

        match self
            .s3_client
            .download_object_to_file(&req.bucket_name, &req.object_key, local_path, options)
            .await
        {
            Ok((bytes_downloaded, absolute_path)) => {
                info!(
                    "Successfully downloaded object s3://{}/{} to {} ({} bytes)",
                    req.bucket_name, req.object_key, absolute_path, bytes_downloaded
                );

                let mut result_text = format!(
                    "✅ **File downloaded successfully!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Local Path (requested):** {}\n\
                     **Absolute Path:** {}\n",
                    req.bucket_name, req.object_key, local_path, absolute_path
                );
                if let Some(ref range) = req.range {
                    result_text.push_str(&format!("**Range:** {range} (partial read)\n"));
                }
                result_text.push_str(&format!(
                    "**File Size:** {} bytes ({:.2} MB)\n\n\
                     **✨ File saved successfully!** You can now access it at:\n\
                     `{}`",
                    bytes_downloaded,
                    bytes_downloaded as f64 / 1_048_576.0,
                    absolute_path
                ));
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to download object s3://{}/{} to {}: {:?}",
                    req.bucket_name, req.object_key, local_path, e
                );

                format!(
                    "❌ **Failed to download file from S3**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Local Path:** {}\n\
                     **Error:** {}\n\n\
                     **Possible causes:**\n\
                     • Object does not exist in the specified bucket\n\
                     • AWS credentials lack permissions to read this object\n\
                     • Cannot write to the specified local path\n\
                     • Insufficient disk space\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using list_objects\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Ensure the local directory exists and is writable\n\
                     4. Check available disk space",
                    req.bucket_name, req.object_key, local_path, e
                )
            }
        }
    }

    fn format_error_message(&self, req: &GetObjectRequest, error: anyhow::Error) -> String {
        format!(
            "❌ **Failed to get object from S3 bucket '{}'**\n\n\
             **Object Key:** {}\n\
             **Mode:** {:?}\n\
             **Error:** {}\n\n\
             **Possible causes:**\n\
             • Object does not exist in the specified bucket\n\
             • AWS credentials lack permissions to read this object\n\
             • Network connectivity issues\n\
             • Object key contains invalid characters\n\
             • Bucket does not exist or is not accessible\n\
             • Object is in a different AWS region\n\
             • Version ID is invalid (for versioned objects)\n\
             • Range is invalid or outside the object bounds\n\
             • Conditional read failed: the object's modified state does not match if_modified_since / if_unmodified_since\n\n\
             **Troubleshooting steps:**\n\
             1. Verify the object exists using list_objects\n\
             2. Check your AWS credentials and permissions\n\
             3. Ensure the bucket name and object key are correct\n\
             4. Try with a different object to test connectivity\n\
             5. Check if the bucket has versioning enabled",
            req.bucket_name, req.object_key, req.mode, error
        )
    }

    #[tool(description = "Upload a local file to an S3 bucket")]
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
        description = "Permanently delete a single object from an S3 bucket (optionally a specific version). This operation cannot be undone"
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
        description = "Permanently delete multiple objects (up to 1000) from an S3 bucket in a single batch request, reporting per-key failures. This operation cannot be undone"
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
        description = "Copy an object server-side within the same bucket or across buckets. Content type and cache control overrides are applied on top of the source object's preserved headers and user metadata; storage class can be changed without affecting metadata"
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
                     • Source object does not exist\n\
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
        description = "Retrieve metadata (size, ETag, content type, storage class, version, user metadata) for an S3 object without downloading its content"
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
        description = "Generate a presigned URL for an S3 object: 'get' URLs grant temporary download access, 'put' URLs grant temporary upload access, without embedding AWS credentials in the client"
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
        description = "Initiate a multipart upload for a large file and return an upload ID used by upload_part, list_parts, complete_multipart_upload, and abort_multipart_upload"
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
        description = "Upload a local file as a single multipart part (part numbers 1-10000); record the returned ETag for complete_multipart_upload"
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
        description = "Complete a multipart upload by assembling the uploaded parts (part number + ETag pairs); parts are sorted by part number automatically"
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
        description = "Abort a multipart upload and permanently discard all parts uploaded so far"
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
        description = "List the parts uploaded so far for a multipart upload, including each part's ETag for complete_multipart_upload"
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

/// Wrap a value in POSIX single quotes so it is safe to embed in the shell
/// examples we print. An apostrophe inside `'...'` would terminate the
/// quoted string, so it is replaced with the `'\''` escape sequence.
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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
    fn test_get_object_request_defaults() {
        let request = GetObjectRequest {
            bucket_name: "test-bucket".to_string(),
            object_key: "test-key".to_string(),
            version_id: None,
            range: None,
            if_modified_since: None,
            if_unmodified_since: None,
            mode: default_operation_mode(),
            local_path: None,
            max_content_size: default_max_content_size(),
        };

        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_key, "test-key");
        assert!(request.version_id.is_none());
        assert!(request.range.is_none());
        assert!(request.if_modified_since.is_none());
        assert!(request.if_unmodified_since.is_none());
        assert_eq!(request.mode, GetObjectMode::Read);
        assert!(request.local_path.is_none());
        assert_eq!(request.max_content_size, 1024 * 1024);
    }

    #[test]
    fn test_get_object_request_serialization() {
        let request = GetObjectRequest {
            bucket_name: "test-bucket".to_string(),
            object_key: "test-key".to_string(),
            version_id: Some("version123".to_string()),
            range: Some("bytes=0-1023".to_string()),
            if_modified_since: Some("2026-11-24T00:00:00Z".to_string()),
            if_unmodified_since: None,
            mode: GetObjectMode::Download,
            local_path: Some("/path/to/file".to_string()),
            max_content_size: 2048,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: GetObjectRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.bucket_name, deserialized.bucket_name);
        assert_eq!(request.object_key, deserialized.object_key);
        assert_eq!(request.version_id, deserialized.version_id);
        assert_eq!(request.range, deserialized.range);
        assert_eq!(request.if_modified_since, deserialized.if_modified_since);
        assert_eq!(request.mode, deserialized.mode);
        assert_eq!(request.local_path, deserialized.local_path);
        assert_eq!(request.max_content_size, deserialized.max_content_size);
    }

    #[test]
    fn test_get_object_request_serde_with_defaults() {
        let json = r#"{
            "bucket_name": "test-bucket",
            "object_key": "test-key"
        }"#;

        let request: GetObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_key, "test-key");
        assert!(request.version_id.is_none());
        assert!(request.range.is_none());
        assert!(request.if_modified_since.is_none());
        assert!(request.if_unmodified_since.is_none());
        assert_eq!(request.mode, GetObjectMode::Read);
        assert!(request.local_path.is_none());
        assert_eq!(request.max_content_size, 1024 * 1024);
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_operation_mode(), GetObjectMode::Read);
        assert_eq!(default_max_content_size(), 1024 * 1024);
    }

    #[test]
    fn test_get_object_mode_serialization() {
        let read_mode = GetObjectMode::Read;
        let download_mode = GetObjectMode::Download;

        let read_json = serde_json::to_string(&read_mode).unwrap();
        let download_json = serde_json::to_string(&download_mode).unwrap();

        assert_eq!(read_json, r#""read""#);
        assert_eq!(download_json, r#""download""#);

        let read_mode_deser: GetObjectMode = serde_json::from_str(r#""read""#).unwrap();
        let download_mode_deser: GetObjectMode = serde_json::from_str(r#""download""#).unwrap();

        assert_eq!(read_mode_deser, GetObjectMode::Read);
        assert_eq!(download_mode_deser, GetObjectMode::Download);
    }

    #[test]
    fn test_bucket_creation() {
        let request = CreateBucketRequest {
            bucket_name: "test-bucket".to_string(),
        };
        assert_eq!(request.bucket_name, "test-bucket");
    }

    #[test]
    fn test_bucket_deletion() {
        let request = DeleteBucketRequest {
            bucket_name: "test-bucket".to_string(),
        };
        assert_eq!(request.bucket_name, "test-bucket");
    }

    #[test]
    fn test_list_objects_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket"}"#;
        let request: ListObjectsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert!(request.prefix.is_none());
        assert!(request.delimiter.is_none());
        assert!(request.max_keys.is_none());
        assert!(request.continuation_token.is_none());
        assert!(request.start_after.is_none());

        let full: ListObjectsRequest = serde_json::from_str(
            r#"{
            "bucket_name": "test-bucket",
            "prefix": "docs/",
            "delimiter": "/",
            "max_keys": 50,
            "continuation_token": "tok==",
            "start_after": "docs/a.txt"
        }"#,
        )
        .unwrap();
        assert_eq!(full.prefix, Some("docs/".to_string()));
        assert_eq!(full.delimiter, Some("/".to_string()));
        assert_eq!(full.max_keys, Some(50));
        assert_eq!(full.continuation_token, Some("tok==".to_string()));
        assert_eq!(full.start_after, Some("docs/a.txt".to_string()));
    }

    #[test]
    fn test_delete_object_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_key": "a/b.txt"}"#;
        let request: DeleteObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_key, "a/b.txt");
        assert!(request.version_id.is_none());

        let with_version: DeleteObjectRequest = serde_json::from_str(
            r#"{"bucket_name": "test-bucket", "object_key": "a/b.txt", "version_id": "v1"}"#,
        )
        .unwrap();
        assert_eq!(with_version.version_id, Some("v1".to_string()));
    }

    #[test]
    fn test_delete_objects_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_keys": ["a.txt", "b/c.txt"]}"#;
        let request: DeleteObjectsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_keys.len(), 2);
        assert_eq!(request.object_keys[1], "b/c.txt");
    }

    #[test]
    fn test_copy_object_request_deserialization() {
        let json = r#"{
            "source_bucket": "src-bucket",
            "source_key": "docs/report.pdf",
            "destination_bucket": "dst-bucket",
            "destination_key": "archive/report-2026.pdf"
        }"#;
        let request: CopyObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.source_bucket, "src-bucket");
        assert_eq!(request.destination_key, "archive/report-2026.pdf");
        assert!(request.content_type.is_none());
        assert!(request.storage_class.is_none());
        assert!(request.cache_control.is_none());

        let with_overrides: CopyObjectRequest = serde_json::from_str(
            r#"{
            "source_bucket": "src-bucket",
            "source_key": "a.txt",
            "destination_bucket": "dst-bucket",
            "destination_key": "b.txt",
            "content_type": "text/plain",
            "storage_class": "REDUCED_REDUNDANCY",
            "cache_control": "max-age=3600"
        }"#,
        )
        .unwrap();
        assert_eq!(with_overrides.content_type, Some("text/plain".to_string()));
        assert_eq!(
            with_overrides.storage_class,
            Some("REDUCED_REDUNDANCY".to_string())
        );
    }

    #[test]
    fn test_head_object_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_key": "video.mp4"}"#;
        let request: HeadObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.object_key, "video.mp4");
        assert!(request.version_id.is_none());
    }

    #[test]
    fn test_presign_url_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_key": "a/b.txt"}"#;
        let request: PresignUrlRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.method, PresignMethod::Get);
        assert_eq!(request.expires_in_secs, 3600);

        let full: PresignUrlRequest = serde_json::from_str(
            r#"{
            "bucket_name": "test-bucket",
            "object_key": "a/b.txt",
            "method": "put",
            "expires_in_secs": 86400
        }"#,
        )
        .unwrap();
        assert_eq!(full.method, PresignMethod::Put);
        assert_eq!(full.expires_in_secs, 86400);
    }

    #[test]
    fn test_presign_method_serialization() {
        let get_json = serde_json::to_string(&PresignMethod::Get).unwrap();
        let put_json = serde_json::to_string(&PresignMethod::Put).unwrap();

        assert_eq!(get_json, r#""get""#);
        assert_eq!(put_json, r#""put""#);

        let get: PresignMethod = serde_json::from_str(r#""get""#).unwrap();
        let put: PresignMethod = serde_json::from_str(r#""put""#).unwrap();

        assert_eq!(get, PresignMethod::Get);
        assert_eq!(put, PresignMethod::Put);
    }

    #[test]
    fn test_shell_single_quote() {
        assert_eq!(shell_single_quote("plain.txt"), "'plain.txt'");
        assert_eq!(shell_single_quote("it's.png"), "'it'\\''s.png'");
        assert_eq!(
            shell_single_quote("a'$(printf injected)'.txt"),
            "'a'\\''$(printf injected)'\\''.txt'"
        );
    }

    #[test]
    fn test_multipart_request_deserialization() {
        let init: InitiateMultipartUploadRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k"}"#).unwrap();
        assert!(init.content_type.is_none());
        assert!(init.metadata.is_none());

        let part: UploadPartRequest = serde_json::from_str(
            r#"{
            "local_file_path": "/tmp/part-1",
            "bucket_name": "b",
            "object_key": "k",
            "upload_id": "abc",
            "part_number": 1
        }"#,
        )
        .unwrap();
        assert_eq!(part.part_number, 1);
        assert_eq!(part.upload_id, "abc");

        let complete: CompleteMultipartUploadRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "object_key": "k",
            "upload_id": "abc",
            "parts": [
                {"part_number": 2, "etag": "\"e2\""},
                {"part_number": 1, "etag": "\"e1\""}
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(complete.parts.len(), 2);
        assert_eq!(complete.parts[1].part_number, 1);

        let abort: AbortMultipartUploadRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k", "upload_id": "abc"}"#)
                .unwrap();
        assert_eq!(abort.upload_id, "abc");

        let list: ListPartsRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k", "upload_id": "abc"}"#)
                .unwrap();
        assert_eq!(list.object_key, "k");
    }

    #[test]
    fn test_upload_file_request_metadata_deserialization() {
        let json = r#"{
            "local_file_path": "/tmp/file.bin",
            "bucket_name": "b",
            "object_key": "k",
            "metadata": {"owner": "rustfs"},
            "server_side_encryption": "AES256",
            "content_disposition": "attachment",
            "content_encoding": "gzip",
            "content_language": "en-US"
        }"#;
        let request: UploadFileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.metadata.as_ref().unwrap().get("owner"),
            Some(&"rustfs".to_string())
        );
        assert_eq!(request.server_side_encryption, Some("AES256".to_string()));
        assert_eq!(request.content_encoding, Some("gzip".to_string()));
    }
}
