use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    };
}

typed_id!(ProjectId);
typed_id!(ComponentDefinitionId);
typed_id!(ComponentInstanceId);
typed_id!(PinId);
typed_id!(NetId);
typed_id!(WireId);
typed_id!(ConstraintId);
typed_id!(OperationId);
typed_id!(CommitId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_ids_serialize_as_uuid_strings() {
        let id: ProjectId = "00000000-0000-4000-8000-000000000001"
            .parse()
            .expect("valid UUID");

        assert_eq!(
            serde_json::to_string(&id).expect("serializable"),
            "\"00000000-0000-4000-8000-000000000001\""
        );
    }

    #[test]
    fn distinct_id_types_do_not_compare_accidentally() {
        let uuid = Uuid::nil();
        let project = ProjectId::from_uuid(uuid);
        let wire = WireId::from_uuid(uuid);

        assert_eq!(project.to_string(), wire.to_string());
        // Their distinct Rust types intentionally prevent `project == wire`.
    }
}
