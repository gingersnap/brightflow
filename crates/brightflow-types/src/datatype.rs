//! Logical data types: the type contract between a Parquet writer and the store.
//!
//! These are Apache Ossie's ten portable datatypes, spelled exactly as the
//! spec spells them so a serialised field is a valid Ossie field. They map
//! one-to-one onto Parquet logical types, which is what lets a producer declare
//! `DateTime` on a column, write a real timestamp, and have the store read the
//! same fact back from the file with no side channel. Anything outside the ten
//! is `Opaque`, with the engine's own spelling kept in `ColumnSchema::physical`
//! so nothing is lost, only not portable.
//!
//! Conversions to and from Polars and arrow-rs are feature-gated. Both
//! directions are lossy by design: `Integer` widens every integer width to
//! `Int64`, `Float` to `Float64`, and `Decimal` has no precision here, so a
//! writer that needs one must carry it in an extension.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One of Ossie's ten logical types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
#[serde(rename_all = "PascalCase")]
pub enum LogicalType {
    /// Variable-length Unicode text.
    String,
    /// Exact integral number.
    Integer,
    /// Exact base-10 number, precision unspecified.
    Decimal,
    /// Approximate floating-point number.
    Float,
    /// Two-valued truth.
    Boolean,
    /// Calendar date, no time of day.
    Date,
    /// Time of day, no date, no zone.
    Time,
    /// Date and time with no zone or offset.
    DateTime,
    /// An instant, identified with an offset or zone.
    DateTimeTz,
    /// A known type outside the portable vocabulary.
    Opaque,
}

impl LogicalType {
    /// Every value, in the spec's order — for pickers and validation messages.
    pub const ALL: [Self; 10] = [
        Self::String,
        Self::Integer,
        Self::Decimal,
        Self::Float,
        Self::Boolean,
        Self::Date,
        Self::Time,
        Self::DateTime,
        Self::DateTimeTz,
        Self::Opaque,
    ];

    /// The wire and stored spelling — the inverse of `parse`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Integer => "Integer",
            Self::Decimal => "Decimal",
            Self::Float => "Float",
            Self::Boolean => "Boolean",
            Self::Date => "Date",
            Self::Time => "Time",
            Self::DateTime => "DateTime",
            Self::DateTimeTz => "DateTimeTz",
            Self::Opaque => "Opaque",
        }
    }

    /// Parse the wire spelling. Strict: `"string"` is not `String`.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Integer, Decimal or Float.
    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::Integer | Self::Decimal | Self::Float)
    }

    /// Date, Time, DateTime or DateTimeTz — the types Ossie treats as a time
    /// dimension by default.
    pub const fn is_temporal(self) -> bool {
        matches!(
            self,
            Self::Date | Self::Time | Self::DateTime | Self::DateTimeTz
        )
    }
}

/// One column of a physical table: its name, logical type, nullability, and
/// the engine's own spelling of the type when it matters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct ColumnSchema {
    pub name: String,
    pub datatype: LogicalType,
    #[serde(default = "default_true")]
    pub nullable: bool,
    /// The physical type as the engine that read or wrote it spells it
    /// (`datetime[μs]`, `list[str]`, `Int32`). Informative; never parsed.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub physical: Option<String>,
}

const fn default_true() -> bool {
    true
}

impl ColumnSchema {
    pub fn new(name: impl Into<String>, datatype: LogicalType) -> Self {
        Self {
            name: name.into(),
            datatype,
            nullable: true,
            physical: None,
        }
    }
}

/// The columns of a table, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export)]
pub struct TableSchema {
    pub columns: Vec<ColumnSchema>,
}

impl TableSchema {
    pub fn column(&self, name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn has_column(&self, name: &str) -> bool {
        self.column(name).is_some()
    }
}

#[cfg(feature = "polars")]
mod polars_impl {
    use super::{ColumnSchema, LogicalType, TableSchema};
    use polars::prelude::{DataType, Field, Schema, TimeUnit, TimeZone};

    impl LogicalType {
        /// Classify a Polars dtype. Every integer width is `Integer`, every
        /// float width is `Float`, categoricals are `String`; anything else is
        /// `Opaque`.
        pub fn from_polars(dtype: &DataType) -> Self {
            match dtype {
                DataType::Boolean => Self::Boolean,
                DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64 => Self::Integer,
                DataType::Float32 | DataType::Float64 => Self::Float,
                DataType::String | DataType::Categorical(_, _) => Self::String,
                DataType::Date => Self::Date,
                DataType::Time => Self::Time,
                DataType::Datetime(_, None) => Self::DateTime,
                DataType::Datetime(_, Some(_)) => Self::DateTimeTz,
                _ => Self::Opaque,
            }
        }

