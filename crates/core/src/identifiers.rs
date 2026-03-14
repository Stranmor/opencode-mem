//! Newtype wrappers for semantically distinct identifiers.
//!
//! Prevents accidental swaps (e.g., passing a `ContentSessionId` where a
//! `SessionId` is expected) at compile time.

use std::fmt;
use std::ops::Deref;

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

/// Internal memory session identifier (generated UUID).
///
/// Distinct from [`ContentSessionId`] which comes from the IDE.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

/// External content session identifier provided by the IDE (e.g., OpenCode).
///
/// Distinct from [`SessionId`] which is the internal memory session UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentSessionId(pub String);

/// Project name or path identifier.
///
/// Inner field is private — all construction goes through [`ProjectId::new`],
/// which enforces canonical normalization:
/// lowercase, hyphens→underscores, trim whitespace, trim trailing slashes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ProjectId(String);

impl ProjectId {
    /// Creates a normalized `ProjectId`. Rules documented on the struct.
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(Self::normalize(raw.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn normalize(raw: String) -> String {
        raw.trim()
            .to_lowercase()
            .replace('-', "_")
            .trim_end_matches('/')
            .to_string()
    }
}

impl<'de> Deserialize<'de> for ProjectId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self::new(s))
    }
}

/// Observation identifier (generated UUID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservationId(pub String);

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for ContentSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for ObservationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<String> for ContentSessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<String> for ProjectId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<String> for ObservationId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<&str> for ContentSessionId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<&str> for ProjectId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<&str> for ObservationId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ContentSessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ProjectId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ObservationId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for SessionId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl Deref for ContentSessionId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl Deref for ProjectId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl Deref for ObservationId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl From<SessionId> for String {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

impl From<ContentSessionId> for String {
    fn from(id: ContentSessionId) -> Self {
        id.0
    }
}

impl From<ProjectId> for String {
    fn from(id: ProjectId) -> Self {
        id.0
    }
}

impl From<ObservationId> for String {
    fn from(id: ObservationId) -> Self {
        id.0
    }
}

#[cfg(feature = "sqlx-types")]
mod sqlx_impls {
    use super::*;
    use sqlx::Database;
    use sqlx::encode::IsNull;
    use sqlx::error::BoxDynError;

    macro_rules! impl_sqlx_transparent {
        ($ty:ty) => {
            impl<DB: Database> sqlx::Type<DB> for $ty
            where
                String: sqlx::Type<DB>,
            {
                fn type_info() -> DB::TypeInfo {
                    <String as sqlx::Type<DB>>::type_info()
                }

                fn compatible(ty: &DB::TypeInfo) -> bool {
                    <String as sqlx::Type<DB>>::compatible(ty)
                }
            }

            impl<'q, DB: Database> sqlx::Encode<'q, DB> for $ty
            where
                String: sqlx::Encode<'q, DB>,
            {
                fn encode_by_ref(
                    &self,
                    buf: &mut <DB as Database>::ArgumentBuffer<'q>,
                ) -> Result<IsNull, BoxDynError> {
                    self.0.encode_by_ref(buf)
                }
            }

            impl<'r, DB: Database> sqlx::Decode<'r, DB> for $ty
            where
                String: sqlx::Decode<'r, DB>,
            {
                fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
                    let s = <String as sqlx::Decode<'r, DB>>::decode(value)?;
                    Ok(Self(s))
                }
            }
        };
    }

    impl_sqlx_transparent!(SessionId);
    impl_sqlx_transparent!(ContentSessionId);
    impl_sqlx_transparent!(ObservationId);

    // ProjectId gets manual impls to normalize on decode from DB
    impl<DB: Database> sqlx::Type<DB> for ProjectId
    where
        String: sqlx::Type<DB>,
    {
        fn type_info() -> DB::TypeInfo {
            <String as sqlx::Type<DB>>::type_info()
        }

        fn compatible(ty: &DB::TypeInfo) -> bool {
            <String as sqlx::Type<DB>>::compatible(ty)
        }
    }

    impl<'q, DB: Database> sqlx::Encode<'q, DB> for ProjectId
    where
        String: sqlx::Encode<'q, DB>,
    {
        fn encode_by_ref(
            &self,
            buf: &mut <DB as Database>::ArgumentBuffer<'q>,
        ) -> Result<IsNull, BoxDynError> {
            self.0.encode_by_ref(buf)
        }
    }

    impl<'r, DB: Database> sqlx::Decode<'r, DB> for ProjectId
    where
        String: sqlx::Decode<'r, DB>,
    {
        fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
            let s = <String as sqlx::Decode<'r, DB>>::decode(value)?;
            Ok(ProjectId::new(s))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hyphens_to_underscores() {
        assert_eq!(ProjectId::new("hermes-ai").as_str(), "hermes_ai");
    }

    #[test]
    fn normalizes_to_lowercase() {
        assert_eq!(
            ProjectId::new("Antigravity-Manager").as_str(),
            "antigravity_manager"
        );
    }

    #[test]
    fn trims_whitespace_and_trailing_slash() {
        assert_eq!(ProjectId::new("  my-project/  ").as_str(), "my_project");
    }

    #[test]
    fn from_string_normalizes() {
        let id: ProjectId = "Test-Project".into();
        assert_eq!(id.as_str(), "test_project");
    }

    #[test]
    fn from_str_normalizes() {
        let id = ProjectId::from("Hermes-AI");
        assert_eq!(id.as_str(), "hermes_ai");
    }

    #[test]
    fn deserialize_normalizes() {
        let id: ProjectId = serde_json::from_str("\"Hermes-AI\"").unwrap();
        assert_eq!(id.as_str(), "hermes_ai");
    }

    #[test]
    fn serialize_returns_normalized() {
        let id = ProjectId::new("Test-Project");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"test_project\"");
    }

    #[test]
    fn display_returns_normalized() {
        let id = ProjectId::new("My-App/");
        assert_eq!(id.to_string(), "my_app");
    }

    #[test]
    fn deref_returns_normalized() {
        let id = ProjectId::new("FOO-BAR");
        let s: &str = &id;
        assert_eq!(s, "foo_bar");
    }

    #[test]
    fn empty_string_stays_empty() {
        assert_eq!(ProjectId::new("").as_str(), "");
    }

    #[test]
    fn already_normalized_unchanged() {
        assert_eq!(ProjectId::new("opencode_mem").as_str(), "opencode_mem");
    }

    #[test]
    fn equality_after_normalization() {
        assert_eq!(ProjectId::new("hermes-ai"), ProjectId::new("hermes_ai"));
        assert_eq!(ProjectId::new("Hermes-AI"), ProjectId::new("hermes_ai"));
    }
}
