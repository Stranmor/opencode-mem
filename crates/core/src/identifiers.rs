//! Newtype wrappers for semantically distinct identifiers.
//!
//! Prevents accidental swaps (e.g., passing a `ContentSessionId` where a
//! `SessionId` is expected) at compile time.

use std::fmt;
use std::ops::Deref;

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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub String);

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
        Self(s)
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
        Self(s.to_owned())
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
    impl_sqlx_transparent!(ProjectId);
    impl_sqlx_transparent!(ObservationId);
}