        /// The Polars dtype a column of this logical type is written as.
        /// `Decimal` becomes `Float64` (no precision to carry); `Opaque` has no
        /// answer.
        pub fn to_polars(self) -> Option<DataType> {
            Some(match self {
                Self::String => DataType::String,
                Self::Integer => DataType::Int64,
                Self::Decimal | Self::Float => DataType::Float64,
                Self::Boolean => DataType::Boolean,
                Self::Date => DataType::Date,
                Self::Time => DataType::Time,
                Self::DateTime => DataType::Datetime(TimeUnit::Microseconds, None),
                Self::DateTimeTz => DataType::Datetime(TimeUnit::Microseconds, Some(TimeZone::UTC)),
                Self::Opaque => return None,
            })
        }
    }

    impl ColumnSchema {
        /// A column from a Polars field, keeping the Polars spelling as
        /// `physical` whenever the logical type does not name it exactly.
        pub fn from_polars_field(field: &Field) -> Self {
            let datatype = LogicalType::from_polars(field.dtype());
            let physical = format!("{}", field.dtype());
            let exact = matches!(
                (datatype, field.dtype()),
                (LogicalType::String, DataType::String)
                    | (LogicalType::Integer, DataType::Int64)
                    | (LogicalType::Float, DataType::Float64)
                    | (LogicalType::Boolean, DataType::Boolean)
                    | (LogicalType::Date, DataType::Date)
            );
            Self {
                name: field.name().to_string(),
                datatype,
                nullable: true,
                physical: (!exact).then_some(physical),
            }
        }
    }

    impl TableSchema {
        pub fn from_polars_schema(schema: &Schema) -> Self {
            Self {
                columns: schema
                    .iter()
                    .map(|(name, dtype)| {
                        ColumnSchema::from_polars_field(&Field::new(name.clone(), dtype.clone()))
                    })
                    .collect(),
            }
        }
    }
}

#[cfg(feature = "arrow")]
mod arrow_impl {
    use super::LogicalType;
    use arrow_schema::{DataType, TimeUnit};

    impl LogicalType {
        /// Classify an arrow-rs dtype, with the same widening as `from_polars`.
        pub fn from_arrow(dtype: &DataType) -> Self {
            match dtype {
                DataType::Boolean => Self::Boolean,
                DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64 => Self::Integer,
                DataType::Float16 | DataType::Float32 | DataType::Float64 => Self::Float,
                DataType::Decimal128(_, _) | DataType::Decimal256(_, _) => Self::Decimal,
                DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => Self::String,
                DataType::Date32 | DataType::Date64 => Self::Date,
                DataType::Time32(_) | DataType::Time64(_) => Self::Time,
                DataType::Timestamp(_, None) => Self::DateTime,
                DataType::Timestamp(_, Some(_)) => Self::DateTimeTz,
                _ => Self::Opaque,
            }
        }

        /// The arrow-rs dtype a Parquet writer uses for this logical type.
        /// `Decimal` is written as `Float64` until a precision is declared;
        /// `Opaque` has no answer and the writer keeps its own inference.
        pub fn to_arrow(self) -> Option<DataType> {
            Some(match self {
                Self::String => DataType::Utf8,
                Self::Integer => DataType::Int64,
                Self::Decimal | Self::Float => DataType::Float64,
                Self::Boolean => DataType::Boolean,
                Self::Date => DataType::Date32,
                Self::Time => DataType::Time64(TimeUnit::Microsecond),
                Self::DateTime => DataType::Timestamp(TimeUnit::Microsecond, None),
                Self::DateTimeTz => DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                Self::Opaque => return None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_are_the_spec_spellings() {
        assert_eq!(
            serde_json::to_string(&LogicalType::DateTimeTz).unwrap(),
            "\"DateTimeTz\""
        );
        assert_eq!(
            serde_json::to_string(&LogicalType::String).unwrap(),
            "\"String\""
        );
        for t in LogicalType::ALL {
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(json, format!("\"{}\"", t.as_str()));
            assert_eq!(serde_json::from_str::<LogicalType>(&json).unwrap(), t);
            assert_eq!(LogicalType::parse(t.as_str()), Some(t));
        }
        assert_eq!(LogicalType::parse("string"), None);
    }

    #[test]
    fn numeric_and_temporal_predicates() {
        assert!(LogicalType::Integer.is_numeric());
        assert!(LogicalType::Decimal.is_numeric());
        assert!(!LogicalType::String.is_numeric());
        assert!(LogicalType::Date.is_temporal());
        assert!(LogicalType::DateTimeTz.is_temporal());
        assert!(!LogicalType::Integer.is_temporal());
        assert!(!LogicalType::Opaque.is_temporal());
    }

    #[test]
    fn column_schema_defaults_nullable_and_omits_physical() {
        let col = ColumnSchema::new("id", LogicalType::Integer);
        let json = serde_json::to_value(&col).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"name": "id", "datatype": "Integer", "nullable": true})
        );
        let back: ColumnSchema =
            serde_json::from_value(serde_json::json!({"name": "id", "datatype": "Integer"}))
                .unwrap();
        assert_eq!(back, col);
    }

