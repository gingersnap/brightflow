//! Semantic queries: layered opinions about tables, columns, relationships
//! and metrics — the "what does it mean" half of the store, typed against
//! `brightflow-types` at the door.
//!
//! One row per (object, layer, producer). A producer applies a whole
//! `TableDeclaration` under its provenance in one transaction, replacing only
//! its own rows; a person or an agent writes one opinion at a time through
//! the action bus. Readers get the resolved view (`resolve_columns` /
//! `resolve_table` from the contract crate) and never see layers unless they
//! ask. The store holds no template of its own: with no producer and no edit
//! a table simply has no semantic rows.
//!
//! Rows stay stringly typed in `models.rs` so the migration is the schema
//! authority; the conversions here are the one place a stored string becomes
//! a contract enum, and an unknown stored value degrades to "no opinion" with
//! a warning rather than failing the read.

use brightflow_types::{
    diff_declarations, resolve_columns, resolve_table, ColumnExt, ColumnOpinion, ColumnRole,
    CustomExtension, Dataset, DatasetExt, DeclarationDiff, DocFields, Layer, LogicalType, Metric,
    MetricExpr, MetricExt, Polarity, Provenance, Relationship, ResolvedColumn, ResolvedTable,
    SemanticModel, TableDeclaration, TableOpinion, TimeGranularity,
};
use rusqlite::{params, OptionalExtension, Transaction};

use super::StoreDb;
use crate::error::{StoreError, StoreResult};
use crate::models::{
    ColumnSemanticRow, DeclarationChangeRow, MetricRow, RelationshipRow, TableSemanticsRow,
};
use crate::row::{execute, fetch_all};

/// What `apply_declaration` did, for logs and for the caller's own checks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedDeclaration {
    pub columns: usize,
    pub relationships: usize,
    /// Relationship names whose target table does not exist yet in this
    /// source. Not an error: the next apply picks them up.
    pub relationships_skipped: Vec<String>,
    pub metrics: usize,
    /// Metric names that carried no structured expression and were not
    /// stored; the store executes structured metrics only.
    pub metrics_skipped: Vec<String>,
    /// Declared columns the table's current schema does not have.
    pub columns_without_data: Vec<String>,
    /// What this apply changed against the same producer's previous rows;
    /// `None` on a first declaration or when nothing differed. Recorded in
    /// `declaration_changes` when present.
    pub diff: Option<DeclarationDiff>,
}

/// A relationship as stored, with table names resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRelationship {
    pub relationship: Relationship,
    pub provenance: Provenance,
}

/// A metric as stored, rebuilt into its Ossie shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMetric {
    pub metric: Metric,
    pub provenance: Provenance,
}

fn parse_json<T: serde::de::DeserializeOwned>(s: Option<&str>) -> Option<T> {
    s.and_then(|s| serde_json::from_str(s).ok())
}

fn to_json<T: serde::Serialize>(v: &T) -> Option<String> {
    serde_json::to_string(v).ok()
}

