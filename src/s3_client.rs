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

use anyhow::{Context, Result};
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{Delete, ObjectIdentifier};
use aws_sdk_s3::{Client, Config as S3Config};
use aws_smithy_http_client::Builder as SmithyHttpClientBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use crate::config::Config;

/// Basic S3 bucket information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    pub creation_date: Option<String>,
}

/// Basic S3 object metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub storage_class: Option<String>,
}

/// Options for `list_objects_v2`.
#[derive(Debug, Clone, Default)]
pub struct ListObjectsOptions {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<i32>,
    pub continuation_token: Option<String>,
    pub start_after: Option<String>,
}

/// Result payload for `list_objects_v2`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListObjectsResult {
    pub objects: Vec<ObjectInfo>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
    pub max_keys: Option<i32>,
    pub key_count: i32,
}

/// Options for uploading a local file to S3.
#[derive(Debug, Clone, Default)]
pub struct UploadFileOptions {
    pub content_type: Option<String>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub storage_class: Option<String>,
    pub server_side_encryption: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
}

/// Options for reading/downloading an object from S3.
#[derive(Debug, Clone, Default)]
pub struct GetObjectOptions {
    pub version_id: Option<String>,
    pub range: Option<String>,
    pub if_modified_since: Option<String>,
    pub if_unmodified_since: Option<String>,
    pub max_content_size: Option<usize>,
}

/// File type classification used by get-object operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectedFileType {
    Text,
    NonText(String), // mime type for non-text files
}

/// Result payload for object retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetObjectResult {
    pub bucket: String,
    pub key: String,
    pub content_type: String,
    pub content_length: u64,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub version_id: Option<String>,
    pub detected_type: DetectedFileType,
    pub content: Option<Vec<u8>>,     // Raw content bytes
    pub text_content: Option<String>, // UTF-8 decoded content for text files
}

/// Result payload for file upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    pub bucket: String,
    pub key: String,
    pub etag: String,
    pub location: String,
    pub version_id: Option<String>,
    pub file_size: u64,
    pub content_type: String,
    pub upload_id: Option<String>,
}

/// Per-key failure inside a batch delete response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteObjectsError {
    pub key: String,
    pub message: String,
}

/// Result payload for batch object deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteObjectsResult {
    pub bucket: String,
    pub deleted: Vec<String>,
    pub errors: Vec<DeleteObjectsError>,
}

/// Options for server-side object copy.
#[derive(Debug, Clone, Default)]
pub struct CopyObjectOptions {
    pub content_type: Option<String>,
    pub storage_class: Option<String>,
    pub cache_control: Option<String>,
}

/// Result payload for server-side object copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyObjectResultData {
    pub source_bucket: String,
    pub source_key: String,
    pub destination_bucket: String,
    pub destination_key: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub version_id: Option<String>,
    pub content_type: Option<String>,
    pub storage_class: Option<String>,
}

/// Result payload for object metadata retrieval (HEAD).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadObjectResult {
    pub bucket: String,
    pub key: String,
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub content_type: Option<String>,
    pub cache_control: Option<String>,
    pub content_encoding: Option<String>,
    pub content_disposition: Option<String>,
    pub content_language: Option<String>,
    pub storage_class: Option<String>,
    pub version_id: Option<String>,
    pub user_metadata: std::collections::HashMap<String, String>,
}

/// Metadata values to apply on a REPLACE copy: overrides win, everything
/// else falls back to the source object's current values.
#[derive(Debug, Clone, PartialEq)]
struct EffectiveCopyMetadata {
    content_type: Option<String>,
    cache_control: Option<String>,
    content_encoding: Option<String>,
    content_disposition: Option<String>,
    content_language: Option<String>,
    user_metadata: std::collections::HashMap<String, String>,
}

/// Compute the metadata applied when copying with the REPLACE directive,
/// preserving source headers and user metadata that are not overridden.
fn effective_copy_metadata(
    options: &CopyObjectOptions,
    source: &HeadObjectResult,
) -> EffectiveCopyMetadata {
    EffectiveCopyMetadata {
        content_type: options
            .content_type
            .clone()
            .or_else(|| source.content_type.clone()),
        cache_control: options
            .cache_control
            .clone()
            .or_else(|| source.cache_control.clone()),
        content_encoding: source.content_encoding.clone(),
        content_disposition: source.content_disposition.clone(),
        content_language: source.content_language.clone(),
        user_metadata: source.user_metadata.clone(),
    }
}

/// HTTP method a presigned URL grants.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum PresignMethod {
    /// Presigned URL for downloading the object (HTTP GET).
    #[serde(rename = "get")]
    Get,
    /// Presigned URL for uploading the object (HTTP PUT).
    #[serde(rename = "put")]
    Put,
}

/// Result payload for presigned URL generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignedUrlResult {
    pub url: String,
    pub method: String,
    pub bucket: String,
    pub key: String,
    pub expires_in_secs: u64,
}

/// Percent-encode an S3 object key for the `x-amz-copy-source` header,
/// preserving `/` path separators. Unreserved characters
/// (`A-Z a-z 0-9 - _ . ~`) are kept as-is; everything else becomes `%XX`.
fn encode_copy_source_key(key: &str) -> String {
    let mut encoded = String::with_capacity(key.len());
    for byte in key.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
}

#[derive(Debug, Clone)]
pub struct S3Client {
    client: Client,
}

