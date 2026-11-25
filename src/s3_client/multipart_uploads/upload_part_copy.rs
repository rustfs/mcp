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
use aws_sdk_s3::operation::upload_part_copy::UploadPartCopyOutput;
use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::super::S3Client;

/// Inclusive byte range of the source object copied into one part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyRange {
    /// Offset of the first byte copied, zero-based.
    pub start: u64,
    /// Offset of the last byte copied.
    pub end: u64,
}

/// Result payload for one part uploaded by a server-side copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopiedPart {
    pub part_number: i32,
    pub etag: String,
    /// Size in bytes; S3 does not report it for a copied part.
    pub size: Option<i64>,
    pub last_modified: Option<String>,
}

impl CopyRange {
    /// Parse a range written as `"<start>-<end>"`, e.g. `"0-5242879"`.
    ///
    /// Both offsets must be non-negative integers; surrounding whitespace is
    /// ignored. Everything else is rejected, including HTTP range syntax such
    /// as `"bytes=0-9"` and the suffix form `"-500"`.
    pub fn parse(input: &str) -> Result<Self> {
        let (start, end) = input.trim().split_once('-').ok_or_else(|| {
            invalid_range(
                input,
                "it is missing the '-' between the start and end offsets",
            )
        })?;

        let start = parse_offset(start, "start", input)?;
        let end = parse_offset(end, "end", input)?;

        if start > end {
            return Err(invalid_range(
                input,
                &format!("the start offset {start} is greater than the end offset {end}"),
            ));
        }

        Ok(Self { start, end })
    }

    /// `x-amz-copy-source-range` header value for this range.
    pub fn to_header(self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }
}

/// Explain that `input` is not a range of the form `"<start>-<end>"`.
fn invalid_range(input: &str, reason: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "Invalid range '{input}': {reason}; expected \"<start>-<end>\", e.g. \"0-5242879\""
    )
}

/// Parse one offset of range `input`. Digits only: a sign, whitespace, or a
/// `bytes=` prefix would make the offset something else entirely.
fn parse_offset(offset: &str, name: &str, input: &str) -> Result<u64> {
    if offset.is_empty() || !offset.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_range(
            input,
            &format!("the {name} offset '{offset}' is not a non-negative integer"),
        ));
    }

    offset
        .parse()
        .map_err(|_| invalid_range(input, &format!("the {name} offset is too large")))
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

/// Build the `x-amz-copy-source` header value for an object or, when a
/// version ID is given, for one specific version of it.
fn copy_source_header(
    source_bucket: &str,
    source_key: &str,
    source_version_id: Option<&str>,
) -> String {
    let source = format!("{source_bucket}/{}", percent_encode(source_key, true));
    match source_version_id {
        // The `?` follows the encoded key, so a `?` or `#` inside the key
        // cannot be mistaken for the query. A version ID is an opaque token,
        // so its `/` is encoded like any other reserved character.
        Some(version_id) => format!("{source}?versionId={}", percent_encode(version_id, false)),
        None => source,
    }
}

/// Reject a part number outside the 1-10000 range S3 accepts.
fn ensure_part_number(part_number: i32) -> Result<()> {
    if !(1..=10000).contains(&part_number) {
        anyhow::bail!("part_number must be between 1 and 10000, got {part_number}");
    }
    Ok(())
}

fn format_timestamp(dt: &DateTime) -> String {
    dt.fmt(DateTimeFormat::DateTime).unwrap()
}

/// Extract the details of the copied part from an `UploadPartCopy` response.
fn copied_part(part_number: i32, response: &UploadPartCopyOutput) -> Result<CopiedPart> {
    let etag = response
        .copy_part_result()
        .and_then(|result| result.e_tag())
        .context("S3 did not return an ETag for the copied part")?;

    Ok(CopiedPart {
        part_number,
        etag: etag.to_string(),
        // S3 reports no size for a copied part, so none is made up here.
        size: None,
        last_modified: response
            .copy_part_result()
            .and_then(|result| result.last_modified())
            .map(format_timestamp),
    })
}

