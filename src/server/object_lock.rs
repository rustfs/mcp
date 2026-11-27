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

//! Notes of the Object Lock reads that found nothing to report.

use crate::s3_client::UnsetLockReason;

/// The note of a retention read that found no retention period. Without
/// Object Lock no version of the bucket can carry a retention period, so
/// the note then names the bucket instead of the version.
pub(super) fn unset_retention_note(reason: UnsetLockReason, bucket: &str) -> String {
    match reason {
        UnsetLockReason::BucketNotEnabled => format!(
            "Object Lock is not enabled for bucket '{bucket}', so no retention period applies to \
             this object version: it can be deleted or overwritten. put_object_retention needs \
             Object Lock on the bucket (see put_bucket_lock_configuration)."
        ),
        UnsetLockReason::NotSet => {
            "This object version has no retention period. It can be deleted or overwritten \
             unless a legal hold is still on (get_object_legal_hold). Set one with \
             put_object_retention (the bucket needs Object Lock enabled)."
                .to_string()
        }
    }
}

/// The note of a legal hold read that found no hold.
pub(super) fn unset_legal_hold_note(reason: UnsetLockReason, bucket: &str) -> String {
    match reason {
        UnsetLockReason::BucketNotEnabled => format!(
            "Object Lock is not enabled for bucket '{bucket}', so no legal hold applies to this \
             object version: it can be deleted or overwritten. put_object_legal_hold needs \
             Object Lock on the bucket (see put_bucket_lock_configuration)."
        ),
        UnsetLockReason::NotSet => {
            "No legal hold is recorded for this object version. It can be deleted or \
             overwritten unless an active retention period still protects it \
             (get_object_retention). Set one with put_object_legal_hold (the bucket needs \
             Object Lock enabled)."
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notes_name_the_bucket_only_when_object_lock_is_disabled() {
        // A bucket without Object Lock: the note tells where to look and
        // does not point at the object.
        let retention = unset_retention_note(UnsetLockReason::BucketNotEnabled, "plain");
        assert!(retention.starts_with("Object Lock is not enabled for bucket 'plain'"));
        assert!(retention.contains("put_bucket_lock_configuration"));
        assert!(!retention.contains("get_object_legal_hold"));

        let hold = unset_legal_hold_note(UnsetLockReason::BucketNotEnabled, "plain");
        assert!(hold.starts_with("Object Lock is not enabled for bucket 'plain'"));
        assert!(hold.contains("put_object_legal_hold"));
        assert!(!hold.contains("get_object_retention"));

        // A version of a bucket that has Object Lock: the note is about the
        // version and names the tool that sets the setting.
        let retention = unset_retention_note(UnsetLockReason::NotSet, "plain");
        assert!(retention.starts_with("This object version has no retention period"));
        assert!(retention.contains("get_object_legal_hold"));
        assert!(retention.contains("put_object_retention"));

        let hold = unset_legal_hold_note(UnsetLockReason::NotSet, "plain");
        assert!(hold.starts_with("No legal hold is recorded for this object version"));
        assert!(hold.contains("get_object_retention"));
        assert!(hold.contains("put_object_legal_hold"));
    }
}
