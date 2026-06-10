//! JSON ↔ TOML bridging for the configuration editor's "TOML Review" tab.
//!
//! The control API speaks JSON (a serialized `MeshConfig`); the file format
//! users know is TOML. With the `embedded` feature we validate through the
//! real `mesh-llm-config` parser, so what the editor accepts is exactly what
//! the daemon accepts.

use serde_json::Value;

/// Render the config JSON snapshot as TOML text.
pub fn json_to_toml(config: &Value) -> Result<String, String> {
    let toml_value = json_value_to_toml(config).ok_or("config is not a TOML-compatible object")?;
    toml::to_string_pretty(&toml_value).map_err(|e| format!("TOML render error: {e}"))
}

/// Parse TOML text back into the JSON shape `apply-config` expects.
/// With the `embedded` feature the text is validated through the actual
/// mesh-llm config parser (schema + semantic checks); otherwise it is parsed
/// as plain TOML.
pub fn toml_to_json(raw: &str) -> Result<Value, String> {
    #[cfg(feature = "embedded")]
    {
        let config = mesh_llm_config::parse_config_toml(raw)
            .map_err(|e| format!("invalid mesh config: {e:#}"))?;
        mesh_llm_config::validate_config(&config).map_err(|e| format!("invalid config: {e:#}"))?;
        serde_json::to_value(&config).map_err(|e| format!("serialize config: {e}"))
    }
    #[cfg(not(feature = "embedded"))]
    {
        let value: toml::Value =
            toml::from_str(raw).map_err(|e| format!("TOML parse error: {e}"))?;
        Ok(toml_value_to_json(value))
    }
}

fn json_value_to_toml(value: &Value) -> Option<toml::Value> {
    Some(match value {
        // TOML has no null: omit the key entirely (handled in Object arm).
        Value::Null => return None,
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else {
                toml::Value::Float(n.as_f64()?)
            }
        }
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Array(items) => {
            toml::Value::Array(items.iter().filter_map(json_value_to_toml).collect())
        }
        Value::Object(map) => {
            let mut table = toml::map::Map::new();
            for (key, item) in map {
                if let Some(converted) = json_value_to_toml(item) {
                    table.insert(key.clone(), converted);
                }
            }
            toml::Value::Table(table)
        }
    })
}

#[cfg(not(feature = "embedded"))]
fn toml_value_to_json(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => {
            Value::Array(items.into_iter().map(toml_value_to_json).collect())
        }
        toml::Value::Table(table) => Value::Object(
            table
                .into_iter()
                .map(|(k, v)| (k, toml_value_to_json(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_renders_as_toml_dropping_nulls() {
        let config = json!({
            "version": 1,
            "gpu": {"assignment": "auto", "parallel": null},
            "models": [{"model": "Qwen3-4B-Q4_K_M", "ctx_size": 8192}]
        });
        let toml_text = json_to_toml(&config).expect("render");
        assert!(toml_text.contains("version = 1"));
        assert!(toml_text.contains("[[models]]"));
        assert!(toml_text.contains("ctx_size = 8192"));
        assert!(!toml_text.contains("parallel"), "null keys omitted");
    }

    #[test]
    fn toml_round_trips_to_json() {
        let raw = "version = 1\n\n[[models]]\nmodel = \"Qwen3-4B-Q4_K_M\"\nctx_size = 8192\n";
        let value = toml_to_json(raw).expect("parse");
        assert_eq!(value["models"][0]["ctx_size"], json!(8192));
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(toml_to_json("this is { not toml").is_err());
    }
}
