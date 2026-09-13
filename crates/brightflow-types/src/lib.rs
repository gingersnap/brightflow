//! The contract between Longbow, Litehouse and the tools.
//!
//! Every crate that produces, stores or reads a table's types and meaning
//! depends on this one, so no two of them can hold a different vocabulary.
//! What lives here is exactly the set of types that cross a boundary between
//! two of: a producer (a connector, an enrichment function, the detector), the
//! store, the engine, the API and the generated TypeScript. Nothing here does
//! I/O; apply and export belong to the store, writing Parquet to the producer.
//!
//! The shape of the semantic model follows Apache Ossie (the Open Semantic
//! Interchange spec, `0.2.0.dev0`): datasets, fields, relationships, metrics,
//! `ai_context`, `custom_extensions`. Serialising a [`SemanticModel`] with
//! serde yields an Ossie document; deserialising accepts that form and a
//! shorter one for hand-authored declarations (see [`semantic`]). Brightflow's
//! own vocabulary — column role, polarity, KPI, display label, per-table
//! analysis defaults, structured metric expressions — is not in Ossie's core
//! and rides in a `BRIGHTFLOW` custom extension (see [`ext`]).
//!
//! Data types are Ossie's ten logical types (see [`datatype`]), which map onto
//! Parquet logical types. That mapping is the type contract between a Parquet
//! writer and the store; conversions to Polars and arrow-rs are behind the
//! `polars` and `arrow` features so a producer never compiles an engine it
//! does not use.
//!
//! A type enters this crate only once it crosses a second boundary. The
//! operations language (filter, group, pivot) is the next candidate and is
//! deliberately not here yet.

pub mod changes;
pub mod datatype;
pub mod declaration;
pub mod ext;
pub mod import;
pub mod provenance;
pub mod resolved;
pub mod semantic;
pub mod validate;

pub use changes::{diff_declarations, DeclarationChange, DeclarationDiff};
pub use datatype::{ColumnSchema, LogicalType, TableSchema};
pub use declaration::{TableDeclaration, CONTRACT_VERSION};
pub use ext::{
    Aggregation, ColumnExt, ColumnRole, DatasetExt, DocFields, FilterOp, MetricExpr, MetricExt,
    MetricFilter, Polarity, TimeGranularity, BRIGHTFLOW_VENDOR,
};
pub use import::{declarations_of, parse_model};
pub use provenance::{Layer, Provenance};
pub use resolved::{
    resolve_columns, resolve_table, ColumnOpinion, ResolvedColumn, ResolvedTable, TableOpinion,
};
pub use semantic::{
    AiContext, AiContextFields, CustomExtension, Dataset, DialectExpression, Dimension, Expression,
    Field, Metric, OssieDocument, Relationship, SemanticModel, ANSI_SQL, OSSIE_VERSION,
};
pub use validate::Violation;
