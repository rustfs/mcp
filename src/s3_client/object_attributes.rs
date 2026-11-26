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
use aws_sdk_s3::primitives::DateTimeFormat;
use aws_sdk_s3::types::ObjectAttributes;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::S3Client;

/// Largest number of parts S3 lists per response.
const MAX_PARTS_PER_PAGE: i32 = 1000;

/// A checksum the object was stored with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checksum {
    /// Algorithm, e.g. `CRC32`, `CRC64NVME`, `SHA256`, or `SHA512`.
    pub algorithm: String,
    /// Base64-encoded checksum value.
    pub value: String,
}

/// One part of a multipart object, as reported by the storage backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectPart {
    pub part_number: Option<i32>,
    pub size: Option<i64>,
}

/// Result payload for reading the attributes of an object version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectAttributesInfo {
    pub bucket: String,
    pub key: String,
    /// Version whose attributes were read; the current version when unset.
    pub version_id: Option<String>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub size: Option<i64>,
    pub storage_class: Option<String>,
    /// Whether the version is a delete marker.
    pub delete_marker: bool,
    /// Checksums the object was stored with; empty when none was recorded.
    pub checksums: Vec<Checksum>,
    /// Whether each checksum covers the whole object (`FULL_OBJECT`) or the
    /// parts of it (`COMPOSITE`); `None` when the backend does not say.
    pub checksum_type: Option<String>,
    /// The parts reported for the object; empty when the storage backend
    /// reported no part details.
    pub parts: Vec<ObjectPart>,
    pub total_parts_count: Option<i32>,
}

/// The checksums an SDK value carries, in a fixed order.
fn checksums_from_sdk(checksum: Option<&aws_sdk_s3::types::Checksum>) -> Vec<Checksum> {
    let Some(checksum) = checksum else {
        return Vec::new();
    };
    [
        ("CRC32", checksum.checksum_crc32()),
        ("CRC32C", checksum.checksum_crc32_c()),
        ("CRC64NVME", checksum.checksum_crc64_nvme()),
        ("SHA1", checksum.checksum_sha1()),
        ("SHA256", checksum.checksum_sha256()),
        ("SHA512", checksum.checksum_sha512()),
        ("MD5", checksum.checksum_md5()),
        ("XXHASH64", checksum.checksum_xxhash64()),
        ("XXHASH3", checksum.checksum_xxhash3()),
        ("XXHASH128", checksum.checksum_xxhash128()),
    ]
    .into_iter()
    .filter_map(|(algorithm, value)| {
        value.map(|value| Checksum {
            algorithm: algorithm.to_string(),
            value: value.to_string(),
        })
    })
    .collect()
}

/// An ETag in the quoted form the other tools report it in. Some storage
/// backends (RustFS among them) return the value of this operation without
/// the surrounding quotes.
fn quoted_etag(etag: &str) -> String {
    if etag.starts_with('"') {
        etag.to_string()
    } else {
        format!("\"{etag}\"")
    }
}

