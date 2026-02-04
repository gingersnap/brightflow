use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::analysis::tree::{AnalysisCategory, AnalysisTree, AnalysisType, NodeId};

pub fn write_html(path: &Path, tree: &AnalysisTree, title: Option<&str>) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("Failed to create HTML file {:?}", path))?;
    let mut writer = BufWriter::new(file);

    let title = title.unwrap_or("Analysis Insights");

    // Write HTML header with minimal styling
    writeln!(
        writer,
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            line-height: 1.6;
            color: #333;
        }}
        h1 {{
            border-bottom: 2px solid #eee;
            padding-bottom: 10px;
        }}
        details {{
            margin: 10px 0;
            padding: 10px;
            background: #f9f9f9;
            border-radius: 5px;
            border: 1px solid #eee;
        }}
        details[open] {{
            background: #fff;
        }}
        summary {{
            cursor: pointer;
            font-weight: 500;
            padding: 5px;
        }}
        summary:hover {{
            background: #f0f0f0;
            border-radius: 3px;
        }}
        .description {{
            color: #666;
            font-size: 0.9em;
            margin: 10px 0;
            padding: 10px;
            background: #f5f5f5;
            border-left: 3px solid #ddd;
        }}
        .children {{
            margin-left: 20px;
            padding-left: 15px;
            border-left: 2px solid #eee;
        }}
        .child-item {{
            margin: 8px 0;
            padding: 5px 0;
        }}
        .up {{ color: #22863a; }}
        .down {{ color: #cb2431; }}
        .tech {{
            font-family: monospace;
            font-size: 0.85em;
            color: #666;
            background: #f0f0f0;
            padding: 2px 6px;
            border-radius: 3px;
            margin-top: 5px;
            display: inline-block;
        }}
    </style>
</head>
<body>
    <h1>{}</h1>"#,
        title, title
    )?;

    if tree.roots.is_empty() {
        writeln!(writer, "    <p>No significant findings.</p>")?;
    } else {
        // Group root findings by category
        let roots_by_category = group_roots_by_category(tree);

        // Write each category section
        for category in AnalysisCategory::all() {
            if let Some(root_ids) = roots_by_category.get(category) {
                if !root_ids.is_empty() {
                    writeln!(writer, "    <section>")?;
                    writeln!(writer, "        <h2>{}</h2>", category.display_name())?;
                    writeln!(writer, "        <p class=\"description\" style=\"margin-top: -10px; border-left: none; background: none;\"><em>{}</em></p>", category.description())?;

                    for root_id in root_ids {
                        write_finding(&mut writer, tree, *root_id)?;
                    }

                    writeln!(writer, "    </section>")?;
                }
            }
        }
    }

    writeln!(writer, "</body>\n</html>")?;

    Ok(())
}

fn write_finding<W: Write>(writer: &mut W, tree: &AnalysisTree, node_id: NodeId) -> Result<()> {
    let node = &tree.nodes[node_id.0];

    // Determine if this is an increase or decrease for styling
    let direction_class = match &node.analysis {
        AnalysisType::PeriodComparison { change_percent, .. }
        | AnalysisType::PeriodAnomaly { change_percent, .. } => {
            if *change_percent > 0.0 {
                "up"
            } else {
                "down"
            }
        },
        AnalysisType::Anomaly { z_score, .. } => {
            if *z_score > 0.0 {
                "up"
            } else {
                "down"
            }
        },
        AnalysisType::Trend { direction, .. } => match direction {
            crate::analysis::tree::TrendDirection::Increasing => "up",
            crate::analysis::tree::TrendDirection::Decreasing => "down",
        },
        _ => "",
    };

    writeln!(writer, "    <details>")?;
    writeln!(
        writer,
        "        <summary class=\"{}\">{}</summary>",
        direction_class,
        html_escape(&node.summary)
    )?;
    writeln!(
        writer,
        "        <div class=\"description\">{}</div>",
        html_escape(&node.description)
    )?;
    writeln!(
        writer,
        "        <span class=\"tech\">{}</span>",
        html_escape(&node.tech_summary)
    )?;

    if !node.children.is_empty() {
        writeln!(writer, "        <div class=\"children\">")?;
        writeln!(writer, "            <strong>Why?</strong>")?;

        // Sort children by significance descending
        let mut sorted_children = node.children.clone();
        sorted_children.sort_by(|a, b| {
            let a_sig = tree.nodes[a.0].significance;
            let b_sig = tree.nodes[b.0].significance;
            b_sig
                .partial_cmp(&a_sig)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for child_id in &sorted_children {
            let child = &tree.nodes[child_id.0];
            let child_class = match &child.analysis {
                AnalysisType::Segment { change_percent, .. } => {
                    if *change_percent > 0.0 {
                        "up"
                    } else {
                        "down"
                    }
                },
                _ => "",
            };
            writeln!(writer, "            <div class=\"child-item {}\">- {} <span class=\"tech\">{}</span></div>",
                child_class, html_escape(&child.summary), html_escape(&child.tech_summary))?;
        }

        writeln!(writer, "        </div>")?;
    }

    writeln!(writer, "    </details>")?;

    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
