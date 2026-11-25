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

//! Bucket and object tagging.

mod delete_bucket_tagging;
mod delete_object_tagging;
mod get_bucket_tagging;
mod get_object_tagging;
mod put_bucket_tagging;
mod put_object_tagging;

pub use get_bucket_tagging::BucketTags;
pub use get_object_tagging::ObjectTags;

use anyhow::{Context, Result};
use aws_sdk_s3::types::{Tag, Tagging};
use std::collections::{BTreeMap, HashMap};

/// Maximum number of tags allowed on a bucket or on an object version.
pub const MAX_TAGS: usize = 10;
/// Maximum length of a tag key, in Unicode characters.
pub const MAX_TAG_KEY_LENGTH: usize = 128;
/// Maximum length of a tag value, in Unicode characters.
pub const MAX_TAG_VALUE_LENGTH: usize = 256;

/// Error code S3 answers with when the bucket or object was never tagged.
const UNTAGGED_ERROR_CODE: &str = "NoSuchTagSet";

/// Validate a tag set before it is sent to S3, so that size problems are
/// reported up front instead of as an opaque `MalformedXML` error.
fn validate_tags(tags: &HashMap<String, String>) -> Result<()> {
    if tags.is_empty() {
        anyhow::bail!("At least one tag is required");
    }
    if tags.len() > MAX_TAGS {
        anyhow::bail!(
            "Too many tags: {} given, but S3 allows at most {MAX_TAGS}",
            tags.len()
        );
    }
    for (key, value) in tags {
        let key_length = key.chars().count();
        if key_length == 0 {
            anyhow::bail!("Tag keys must not be empty");
        }
        if key_length > MAX_TAG_KEY_LENGTH {
            anyhow::bail!(
                "Tag key '{key}' is {key_length} characters long; S3 allows at most {MAX_TAG_KEY_LENGTH}"
            );
        }
        let value_length = value.chars().count();
        if value_length > MAX_TAG_VALUE_LENGTH {
            anyhow::bail!(
                "The value of tag '{key}' is {value_length} characters long; S3 allows at most {MAX_TAG_VALUE_LENGTH}"
            );
        }
    }
    Ok(())
}

/// Build the `Tagging` request payload for a validated tag set.
fn tagging_request(tags: &HashMap<String, String>) -> Result<Tagging> {
    let tag_set = tags
        .iter()
        .map(|(key, value)| Tag::builder().key(key).value(value).build())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to build the tag set")?;

    Tagging::builder()
        .set_tag_set(Some(tag_set))
        .build()
        .context("Failed to build the tagging request")
}

/// Read the tag set of a `GetBucketTagging` / `GetObjectTagging` response,
/// sorted by key.
fn tag_set_from_sdk(tags: &[Tag]) -> BTreeMap<String, String> {
    tags.iter()
        .map(|tag| (tag.key().to_string(), tag.value().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn test_validate_tags_accepts_valid_tag_sets() {
        assert!(validate_tags(&tags(&[("team", "storage")])).is_ok());
        // An empty value is allowed.
        assert!(validate_tags(&tags(&[("placeholder", "")])).is_ok());
        // Limits are inclusive.
        let key = "k".repeat(MAX_TAG_KEY_LENGTH);
        let value = "v".repeat(MAX_TAG_VALUE_LENGTH);
        assert!(validate_tags(&tags(&[(&key, &value)])).is_ok());
        let ten: Vec<(String, String)> = (0..MAX_TAGS)
            .map(|index| (format!("key{index}"), format!("value{index}")))
            .collect();
        assert!(validate_tags(&ten.into_iter().collect()).is_ok());
    }

    #[test]
    fn test_validate_tags_rejects_invalid_tag_sets() {
        let error = validate_tags(&HashMap::new()).unwrap_err();
        assert!(error.to_string().contains("At least one tag"));

        let eleven: Vec<(String, String)> = (0..=MAX_TAGS)
            .map(|index| (format!("key{index}"), format!("value{index}")))
            .collect();
        let error = validate_tags(&eleven.into_iter().collect()).unwrap_err();
        assert!(error.to_string().contains("at most 10"));

        let error = validate_tags(&tags(&[("", "value")])).unwrap_err();
        assert!(error.to_string().contains("must not be empty"));

        let long_key = "k".repeat(MAX_TAG_KEY_LENGTH + 1);
        let error = validate_tags(&tags(&[(&long_key, "value")])).unwrap_err();
        assert!(error.to_string().contains("129 characters"));

        let long_value = "v".repeat(MAX_TAG_VALUE_LENGTH + 1);
        let error = validate_tags(&tags(&[("key", &long_value)])).unwrap_err();
        assert!(error.to_string().contains("257 characters"));
        assert!(error.to_string().contains("tag 'key'"));
    }

    #[test]
    fn test_validate_tags_measures_characters_not_bytes() {
        // 128 CJK characters are 384 bytes but still a valid key.
        let key = "文".repeat(MAX_TAG_KEY_LENGTH);
        assert!(validate_tags(&tags(&[(&key, "值")])).is_ok());
    }

    #[test]
    fn test_tagging_request_round_trips_through_sdk_types() {
        let input = tags(&[("team", "storage"), ("cost-center", "42")]);

        let tagging = tagging_request(&input).unwrap();
        let round_tripped = tag_set_from_sdk(tagging.tag_set());

        assert_eq!(round_tripped, input.into_iter().collect::<BTreeMap<_, _>>());
    }

    #[test]
    fn test_tag_set_from_sdk_keeps_the_last_value_of_a_repeated_key() {
        let tag_set = [
            Tag::builder().key("env").value("dev").build().unwrap(),
            Tag::builder().key("env").value("prod").build().unwrap(),
            Tag::builder().key("team").value("storage").build().unwrap(),
        ];

        let expected: BTreeMap<String, String> = [("env", "prod"), ("team", "storage")]
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        assert_eq!(tag_set_from_sdk(&tag_set), expected);
    }
}
