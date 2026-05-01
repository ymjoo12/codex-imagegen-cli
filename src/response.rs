use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use serde_json::Value;

pub struct ImageCall {
    pub id: Option<String>,
    pub revised_prompt: Option<String>,
    pub result: String,
}

pub fn response_id(response: &Value) -> Option<String> {
    response
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub fn extract_first_image(response: &Value) -> Result<ImageCall> {
    let output = response
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("response has no output array"))?;

    for item in output {
        if item.get("type").and_then(Value::as_str) != Some("image_generation_call") {
            continue;
        }
        let result = item
            .get("result")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("image_generation_call has no result"))?;
        if result.trim().is_empty() {
            return Err(anyhow!("image_generation_call result is empty"));
        }
        return Ok(ImageCall {
            id: item.get("id").and_then(Value::as_str).map(str::to_string),
            revised_prompt: item
                .get("revised_prompt")
                .and_then(Value::as_str)
                .map(str::to_string),
            result: result.to_string(),
        });
    }

    let output_types: Vec<&str> = output
        .iter()
        .filter_map(|item| item.get("type").and_then(Value::as_str))
        .collect();
    Err(anyhow!(
        "response output does not contain image_generation_call; output types: {}",
        output_types.join(", ")
    ))
}

pub fn parse_sse_response(raw: &str) -> Result<Value> {
    let mut image_items = Vec::new();
    let mut event_types = Vec::new();
    let normalized = raw.replace("\r\n", "\n");

    for payload in sse_data_payloads(&normalized) {
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }

        let event: Value = serde_json::from_str(payload)
            .with_context(|| format!("invalid SSE JSON payload: {payload}"))?;
        if let Some(event_type) = event.get("type").and_then(Value::as_str) {
            event_types.push(event_type.to_string());
        }

        if event.get("type").and_then(Value::as_str) == Some("response.completed")
            && let Some(response) = event.get("response")
        {
            return Ok(response.clone());
        }

        if event.get("type").and_then(Value::as_str) == Some("response.output_item.done")
            && let Some(item) = event.get("item")
            && item.get("type").and_then(Value::as_str) == Some("image_generation_call")
        {
            image_items.push(item.clone());
        }
    }

    if !image_items.is_empty() {
        return Ok(serde_json::json!({ "output": image_items }));
    }

    Err(anyhow!(
        "stream did not include response.completed or image_generation_call output item; event types: {}",
        event_types.join(", ")
    ))
}

fn sse_data_payloads(raw: &str) -> Vec<String> {
    raw.split("\n\n")
        .filter_map(|block| {
            let data_lines: Vec<&str> = block
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect();
            if data_lines.is_empty() {
                None
            } else {
                Some(data_lines.join("\n"))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_image_generation_result() {
        let response = serde_json::json!({
            "id": "resp_1",
            "output": [
                {"type": "message", "content": []},
                {
                    "type": "image_generation_call",
                    "id": "ig_1",
                    "revised_prompt": "draw a test image",
                    "result": "aGVsbG8="
                }
            ]
        });

        let image = extract_first_image(&response).expect("extract image");
        assert_eq!(image.id.as_deref(), Some("ig_1"));
        assert_eq!(image.revised_prompt.as_deref(), Some("draw a test image"));
        assert_eq!(image.result, "aGVsbG8=");
        assert_eq!(response_id(&response).as_deref(), Some("resp_1"));
    }

    #[test]
    fn parses_completed_sse_response() {
        let raw = concat!(
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"image_generation_call\",\"result\":\"aGVsbG8=\"}]}}\n\n",
            "data: [DONE]\n\n"
        );

        let response = parse_sse_response(raw).expect("parse sse");
        let image = extract_first_image(&response).expect("extract image");

        assert_eq!(response_id(&response).as_deref(), Some("resp_1"));
        assert_eq!(image.result, "aGVsbG8=");
    }

    #[test]
    fn parses_output_item_done_sse_response() {
        let raw = concat!(
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"image_generation_call\",\"id\":\"ig_1\",\"result\":\"aGVsbG8=\"}}\n\n",
            "data: [DONE]\n\n"
        );

        let response = parse_sse_response(raw).expect("parse sse");
        let image = extract_first_image(&response).expect("extract image");

        assert_eq!(image.id.as_deref(), Some("ig_1"));
        assert_eq!(image.result, "aGVsbG8=");
    }
}
