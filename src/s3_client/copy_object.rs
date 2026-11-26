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
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::S3Client;
use super::head_object::HeadObjectResult;

/// Options for server-side object copy.
#[derive(Debug, Clone, Default)]
pub struct CopyObjectOptions {
    pub content_type: Option<String>,
    pub storage_class: Option<String>,
    pub cache_control: Option<String>,
    /// Copy this specific version of the source object (the latest one when
    /// unset), e.g. to roll back to an earlier version.
    pub source_version_id: Option<String>,
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

/// Percent-encode an S3 object key for the `x-amz-copy-source` header,
/// preserving `/` path separators. Unreserved characters
/// (`A-Z a-z 0-9 - _ . ~`) are kept as-is; everything else becomes `%XX`.
fn encode_copy_source_key(key: &str) -> String {
    percent_encode(key, true)
}

/// Percent-encode a version ID for the `versionId` part of the
/// `x-amz-copy-source` header. A version ID is an opaque token, so `/` is
/// encoded like any other reserved character.
fn encode_copy_source_version(version_id: &str) -> String {
    percent_encode(version_id, false)
}

/// Build the `x-amz-copy-source` header value for an object or, when a
/// version ID is given, for one specific version of it.
fn copy_source_header(
    source_bucket: &str,
    source_key: &str,
    source_version_id: Option<&str>,
) -> String {
    let source = format!("{source_bucket}/{}", encode_copy_source_key(source_key));
    match source_version_id {
        Some(version_id) => format!(
            "{source}?versionId={}",
            encode_copy_source_version(version_id)
        ),
        None => source,
    }
}

/// Percent-encode `value`, keeping unreserved characters
/// (`A-Z a-z 0-9 - _ . ~`) and, when `keep_slash` is set, `/`.
fn percent_encode(value: &str, keep_slash: bool) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let unreserved =
            matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~');
        if unreserved || (keep_slash && byte == b'/') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

impl S3Client {
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

        let copy_source = copy_source_header(
            source_bucket,
            source_key,
            options.source_version_id.as_deref(),
        );

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
            // Read the source version's current headers so that anything not
            // overridden is preserved instead of being reset by REPLACE.
            let source_meta = self
                .head_object(
                    source_bucket,
                    source_key,
                    options.source_version_id.as_deref(),
                )
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_object_options_default() {
        let options = CopyObjectOptions::default();
        assert!(options.content_type.is_none());
        assert!(options.storage_class.is_none());
        assert!(options.cache_control.is_none());
        assert!(options.source_version_id.is_none());
    }

    #[test]
    fn test_copy_source_header() {
        assert_eq!(
            copy_source_header("src", "plain.txt", None),
            "src/plain.txt"
        );
        assert_eq!(
            copy_source_header("src", "with spaces/文件.txt", None),
            "src/with%20spaces/%E6%96%87%E4%BB%B6.txt"
        );

        // A version ID is opaque: `/` and other reserved characters are encoded.
        assert_eq!(
            copy_source_header(
                "src",
                "a.txt",
                Some("3sL4kqtJlcpXroDTDmJ+rmSpXd3dIbrHY+MTRCxf3vjVBH40Nr8X8gdRQBpUMLUo")
            ),
            "src/a.txt?versionId=3sL4kqtJlcpXroDTDmJ%2BrmSpXd3dIbrHY%2BMTRCxf3vjVBH40Nr8X8gdRQBpUMLUo"
        );
        assert_eq!(encode_copy_source_version("a/b=c"), "a%2Fb%3Dc");
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
            object_lock_mode: None,
            object_lock_retain_until: None,
            object_lock_legal_hold: None,
        };

        // Overrides take precedence; unspecified fields fall back to the source.
        let options = CopyObjectOptions {
            content_type: Some("text/plain".to_string()),
            cache_control: None,
            storage_class: Some("REDUCED_REDUNDANCY".to_string()),
            source_version_id: None,
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