impl S3Client {
    /// Upload one part of a multipart upload by copying an existing object
    /// server-side: either the whole object or an inclusive byte `range` of
    /// it. Record the returned ETag for the completion call.
    // Each parameter maps to one header of S3's UploadPartCopy request, so
    // the call keeps them separate instead of hiding them in a struct.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_part_copy(
        &self,
        bucket_name: &str,
        object_key: &str,
        upload_id: &str,
        part_number: i32,
        source_bucket: &str,
        source_key: &str,
        source_version_id: Option<&str>,
        range: Option<CopyRange>,
    ) -> Result<CopiedPart> {
        info!(
            "Copying part {} for multipart upload '{}': s3://{}/{} -> s3://{}/{}",
            part_number, upload_id, source_bucket, source_key, bucket_name, object_key
        );

        ensure_part_number(part_number)?;

        let mut request = self
            .client
            .upload_part_copy()
            .bucket(bucket_name)
            .key(object_key)
            .upload_id(upload_id)
            .part_number(part_number)
            .copy_source(copy_source_header(
                source_bucket,
                source_key,
                source_version_id,
            ));

        if let Some(range) = range {
            request = request.copy_source_range(range.to_header());
        }

        debug!("Executing S3 upload_part_copy request");
        let response = request.send().await.context(format!(
            "Failed to copy part {part_number} for s3://{bucket_name}/{object_key} from s3://{source_bucket}/{source_key}"
        ))?;

        let result = copied_part(part_number, &response)?;

        info!(
            "Part {} uploaded by server-side copy from s3://{}/{} (ETag {})",
            part_number, source_bucket, source_key, result.etag
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::types::CopyPartResult;

    #[test]
    fn test_copy_range_parse_accepts_inclusive_offsets() {
        assert_eq!(
            CopyRange::parse("0-5242879").unwrap(),
            CopyRange {
                start: 0,
                end: 5_242_879
            }
        );
        assert_eq!(
            CopyRange::parse("6291456-12582911").unwrap(),
            CopyRange {
                start: 6_291_456,
                end: 12_582_911
            }
        );

        // A single byte, and the full 64-bit address space.
        assert_eq!(
            CopyRange::parse("7-7").unwrap(),
            CopyRange { start: 7, end: 7 }
        );
        assert_eq!(
            CopyRange::parse("0-18446744073709551615").unwrap(),
            CopyRange {
                start: 0,
                end: u64::MAX
            }
        );
    }

    #[test]
    fn test_copy_range_parse_ignores_surrounding_whitespace() {
        assert_eq!(
            CopyRange::parse("  0-9\n").unwrap(),
            CopyRange { start: 0, end: 9 }
        );
        assert_eq!(
            CopyRange::parse("\t100-200\t").unwrap(),
            CopyRange {
                start: 100,
                end: 200
            }
        );
    }

    #[test]
    fn test_copy_range_parse_rejects_reversed_offsets() {
        let error = CopyRange::parse("500-100").unwrap_err().to_string();

        assert!(error.contains("Invalid range '500-100'"), "{error}");
        assert!(error.contains("start offset 500"), "{error}");
        assert!(error.contains("end offset 100"), "{error}");
        assert!(error.contains("\"<start>-<end>\""), "{error}");
    }

    #[test]
    fn test_copy_range_parse_rejects_other_formats() {
        for input in [
            // Non-numeric offsets.
            "abc",
            "a-b",
            "1-x",
            "x-1",
            "1.5-2",
            "0x10-20",
            "1_000-2000",
            "+1-2",
            // No dash, or a missing offset.
            "0",
            "",
            "   ",
            "5-",
            "-5",
            // Negative offsets.
            "-5-10",
            "5--10",
            // HTTP range syntax instead of bare offsets.
            "bytes=0-9",
            "bytes=-500",
            "bytes=0-9/",
            // Whitespace inside the range.
            "0 - 5",
            "0- 5",
            "0 -5",
            // Offsets beyond 64 bits.
            "18446744073709551616-1",
            "0-99999999999999999999",
        ] {
            let error = CopyRange::parse(input).unwrap_err().to_string();
            assert!(error.contains("\"<start>-<end>\""), "{input:?}: {error}");
        }
    }

    #[test]
    fn test_copy_range_to_header() {
        assert_eq!(
            CopyRange {
                start: 0,
                end: 5_242_879
            }
            .to_header(),
            "bytes=0-5242879"
        );
        assert_eq!(
            CopyRange {
                start: 6_291_456,
                end: 12_582_911
            }
            .to_header(),
            "bytes=6291456-12582911"
        );

        for input in ["0-5242879", "6291456-12582911", "0-0"] {
            assert_eq!(
                CopyRange::parse(input).unwrap().to_header(),
                format!("bytes={input}")
            );
        }
    }

    #[test]
    fn test_copy_source_header_without_version() {
        assert_eq!(
            copy_source_header("src", "plain.txt", None),
            "src/plain.txt"
        );

        assert_eq!(
            copy_source_header("src", "with spaces/文件 name.txt", None),
            "src/with%20spaces/%E6%96%87%E4%BB%B6%20name.txt"
        );

        // Characters that would otherwise read as separators are encoded.
        assert_eq!(
            copy_source_header("src", "special~chars_-.,()", None),
            "src/special~chars_-.%2C%28%29"
        );
        assert_eq!(
            copy_source_header("src", "a?b=c&d.txt", None),
            "src/a%3Fb%3Dc%26d.txt"
        );
    }

    #[test]
    fn test_copy_source_header_with_version() {
        assert_eq!(
            copy_source_header(
                "src",
                "a.txt",
                Some("3sL4kqtJlcpXroDTDmJ+rmSpXd3dIbrHY+MTRCxf3vjVBH40Nr8X8gdRQBpUMLUo")
            ),
            "src/a.txt?versionId=3sL4kqtJlcpXroDTDmJ%2BrmSpXd3dIbrHY%2BMTRCxf3vjVBH40Nr8X8gdRQBpUMLUo"
        );

        // The version ID follows the key, and its `/` is encoded.
        assert_eq!(
            copy_source_header("src", "with spaces/a b.txt", Some("a/b=c")),
            "src/with%20spaces/a%20b.txt?versionId=a%2Fb%3Dc"
        );
    }

    #[test]
    fn test_ensure_part_number_accepts_the_full_range_only() {
        assert!(ensure_part_number(1).is_ok());
        assert!(ensure_part_number(10_000).is_ok());
    }

    #[test]
    fn test_ensure_part_number_rejects_other_values() {
        for part_number in [0, -1, 10_001] {
            let error = ensure_part_number(part_number).unwrap_err().to_string();
            assert!(
                error.contains("between 1 and 10000") && error.contains(&part_number.to_string()),
                "{part_number}: {error}"
            );
        }
    }

    #[test]
    fn test_copied_part_reads_the_copy_result() {
        let output = UploadPartCopyOutput::builder()
            .copy_part_result(
                CopyPartResult::builder()
                    .e_tag("\"d41d8cd98f00b204e9800998ecf8427e-2\"")
                    .last_modified(DateTime::from_secs(1_000_000_000))
                    .build(),
            )
            .build();

        let part = copied_part(3, &output).unwrap();
        assert_eq!(part.part_number, 3);
        assert_eq!(part.etag, "\"d41d8cd98f00b204e9800998ecf8427e-2\"");
        // S3 reports no size for a copied part.
        assert_eq!(part.size, None);
        assert_eq!(part.last_modified, Some("2001-09-09T01:46:40Z".to_string()));
    }

    #[test]
    fn test_copied_part_without_etag_is_an_error() {
        for output in [
            UploadPartCopyOutput::builder().build(),
            UploadPartCopyOutput::builder()
                .copy_part_result(
                    CopyPartResult::builder()
                        .last_modified(DateTime::from_secs(0))
                        .build(),
                )
                .build(),
        ] {
            let error = copied_part(1, &output).unwrap_err().to_string();
            assert!(error.contains("ETag"), "{error}");
        }

        // An ETag alone is enough.
        let output = UploadPartCopyOutput::builder()
            .copy_part_result(CopyPartResult::builder().e_tag("\"etag\"").build())
            .build();
        let part = copied_part(1, &output).unwrap();
        assert_eq!(part.etag, "\"etag\"");
        assert_eq!(part.last_modified, None);
    }

    #[test]
    fn test_copied_part_serialization() {
        let part = CopiedPart {
            part_number: 2,
            etag: "\"etag2\"".to_string(),
            size: None,
            last_modified: Some("2026-11-25T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&part).unwrap();
        let deserialized: CopiedPart = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.part_number, 2);
        assert_eq!(deserialized.etag, "\"etag2\"");
        assert_eq!(deserialized.size, None);
        assert_eq!(
            deserialized.last_modified,
            Some("2026-11-25T00:00:00Z".to_string())
        );
    }
}
