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

//! Bucket versioning and object version operations.

mod delete_object_versions;
mod get_bucket_versioning;
mod list_object_versions;
mod put_bucket_versioning;

pub use delete_object_versions::{
    DeleteObjectVersionsResult, DeletedObjectVersion, ObjectVersion, VersionDeleteError,
};
pub use get_bucket_versioning::BucketVersioningConfig;
pub use list_object_versions::{ListObjectVersionsOptions, ListObjectVersionsResult, VersionEntry};

use serde::{Deserialize, Serialize};

/// Versioning state of a bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum BucketVersioningStatus {
    /// Keep every version of every object: overwrites create new versions and
    /// a delete only adds a delete marker.
    #[serde(rename = "enabled")]
    Enabled,
    /// Stop creating new versions; the existing ones are kept.
    #[serde(rename = "suspended")]
    Suspended,
}

impl BucketVersioningStatus {
    /// Lower-case name used in requests and log lines.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Suspended => "suspended",
        }
    }

    /// Display name for tool output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enabled => "Enabled",
            Self::Suspended => "Suspended",
        }
    }

    /// What the status means for the objects in the bucket.
    pub fn effect(&self) -> &'static str {
        match self {
            Self::Enabled => {
                "Every write creates a new version and a delete only adds a delete marker; use list_object_versions to see them."
            }
            Self::Suspended => {
                "Existing versions are kept, but new writes no longer create versions."
            }
        }
    }

    /// Map the SDK enum, dropping values this crate does not model.
    fn from_sdk(status: &aws_sdk_s3::types::BucketVersioningStatus) -> Option<Self> {
        match status {
            aws_sdk_s3::types::BucketVersioningStatus::Enabled => Some(Self::Enabled),
            aws_sdk_s3::types::BucketVersioningStatus::Suspended => Some(Self::Suspended),
            _ => None,
        }
    }

    fn to_sdk(self) -> aws_sdk_s3::types::BucketVersioningStatus {
        match self {
            Self::Enabled => aws_sdk_s3::types::BucketVersioningStatus::Enabled,
            Self::Suspended => aws_sdk_s3::types::BucketVersioningStatus::Suspended,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_versioning_status_serialization() {
        assert_eq!(
            serde_json::to_string(&BucketVersioningStatus::Enabled).unwrap(),
            r#""enabled""#
        );
        assert_eq!(
            serde_json::to_string(&BucketVersioningStatus::Suspended).unwrap(),
            r#""suspended""#
        );

        let enabled: BucketVersioningStatus = serde_json::from_str(r#""enabled""#).unwrap();
        let suspended: BucketVersioningStatus = serde_json::from_str(r#""suspended""#).unwrap();
        assert_eq!(enabled, BucketVersioningStatus::Enabled);
        assert_eq!(suspended, BucketVersioningStatus::Suspended);

        assert!(serde_json::from_str::<BucketVersioningStatus>(r#""on""#).is_err());
    }

    #[test]
    fn test_bucket_versioning_status_sdk_round_trip() {
        for status in [
            BucketVersioningStatus::Enabled,
            BucketVersioningStatus::Suspended,
        ] {
            assert_eq!(
                BucketVersioningStatus::from_sdk(&status.to_sdk()),
                Some(status)
            );
        }

        // Values the SDK does not model arrive as its `Unknown` variant.
        let unknown = aws_sdk_s3::types::BucketVersioningStatus::from("future-value");
        assert_eq!(BucketVersioningStatus::from_sdk(&unknown), None);
    }

    #[test]
    fn test_bucket_versioning_status_text() {
        assert_eq!(BucketVersioningStatus::Enabled.as_str(), "enabled");
        assert_eq!(BucketVersioningStatus::Enabled.label(), "Enabled");
        assert_eq!(BucketVersioningStatus::Suspended.as_str(), "suspended");
        assert_eq!(BucketVersioningStatus::Suspended.label(), "Suspended");

        let enabled_effect = BucketVersioningStatus::Enabled.effect();
        let suspended_effect = BucketVersioningStatus::Suspended.effect();
        assert!(enabled_effect.contains("list_object_versions"));
        assert!(suspended_effect.contains("Existing versions are kept"));
    }
}
