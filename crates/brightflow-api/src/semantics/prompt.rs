//! The resolved semantic model rendered as prompt text: the one "table card"
//! every LLM call in the system appends to its system prompt.
//!
//! What a table is (its description and doc columns), what each column
//! means (logical type, role, label, description, polarity), how it relates
//! to other tables, and any `ai_context` a producer wrote for a model. All of
//! it is what the store already resolved; this module only renders, so a
//! person's edit in Explore reaches the next model call without a redeploy.
//!
//! The block is not part of the enrichment cache key on purpose: a
//! description edit refreshes what new rows see without invalidating every
//! cached cell. Anything that should recompute cells belongs in the function
//! spec, not here.
//!
//! Size is bounded. Analysed columns are listed up to `MAX_LISTED_COLUMNS`;
//! `ignored` columns collapse to one line of names, capped too.

use brightflow_store::ParquetStore;
use brightflow_types::{
    AiContext, ColumnRole, Polarity, Relationship, ResolvedColumn, ResolvedTable,
};

use crate::shared::AppResult;

/// `write!` into a String; infallible, so the Result is not worth a `?`.
fn push_fmt(out: &mut String, args: std::fmt::Arguments<'_>) {
    use std::fmt::Write as _;
    if out.write_fmt(args).is_err() {
        unreachable!("fmt::Write for String is infallible");
    }
}

/// Analysed columns listed in full before the rest is summarised.
const MAX_LISTED_COLUMNS: usize = 80;
/// Names shown on the "not analysed" line.
const MAX_IGNORED_NAMES: usize = 30;

/// Everything the renderer needs, gathered by `for_table`.
#[derive(Debug, Clone, Default)]
pub struct TableContext {
    pub name: String,
    pub table: Option<ResolvedTable>,
    pub columns: Vec<ResolvedColumn>,
    /// Relationships touching this table, from either side.
    pub relationships: Vec<Relationship>,
}

/// The table's context from the store, by catalog id.
pub async fn for_table_id(store: &ParquetStore, table_id: &str) -> AppResult<String> {
    let Some(row) = store.db().get_table_by_id(table_id).await? else {
        return Ok(String::new());
    };
    for_table(store, &row.source_id, &row.name).await
}

/// The table's context from the store, by source and name. Empty when the
/// table has no semantic rows at all.
pub async fn for_table(store: &ParquetStore, source_id: &str, table: &str) -> AppResult<String> {
    let Some(row) = store.db().get_table(source_id, table).await? else {
        return Ok(String::new());
    };
    let relationships = store
        .db()
        .relationships_for_source(source_id)
        .await?
        .into_iter()
        .map(|r| r.relationship)
        .filter(|r| r.from == table || r.to == table)
        .collect();
    let ctx = TableContext {
        name: table.to_string(),
        table: store.db().resolved_table(&row.id).await?,
        columns: store.db().resolved_columns(&row.id).await?,
        relationships,
    };
    Ok(render(&ctx))
}

