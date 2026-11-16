macro_rules! string_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates an identifier from a nonempty string.
            ///
            /// # Errors
            ///
            /// Returns an error when the supplied string is empty.
            pub fn new(value: impl Into<String>) -> Result<Self, crate::CoreError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(crate::CoreError::InvalidDocument(format!(
                        "{} cannot be empty",
                        stringify!($name)
                    )));
                }
                Ok(Self(value))
            }

            /// Returns the identifier string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

string_id!(DocumentId, "A stable document identifier.");
string_id!(RunId, "A stable analyser run identifier.");
string_id!(NodeId, "A stable run-scoped result node identifier.");
string_id!(TraceRecordId, "A stable trace record identifier.");