impl S3Client {
    /// Create an S3 client from validated [`Config`].
    pub async fn new(config: &Config) -> Result<Self> {
        info!("Initializing S3 client from configuration");

        let access_key = config.access_key_id();
        let secret_key = config.secret_access_key();

        debug!("Using AWS region: {}", config.region);
        if let Some(ref endpoint) = config.endpoint_url {
            debug!("Using custom endpoint: {}", endpoint);
        }

        let credentials = Credentials::new(access_key, secret_key, None, None, "rustfs-mcp-server");

        let mut config_builder = S3Config::builder()
            .credentials_provider(credentials)
            .region(Region::new(config.region.clone()))
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest());

        if config
            .endpoint_url
            .as_deref()
            .is_some_and(|endpoint| endpoint.starts_with("http://"))
        {
            config_builder =
                config_builder.http_client(SmithyHttpClientBuilder::new().build_http());
        }

        // Set force path style if custom endpoint or explicitly requested
        let should_force_path_style = config.endpoint_url.is_some() || config.force_path_style;
        if should_force_path_style {
            config_builder = config_builder.force_path_style(true);
        }

        if let Some(endpoint) = &config.endpoint_url {
            config_builder = config_builder.endpoint_url(endpoint);
        }

        let s3_config = config_builder.build();
        let client = Client::from_conf(s3_config);

        info!("S3 client initialized successfully");