impl S3Client {
    /// Read the attributes of an object, or of one specific version of it,
    /// without downloading its content: ETag, checksums, the parts it was
    /// assembled from, storage class, and size.
    ///
    /// The parts list follows S3's pagination markers, so objects with more
    /// than 1,000 parts report every part.
    pub async fn get_object_attributes(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<ObjectAttributesInfo> {
        debug!(
            "Fetching attributes for object: s3://{}/{}",
            bucket_name, object_key
        );

        let request = || {
            let mut request = self
                .client
                .get_object_attributes()
                .bucket(bucket_name)
                .key(object_key)
                .object_attributes(ObjectAttributes::Etag)
                .object_attributes(ObjectAttributes::Checksum)
                .object_attributes(ObjectAttributes::ObjectParts)
                .object_attributes(ObjectAttributes::ObjectSize)
                .object_attributes(ObjectAttributes::StorageClass)
                .max_parts(MAX_PARTS_PER_PAGE);

            if let Some(version_id) = version_id {
                request = request.version_id(version_id);
            }
            request
        };

        let first = request().send().await.context(format!(
            "Failed to get the attributes of s3://{bucket_name}/{object_key}"
        ))?;

        // Every response carries the whole object's attributes; only the
        // parts list is paginated.
        let checksum = first.checksum();
        let mut result = ObjectAttributesInfo {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            version_id: first
                .version_id()
                .map(|v| v.to_string())
                .or_else(|| version_id.map(|v| v.to_string())),
            last_modified: first
                .last_modified()
                .map(|dt| dt.fmt(DateTimeFormat::DateTime).unwrap()),
            etag: first.e_tag().map(quoted_etag),
            size: first.object_size(),
            storage_class: first
                .storage_class()
                .map(|class| class.as_str().to_string()),
            delete_marker: first.delete_marker().unwrap_or(false),
            checksums: checksums_from_sdk(checksum),
            checksum_type: checksum
                .and_then(|checksum| checksum.checksum_type())
                .map(|kind| kind.as_str().to_string()),
            parts: Vec::new(),
            total_parts_count: first.object_parts().and_then(|p| p.total_parts_count()),
        };

        // Later pages must describe the same version the first page read:
        // without pinning it, a page request without a version ID reads the
        // current version again and mixes attributes when the key is
        // overwritten in between.
        let pinned_version = first.version_id().map(|v| v.to_string());
        let mut page = first;
        let mut part_number_marker: Option<String> = None;
        while let Some(object_parts) = page.object_parts() {
            result
                .parts
                .extend(object_parts.parts().iter().map(|part| ObjectPart {
                    part_number: part.part_number(),
                    size: part.size(),
                }));

            let next_marker = object_parts
                .next_part_number_marker()
                .filter(|marker| !marker.is_empty());
            let is_truncated = object_parts.is_truncated() == Some(true);
            match next_marker {
                // A truncated page continues at its marker; a marker that
                // repeats the one just sent cannot advance the listing.
                Some(next_marker)
                    if is_truncated && part_number_marker.as_deref() != Some(next_marker) =>
                {
                    part_number_marker = Some(next_marker.to_string());
                    let mut page_request = request();
                    if let Some(ref version_id) = pinned_version {
                        page_request = page_request.version_id(version_id);
                    }
                    page = page_request
                        .part_number_marker(next_marker)
                        .send()
                        .await
                        .context(format!(
                            "Failed to list the parts of s3://{bucket_name}/{object_key} after part {next_marker}"
                        ))?;
                }
                _ => break,
            }
        }

        debug!(
            "Attributes of s3://{}/{} read: {} part(s), {} checksum(s)",
            bucket_name,
            object_key,
            result.parts.len(),
            result.checksums.len()
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::types::Checksum as SdkChecksum;

    #[test]
    fn test_checksums_from_sdk_keeps_the_set_values_in_a_fixed_order() {
        let checksum = SdkChecksum::builder()
            .checksum_sha256("sha-value")
            .checksum_crc32("crc-value")
            .checksum_crc64_nvme("nvme-value")
            .checksum_type(aws_sdk_s3::types::ChecksumType::FullObject)
            .build();

        assert_eq!(
            checksums_from_sdk(Some(&checksum)),
            [
                Checksum {
                    algorithm: "CRC32".to_string(),
                    value: "crc-value".to_string()
                },
                Checksum {
                    algorithm: "CRC64NVME".to_string(),
                    value: "nvme-value".to_string()
                },
                Checksum {
                    algorithm: "SHA256".to_string(),
                    value: "sha-value".to_string()
                },
            ]
        );

        // An empty checksum element - how some backends report "none" - and
        // a missing one both mean that nothing was recorded.
        assert!(checksums_from_sdk(Some(&SdkChecksum::builder().build())).is_empty());
        assert!(checksums_from_sdk(None).is_empty());
    }

    #[test]
    fn test_quoted_etag_matches_the_form_of_the_other_tools() {
        assert_eq!(quoted_etag("\"abc123\""), "\"abc123\"");
        assert_eq!(quoted_etag("abc123"), "\"abc123\"");
        assert_eq!(quoted_etag("\"abc-2\""), "\"abc-2\"");
    }

    #[test]
    fn test_object_attributes_info_serialization() {
        let info = ObjectAttributesInfo {
            bucket: "test-bucket".to_string(),
            key: "backups/db.dump".to_string(),
            version_id: Some("v1".to_string()),
            last_modified: Some("2026-11-26T00:00:00Z".to_string()),
            etag: Some("\"abc123-2\"".to_string()),
            size: Some(5_242_889),
            storage_class: Some("STANDARD".to_string()),
            delete_marker: false,
            checksums: vec![Checksum {
                algorithm: "CRC64NVME".to_string(),
                value: "AAAAAA==".to_string(),
            }],
            checksum_type: Some("COMPOSITE".to_string()),
            parts: vec![
                ObjectPart {
                    part_number: Some(1),
                    size: Some(5_242_880),
                },
                ObjectPart {
                    part_number: Some(2),
                    size: Some(9),
                },
            ],
            total_parts_count: Some(2),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ObjectAttributesInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.key, "backups/db.dump");
        assert_eq!(deserialized.size, Some(5_242_889));
        assert_eq!(deserialized.checksums, info.checksums);
        assert_eq!(deserialized.checksum_type.as_deref(), Some("COMPOSITE"));
        assert_eq!(deserialized.parts, info.parts);
        assert_eq!(deserialized.total_parts_count, Some(2));
    }
}
