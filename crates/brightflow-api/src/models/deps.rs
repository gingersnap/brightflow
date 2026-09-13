//! Which models to rebuild after a table changed, and in what order.
//!
//! Edges run from a model's input table to its output table. A write to a
//! table rebuilds the models fed by it, then the models fed by those, depth
//! first, each once. The create path refuses a recipe whose input is the
//! model's own output or anything downstream of it, so the walk here only
//! guards against a cycle rather than expecting one.

use brightflow_store::ModelRow;

/// Model ids to rebuild after `changed_table_id` was written, in an order
/// where every model comes after the model that feeds it. Ids the walk has
/// already scheduled are not scheduled again.
pub fn dependents_in_order(models: &[ModelRow], changed_table_id: &str) -> Vec<String> {
    let mut order = Vec::new();
    let mut visited = Vec::new();
    walk(models, changed_table_id, &mut order, &mut visited);
    order
}

fn walk(models: &[ModelRow], table_id: &str, order: &mut Vec<String>, visited: &mut Vec<String>) {
    if visited.iter().any(|t| t == table_id) {
        return;
    }
    visited.push(table_id.to_string());
    for model in models
        .iter()
        .filter(|m| m.input_table_id.as_deref() == Some(table_id))
    {
        if order.iter().any(|id| id == &model.id) {
            continue;
        }
        order.push(model.id.clone());
        walk(models, &model.output_table_id, order, visited);
    }
}

/// Whether making `output_table_id` a model over `input_table_id` would
/// close a loop: the input is the output itself, or something the output
/// already feeds, directly or through other models.
pub fn would_cycle(models: &[ModelRow], input_table_id: &str, output_table_id: &str) -> bool {
    if input_table_id == output_table_id {
        return true;
    }
    let downstream = dependents_in_order(models, output_table_id);
    models
        .iter()
        .filter(|m| downstream.contains(&m.id))
        .any(|m| m.output_table_id == input_table_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, input: &str, output: &str) -> ModelRow {
        ModelRow {
            id: id.to_string(),
            output_table_id: output.to_string(),
            input_table_id: Some(input.to_string()),
            current_version: 1,
            created_by: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn a_chain_rebuilds_in_feeding_order_and_a_fan_out_rebuilds_both() {
        let models = vec![
            model("m_b", "t_a", "t_b"),
            model("m_c", "t_b", "t_c"),
            model("m_d", "t_a", "t_d"),
            model("m_other", "t_x", "t_y"),
        ];
        assert_eq!(
            dependents_in_order(&models, "t_a"),
            vec!["m_b", "m_c", "m_d"]
        );
        assert_eq!(dependents_in_order(&models, "t_b"), vec!["m_c"]);
        assert!(dependents_in_order(&models, "t_c").is_empty());
    }

    #[test]
    fn a_cycle_in_the_data_terminates_and_is_refused_at_create() {
        // Not creatable through the bus, but the walk must not spin on it.
        let cyclic = vec![model("m1", "t_a", "t_b"), model("m2", "t_b", "t_a")];
        assert_eq!(dependents_in_order(&cyclic, "t_a"), vec!["m1", "m2"]);

        let models = vec![model("m_b", "t_a", "t_b"), model("m_c", "t_b", "t_c")];
        assert!(
            would_cycle(&models, "t_c", "t_a"),
            "t_c is downstream of t_a"
        );
        assert!(would_cycle(&models, "t_z", "t_z"), "self");
        assert!(
            !would_cycle(&models, "t_a", "t_z"),
            "a fresh output over the root"
        );
        assert!(
            !would_cycle(&models, "t_c", "t_z"),
            "a fresh output over a leaf"
        );
    }
}
