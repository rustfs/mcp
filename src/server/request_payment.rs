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

//! Notes of the request payment tools.

use crate::s3_client::BucketPayer;

/// What the payer means for the requests to a bucket. Only AWS S3 enforces
/// requester pays; RustFS stores the setting without acting on it, so the
/// note never lets that setting pass for a restriction of access.
pub(super) fn payer_note(payer: BucketPayer) -> String {
    match payer {
        BucketPayer::BucketOwner => {
            "The bucket owner pays for the requests to this bucket, including the ones made \
             anonymously. Set the payer to \"Requester\" to charge callers on AWS S3."
                .to_string()
        }
        BucketPayer::Requester => {
            "On AWS S3, requests to this bucket cost the callers: they must authenticate and \
             cover the request and data-transfer charges, and anonymous access to the bucket is \
             disabled while requester pays is set. Storage backends that only store the setting \
             leave both unchanged: RustFS (1.0.7) stores it without charging callers or disabling \
             anonymous access, so restrict access there with a bucket policy (put_bucket_policy) \
             or with ACLs (put_bucket_acl)."
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payer_note_names_the_backend_that_enforces_requester_pays() {
        // Requester pays is an AWS S3 behavior; the note says so and does
        // not let the setting pass for a restriction of access elsewhere.
        let requester = payer_note(BucketPayer::Requester);
        assert!(requester.starts_with("On AWS S3, requests to this bucket cost the callers"));
        assert!(requester.contains("RustFS (1.0.7) stores it without charging callers"));
        assert!(requester.contains("put_bucket_policy"));
        assert!(requester.contains("put_bucket_acl"));

        let owner = payer_note(BucketPayer::BucketOwner);
        assert!(owner.starts_with("The bucket owner pays for the requests to this bucket"));
        assert!(!owner.contains("RustFS"));
    }
}
