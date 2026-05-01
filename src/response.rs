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

    Err(anyhow!(
        "response output does not contain image_generation_call"
    ))
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
}
