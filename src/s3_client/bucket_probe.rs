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

//! Existence probe for reads whose "not configured" answer some S3
//! implementations also give for a bucket that does not exist.

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::head_bucket::HeadBucketError;

use super::S3Client;

/// HTTP status of a failed bucket HEAD, when the server answered at all.
fn head_status(err: &SdkError<HeadBucketError>) -> Option<u16> {
    match err {
        SdkError::ServiceError(err) => Some(err.raw().status().as_u16()),
        SdkError::ResponseError(err) => Some(err.raw().status().as_u16()),
        _ => None,
    }
}

/// Whether a failed HEAD still proves the bucket is there. An AccessDenied
/// answer (403) does: the bucket exists but the caller may not HEAD it -
/// exactly the situation of credentials that may read a bucket's
/// configuration (such as its tags) without the `s3:ListBucket` permission a
/// HEAD requires. A 404 - or no answer at all - does not.
fn head_proves_existence(status: Option<u16>) -> bool {
    status == Some(403)
}

impl S3Client {
    /// Whether the bucket is known to exist: the HEAD succeeded, or was
    /// answered with AccessDenied. `false` for a 404, an unanswered HEAD, or
    /// any other failure.
    pub(crate) async fn bucket_is_reachable(&self, bucket_name: &str) -> bool {
        match self.client.head_bucket().bucket(bucket_name).send().await {
            Ok(_) => true,
            Err(err) => head_proves_existence(head_status(&err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::head_proves_existence;

    #[test]
    fn test_head_proves_existence_distinguishes_denied_from_missing() {
        // AccessDenied: the bucket exists but may not be HEADed.
        assert!(head_proves_existence(Some(403)));
        // A missing bucket (or no answer at all) proves nothing.
        assert!(!head_proves_existence(Some(404)));
        assert!(!head_proves_existence(None));
        assert!(!head_proves_existence(Some(500)));
    }
}