        Ok(Self { client })
    }

    /// Create a bucket.
    pub async fn create_bucket(&self, bucket_name: &str) -> Result<BucketInfo> {
        info!("Creating S3 bucket: {}", bucket_name);

        self.client
            .create_bucket()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!("Failed to create S3 bucket: {bucket_name}"))?;

        info!("Bucket '{}' created successfully", bucket_name);
        Ok(BucketInfo {
            name: bucket_name.to_string(),
            creation_date: None, // Creation date not returned by create_bucket
        })
    }

    /// Delete a bucket.
    pub async fn delete_bucket(&self, bucket_name: &str) -> Result<()> {
        info!("Deleting S3 bucket: {}", bucket_name);
        self.client
            .delete_bucket()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!("Failed to delete S3 bucket: {bucket_name}"))?;

        info!("Bucket '{}' deleted successfully", bucket_name);
        Ok(())
    }

    /// Delete a single object, optionally by version ID.
    pub async fn delete_object(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<()> {
        info!("Deleting object: s3://{}/{}", bucket_name, object_key);

        let mut request = self
            .client
            .delete_object()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        request.send().await.context(format!(
            "Failed to delete object s3://{bucket_name}/{object_key}"
        ))?;

        info!(
            "Object '{}' deleted successfully from bucket '{}'",
            object_key, bucket_name
        );
        Ok(())
    }

    /// Delete up to 1,000 objects in a single batch request.
    pub async fn delete_objects(
        &self,
        bucket_name: &str,
        object_keys: &[String],
    ) -> Result<DeleteObjectsResult> {
        info!(
            "Batch deleting {} objects from bucket '{}'",
            object_keys.len(),
            bucket_name
        );

        if object_keys.is_empty() {
            anyhow::bail!("No object keys provided for batch deletion");
        }
        if object_keys.len() > 1000 {
            anyhow::bail!(
                "Cannot delete more than 1000 objects in a single request (got {}); \
                 split the list into smaller batches",
                object_keys.len()
            );
        }

        let identifiers = object_keys
            .iter()
            .map(|key| ObjectIdentifier::builder().key(key).build())
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to build object identifiers")?;

        let delete = Delete::builder()
            .set_objects(Some(identifiers))
            .build()
            .context("Failed to build batch delete request")?;

        let response = self
            .client
            .delete_objects()
            .bucket(bucket_name)
            .delete(delete)
            .send()
            .await
            .context(format!(
                "Failed to batch delete objects in bucket '{bucket_name}'"
            ))?;

        let deleted: Vec<String> = response
            .deleted()
            .iter()
            .filter_map(|d| d.key())
            .map(|k| k.to_string())
            .collect();

        let errors: Vec<DeleteObjectsError> = response
            .errors()
            .iter()
            .map(|e| DeleteObjectsError {
                key: e.key().unwrap_or("unknown").to_string(),
                message: e.message().unwrap_or("unknown error").to_string(),
            })
            .collect();

        debug!(
            "Batch delete completed: {} deleted, {} errors",
            deleted.len(),
            errors.len()
        );

        Ok(DeleteObjectsResult {
            bucket: bucket_name.to_string(),
            deleted,
            errors,
        })
    }

    /// List all accessible buckets.
    pub async fn list_buckets(&self) -> Result<Vec<BucketInfo>> {
        debug!("Listing S3 buckets");

        let response = self
            .client
            .list_buckets()
            .send()
            .await
            .context("Failed to list S3 buckets")?;

        let buckets: Vec<BucketInfo> = response
            .buckets()
            .iter()
            .map(|bucket| {
                let name = bucket.name().unwrap_or("unknown").to_string();
                let creation_date = bucket.creation_date().map(|dt| {
                    dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                        .unwrap()
                });

                BucketInfo {
                    name,
                    creation_date,
                }
            })
            .collect();

        debug!("Found {} buckets", buckets.len());
        Ok(buckets)
    }

    /// List objects in a bucket with optional pagination/filtering.
    pub async fn list_objects_v2(
        &self,
        bucket_name: &str,
        options: ListObjectsOptions,
    ) -> Result<ListObjectsResult> {
        debug!(
            "Listing objects in bucket '{}' with options: {:?}",
            bucket_name, options
        );

        let mut request = self.client.list_objects_v2().bucket(bucket_name);

        if let Some(prefix) = options.prefix {
            request = request.prefix(prefix);
        }

        if let Some(delimiter) = options.delimiter {
            request = request.delimiter(delimiter);
        }

        if let Some(max_keys) = options.max_keys {
            request = request.max_keys(max_keys);
        }

        if let Some(continuation_token) = options.continuation_token {
            request = request.continuation_token(continuation_token);
        }

        if let Some(start_after) = options.start_after {
            request = request.start_after(start_after);
        }

        let response = request
            .send()
            .await
            .context(format!("Failed to list objects in bucket '{bucket_name}'"))?;

        let objects: Vec<ObjectInfo> = response
            .contents()
            .iter()
            .map(|obj| {
                let key = obj.key().unwrap_or("unknown").to_string();
                let size = obj.size();
                let last_modified = obj.last_modified().map(|dt| {
                    dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                        .unwrap()
                });
                let etag = obj.e_tag().map(|e| e.to_string());
                let storage_class = obj.storage_class().map(|sc| sc.as_str().to_string());

                ObjectInfo {
                    key,
                    size,
                    last_modified,
                    etag,
                    storage_class,
                }
            })
            .collect();

        let common_prefixes: Vec<String> = response
            .common_prefixes()
            .iter()
            .filter_map(|cp| cp.prefix())
            .map(|p| p.to_string())
            .collect();

        let result = ListObjectsResult {
            objects,
            common_prefixes,
            is_truncated: response.is_truncated().unwrap_or(false),
            next_continuation_token: response.next_continuation_token().map(|t| t.to_string()),
            max_keys: response.max_keys(),
            key_count: response.key_count().unwrap_or(0),
        };

        debug!(
            "Found {} objects and {} common prefixes in bucket '{}'",
            result.objects.len(),
            result.common_prefixes.len(),
            bucket_name
        );

        Ok(result)
    }

    /// Upload a local file to S3.
    pub async fn upload_file(
        &self,
        local_path: &str,
        bucket_name: &str,
        object_key: &str,
        options: UploadFileOptions,
    ) -> Result<UploadResult> {
        info!(
            "Starting file upload: '{}' -> s3://{}/{}",
            local_path, bucket_name, object_key
        );

        let path = Path::new(local_path);
        let canonical_path = path
            .canonicalize()
            .context(format!("Failed to resolve file path: {local_path}"))?;

        if !canonical_path.exists() {
            anyhow::bail!("File does not exist: {local_path}");
        }

        if !canonical_path.is_file() {
            anyhow::bail!("Path is not a file: {local_path}");
        }

        let metadata = tokio::fs::metadata(&canonical_path)
            .await
            .context(format!("Failed to read file metadata: {local_path}"))?;

        let file_size = metadata.len();
        debug!("File size: {file_size} bytes");

        let content_type = options.content_type.unwrap_or_else(|| {
            let detected = mime_guess::from_path(&canonical_path)
                .first_or_octet_stream()
                .to_string();
            debug!("Auto-detected content type: {detected}");
            detected
        });

        let file_content = tokio::fs::read(&canonical_path)
            .await
            .context(format!("Failed to read file content: {local_path}"))?;

        let byte_stream = ByteStream::from(file_content);

        let mut request = self
            .client
            .put_object()
            .bucket(bucket_name)
            .key(object_key)
            .body(byte_stream)
            .content_type(&content_type)
            .content_length(file_size as i64);

        if let Some(storage_class) = &options.storage_class {
            request = request.storage_class(storage_class.as_str().into());
        }

        if let Some(cache_control) = &options.cache_control {
            request = request.cache_control(cache_control);
        }

        if let Some(content_disposition) = &options.content_disposition {
            request = request.content_disposition(content_disposition);
        }

        if let Some(content_encoding) = &options.content_encoding {
            request = request.content_encoding(content_encoding);
        }

        if let Some(content_language) = &options.content_language {
            request = request.content_language(content_language);
        }

        if let Some(sse) = &options.server_side_encryption {
            request = request.server_side_encryption(sse.as_str().into());
        }

        if let Some(metadata_map) = &options.metadata {
            for (key, value) in metadata_map {
                request = request.metadata(key, value);
            }
        }

        debug!("Executing S3 put_object request");
        let response = request.send().await.context(format!(
            "Failed to upload file to s3://{bucket_name}/{object_key}"
        ))?;

        let etag = response.e_tag().unwrap_or("unknown").to_string();
        let version_id = response.version_id().map(|v| v.to_string());

        let location = format!("s3://{bucket_name}/{object_key}");

        let upload_result = UploadResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            etag,
            location,
            version_id,
            file_size,
            content_type,
            upload_id: None,
        };

        info!(
            "File upload completed successfully: {} bytes uploaded to s3://{}/{}",
            file_size, bucket_name, object_key
        );

        Ok(upload_result)
    }

    /// Copy an object server-side within or across buckets.
    pub async fn copy_object(
        &self,
        source_bucket: &str,
        source_key: &str,
        destination_bucket: &str,
        destination_key: &str,
        options: CopyObjectOptions,
    ) -> Result<CopyObjectResultData> {
        info!(
            "Copying object: s3://{}/{} -> s3://{}/{}",
            source_bucket, source_key, destination_bucket, destination_key
        );

        let copy_source = format!("{}/{}", source_bucket, encode_copy_source_key(source_key));

        let mut request = self
            .client
            .copy_object()
            .bucket(destination_bucket)
            .key(destination_key)
            .copy_source(copy_source);

        // Content-type/cache-control overrides require the REPLACE directive,
        // which resets all metadata. Storage class, however, is applied
        // independently of the directive and does not affect metadata.
        let mut effective = None;
        if options.content_type.is_some() || options.cache_control.is_some() {
            // Read the source object's current headers so that anything not
            // overridden is preserved instead of being reset by REPLACE.
            let source_meta = self
                .head_object(source_bucket, source_key, None)
                .await
                .context(format!(
                    "Failed to read source metadata from s3://{source_bucket}/{source_key} before applying copy overrides"
                ))?;

            let metadata = effective_copy_metadata(&options, &source_meta);

            request = request.metadata_directive(aws_sdk_s3::types::MetadataDirective::Replace);

            if let Some(ref content_type) = metadata.content_type {
                request = request.content_type(content_type);
            }
            if let Some(ref cache_control) = metadata.cache_control {
                request = request.cache_control(cache_control);
            }
            if let Some(ref content_encoding) = metadata.content_encoding {
                request = request.content_encoding(content_encoding);
            }
            if let Some(ref content_disposition) = metadata.content_disposition {
                request = request.content_disposition(content_disposition);
            }
            if let Some(ref content_language) = metadata.content_language {
                request = request.content_language(content_language);
            }
            for (key, value) in &metadata.user_metadata {
                request = request.metadata(key, value);
            }

            effective = Some(metadata);
        }

        if let Some(storage_class) = &options.storage_class {
            request = request.storage_class(storage_class.as_str().into());
        }

        debug!("Executing S3 copy_object request");
        let response = request.send().await.context(format!(
            "Failed to copy s3://{source_bucket}/{source_key} to s3://{destination_bucket}/{destination_key}"
        ))?;

        let copy_result = response.copy_object_result();
        let result = CopyObjectResultData {
            source_bucket: source_bucket.to_string(),
            source_key: source_key.to_string(),
            destination_bucket: destination_bucket.to_string(),
            destination_key: destination_key.to_string(),
            etag: copy_result.and_then(|r| r.e_tag()).map(|e| e.to_string()),
            last_modified: copy_result.and_then(|r| r.last_modified()).map(|dt| {
                dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                    .unwrap()
            }),
            version_id: response.version_id().map(|v| v.to_string()),
            // CopyObjectOutput does not echo these back; report what was applied.
            content_type: effective.as_ref().and_then(|m| m.content_type.clone()),
            storage_class: options.storage_class,
        };

        info!(
            "Object copied successfully: s3://{}/{} -> s3://{}/{}",
            source_bucket, source_key, destination_bucket, destination_key
        );

        Ok(result)
    }

    /// Fetch object content and metadata from S3.
    pub async fn get_object(
        &self,
        bucket_name: &str,
        object_key: &str,
        options: GetObjectOptions,
    ) -> Result<GetObjectResult> {
        info!("Getting object: s3://{}/{}", bucket_name, object_key);

        let mut request = self.client.get_object().bucket(bucket_name).key(object_key);

        if let Some(version_id) = &options.version_id {
            request = request.version_id(version_id);
        }

        if let Some(range) = &options.range {
            request = request.range(range);
        }

        if let Some(if_modified_since) = &options.if_modified_since {
            request = request.if_modified_since(
                aws_sdk_s3::primitives::DateTime::from_str(
                    if_modified_since,
                    aws_sdk_s3::primitives::DateTimeFormat::DateTime,
                )
                .context("Failed to parse if_modified_since date")?,
            );
        }

        if let Some(if_unmodified_since) = &options.if_unmodified_since {
            request = request.if_unmodified_since(
                aws_sdk_s3::primitives::DateTime::from_str(
                    if_unmodified_since,
                    aws_sdk_s3::primitives::DateTimeFormat::DateTime,
                )
                .context("Failed to parse if_unmodified_since date")?,
            );
        }

        debug!("Executing S3 get_object request");
        let response = request.send().await.context(format!(
            "Failed to get object from s3://{bucket_name}/{object_key}"
        ))?;

        let content_type = response
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let content_length = response.content_length().unwrap_or(0) as u64;
        let last_modified = response.last_modified().map(|dt| {
            dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                .unwrap()
        });
        let etag = response.e_tag().map(|e| e.to_string());
        let version_id = response.version_id().map(|v| v.to_string());

        let max_size = options.max_content_size.unwrap_or(10 * 1024 * 1024);
        let mut content = Vec::new();
        let mut byte_stream = response.body;
        let mut total_read = 0;

        while let Some(bytes_result) = byte_stream
            .try_next()
            .await
            .context("Failed to read object content")?
        {
            if total_read + bytes_result.len() > max_size {
                anyhow::bail!("Object size exceeds maximum allowed size of {max_size} bytes");
            }
            content.extend_from_slice(&bytes_result);
            total_read += bytes_result.len();
        }

        debug!("Read {} bytes from object", content.len());

        let detected_type = Self::detect_file_type(Some(&content_type), &content);
        debug!("Detected file type: {detected_type:?}");

        let text_content = match &detected_type {
            DetectedFileType::Text => match std::str::from_utf8(&content) {
                Ok(text) => Some(text.to_string()),
                Err(_) => {
                    debug!("Failed to decode content as UTF-8, treating as binary");
                    None
                }
            },
            _ => None,
        };

        let result = GetObjectResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            content_type,
            content_length,
            last_modified,
            etag,
            version_id,
            detected_type,
            content: Some(content),
            text_content,
        };

        info!(
            "Object retrieved successfully: {} bytes from s3://{}/{}",
            result.content_length, bucket_name, object_key
        );

        Ok(result)
    }

    fn detect_file_type(content_type: Option<&str>, content_bytes: &[u8]) -> DetectedFileType {
        if let Some(ct) = content_type {
            let ct_lower = ct.to_lowercase();

            if ct_lower.starts_with("text/")
                || ct_lower == "application/json"
                || ct_lower == "application/xml"
                || ct_lower == "application/yaml"
                || ct_lower == "application/javascript"
                || ct_lower == "application/x-yaml"
                || ct_lower == "application/x-sh"
                || ct_lower == "application/x-shellscript"
                || ct_lower.contains("script")
                || ct_lower.contains("xml")
                || ct_lower.contains("json")
            {
                return DetectedFileType::Text;
            }

            return DetectedFileType::NonText(ct.to_string());
        }

        if content_bytes.len() >= 4 {
            match &content_bytes[0..4] {
                // PNG: 89 50 4E 47
                [0x89, 0x50, 0x4E, 0x47] => {
                    return DetectedFileType::NonText("image/png".to_string());
                }
                // JPEG: FF D8 FF
                [0xFF, 0xD8, 0xFF, _] => {
                    return DetectedFileType::NonText("image/jpeg".to_string());
                }
                // GIF: 47 49 46 38
                [0x47, 0x49, 0x46, 0x38] => {
                    return DetectedFileType::NonText("image/gif".to_string());
                }
                // BMP: 42 4D
                [0x42, 0x4D, _, _] => return DetectedFileType::NonText("image/bmp".to_string()),
                // RIFF container (WebP/WAV)
                [0x52, 0x49, 0x46, 0x46] if content_bytes.len() >= 12 => {
                    if &content_bytes[8..12] == b"WEBP" {
                        return DetectedFileType::NonText("image/webp".to_string());
                    } else if &content_bytes[8..12] == b"WAVE" {
                        return DetectedFileType::NonText("audio/wav".to_string());
                    }
                    return DetectedFileType::NonText("application/octet-stream".to_string());
                }
                _ => {}
            }
        }

        // 3. Check if content is valid UTF-8 text as fallback
        if std::str::from_utf8(content_bytes).is_ok() {
            // Additional heuristics for text detection
            let non_printable_count = content_bytes
                .iter()
                .filter(|&&b| b < 0x20 && b != 0x09 && b != 0x0A && b != 0x0D) // Control chars except tab, LF, CR
                .count();
            let total_chars = content_bytes.len();

            // If less than 5% are non-printable control characters, consider it text
            if total_chars > 0 && (non_printable_count as f64 / total_chars as f64) < 0.05 {
                return DetectedFileType::Text;
            }
        }

        // Default to non-text binary
        DetectedFileType::NonText("application/octet-stream".to_string())
    }

    /// Download an S3 object directly to a local file path.
    pub async fn download_object_to_file(
        &self,
        bucket_name: &str,
        object_key: &str,
        local_path: &str,
        options: GetObjectOptions,
    ) -> Result<(u64, String)> {
        info!(
            "Downloading object: s3://{}/{} -> {}",
            bucket_name, object_key, local_path
        );

        let mut request = self.client.get_object().bucket(bucket_name).key(object_key);

        if let Some(version_id) = &options.version_id {
            request = request.version_id(version_id);
        }

        if let Some(range) = &options.range {
            request = request.range(range);
        }

        if let Some(if_modified_since) = &options.if_modified_since {
            request = request.if_modified_since(
                aws_sdk_s3::primitives::DateTime::from_str(
                    if_modified_since,
                    aws_sdk_s3::primitives::DateTimeFormat::DateTime,
                )
                .context("Failed to parse if_modified_since date")?,
            );
        }

        if let Some(if_unmodified_since) = &options.if_unmodified_since {
            request = request.if_unmodified_since(
                aws_sdk_s3::primitives::DateTime::from_str(
                    if_unmodified_since,
                    aws_sdk_s3::primitives::DateTimeFormat::DateTime,
                )
                .context("Failed to parse if_unmodified_since date")?,
            );
        }

        debug!("Executing S3 get_object request for download");
        let response = request.send().await.context(format!(
            "Failed to get object from s3://{bucket_name}/{object_key}"
        ))?;

        let local_file_path = Path::new(local_path);

        if let Some(parent) = local_file_path.parent() {
            tokio::fs::create_dir_all(parent).await.context(format!(
                "Failed to create parent directories for {local_path}"
            ))?;
        }

        let mut file = tokio::fs::File::create(local_file_path)
            .await
            .context(format!("Failed to create local file: {local_path}"))?;

        let mut byte_stream = response.body;
        let mut total_bytes = 0u64;

        while let Some(bytes_result) = byte_stream
            .try_next()
            .await
            .context("Failed to read object content")?
        {
            file.write_all(&bytes_result)
                .await
                .context(format!("Failed to write to local file: {local_path}"))?;
            total_bytes += bytes_result.len() as u64;
        }

        file.flush().await.context("Failed to flush file to disk")?;

        let absolute_path = local_file_path
            .canonicalize()
            .unwrap_or_else(|_| local_file_path.to_path_buf())
            .to_string_lossy()
            .to_string();

        info!(
            "Object downloaded successfully: {} bytes from s3://{}/{} to {}",
            total_bytes, bucket_name, object_key, absolute_path
        );

        Ok((total_bytes, absolute_path))
    }

    /// Retrieve object metadata without downloading its content.
    pub async fn head_object(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<HeadObjectResult> {
        debug!(
            "Fetching metadata for object: s3://{}/{}",
            bucket_name, object_key
        );

        let mut request = self
            .client
            .head_object()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        let response = request.send().await.context(format!(
            "Failed to fetch metadata for s3://{bucket_name}/{object_key}"
        ))?;

        let last_modified = response.last_modified().map(|dt| {
            dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                .unwrap()
        });

        let result = HeadObjectResult {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            size: response.content_length(),
            last_modified,
            etag: response.e_tag().map(|e| e.to_string()),
            content_type: response.content_type().map(|ct| ct.to_string()),
            cache_control: response.cache_control().map(|cc| cc.to_string()),
            content_encoding: response.content_encoding().map(|ce| ce.to_string()),
            content_disposition: response.content_disposition().map(|cd| cd.to_string()),
            content_language: response.content_language().map(|cl| cl.to_string()),
            storage_class: response.storage_class().map(|sc| sc.as_str().to_string()),
            version_id: response.version_id().map(|v| v.to_string()),
            user_metadata: response.metadata().cloned().unwrap_or_default(),
        };

        debug!(
            "Object metadata retrieved: {} bytes",
            result.size.unwrap_or(0)
        );

        Ok(result)
    }

    /// Generate a presigned URL for downloading or uploading an object.
    ///
    /// The URL embeds SigV4 authentication in its query string, so the
    /// caller can perform the operation without credentials until it expires.
    pub async fn presigned_url(
        &self,
        bucket_name: &str,
        object_key: &str,
        method: PresignMethod,
        expires_in_secs: u64,
    ) -> Result<PresignedUrlResult> {
        info!(
            "Generating presigned {:?} URL for s3://{}/{} (expires in {}s)",
            method, bucket_name, object_key, expires_in_secs
        );

        if !(1..=604800).contains(&expires_in_secs) {
            anyhow::bail!(
                "expires_in_secs must be between 1 and 604800 seconds (7 days), got {expires_in_secs}"
            );
        }

        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::builder()
            .expires_in(std::time::Duration::from_secs(expires_in_secs))
            .build()
            .context("Failed to build presigning config")?;

        let presigned = match method {
            PresignMethod::Get => self
                .client
                .get_object()
                .bucket(bucket_name)
                .key(object_key)
                .presigned(presigning_config)
                .await
                .context(format!(
                    "Failed to generate presigned URL for s3://{bucket_name}/{object_key}"
                ))?,
            PresignMethod::Put => self
                .client
                .put_object()
                .bucket(bucket_name)
                .key(object_key)
                .presigned(presigning_config)
                .await
                .context(format!(
                    "Failed to generate presigned URL for s3://{bucket_name}/{object_key}"
                ))?,
        };

        let method_label = match method {
            PresignMethod::Get => "GET",
            PresignMethod::Put => "PUT",
        };

        // Never log the URL itself: it embeds SigV4 credentials in its query string.
        debug!(
            "Presigned {} URL generated for s3://{}/{}",
            method_label, bucket_name, object_key
        );

        Ok(PresignedUrlResult {
            url: presigned.uri().to_string(),
            method: method_label.to_string(),
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            expires_in_secs,
        })
    }

    /// Verify S3 connectivity by performing a lightweight list-buckets call.
    pub async fn health_check(&self) -> Result<()> {
        debug!("Performing S3 health check");

        self.client
            .list_buckets()
            .send()
            .await
            .context("S3 health check failed")?;

        debug!("S3 health check passed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires AWS credentials
    async fn test_s3_client_creation() {
        let config = Config {
            access_key_id: Some("test_key".to_string()),
            secret_access_key: Some("test_secret".to_string()),
            region: "us-east-1".to_string(),
            ..Config::default()
        };

        let result = S3Client::new(&config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_bucket_info_serialization() {
        let bucket = BucketInfo {
            name: "test-bucket".to_string(),
            creation_date: Some("2024-01-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&bucket).unwrap();
        let deserialized: BucketInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(bucket.name, deserialized.name);
        assert_eq!(bucket.creation_date, deserialized.creation_date);
    }

    #[test]
    fn test_detect_file_type_text_content_type() {
        let test_cases = vec![
            ("text/plain", "Hello world"),
            ("text/html", "<html></html>"),
            ("application/json", r#"{"key": "value"}"#),
            ("application/xml", "<xml></xml>"),
            ("application/yaml", "key: value"),
            ("application/javascript", "console.log('hello');"),
        ];

        for (content_type, content) in test_cases {
            let result = S3Client::detect_file_type(Some(content_type), content.as_bytes());
            match result {
                DetectedFileType::Text => {}
                _ => panic!("Expected Text for content type {content_type}"),
            }
        }
    }

    #[test]
    fn test_detect_file_type_non_text_content_type() {
        // Test various non-text content types
        let test_cases = vec![
            ("image/png", "image/png"),
            ("image/jpeg", "image/jpeg"),
            ("audio/mp3", "audio/mp3"),
            ("video/mp4", "video/mp4"),
            ("application/pdf", "application/pdf"),
        ];

        for (content_type, expected_mime) in test_cases {
            let result = S3Client::detect_file_type(Some(content_type), b"some content");
            match result {
                DetectedFileType::NonText(mime_type) => {
                    assert_eq!(mime_type, expected_mime);
                }
                _ => panic!("Expected NonText for content type {content_type}"),
            }
        }
    }

    #[test]
    fn test_detect_file_type_magic_bytes_simplified() {
        // Test magic bytes detection (now all return NonText)
        let test_cases = vec![
            // PNG magic bytes: 89 50 4E 47
            (
                vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
                "image/png",
            ),
            // JPEG magic bytes: FF D8 FF
            (vec![0xFF, 0xD8, 0xFF, 0xE0], "image/jpeg"),
            // GIF magic bytes: 47 49 46 38
            (vec![0x47, 0x49, 0x46, 0x38, 0x37, 0x61], "image/gif"),
        ];

        for (content, expected_mime) in test_cases {
            let result = S3Client::detect_file_type(None, &content);
            match result {
                DetectedFileType::NonText(mime_type) => {
                    assert_eq!(mime_type, expected_mime);
                }
                _ => panic!("Expected NonText for magic bytes: {content:?}"),
            }
        }
    }

    #[test]
    fn test_detect_file_type_webp_magic_bytes() {
        // WebP has more complex magic bytes: RIFF....WEBP
        let mut webp_content = vec![0x52, 0x49, 0x46, 0x46]; // RIFF
        webp_content.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Size (4 bytes)
        webp_content.extend_from_slice(b"WEBP"); // WEBP signature

        let result = S3Client::detect_file_type(None, &webp_content);
        match result {
            DetectedFileType::NonText(mime_type) => {
                assert_eq!(mime_type, "image/webp");
            }
            _ => panic!("Expected WebP NonText detection"),
        }
    }

    #[test]
    fn test_detect_file_type_wav_magic_bytes() {
        // WAV has magic bytes: RIFF....WAVE
        let mut wav_content = vec![0x52, 0x49, 0x46, 0x46]; // RIFF
        wav_content.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Size (4 bytes)
        wav_content.extend_from_slice(b"WAVE"); // WAVE signature

        let result = S3Client::detect_file_type(None, &wav_content);
        match result {
            DetectedFileType::NonText(mime_type) => {
                assert_eq!(mime_type, "audio/wav");
            }
            _ => panic!("Expected WAV NonText detection"),
        }
    }

    #[test]
    fn test_detect_file_type_utf8_text() {
        // Test UTF-8 text detection
        let utf8_content = "Hello, World! 🌍".as_bytes();
        let result = S3Client::detect_file_type(None, utf8_content);
        match result {
            DetectedFileType::Text => {}
            _ => panic!("Expected Text for UTF-8 content"),
        }

        // Test ASCII text
        let ascii_content = b"Hello, world! This is ASCII text.";
        let result = S3Client::detect_file_type(None, ascii_content);
        match result {
            DetectedFileType::Text => {}
            _ => panic!("Expected Text for ASCII content"),
        }
    }

    #[test]
    fn test_detect_file_type_binary() {
        // Test binary content that should not be detected as text
        let binary_content = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD, 0xFC];
        let result = S3Client::detect_file_type(None, &binary_content);
        match result {
            DetectedFileType::NonText(mime_type) => {
                assert_eq!(mime_type, "application/octet-stream");
            }
            _ => panic!("Expected NonText for binary content"),
        }
    }

    #[test]
    fn test_detect_file_type_priority() {
        // Content-Type should take priority over magic bytes
        let png_magic_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

        // Even with PNG magic bytes, text content-type should win
        let result = S3Client::detect_file_type(Some("text/plain"), &png_magic_bytes);
        match result {
            DetectedFileType::Text => {}
            _ => panic!("Expected Text due to content-type priority"),
        }
    }

    #[test]
    fn test_get_object_options_default() {
        let options = GetObjectOptions::default();
        assert!(options.version_id.is_none());
        assert!(options.range.is_none());
        assert!(options.if_modified_since.is_none());
        assert!(options.if_unmodified_since.is_none());
        assert!(options.max_content_size.is_none());
    }

    #[test]
    fn test_list_objects_options_default() {
        let options = ListObjectsOptions::default();
        assert!(options.prefix.is_none());
        assert!(options.delimiter.is_none());
        assert!(options.max_keys.is_none());
        assert!(options.continuation_token.is_none());
        assert!(options.start_after.is_none());
    }

    #[test]
    fn test_presign_method_serialization() {
        let json = serde_json::to_string(&PresignMethod::Put).unwrap();
        assert_eq!(json, r#""put""#);

        let deserialized: PresignMethod = serde_json::from_str(r#""get""#).unwrap();
        assert_eq!(deserialized, PresignMethod::Get);
    }

    #[test]
    fn test_presigned_url_result_serialization() {
        let result = PresignedUrlResult {
            url: "https://example.com/bucket/key?X-Amz-Signature=abc".to_string(),
            method: "GET".to_string(),
            bucket: "test-bucket".to_string(),
            key: "docs/a.txt".to_string(),
            expires_in_secs: 3600,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: PresignedUrlResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.method, "GET");
        assert_eq!(deserialized.expires_in_secs, 3600);
        assert!(deserialized.url.contains("X-Amz-Signature"));
    }

    #[test]
    fn test_detected_file_type_serialization() {
        let test_cases = vec![
            DetectedFileType::Text,
            DetectedFileType::NonText("image/png".to_string()),
            DetectedFileType::NonText("audio/mpeg".to_string()),
            DetectedFileType::NonText("application/octet-stream".to_string()),
        ];

        for file_type in test_cases {
            let json = serde_json::to_string(&file_type).unwrap();
            let deserialized: DetectedFileType = serde_json::from_str(&json).unwrap();

            match (&file_type, &deserialized) {
                (DetectedFileType::Text, DetectedFileType::Text) => {}
                (DetectedFileType::NonText(a), DetectedFileType::NonText(b)) => assert_eq!(a, b),
                _ => panic!("Serialization/deserialization mismatch"),
            }
        }
    }

    #[test]
    fn test_encode_copy_source_key() {
        assert_eq!(encode_copy_source_key("plain-key.txt"), "plain-key.txt");
        assert_eq!(
            encode_copy_source_key("path/with spaces/file name.json"),
            "path/with%20spaces/file%20name.json"
        );
        assert_eq!(
            encode_copy_source_key("unicode/文件.txt"),
            "unicode/%E6%96%87%E4%BB%B6.txt"
        );
        assert_eq!(
            encode_copy_source_key("special~chars_-.,()"),
            "special~chars_-.%2C%28%29"
        );
    }

    #[test]
    fn test_delete_objects_result_serialization() {
        let result = DeleteObjectsResult {
            bucket: "test-bucket".to_string(),
            deleted: vec!["a.txt".to_string(), "b.txt".to_string()],
            errors: vec![DeleteObjectsError {
                key: "locked.txt".to_string(),
                message: "Access Denied".to_string(),
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: DeleteObjectsResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.deleted.len(), 2);
        assert_eq!(deserialized.errors.len(), 1);
        assert_eq!(deserialized.errors[0].key, "locked.txt");
    }

    #[test]
    fn test_copy_object_options_default() {
        let options = CopyObjectOptions::default();
        assert!(options.content_type.is_none());
        assert!(options.storage_class.is_none());
        assert!(options.cache_control.is_none());
    }

    #[test]
    fn test_head_object_result_serialization() {
        let result = HeadObjectResult {
            bucket: "test-bucket".to_string(),
            key: "docs/report.pdf".to_string(),
            size: Some(2048),
            last_modified: Some("2026-11-23T00:00:00Z".to_string()),
            etag: Some("\"abc123\"".to_string()),
            content_type: Some("application/pdf".to_string()),
            cache_control: Some("max-age=3600".to_string()),
            content_encoding: Some("gzip".to_string()),
            content_disposition: Some("attachment".to_string()),
            content_language: Some("en-US".to_string()),
            storage_class: Some("STANDARD".to_string()),
            version_id: Some("v1".to_string()),
            user_metadata: std::collections::HashMap::from([(
                "author".to_string(),
                "rustfs".to_string(),
            )]),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: HeadObjectResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.key, "docs/report.pdf");
        assert_eq!(deserialized.size, Some(2048));
        assert_eq!(deserialized.content_encoding, Some("gzip".to_string()));
        assert_eq!(
            deserialized.user_metadata.get("author"),
            Some(&"rustfs".to_string())
        );
    }

    #[test]
    fn test_effective_copy_metadata_preserves_source() {
        let source = HeadObjectResult {
            bucket: "src".to_string(),
            key: "data.gz".to_string(),
            size: Some(1024),
            last_modified: None,
            etag: None,
            content_type: Some("application/octet-stream".to_string()),
            cache_control: Some("max-age=60".to_string()),
            content_encoding: Some("gzip".to_string()),
            content_disposition: Some("attachment".to_string()),
            content_language: Some("en-US".to_string()),
            storage_class: Some("STANDARD".to_string()),
            version_id: None,
            user_metadata: std::collections::HashMap::from([(
                "owner".to_string(),
                "rustfs".to_string(),
            )]),
        };

        // Overrides take precedence; unspecified fields fall back to the source.
        let options = CopyObjectOptions {
            content_type: Some("text/plain".to_string()),
            cache_control: None,
            storage_class: Some("REDUCED_REDUNDANCY".to_string()),
        };
        let effective = effective_copy_metadata(&options, &source);

        assert_eq!(effective.content_type, Some("text/plain".to_string()));
        assert_eq!(effective.cache_control, Some("max-age=60".to_string()));
        assert_eq!(effective.content_encoding, Some("gzip".to_string()));
        assert_eq!(
            effective.user_metadata.get("owner"),
            Some(&"rustfs".to_string())
        );

        // No content-type/cache-control overrides keeps source values as-is.
        let options = CopyObjectOptions::default();
        let effective = effective_copy_metadata(&options, &source);

        assert_eq!(
            effective.content_type,
            Some("application/octet-stream".to_string())
        );
        assert_eq!(effective.cache_control, Some("max-age=60".to_string()));
    }
}