fn empty_to_none(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

/// SQLite's `datetime('now')` text as unix seconds; `0` when unparsable so an
/// odd row sorts last within its layer rather than failing the read.
fn updated_at_epoch(s: &str) -> i64 {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map_or(0, |t| t.and_utc().timestamp())
}

fn provenance_of(
    layer: &str,
    producer: &str,
    version: Option<&str>,
    hash: Option<&str>,
) -> Option<Provenance> {
    let layer = Layer::parse(layer)?;
    Some(Provenance {
        layer,
        producer: producer.to_string(),
        version: version.map(str::to_string),
        hash: hash.map(str::to_string),
    })
}

impl ColumnSemanticRow {
    /// The typed opinion this row states. `None` for an unknown layer, which
    /// the migration's `CHECK` makes impossible but the read still guards.
    pub fn to_opinion(&self) -> Option<ColumnOpinion> {
        let provenance = provenance_of(
            &self.layer,
            &self.producer,
            self.producer_version.as_deref(),
            self.producer_hash.as_deref(),
        )?;
        let warn_unknown = |what: &str, value: &str| {
            tracing::warn!(
                "unknown stored {what} '{value}' on column '{}' — treated as no opinion",
                self.column_name
            );
        };
        let datatype = self.datatype.as_deref().and_then(|s| {
            LogicalType::parse(s).or_else(|| {
                warn_unknown("datatype", s);
                None
            })
        });
        let role = self.role.as_deref().and_then(|s| {
            ColumnRole::parse(s).or_else(|| {
                warn_unknown("role", s);
                None
            })
        });
        let polarity = self.polarity.as_deref().and_then(|s| {
            Polarity::parse(s).or_else(|| {
                warn_unknown("polarity", s);
                None
            })
        });
        Some(ColumnOpinion {
            column: self.column_name.clone(),
            provenance,
            updated_at: updated_at_epoch(&self.updated_at),
            datatype,
            is_time: self.is_time,
            ext: ColumnExt {
                role,
                is_kpi: self.is_kpi,
                polarity,
                label: self.label.clone(),
            },
            description: self.description.clone(),
            ai_context: parse_json(self.ai_context_json.as_deref()),
            custom_extensions: parse_json(self.extensions_json.as_deref()).unwrap_or_default(),
        })
    }
}

impl TableSemanticsRow {
    pub fn to_opinion(&self) -> Option<TableOpinion> {
        let provenance = provenance_of(
            &self.layer,
            &self.producer,
            self.producer_version.as_deref(),
            self.producer_hash.as_deref(),
        )?;
        Some(TableOpinion {
            provenance,
            updated_at: updated_at_epoch(&self.updated_at),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            time_granularity: self
                .time_granularity
                .as_deref()
                .and_then(TimeGranularity::parse),
            comparison_periods: self.comparison_periods.and_then(|p| u32::try_from(p).ok()),
            doc: parse_json::<DocFields>(self.doc_json.as_deref()),
            ai_context: parse_json(self.ai_context_json.as_deref()),
            custom_extensions: parse_json(self.extensions_json.as_deref()).unwrap_or_default(),
        })
    }
}

/// Insert or replace one column opinion at its (layer, producer).
fn write_column_opinion_tx(
    tx: &Transaction<'_>,
    table_id: &str,
    o: &ColumnOpinion,
) -> rusqlite::Result<()> {
    tx.execute(
        r"INSERT INTO column_semantics
            (table_id, column_name, layer, producer, producer_version, producer_hash,
             datatype, is_time, role, is_kpi, polarity, label, description,
             ai_context_json, extensions_json)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
          ON CONFLICT (table_id, column_name, layer, producer) DO UPDATE SET
            producer_version = excluded.producer_version,
            producer_hash = excluded.producer_hash,
            datatype = excluded.datatype,
            is_time = excluded.is_time,
            role = excluded.role,
            is_kpi = excluded.is_kpi,
            polarity = excluded.polarity,
            label = excluded.label,
            description = excluded.description,
            ai_context_json = excluded.ai_context_json,
            extensions_json = excluded.extensions_json,
            updated_at = datetime('now')",
        params![
            table_id,
            o.column,
            o.provenance.layer.as_str(),
            o.provenance.producer,
            o.provenance.version,
            o.provenance.hash,
            o.datatype.map(LogicalType::as_str),
            o.is_time,
            o.ext.role.map(ColumnRole::as_str),
            o.ext.is_kpi,
            o.ext.polarity.map(Polarity::as_str),
            o.ext.label,
            o.description,
            o.ai_context.as_ref().and_then(to_json),
            (!o.custom_extensions.is_empty())
                .then(|| to_json(&o.custom_extensions))
                .flatten(),
        ],
    )?;
    Ok(())
}

fn write_table_opinion_tx(
    tx: &Transaction<'_>,
    table_id: &str,
    o: &TableOpinion,
) -> rusqlite::Result<()> {
    tx.execute(
        r"INSERT INTO table_semantics
            (table_id, layer, producer, producer_version, producer_hash,
             display_name, description, time_granularity, comparison_periods,
             doc_json, ai_context_json, extensions_json)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
          ON CONFLICT (table_id, layer, producer) DO UPDATE SET
            producer_version = excluded.producer_version,
            producer_hash = excluded.producer_hash,
            display_name = excluded.display_name,
            description = excluded.description,
            time_granularity = excluded.time_granularity,
            comparison_periods = excluded.comparison_periods,
            doc_json = excluded.doc_json,
            ai_context_json = excluded.ai_context_json,
            extensions_json = excluded.extensions_json,
            updated_at = datetime('now')",
        params![
            table_id,
            o.provenance.layer.as_str(),
            o.provenance.producer,
            o.provenance.version,
            o.provenance.hash,
            o.display_name,
            o.description,
            o.time_granularity.map(TimeGranularity::as_str),
            o.comparison_periods,
            o.doc.as_ref().and_then(to_json),
            o.ai_context.as_ref().and_then(to_json),
            (!o.custom_extensions.is_empty())
                .then(|| to_json(&o.custom_extensions))
                .flatten(),
        ],
    )?;
    Ok(())
}

fn table_id_by_name(
    tx: &Transaction<'_>,
    source_id: &str,
    name: &str,
) -> rusqlite::Result<Option<String>> {
    tx.query_row(
        "SELECT id FROM tables WHERE source_id = ? AND name = ?",
        params![source_id, name],
        |r| r.get(0),
    )
    .optional()
}

/// Column names in a table's stored schema (the contract's `TableSchema`).
fn schema_column_names(schema_json: Option<&str>) -> Option<Vec<String>> {
    let schema: brightflow_types::TableSchema = serde_json::from_str(schema_json?).ok()?;
    Some(schema.columns.into_iter().map(|c| c.name).collect())
}

impl StoreDb {
    /// Apply a producer's declaration for one table under `prov`, in one
    /// transaction: this producer's earlier rows at this layer go, the
    /// declaration's column, table, relationship and metric rows land. The
    /// table must already exist (data before meaning). Other producers' rows
    /// and every edit above this layer are untouched.
    pub async fn apply_declaration(
        &self,
        source_id: &str,
        decl: &TableDeclaration,
        prov: &Provenance,
    ) -> StoreResult<AppliedDeclaration> {
        if let Err(violations) = decl.validate() {
            let list: Vec<String> = violations.iter().map(ToString::to_string).collect();
            return Err(StoreError::InvalidDeclaration(list.join("; ")));
        }
        // Data before meaning: the table row must exist. Resolved outside the
        // transaction so the error can name the table.
        let table = self
            .get_table(source_id, &decl.name)
            .await?
            .ok_or_else(|| StoreError::TableNotFound(decl.name.clone()))?;
        let table_id = table.id;
        let schema_json = table.schema_json;
        let source_id = source_id.to_owned();
        let decl = decl.clone();
        let prov = prov.clone();
        let out = self
            .pool
            .transaction(move |tx| {
                let mut out = AppliedDeclaration::default();

                let layer = prov.layer.as_str();
                // The same producer's previous rows, kept only to diff.
                let previous_columns: Vec<ColumnOpinion> = fetch_all::<ColumnSemanticRow, _>(
                    tx,
                    "SELECT * FROM column_semantics WHERE table_id = ? AND layer = ? AND producer = ? ORDER BY column_name",
                    params![table_id, layer, prov.producer],
                )?
                .iter()
                .filter_map(ColumnSemanticRow::to_opinion)
                .collect();
                let previous_table: Option<TableOpinion> = fetch_all::<TableSemanticsRow, _>(
                    tx,
                    "SELECT * FROM table_semantics WHERE table_id = ? AND layer = ? AND producer = ?",
                    params![table_id, layer, prov.producer],
                )?
                .first()
                .and_then(TableSemanticsRow::to_opinion);
                let had_previous = !previous_columns.is_empty() || previous_table.is_some();
                let previous_version = previous_columns
                    .first()
                    .map(|c| c.provenance.version.clone())
                    .or_else(|| previous_table.as_ref().map(|t| t.provenance.version.clone()))
                    .flatten();
                tx.execute(
                    "DELETE FROM column_semantics WHERE table_id = ? AND layer = ? AND producer = ?",
                    params![table_id, layer, prov.producer],
                )?;
                tx.execute(
                    "DELETE FROM table_semantics WHERE table_id = ? AND layer = ? AND producer = ?",
                    params![table_id, layer, prov.producer],
                )?;
                tx.execute(
                    "DELETE FROM relationships WHERE from_table_id = ? AND layer = ? AND producer = ?",
                    params![table_id, layer, prov.producer],
                )?;
                tx.execute(
                    "DELETE FROM metrics WHERE table_id = ? AND layer = ? AND producer = ?",
                    params![table_id, layer, prov.producer],
                )?;

                let mut next_columns: Vec<ColumnOpinion> = Vec::new();
                let mut next_table: Option<TableOpinion> = None;
                if let Some(dataset) = &decl.dataset {
                    for field in &dataset.fields {
                        let opinion = ColumnOpinion::from_field(field, &prov);
                        write_column_opinion_tx(tx, &table_id, &opinion)?;
                        next_columns.push(opinion);
                        out.columns += 1;
                    }
                    let ext = dataset.brightflow().unwrap_or_default();
                    let table_opinion = TableOpinion {
                        provenance: prov.clone(),
                        updated_at: 0,
                        display_name: ext.display_name,
                        description: dataset.description.clone(),
                        time_granularity: ext.time_granularity,
                        comparison_periods: ext.comparison_periods,
                        doc: ext.doc,
                        ai_context: dataset.ai_context.clone(),
                        custom_extensions: dataset
                            .custom_extensions
                            .iter()
                            .filter(|e| e.vendor_name != brightflow_types::BRIGHTFLOW_VENDOR)
                            .cloned()
                            .collect(),
                    };
                    if !table_opinion.is_empty() {
                        write_table_opinion_tx(tx, &table_id, &table_opinion)?;
                        next_table = Some(table_opinion);
                    }
                    if let Some(names) = schema_column_names(schema_json.as_deref()) {
                        out.columns_without_data = dataset
                            .fields
                            .iter()
                            .filter(|f| !names.contains(&f.name))
                            .map(|f| f.name.clone())
                            .collect();
                    }
                }

                for rel in &decl.relationships {
                    let from_id = if rel.from == decl.name {
                        Some(table_id.clone())
                    } else {
                        table_id_by_name(tx, &source_id, &rel.from)?
                    };
                    let to_id = table_id_by_name(tx, &source_id, &rel.to)?;
                    let (Some(from_id), Some(to_id)) = (from_id, to_id) else {
                        out.relationships_skipped.push(rel.name.clone());
                        continue;
                    };
                    tx.execute(
                        r"INSERT INTO relationships
                            (source_id, name, from_table_id, to_table_id, from_columns_json,
                             to_columns_json, layer, producer, producer_version,
                             ai_context_json, extensions_json)
                          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                          ON CONFLICT (source_id, name, layer, producer) DO UPDATE SET
                            from_table_id = excluded.from_table_id,
                            to_table_id = excluded.to_table_id,
                            from_columns_json = excluded.from_columns_json,
                            to_columns_json = excluded.to_columns_json,
                            producer_version = excluded.producer_version,
                            ai_context_json = excluded.ai_context_json,
                            extensions_json = excluded.extensions_json,
                            updated_at = datetime('now')",
                        params![
                            source_id,
                            rel.name,
                            from_id,
                            to_id,
                            to_json(&rel.from_columns),
                            to_json(&rel.to_columns),
                            layer,
                            prov.producer,
                            prov.version,
                            rel.ai_context.as_ref().and_then(to_json),
                            (!rel.custom_extensions.is_empty())
                                .then(|| to_json(&rel.custom_extensions))
                                .flatten(),
                        ],
                    )?;
                    out.relationships += 1;
                }

                for metric in &decl.metrics {
                    let Some(ext) = metric.brightflow() else {
                        out.metrics_skipped.push(metric.name.clone());
                        continue;
                    };
                    tx.execute(
                        r"INSERT INTO metrics
                            (table_id, name, expr_json, sql, datatype, description, is_kpi,
                             polarity, format, layer, producer, producer_version,
                             ai_context_json, extensions_json)
                          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                          ON CONFLICT (table_id, name, layer, producer) DO UPDATE SET
                            expr_json = excluded.expr_json,
                            sql = excluded.sql,
                            datatype = excluded.datatype,
                            description = excluded.description,
                            is_kpi = excluded.is_kpi,
                            polarity = excluded.polarity,
                            format = excluded.format,
                            producer_version = excluded.producer_version,
                            ai_context_json = excluded.ai_context_json,
                            extensions_json = excluded.extensions_json,
                            updated_at = datetime('now')",
                        params![
                            table_id,
                            metric.name,
                            to_json(&ext.expr),
                            ext.expr.render_sql(),
                            metric.datatype.map(LogicalType::as_str),
                            metric.description,
                            ext.is_kpi,
                            ext.polarity.map(Polarity::as_str),
                            ext.format,
                            layer,
                            prov.producer,
                            prov.version,
                            metric.ai_context.as_ref().and_then(to_json),
                            {
                                let others: Vec<&CustomExtension> = metric
                                    .custom_extensions
                                    .iter()
                                    .filter(|e| e.vendor_name != brightflow_types::BRIGHTFLOW_VENDOR)
                                    .collect();
                                (!others.is_empty()).then(|| to_json(&others)).flatten()
                            },
                        ],
                    )?;
                    out.metrics += 1;
                }

                if had_previous {
                    let changes = diff_declarations(
                        &previous_columns,
                        &next_columns,
                        previous_table.as_ref(),
                        next_table.as_ref(),
                    );
                    if !changes.is_empty() {
                        let diff = DeclarationDiff {
                            producer: prov.producer.clone(),
                            from_version: previous_version,
                            to_version: prov.version.clone(),
                            changes,
                        };
                        tx.execute(
                            r"INSERT INTO declaration_changes
                                (table_id, producer, from_version, to_version, changes_json)
                              VALUES (?, ?, ?, ?, ?)",
                            params![
                                table_id,
                                diff.producer,
                                diff.from_version,
                                diff.to_version,
                                to_json(&diff.changes).unwrap_or_else(|| "[]".to_string()),
                            ],
                        )?;
                        out.diff = Some(diff);
                    }
                }
                Ok(out)
            })
            .await?;
        Ok(out)
    }

    /// Every recorded re-declaration of a table, newest first.
    pub async fn declaration_changes(&self, table_id: &str) -> StoreResult<Vec<DeclarationDiff>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<DeclarationChangeRow, _>(
                    conn,
                    "SELECT * FROM declaration_changes WHERE table_id = ? ORDER BY id DESC",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| DeclarationDiff {
                producer: row.producer,
                from_version: row.from_version,
                to_version: row.to_version,
                changes: parse_json(Some(&row.changes_json)).unwrap_or_default(),
            })
            .collect())
    }

    /// Remove every opinion about one column at the given layers and return
    /// what was removed, so a caller can put it back. This is "reset to
    /// declared": drop the user and agent rows, let the producers show.
    pub async fn delete_column_opinions_at_layers(
        &self,
        table_id: &str,
        column: &str,
        layers: &[Layer],
    ) -> StoreResult<Vec<ColumnOpinion>> {
        let table_id = table_id.to_owned();
        let column = column.to_owned();
        let layers: Vec<String> = layers.iter().map(|l| l.as_str().to_string()).collect();
        let removed = self
            .pool
            .transaction(move |tx| {
                let mut removed = Vec::new();
                for layer in &layers {
                    let rows = fetch_all::<ColumnSemanticRow, _>(
                        tx,
                        "SELECT * FROM column_semantics WHERE table_id = ? AND column_name = ? AND layer = ?",
                        params![table_id, column, layer],
                    )?;
                    removed.extend(rows.iter().filter_map(ColumnSemanticRow::to_opinion));
                    tx.execute(
                        "DELETE FROM column_semantics WHERE table_id = ? AND column_name = ? AND layer = ?",
                        params![table_id, column, layer],
                    )?;
                }
                Ok(removed)
            })
            .await?;
        Ok(removed)
    }

    /// Write one opinion at its (layer, producer): a person's or an agent's
    /// edit, or a detector row.
    pub async fn write_column_opinion(
        &self,
        table_id: &str,
        opinion: &ColumnOpinion,
    ) -> StoreResult<()> {
        let table_id = table_id.to_owned();
        let opinion = opinion.clone();
        self.pool
            .transaction(move |tx| write_column_opinion_tx(tx, &table_id, &opinion))
            .await?;
        Ok(())
    }

    /// Remove one opinion row; `true` when it existed.
    pub async fn delete_column_opinion(
        &self,
        table_id: &str,
        column: &str,
        prov: &Provenance,
    ) -> StoreResult<bool> {
        let table_id = table_id.to_owned();
        let column = column.to_owned();
        let layer = prov.layer.as_str();
        let producer = prov.producer.clone();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM column_semantics WHERE table_id = ? AND column_name = ? AND layer = ? AND producer = ?",
                    params![table_id, column, layer, producer],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    pub async fn write_table_opinion(
        &self,
        table_id: &str,
        opinion: &TableOpinion,
    ) -> StoreResult<()> {
        let table_id = table_id.to_owned();
        let opinion = opinion.clone();
        self.pool
            .transaction(move |tx| write_table_opinion_tx(tx, &table_id, &opinion))
            .await?;
        Ok(())
    }

    /// Remove one layer's table row; `true` when it existed.
    pub async fn delete_table_opinion(
        &self,
        table_id: &str,
        prov: &Provenance,
    ) -> StoreResult<bool> {
        let table_id = table_id.to_owned();
        let layer = prov.layer.as_str();
        let producer = prov.producer.clone();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM table_semantics WHERE table_id = ? AND layer = ? AND producer = ?",
                    params![table_id, layer, producer],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    /// Every layer's opinion about every column of a table.
    pub async fn column_opinions(&self, table_id: &str) -> StoreResult<Vec<ColumnOpinion>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ColumnSemanticRow, _>(
                    conn,
                    "SELECT * FROM column_semantics WHERE table_id = ? ORDER BY column_name, layer, producer",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows
            .iter()
            .filter_map(ColumnSemanticRow::to_opinion)
            .collect())
    }

    /// One opinion row, if that layer and producer have one for the column.
    pub async fn column_opinion(
        &self,
        table_id: &str,
        column: &str,
        prov: &Provenance,
    ) -> StoreResult<Option<ColumnOpinion>> {
        Ok(self.column_opinions(table_id).await?.into_iter().find(|o| {
            o.column == column
                && o.provenance.layer == prov.layer
                && o.provenance.producer == prov.producer
        }))
    }

    /// The resolved view of a table's columns, in column-name order.
    pub async fn resolved_columns(&self, table_id: &str) -> StoreResult<Vec<ResolvedColumn>> {
        Ok(resolve_columns(&self.column_opinions(table_id).await?))
    }

    pub async fn table_opinions(&self, table_id: &str) -> StoreResult<Vec<TableOpinion>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<TableSemanticsRow, _>(
                    conn,
                    "SELECT * FROM table_semantics WHERE table_id = ? ORDER BY layer, producer",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows
            .iter()
            .filter_map(TableSemanticsRow::to_opinion)
            .collect())
    }

    pub async fn resolved_table(&self, table_id: &str) -> StoreResult<Option<ResolvedTable>> {
        Ok(resolve_table(&self.table_opinions(table_id).await?))
    }

    /// Relationships whose `from` table is in `source_id`, table names
    /// resolved, highest layer first per name (readers usually want the
    /// first of each name).
    pub async fn relationships_for_source(
        &self,
        source_id: &str,
    ) -> StoreResult<Vec<StoredRelationship>> {
        let source_id = source_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<RelationshipRow, _>(
                    conn,
                    "SELECT * FROM relationships WHERE source_id = ? ORDER BY name, layer, producer",
                    params![source_id],
                )
            })
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(provenance) = provenance_of(
                &row.layer,
                &row.producer,
                row.producer_version.as_deref(),
                None,
            ) else {
                continue;
            };
            let (Some(from), Some(to)) = (
                self.get_table_by_id(&row.from_table_id).await?,
                self.get_table_by_id(&row.to_table_id).await?,
            ) else {
                continue;
            };
            out.push(StoredRelationship {
                relationship: Relationship {
                    name: row.name,
                    from: from.name,
                    to: to.name,
                    from_columns: parse_json(Some(&row.from_columns_json)).unwrap_or_default(),
                    to_columns: parse_json(Some(&row.to_columns_json)).unwrap_or_default(),
                    ai_context: parse_json(row.ai_context_json.as_deref()),
                    custom_extensions: parse_json(row.extensions_json.as_deref())
                        .unwrap_or_default(),
                },
                provenance,
            });
        }
        Ok(out)
    }

    pub async fn metrics_for_table(&self, table_id: &str) -> StoreResult<Vec<StoredMetric>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<MetricRow, _>(
                    conn,
                    "SELECT * FROM metrics WHERE table_id = ? ORDER BY name, layer, producer",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let provenance = provenance_of(
                    &row.layer,
                    &row.producer,
                    row.producer_version.as_deref(),
                    None,
                )?;
                let expr: MetricExpr = parse_json(Some(&row.expr_json))?;
                let ext = MetricExt {
                    expr,
                    is_kpi: row.is_kpi,
                    polarity: row.polarity.as_deref().and_then(Polarity::parse),
                    format: row.format.clone(),
                };
                let mut metric = Metric::structured(row.name.clone(), &ext);
                metric.description.clone_from(&row.description);
                metric.datatype = row.datatype.as_deref().and_then(LogicalType::parse);
                metric.ai_context = parse_json(row.ai_context_json.as_deref());
                if let Some(others) =
                    parse_json::<Vec<CustomExtension>>(row.extensions_json.as_deref())
                {
                    metric.custom_extensions.extend(others);
                }
                Some(StoredMetric { metric, provenance })
            })
            .collect())
    }

    /// The whole source as one Ossie-shaped model, from the resolved views.
    /// This is the export, and the LLM's context; the caller serialises.
    pub async fn export_model(&self, source_id: &str) -> StoreResult<SemanticModel> {
        let mut model = SemanticModel {
            name: source_id.to_string(),
            ..SemanticModel::default()
        };
        for table in self.list_tables_by_source(source_id).await? {
            let mut dataset = Dataset::new(&table.name, format!("{source_id}/{}", table.name));
            dataset.primary_key = parse_json(table.primary_keys.as_deref()).unwrap_or_default();
            dataset.fields = self
                .resolved_columns(&table.id)
                .await?
                .iter()
                .map(ResolvedColumn::to_field)
                .collect();
            if let Some(resolved) = self.resolved_table(&table.id).await? {
                dataset.description = empty_to_none(resolved.description);
                dataset.ai_context = resolved.ai_context;
                let ext = DatasetExt {
                    display_name: resolved.display_name,
                    time_granularity: resolved.time_granularity,
                    comparison_periods: resolved.comparison_periods,
                    doc: resolved.doc,
                };
                if ext != DatasetExt::default() {
                    dataset.set_brightflow(&ext);
                }
            }
            for stored in self.metrics_for_table(&table.id).await? {
                if !model.metrics.iter().any(|m| m.name == stored.metric.name) {
                    model.metrics.push(stored.metric);
                }
            }
            model.datasets.push(dataset);
        }
        for stored in self.relationships_for_source(source_id).await? {
            if !model
                .relationships
                .iter()
                .any(|r| r.name == stored.relationship.name)
            {
                model.relationships.push(stored.relationship);
            }
        }
        Ok(model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brightflow_types::{Aggregation, ColumnRole, Field, Layer, LogicalType};
    use tempfile::TempDir;

    /// Fresh, fully migrated `StoreDb` backed by a SQLite file in `tmp`.
    async fn temp_db(tmp: &TempDir) -> StoreDb {
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        StoreDb::new(&db_url).await.expect("failed to open db")
    }

    fn github() -> Provenance {
        Provenance::declared("connector:github").with_version("0.3.0")
    }

    fn user() -> Provenance {
        Provenance {
            layer: Layer::User,
            producer: "user:1".into(),
            version: None,
            hash: None,
        }
    }

    fn issues_declaration() -> TableDeclaration {
        TableDeclaration::from_endpoint_json(
            "issues",
            "issues",
            vec!["id".into()],
            Some("updated_at".into()),
            serde_json::json!({
                "description": "Issues and pull requests",
                "columns": {
                    "id": {"datatype": "Integer", "brightflow": {"role": "ignored"}},
                    "number": {"datatype": "Integer", "brightflow": {"role": "ignored"}},
                    "created_at": {"datatype": "DateTimeTz", "description": "Opened at", "brightflow": {"role": "time"}},
                    "reactions_total": {"datatype": "Integer", "description": "Reactions", "brightflow": {"role": "measure", "is_kpi": true}}
                },
                "metrics": {"reactions": {"expression": {"column": "reactions_total", "aggregation": "sum"}}},
                "brightflow": {"display_name": "Issues", "time_granularity": "week", "comparison_periods": 4}
            }),
        )
        .expect("declaration")
    }

    fn comments_declaration() -> TableDeclaration {
        TableDeclaration::from_endpoint_json(
            "issue_comments",
            "issue_comments",
            vec!["id".into()],
            None,
            serde_json::json!({
                "columns": {
                    "id": {"datatype": "Integer"},
                    "issue_number": {"datatype": "Integer"}
                },
                "relationships": {"issue": {"to": "issues", "from_columns": ["issue_number"], "to_columns": ["number"]}}
            }),
        )
        .expect("declaration")
    }

    #[tokio::test]
    async fn apply_then_resolve_and_a_user_edit_layers_on_top() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let table = db.create_table("issues", "c:s1").await.expect("table");

        let applied = db
            .apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("apply");
        assert_eq!(applied.columns, 4);
        assert_eq!(applied.metrics, 1);

        let resolved = db.resolved_columns(&table.id).await.expect("resolved");
        let reactions = resolved
            .iter()
            .find(|c| c.name == "reactions_total")
            .expect("reactions");
        assert_eq!(reactions.role, Some(ColumnRole::Measure));
        assert_eq!(reactions.is_kpi, Some(true));
        assert_eq!(reactions.datatype, Some(LogicalType::Integer));
        assert_eq!(reactions.description.as_deref(), Some("Reactions"));
        assert_eq!(
            reactions.resolved_by.as_ref().map(|p| p.version.as_deref()),
            Some(Some("0.3.0"))
        );
        let created = resolved
            .iter()
            .find(|c| c.name == "created_at")
            .expect("created");
        assert!(created.resolved_is_time());

        let table_resolved = db
            .resolved_table(&table.id)
            .await
            .expect("table")
            .expect("some");
        assert_eq!(table_resolved.display_name.as_deref(), Some("Issues"));
        assert_eq!(table_resolved.comparison_periods, Some(4));
        assert_eq!(
            table_resolved.description.as_deref(),
            Some("Issues and pull requests")
        );

        // A person renames the column and blanks the description.
        let mut edit = ColumnOpinion::empty("reactions_total", user());
        edit.ext.label = Some("Reactions".into());
        edit.description = Some(String::new());
        db.write_column_opinion(&table.id, &edit)
            .await
            .expect("edit");
        let resolved = db.resolved_columns(&table.id).await.expect("resolved");
        let reactions = resolved
            .iter()
            .find(|c| c.name == "reactions_total")
            .expect("reactions");
        assert_eq!(reactions.label.as_deref(), Some("Reactions"));
        assert_eq!(reactions.description, None);
        assert_eq!(reactions.is_kpi, Some(true));
        assert_eq!(
            reactions.resolved_by.as_ref().map(|p| p.layer),
            Some(Layer::User)
        );

        // Re-applying the connector replaces only its rows: the edit stays.
        db.apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("re-apply");
        let resolved = db.resolved_columns(&table.id).await.expect("resolved");
        let reactions = resolved
            .iter()
            .find(|c| c.name == "reactions_total")
            .expect("reactions");
        assert_eq!(reactions.label.as_deref(), Some("Reactions"));
        assert_eq!(
            db.column_opinions(&table.id).await.expect("opinions").len(),
            5
        );

        // Deleting the edit row brings the declared description back.
        assert!(db
            .delete_column_opinion(&table.id, "reactions_total", &user())
            .await
            .expect("delete"));
        let resolved = db.resolved_columns(&table.id).await.expect("resolved");
        let reactions = resolved
            .iter()
            .find(|c| c.name == "reactions_total")
            .expect("reactions");
        assert_eq!(reactions.description.as_deref(), Some("Reactions"));
    }

    /// A re-declaration by the same producer records what changed, with the
    /// versions on either side; an identical re-apply records nothing.
    #[tokio::test]
    async fn redeclaring_records_a_diff_between_versions() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let table = db.create_table("issues", "c:s1").await.expect("table");
        let first = db
            .apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("apply");
        assert_eq!(first.diff, None);
        let again = db
            .apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("re-apply");
        assert_eq!(again.diff, None);

        let mut bumped = issues_declaration();
        let dataset = bumped.dataset.as_mut().expect("dataset");
        let field = dataset
            .fields
            .iter_mut()
            .find(|f| f.name == "reactions_total")
            .expect("field");
        field.description = Some("Reactions on the issue".into());
        let newer = Provenance::declared("connector:github").with_version("0.4.0");
        let applied = db
            .apply_declaration("c:s1", &bumped, &newer)
            .await
            .expect("apply newer");
        let diff = applied.diff.expect("diff");
        assert_eq!(diff.from_version.as_deref(), Some("0.3.0"));
        assert_eq!(diff.to_version.as_deref(), Some("0.4.0"));
        assert_eq!(diff.changed_columns(), ["reactions_total"]);
        assert_eq!(diff.changes[0].field, "description");
        assert_eq!(diff.changes[0].from.as_deref(), Some("Reactions"));

        let history = db.declaration_changes(&table.id).await.expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0], diff);
    }

    /// Reset to declared: the user and agent rows go and come back as
    /// returned; the connector's rows stay.
    #[tokio::test]
    async fn reset_removes_edits_above_the_declaration_and_returns_them() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let table = db.create_table("issues", "c:s1").await.expect("table");
        db.apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("apply");
        let mut edit = ColumnOpinion::empty("reactions_total", user());
        edit.ext.label = Some("Reactions".into());
        db.write_column_opinion(&table.id, &edit)
            .await
            .expect("edit");

        let removed = db
            .delete_column_opinions_at_layers(
                &table.id,
                "reactions_total",
                &[Layer::User, Layer::Agent],
            )
            .await
            .expect("reset");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].ext.label.as_deref(), Some("Reactions"));
        let resolved = db.resolved_columns(&table.id).await.expect("resolved");
        let reactions = resolved
            .iter()
            .find(|c| c.name == "reactions_total")
            .expect("column");
        assert_eq!(reactions.label, None);
        assert_eq!(
            reactions.resolved_by.as_ref().map(|p| p.layer),
            Some(Layer::Declared)
        );
    }

    #[tokio::test]
    async fn a_producer_only_replaces_its_own_rows() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let table = db.create_table("issues", "c:s1").await.expect("table");
        db.apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("apply");

        let enrichment = Provenance::declared("enrichment:classify");
        let mut decl = TableDeclaration::new("issues");
        let mut dataset = Dataset::new("issues", "issues");
        dataset.fields = vec![Field::column("summary")
            .with_brightflow(&ColumnExt::role(ColumnRole::Ignored).with_label("Summary"))];
        decl.dataset = Some(dataset);
        db.apply_declaration("c:s1", &decl, &enrichment)
            .await
            .expect("apply enrichment");
        assert_eq!(
            db.column_opinions(&table.id).await.expect("opinions").len(),
            5
        );

        // The enrichment re-declares with no columns: its row goes, the
        // connector's four stay.
        decl.dataset = Some(Dataset::new("issues", "issues"));
        db.apply_declaration("c:s1", &decl, &enrichment)
            .await
            .expect("re-apply enrichment");
        let opinions = db.column_opinions(&table.id).await.expect("opinions");
        assert_eq!(opinions.len(), 4);
        assert!(opinions
            .iter()
            .all(|o| o.provenance.producer == "connector:github"));
    }

    #[tokio::test]
    async fn relationships_wait_for_their_target_table() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        db.create_table("issue_comments", "c:s1")
            .await
            .expect("table");

        let first = db
            .apply_declaration("c:s1", &comments_declaration(), &github())
            .await
            .expect("apply");
        assert_eq!(first.relationships, 0);
        assert_eq!(first.relationships_skipped, vec!["issue".to_string()]);

        db.create_table("issues", "c:s1").await.expect("issues");
        let second = db
            .apply_declaration("c:s1", &comments_declaration(), &github())
            .await
            .expect("apply again");
        assert_eq!(second.relationships, 1);
        let stored = db.relationships_for_source("c:s1").await.expect("rels");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].relationship.from, "issue_comments");
        assert_eq!(stored[0].relationship.to, "issues");
        assert_eq!(stored[0].relationship.from_columns, ["issue_number"]);
    }

    #[tokio::test]
    async fn export_rebuilds_a_valid_model_from_the_resolved_views() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        db.create_table("issues", "c:s1").await.expect("issues");
        db.create_table("issue_comments", "c:s1")
            .await
            .expect("comments");
        db.apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("apply");
        db.apply_declaration("c:s1", &comments_declaration(), &github())
            .await
            .expect("apply");

        let model = db.export_model("c:s1").await.expect("export");
        assert_eq!(model.name, "c:s1");
        assert_eq!(model.datasets.len(), 2);
        assert_eq!(model.relationships.len(), 1);
        assert_eq!(model.metrics.len(), 1);
        assert_eq!(
            model.metrics[0].structured_expr().map(|e| e.aggregation),
            Some(Aggregation::Sum)
        );
        assert_eq!(
            model.metrics[0].expression.ansi_text(),
            Some("SUM(issues.reactions_total)")
        );
        let issues = model
            .datasets
            .iter()
            .find(|d| d.name == "issues")
            .expect("issues");
        assert_eq!(issues.source, "c:s1/issues");
        assert_eq!(
            issues.description.as_deref(),
            Some("Issues and pull requests")
        );
        assert_eq!(
            issues.brightflow().and_then(|e| e.display_name),
            Some("Issues".into())
        );
        assert_eq!(
            issues.field("created_at").and_then(|f| f.datatype),
            Some(LogicalType::DateTimeTz)
        );
        assert_eq!(model.validate(), Ok(()));
    }

    #[tokio::test]
    async fn apply_refuses_a_missing_table_and_an_invalid_declaration() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let err = db
            .apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect_err("no table");
        assert!(matches!(err, StoreError::TableNotFound(name) if name == "issues"));

        db.create_table("issues", "c:s1").await.expect("table");
        let mut bad = issues_declaration();
        bad.contract_version = brightflow_types::CONTRACT_VERSION + 1;
        let err = db
            .apply_declaration("c:s1", &bad, &github())
            .await
            .expect_err("too new");
        assert!(matches!(err, StoreError::InvalidDeclaration(_)));
    }

    #[tokio::test]
    async fn columns_without_data_come_from_the_stored_schema() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let table = db.create_table("issues", "c:s1").await.expect("table");
        db.update_table_meta(
            &table.id,
            Some(r#"{"columns":[{"name":"id","datatype":"Integer"},{"name":"number","datatype":"Integer"}]}"#),
            None,
            0,
        )
        .await
        .expect("meta");
        let applied = db
            .apply_declaration("c:s1", &issues_declaration(), &github())
            .await
            .expect("apply");
        assert_eq!(
            applied.columns_without_data,
            vec!["created_at".to_string(), "reactions_total".to_string()]
        );
    }

    #[tokio::test]
    async fn table_opinions_write_and_delete_per_producer() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;
        let table = db.create_table("issues", "c:s1").await.expect("table");
        let mut mine = TableOpinion::empty(user());
        mine.time_granularity = Some(TimeGranularity::Month);
        db.write_table_opinion(&table.id, &mine)
            .await
            .expect("write");
        assert_eq!(
            db.resolved_table(&table.id)
                .await
                .expect("resolved")
                .and_then(|t| t.time_granularity),
            Some(TimeGranularity::Month)
        );
        assert!(db
            .delete_table_opinion(&table.id, &user())
            .await
            .expect("delete"));
        assert!(!db
            .delete_table_opinion(&table.id, &user())
            .await
            .expect("again"));
        assert_eq!(db.resolved_table(&table.id).await.expect("resolved"), None);
    }

    /// The SQL `CHECK` literals are a copy of the contract crate's enums;
    /// this is the test that keeps the copy honest.
    #[test]
    fn migration_check_lists_match_the_contract_enums() {
        let sql = include_str!("../../migrations/027_layered_semantics.sql");
        let list_after = |marker: &str| -> Vec<String> {
            let start = sql.find(marker).unwrap_or_else(|| panic!("no {marker}")) + marker.len();
            let rest = &sql[start..];
            let open = rest.find('(').expect("open paren");
            let close = rest[open..].find(')').expect("close paren") + open;
            rest[open + 1..close]
                .split(',')
                .map(|s| s.trim().trim_matches('\'').to_string())
                .collect()
        };
        assert_eq!(
            list_after("layer            TEXT NOT NULL CHECK (layer IN"),
            Layer::PRECEDENCE
                .iter()
                .rev()
                .map(|l| l.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            list_after("role             TEXT CHECK (role IS NULL OR role IN"),
            ColumnRole::ALL
                .iter()
                .map(|r| r.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            list_after("polarity         TEXT CHECK (polarity IS NULL OR polarity IN"),
            Polarity::ALL
                .iter()
                .map(|p| p.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            list_after(
                "time_granularity   TEXT CHECK (time_granularity IS NULL OR time_granularity IN"
            ),
            TimeGranularity::ALL
                .iter()
                .map(|g| g.as_str().to_string())
                .collect::<Vec<_>>()
        );
        let datatypes = list_after("datatype         TEXT CHECK (datatype IS NULL OR datatype IN");
        assert_eq!(
            datatypes,
            LogicalType::ALL
                .iter()
                .map(|t| t.as_str().to_string())
                .collect::<Vec<_>>()
        );
    }
}
