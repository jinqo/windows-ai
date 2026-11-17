//! Small JSON-Schema subset validator for tool inputs (type, required,
//! properties, enum, items, min/max length). Enough to reject malformed
//! model output before anything runs.

use serde_json::Value;

pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    validate_at(schema, value, "input")
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => true,
    }
}

fn validate_at(schema: &Value, v: &Value, path: &str) -> Result<(), String> {
    if let Some(t) = schema.get("type") {
        let ok = match t {
            Value::String(s) => type_ok(s, v),
            Value::Array(a) => a.iter().filter_map(Value::as_str).any(|s| type_ok(s, v)),
            _ => true,
        };
        if !ok {
            return Err(format!("{path}: expected {t}"));
        }
    }
    if let Some(Value::Array(opts)) = schema.get("enum") {
        if !opts.contains(v) {
            return Err(format!("{path}: must be one of {}", Value::Array(opts.clone())));
        }
    }
    if let Value::String(s) = v {
        let n = s.chars().count() as u64;
        if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
            if n < min {
                return Err(format!("{path}: shorter than {min}"));
            }
        }
        if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
            if n > max {
                return Err(format!("{path}: longer than {max}"));
            }
        }
    }
    if let Some(num) = v.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
            if num < min {
                return Err(format!("{path}: below minimum {min}"));
            }
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
            if num > max {
                return Err(format!("{path}: above maximum {max}"));
            }
        }
    }
    if let Value::Object(obj) = v {
        if let Some(Value::Array(req)) = schema.get("required") {
            for r in req.iter().filter_map(Value::as_str) {
                if !obj.contains_key(r) {
                    return Err(format!("{path}: missing required field '{r}'"));
                }
            }
        }
        if let Some(Value::Object(props)) = schema.get("properties") {
            for (k, sub) in props {
                if let Some(child) = obj.get(k) {
                    validate_at(sub, child, &format!("{path}.{k}"))?;
                }
            }
            if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                if let Some(extra) = obj.keys().find(|k| !props.contains_key(*k)) {
                    return Err(format!("{path}: unexpected field '{extra}'"));
                }
            }
        }
    }
    if let (Value::Array(items), Some(item_schema)) = (v, schema.get("items")) {
        if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
            if items.len() as u64 > max {
                return Err(format!("{path}: more than {max} items"));
            }
        }
        for (i, it) in items.iter().enumerate() {
            validate_at(item_schema, it, &format!("{path}[{i}]"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_common_shapes() {
        let s = json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "minLength": 1},
                "n": {"type": "integer", "minimum": 1, "maximum": 10},
                "mode": {"enum": ["a", "b"]},
                "list": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["path"],
            "additionalProperties": false
        });
        assert!(validate(&s, &json!({"path": "x", "n": 3, "mode": "a", "list": ["q"]})).is_ok());
        assert!(validate(&s, &json!({})).unwrap_err().contains("missing required"));
        assert!(validate(&s, &json!({"path": ""})).is_err());
        assert!(validate(&s, &json!({"path": "x", "n": 11})).is_err());
        assert!(validate(&s, &json!({"path": "x", "mode": "c"})).is_err());
        assert!(validate(&s, &json!({"path": "x", "list": [1]})).is_err());
        assert!(validate(&s, &json!({"path": "x", "zzz": 1})).unwrap_err().contains("unexpected"));
    }
}