/// Render the context block. Empty when there is nothing to say.
pub fn render(ctx: &TableContext) -> String {
    if ctx.table.is_none() && ctx.columns.is_empty() && ctx.relationships.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(2048);

    // Header: name, display name, description.
    out.push_str("TABLE ");
    out.push_str(&ctx.name);
    if let Some(display) = ctx
        .table
        .as_ref()
        .and_then(|t| t.display_name.as_deref())
        .filter(|d| !d.is_empty() && *d != ctx.name)
    {
        push_fmt(&mut out, format_args!(" (\"{display}\")"));
    }
    if let Some(description) = ctx
        .table
        .as_ref()
        .and_then(|t| t.description.as_deref())
        .filter(|d| !d.is_empty())
    {
        out.push_str(" — ");
        out.push_str(description);
    }
    out.push('\n');

    // One row, from the doc fields.
    if let Some(doc) = ctx.table.as_ref().and_then(|t| t.doc.as_ref()) {
        let mut parts: Vec<String> = Vec::new();
        if let Some(id) = &doc.id {
            parts.push(format!("identified by `{id}`"));
        }
        if let Some(number) = &doc.number {
            parts.push(format!("numbered by `{number}`"));
        }
        if let Some(title) = &doc.title {
            parts.push(format!("its title is `{title}`"));
        }
        if let Some(body) = &doc.body {
            parts.push(format!("its text is `{body}`"));
        }
        if let Some(ts) = &doc.timestamp {
            parts.push(format!("its time is `{ts}`"));
        }
        if !parts.is_empty() {
            out.push_str("One row: ");
            out.push_str(&parts.join("; "));
            out.push_str(".\n");
        }
    }
    if let Some(note) = ctx
        .table
        .as_ref()
        .and_then(|t| t.ai_context.as_ref())
        .and_then(render_ai_context)
    {
        out.push_str("Note for the model: ");
        out.push_str(&note);
        out.push('\n');
    }

    // Columns.
    let (ignored, analysed): (Vec<&ResolvedColumn>, Vec<&ResolvedColumn>) = ctx
        .columns
        .iter()
        .partition(|c| c.role == Some(ColumnRole::Ignored));
    if !analysed.is_empty() {
        out.push_str("\nCOLUMNS\n");
        for column in analysed.iter().take(MAX_LISTED_COLUMNS) {
            out.push_str(&render_column(column));
            out.push('\n');
        }
        if analysed.len() > MAX_LISTED_COLUMNS {
            push_fmt(
                &mut out,
                format_args!("(+{} more columns)\n", analysed.len() - MAX_LISTED_COLUMNS),
            );
        }
    }
    if !ignored.is_empty() {
        let names: Vec<&str> = ignored
            .iter()
            .take(MAX_IGNORED_NAMES)
            .map(|c| c.name.as_str())
            .collect();
        out.push_str("Not analysed: ");
        out.push_str(&names.join(", "));
        if ignored.len() > MAX_IGNORED_NAMES {
            push_fmt(
                &mut out,
                format_args!(" (+{} more)", ignored.len() - MAX_IGNORED_NAMES),
            );
        }
        out.push('\n');
    }

    // Relationships.
    if !ctx.relationships.is_empty() {
        out.push_str("\nRELATED\n");
        for rel in &ctx.relationships {
            let from: Vec<String> = rel
                .from_columns
                .iter()
                .map(|c| format!("{}.{c}", rel.from))
                .collect();
            let to: Vec<String> = rel
                .to_columns
                .iter()
                .map(|c| format!("{}.{c}", rel.to))
                .collect();
            push_fmt(
                &mut out,
                format_args!("- {} → {} ({})", from.join(", "), to.join(", "), rel.name),
            );
            if let Some(note) = rel.ai_context.as_ref().and_then(render_ai_context) {
                out.push_str(": ");
                out.push_str(&note);
            }
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

fn render_column(column: &ResolvedColumn) -> String {
    let mut tags: Vec<String> = Vec::new();
    if let Some(datatype) = column.datatype {
        tags.push(datatype.as_str().to_string());
    }
    match column.role {
        Some(ColumnRole::Measure) => tags.push("measure".to_string()),
        Some(ColumnRole::Dimension) => tags.push("dimension".to_string()),
        Some(ColumnRole::Time) => tags.push("time axis".to_string()),
        Some(ColumnRole::Entity) => tags.push("identifier".to_string()),
        Some(ColumnRole::Ignored) | None => {},
    }
    if column.is_kpi == Some(true) {
        tags.push("KPI".to_string());
    }
    match column.polarity {
        Some(Polarity::HigherIsBetter) => tags.push("higher is better".to_string()),
        Some(Polarity::LowerIsBetter) => tags.push("lower is better".to_string()),
        Some(Polarity::Neutral) | None => {},
    }
    if let Some(label) = column
        .label
        .as_deref()
        .filter(|l| !l.is_empty() && *l != column.name)
    {
        tags.push(format!("\"{label}\""));
    }
    let mut line = format!("- {}", column.name);
    if !tags.is_empty() {
        push_fmt(&mut line, format_args!(" ({})", tags.join(", ")));
    }
    if let Some(description) = column.description.as_deref().filter(|d| !d.is_empty()) {
        line.push_str(": ");
        line.push_str(description);
    }
    if let Some(note) = column.ai_context.as_ref().and_then(render_ai_context) {
        line.push_str(" — ");
        line.push_str(&note);
    }
    line
}

/// One line from an `ai_context`, or `None` when it is empty.
fn render_ai_context(ctx: &AiContext) -> Option<String> {
    match ctx {
        AiContext::Text(text) => Some(text.trim().to_string()).filter(|t| !t.is_empty()),
        AiContext::Structured(fields) => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(instructions) = fields.instructions.as_deref().filter(|i| !i.is_empty()) {
                parts.push(instructions.trim().to_string());
            }
            if !fields.synonyms.is_empty() {
                parts.push(format!("also called {}", fields.synonyms.join(", ")));
            }
            if !fields.examples.is_empty() {
                parts.push(format!("examples: {}", fields.examples.join("; ")));
            }
            (!parts.is_empty()).then(|| parts.join(". "))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brightflow_types::{AiContextFields, DocFields, Layer, LogicalType, Provenance};

    fn column(name: &str, role: Option<ColumnRole>) -> ResolvedColumn {
        ResolvedColumn {
            name: name.to_string(),
            datatype: Some(LogicalType::Integer),
            role,
            ..ResolvedColumn::default()
        }
    }

    fn issues() -> TableContext {
        let mut reactions = column("reactions_total", Some(ColumnRole::Measure));
        reactions.is_kpi = Some(true);
        reactions.polarity = Some(Polarity::HigherIsBetter);
        reactions.description = Some("Total emoji reactions".into());
        let mut created = column("created_at", Some(ColumnRole::Time));
        created.datatype = Some(LogicalType::DateTimeTz);
        created.label = Some("Opened".into());
        let mut body = column("body", Some(ColumnRole::Ignored));
        body.ai_context = Some(AiContext::Text("Often holds pasted stack traces".into()));
        let mut untyped = column("state", Some(ColumnRole::Dimension));
        untyped.datatype = None;
        TableContext {
            name: "issues".into(),
            table: Some(ResolvedTable {
                display_name: Some("Issues".into()),
                description: Some("Issues and pull requests".into()),
                doc: Some(DocFields {
                    id: Some("id".into()),
                    title: Some("title".into()),
                    body: Some("body".into()),
                    ..DocFields::default()
                }),
                ai_context: Some(AiContext::Structured(AiContextFields {
                    instructions: Some("Bots open many of these".into()),
                    synonyms: vec!["tickets".into()],
                    examples: vec![],
                })),
                resolved_by: Some(Provenance {
                    layer: Layer::Declared,
                    producer: "connector:github".into(),
                    version: None,
                    hash: None,
                }),
                ..ResolvedTable::default()
            }),
            columns: vec![
                reactions,
                created,
                untyped,
                column("id", Some(ColumnRole::Ignored)),
                body,
            ],
            relationships: vec![Relationship {
                name: "issue".into(),
                from: "issue_comments".into(),
                to: "issues".into(),
                from_columns: vec!["issue_number".into()],
                to_columns: vec!["number".into()],
                ai_context: None,
                custom_extensions: vec![],
            }],
        }
    }

    #[test]
    fn renders_header_row_columns_and_relationships() {
        let text = render(&issues());
        let expected = "TABLE issues (\"Issues\") — Issues and pull requests\n\
One row: identified by `id`; its title is `title`; its text is `body`.\n\
Note for the model: Bots open many of these. also called tickets\n\
\n\
COLUMNS\n\
- reactions_total (Integer, measure, KPI, higher is better): Total emoji reactions\n\
- created_at (DateTimeTz, time axis, \"Opened\")\n\
- state (dimension)\n\
Not analysed: id, body\n\
\n\
RELATED\n\
- issue_comments.issue_number → issues.number (issue)";
        assert_eq!(text, expected);
    }

    #[test]
    fn nothing_known_renders_nothing() {
        assert_eq!(render(&TableContext::default()), "");
        let bare = TableContext {
            name: "x".into(),
            columns: vec![column("a", None)],
            ..TableContext::default()
        };
        assert_eq!(render(&bare), "TABLE x\n\nCOLUMNS\n- a (Integer)");
    }

    #[test]
    fn long_tables_are_capped() {
        let mut ctx = TableContext {
            name: "wide".into(),
            ..TableContext::default()
        };
        for i in 0..(MAX_LISTED_COLUMNS + 5) {
            ctx.columns
                .push(column(&format!("m{i}"), Some(ColumnRole::Measure)));
        }
        for i in 0..(MAX_IGNORED_NAMES + 2) {
            ctx.columns
                .push(column(&format!("x{i}"), Some(ColumnRole::Ignored)));
        }
        let text = render(&ctx);
        assert!(text.contains("(+5 more columns)"));
        assert!(text.contains("(+2 more)"));
        assert_eq!(text.matches("\n- m").count(), MAX_LISTED_COLUMNS);
    }

    #[test]
    fn ai_context_lines() {
        assert_eq!(render_ai_context(&AiContext::Text("  ".into())), None);
        assert_eq!(
            render_ai_context(&AiContext::Structured(AiContextFields {
                instructions: None,
                synonyms: vec![],
                examples: vec!["a".into(), "b".into()],
            })),
            Some("examples: a; b".into())
        );
    }
}
