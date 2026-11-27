use serde_json::{Value, json};

/// Sum provider responses, including the responses between tool calls.
pub fn summarize(messages: &[Value], start: usize, end: &mut Value) {
    let mut total = json!({});
    let mut turn_cost = Some(0.0);
    let mut session_cost = Some(0.0);
    let mut budget = None;
    let mut answered = false;
    for (i, record) in messages
        .iter()
        .enumerate()
        .filter(|(_, r)| r["kind"] == "assistant")
    {
        let cost = record["estimatedUsd"].as_f64();
        session_cost = session_cost.zip(cost).map(|(a, b)| a + b);
        if i < start {
            continue;
        }
        answered = true;
        turn_cost = turn_cost.zip(cost).map(|(a, b)| a + b);
        budget = record["budgetLimitUsd"].as_f64();
        let usage = &record["usage"];
        for (field, count) in [
            (
                "inputTokens",
                usage["input_tokens"]
                    .as_u64()
                    .or_else(|| usage["prompt_tokens"].as_u64()),
            ),
            (
                "outputTokens",
                usage["output_tokens"]
                    .as_u64()
                    .or_else(|| usage["completion_tokens"].as_u64()),
            ),
            (
                "cacheReadTokens",
                usage["cache_read_input_tokens"]
                    .as_u64()
                    .or_else(|| usage["prompt_tokens_details"]["cached_tokens"].as_u64()),
            ),
            (
                "cacheWriteTokens",
                usage["cache_creation_input_tokens"].as_u64(),
            ),
        ] {
            if let Some(n) = count {
                total[field] = json!(total[field].as_u64().unwrap_or(0).saturating_add(n));
            }
        }
    }
    if !answered {
        return;
    }
    if total.as_object().is_some_and(|o| !o.is_empty()) {
        end["usage"] = total;
    }
    if let Some(cost) = turn_cost {
        end["estimatedUsd"] = json!(cost);
    }
    if let Some(cost) = session_cost {
        end["sessionEstimatedUsd"] = json!(cost);
        if let Some(limit) = budget {
            end["budgetWarning"] = json!(cost >= limit);
        }
    }
}
#[test]
fn counts_each_response_in_the_turn_and_warns_on_session_cost() {
    let record = json!({"kind":"assistant","usage":{"input_tokens":20,"output_tokens":3,"cache_read_input_tokens":7},"budgetLimitUsd":1.1,"estimatedUsd":0.5});
    let mut end = json!({});
    summarize(&[record.clone(), record.clone(), record], 1, &mut end);
    assert_eq!(
        end,
        json!({"usage":{"inputTokens":40,"outputTokens":6,"cacheReadTokens":14},"estimatedUsd":1.0,"sessionEstimatedUsd":1.5,"budgetWarning":true})
    );
    let mut end = json!({});
    summarize(
        &[
            json!({"kind":"assistant","usage":{"prompt_tokens":4,"prompt_tokens_details":{"cached_tokens":2}}}),
        ],
        0,
        &mut end,
    );
    assert_eq!(end, json!({"usage":{"inputTokens":4,"cacheReadTokens":2}}));
}
