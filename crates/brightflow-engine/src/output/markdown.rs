//! Markdown rendering of an analysis tree, for terminals and pasting into
//! issues or docs.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::analysis::tree::{AnalysisCategory, AnalysisTree, NodeId};

pub fn write_markdown(path: &Path, tree: &AnalysisTree, title: Option<&str>) -> Result<()> {
    let file = File::create(path)
        .with_context(|| format!("Failed to create markdown file {}", path.display()))?;
    let mut writer = BufWriter::new(file);

    // Header
    let title = title.unwrap_or("Analysis Insights");
    writeln!(writer, "# {title}")?;
    writeln!(writer)?;

    if tree.roots.is_empty() {
        writeln!(writer, "No significant findings.")?;
        return Ok(());
    }

    // Group root findings by category
    let roots_by_category = group_roots_by_category(tree);

    // Key Findings section (flat list)
    writeln!(writer, "## Key Findings")?;
    writeln!(writer)?;

    let mut finding_num = 1;
    for category in AnalysisCategory::all() {
        if let Some(root_ids) = roots_by_category.get(category) {
            for root_id in root_ids {
                let node = &tree.nodes[root_id.0];
                writeln!(writer, "{}. {}", finding_num, node.summary)?;
                finding_num += 1;
            }
        }
    }
    writeln!(writer)?;

    // Detailed Analysis grouped by category
    writeln!(writer, "## Detailed Analysis")?;
    writeln!(writer)?;

    for category in AnalysisCategory::all() {
        if let Some(root_ids) = roots_by_category.get(category) {
            if !root_ids.is_empty() {
                writeln!(writer, "### {}", category.display_name())?;
                writeln!(writer, "*{}*", category.description())?;
                writeln!(writer)?;

                for root_id in root_ids {
                    write_node_tree(&mut writer, tree, *root_id, 0)?;
                    writeln!(writer)?;
                }
            }
        }
    }

    Ok(())
}

/// Group root nodes by their analysis category
fn group_roots_by_category(
    tree: &AnalysisTree,
) -> std::collections::HashMap<AnalysisCategory, Vec<NodeId>> {
    use std::collections::HashMap;
    let mut by_category: HashMap<AnalysisCategory, Vec<NodeId>> = HashMap::new();

    for root_id in &tree.roots {
        let node = &tree.nodes[root_id.0];
        let category = node.analysis.category();
        by_category.entry(category).or_default().push(*root_id);
    }

    by_category
}

fn write_node_tree<W: Write>(
    writer: &mut W,
    tree: &AnalysisTree,
    node_id: NodeId,
    depth: usize,
) -> Result<()> {
    let node = &tree.nodes[node_id.0];
    let indent = "  ".repeat(depth);
    // Use #### for top-level findings (since ### is used for category headers)
    let bullet = if depth == 0 { "####" } else { "-" };

    // Write the node
    if depth == 0 {
        writeln!(writer, "{} {}", bullet, node.summary)?;
        writeln!(writer)?;
        writeln!(writer, "> {}", node.description)?;
        writeln!(writer)?;
        writeln!(writer, "`{}`", node.tech_summary)?;
        writeln!(writer)?;
    } else {
        writeln!(writer, "{}{} {}", indent, bullet, node.summary)?;
        writeln!(writer, "{}  `{}`", indent, node.tech_summary)?;
    }

    // Write children with explanation prefix, sorted by significance (descending)
    if !node.children.is_empty() {
        if depth == 0 {
            writeln!(writer, "**Why?**")?;
            writeln!(writer)?;
        }

        // Sort children by significance descending
        let sorted_children = tree.sorted_by_significance_desc(&node.children);

        for child_id in &sorted_children {
            write_node_tree(writer, tree, *child_id, depth + 1)?;
        }
    }

    Ok(())
}
