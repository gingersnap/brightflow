//! What a model's output columns mean, declared under the model's own
//! producer so a person's edit above that layer is never touched.
//!
//! A column that passes through the chain with the same name and datatype
//! as an input column keeps that column's resolved label, description, role,
//! polarity and KPI flag. A period column is the time axis. Any other new
//! column after a reshape (an aggregate, a pivoted value) is a measure.
//! Columns this cannot place carry no opinion and are left to the detector.

use brightflow_types::{
    ColumnExt, ColumnRole, Dataset, Field, LogicalType, Operation, Provenance, ResolvedColumn,
    TableDeclaration, TableSchema,
};

use crate::shared::AppResult;

/// The provenance a model's declarations are filed under. Identity is the
/// model, so a rebuild replaces the previous build's rows wholesale.
pub fn model_provenance(model_id: &str) -> Provenance {
    Provenance::declared(format!("model:{model_id}"))
}

/// One field per output column the model can say something about.
///
/// A column passes through when the input's *physical* schema has it under
/// the same name and datatype; its meaning then comes from the input's
/// resolved opinions, which may say nothing about the datatype.
pub fn carry_over(
    input_schema: &TableSchema,
    input: &[ResolvedColumn],
    output: &TableSchema,
    operations: &[Operation],
) -> Vec<Field> {
    let period_columns: Vec<&str> = operations
        .iter()
        .flat_map(|op| match op {
            Operation::WithColumns { columns } => {
                columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()
            },
            _ => Vec::new(),
        })
        .collect();
    let reshaped = operations.iter().any(Operation::reshapes);

    output
        .columns
        .iter()
        .filter_map(|col| {
            let passed_through = input_schema
                .column(&col.name)
                .filter(|physical| physical.datatype == col.datatype)
                .and_then(|_| input.iter().find(|i| i.name == col.name));
            if let Some(source) = passed_through {
                let mut field = Field::column(&col.name).with_datatype(col.datatype);
                if let Some(description) = &source.description {
                    field = field.with_description(description.clone());
                }
                if let Some(is_time) = source.is_time {
                    field = field.with_is_time(is_time);
                }
                let ext = source.ext();
                if ext != ColumnExt::default() {
                    field = field.with_brightflow(&ext);
                }
                return Some(field);
            }
            if period_columns.contains(&col.name.as_str()) {
                return Some(
                    Field::column(&col.name)
                        .with_datatype(LogicalType::String)
                        .with_is_time(true)
                        .with_brightflow(&ColumnExt::role(ColumnRole::Time)),
                );
            }
            if reshaped {
                return Some(
                    Field::column(&col.name)
                        .with_datatype(col.datatype)
                        .with_brightflow(&ColumnExt::role(ColumnRole::Measure)),
                );
            }
            None
        })
        .collect()
}

/// Apply the model's declaration for its output table. The table row must
/// exist already, which is why this follows the write.
pub async fn declare_model_output(
    store: &brightflow_store::ParquetStore,
    model_id: &str,
    source_id: &str,
    output: &str,
    input: &str,
    fields: Vec<Field>,
) -> AppResult<()> {
    let mut dataset = Dataset::new(output, format!("{source_id}/{output}"));
    dataset.description = Some(format!("Built from {input} by the model {output}."));
    dataset.fields = fields;
    let decl = TableDeclaration {
        dataset: Some(dataset),
        ..TableDeclaration::new(output)
    };
    store
        .apply_declaration(source_id, &decl, &model_provenance(model_id))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use brightflow_types::{
        AggSpec, Aggregation, ColumnSchema, DerivedColumn, DerivedExpr, Polarity, TimeGranularity,
    };

    fn resolved(name: &str, datatype: LogicalType) -> ResolvedColumn {
        ResolvedColumn {
            name: name.to_string(),
            datatype: Some(datatype),
            is_time: None,
            role: None,
            is_kpi: None,
            polarity: None,
            label: None,
            description: None,
            ai_context: None,
            resolved_by: None,
        }
    }

    fn schema(cols: &[(&str, LogicalType)]) -> TableSchema {
        TableSchema {
            columns: cols
                .iter()
                .map(|(n, t)| ColumnSchema::new(*n, *t))
                .collect(),
        }
    }

    #[test]
    fn pass_through_keeps_label_role_and_description() {
        let mut region = resolved("region", LogicalType::String);
        region.label = Some("Region".into());
        region.description = Some("Sales region".into());
        region.role = Some(ColumnRole::Dimension);
        let mut revenue = resolved("revenue", LogicalType::Float);
        revenue.role = Some(ColumnRole::Measure);
        revenue.polarity = Some(Polarity::HigherIsBetter);
        revenue.is_kpi = Some(true);

        let input = schema(&[
            ("region", LogicalType::String),
            ("revenue", LogicalType::Float),
        ]);
        let fields = carry_over(
            &input,
            &[region, revenue],
            &input,
            &[Operation::Limit { n: 10 }],
        );
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].description.as_deref(), Some("Sales region"));
        let ext = fields[0].brightflow().unwrap();
        assert_eq!(ext.label.as_deref(), Some("Region"));
        assert_eq!(ext.role, Some(ColumnRole::Dimension));
        let revenue_ext = fields[1].brightflow().unwrap();
        assert_eq!(revenue_ext.is_kpi, Some(true));
        assert_eq!(revenue_ext.polarity, Some(Polarity::HigherIsBetter));
    }

    #[test]
    fn aggregates_are_measures_and_periods_are_time_after_a_group_by() {
        let mut order_date = resolved("order_date", LogicalType::String);
        order_date.role = Some(ColumnRole::Time);
        let ops = vec![
            Operation::WithColumns {
                columns: vec![DerivedColumn {
                    name: "order_date__month".into(),
                    expr: DerivedExpr::Period {
                        column: "order_date".into(),
                        granularity: TimeGranularity::Month,
                    },
                }],
            },
            Operation::GroupBy {
                by: vec!["order_date__month".into()],
                aggs: vec![AggSpec {
                    column: "revenue".into(),
                    function: Aggregation::Sum,
                    alias: Some("sum".into()),
                }],
            },
        ];
        let input = schema(&[
            ("order_date", LogicalType::String),
            ("revenue", LogicalType::Float),
        ]);
        let fields = carry_over(
            &input,
            &[order_date, resolved("revenue", LogicalType::Float)],
            &schema(&[
                ("order_date__month", LogicalType::String),
                ("sum", LogicalType::Float),
            ]),
            &ops,
        );
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "order_date__month");
        assert!(fields[0].resolved_is_time());
        assert_eq!(fields[0].brightflow().unwrap().role, Some(ColumnRole::Time));
        assert_eq!(
            fields[1].brightflow().unwrap().role,
            Some(ColumnRole::Measure)
        );
    }

    #[test]
    fn a_changed_datatype_carries_nothing_and_unreshaped_unknowns_are_left_alone() {
        let mut n = resolved("n", LogicalType::Integer);
        n.label = Some("Count".into());
        let fields = carry_over(
            &schema(&[("n", LogicalType::Integer)]),
            &[n],
            &schema(&[("n", LogicalType::String), ("other", LogicalType::String)]),
            &[Operation::Limit { n: 1 }],
        );
        assert!(fields.is_empty());
    }
}