    #[test]
    fn table_schema_lookup() {
        let schema = TableSchema {
            columns: vec![ColumnSchema::new("a", LogicalType::String)],
        };
        assert!(schema.has_column("a"));
        assert!(!schema.has_column("b"));
        assert_eq!(
            schema.column("a").map(|c| c.datatype),
            Some(LogicalType::String)
        );
    }

    #[cfg(feature = "polars")]
    mod polars {
        use super::*;
        use ::polars::prelude::{DataType, Field, TimeUnit, TimeZone};

        #[test]
        fn polars_round_trip_for_the_concrete_types() {
            for t in [
                LogicalType::String,
                LogicalType::Integer,
                LogicalType::Float,
                LogicalType::Boolean,
                LogicalType::Date,
                LogicalType::Time,
                LogicalType::DateTime,
                LogicalType::DateTimeTz,
            ] {
                let dt = t.to_polars().unwrap();
                assert_eq!(LogicalType::from_polars(&dt), t, "{t:?}");
            }
            assert_eq!(LogicalType::Decimal.to_polars(), Some(DataType::Float64));
            assert_eq!(LogicalType::Opaque.to_polars(), None);
        }

        #[test]
        fn polars_widening_and_opaque() {
            assert_eq!(
                LogicalType::from_polars(&DataType::UInt8),
                LogicalType::Integer
            );
            assert_eq!(
                LogicalType::from_polars(&DataType::Float32),
                LogicalType::Float
            );
            assert_eq!(
                LogicalType::from_polars(&DataType::Datetime(
                    TimeUnit::Milliseconds,
                    Some(TimeZone::UTC)
                )),
                LogicalType::DateTimeTz
            );
            assert_eq!(
                LogicalType::from_polars(&DataType::List(Box::new(DataType::String))),
                LogicalType::Opaque
            );
        }

        #[test]
        fn physical_is_kept_only_when_the_logical_type_is_not_exact() {
            let exact = ColumnSchema::from_polars_field(&Field::new("n".into(), DataType::Int64));
            assert_eq!(exact.physical, None);
            let widened = ColumnSchema::from_polars_field(&Field::new("n".into(), DataType::Int32));
            assert_eq!(widened.datatype, LogicalType::Integer);
            assert_eq!(widened.physical.as_deref(), Some("i32"));
            let opaque = ColumnSchema::from_polars_field(&Field::new(
                "l".into(),
                DataType::List(Box::new(DataType::String)),
            ));
            assert_eq!(opaque.datatype, LogicalType::Opaque);
            assert_eq!(opaque.physical.as_deref(), Some("list[str]"));
        }
    }

    #[cfg(feature = "arrow")]
    mod arrow {
        use super::*;
        use arrow_schema::{DataType, TimeUnit};

        #[test]
        fn arrow_round_trip_for_the_concrete_types() {
            for t in [
                LogicalType::String,
                LogicalType::Integer,
                LogicalType::Float,
                LogicalType::Boolean,
                LogicalType::Date,
                LogicalType::Time,
                LogicalType::DateTime,
                LogicalType::DateTimeTz,
            ] {
                let dt = t.to_arrow().unwrap();
                assert_eq!(LogicalType::from_arrow(&dt), t, "{t:?}");
            }
            assert_eq!(LogicalType::Decimal.to_arrow(), Some(DataType::Float64));
            assert_eq!(LogicalType::Opaque.to_arrow(), None);
            assert_eq!(
                LogicalType::from_arrow(&DataType::Decimal128(10, 2)),
                LogicalType::Decimal
            );
            assert_eq!(
                LogicalType::from_arrow(&DataType::Timestamp(TimeUnit::Second, None)),
                LogicalType::DateTime
            );
            assert_eq!(
                LogicalType::from_arrow(&DataType::Null),
                LogicalType::Opaque
            );
        }
    }
}
