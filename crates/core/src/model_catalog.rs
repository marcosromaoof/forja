use crate::{contracts::Provider, models};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

fn endpoint(p: &Provider, operation: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(p.base_url.trim())?;
    let wanted = operation.trim_start_matches('/');
    let current_path = url.path().trim_end_matches('/').to_owned();
    let current = if wanted.starts_with("api/")
        && (current_path.ends_with("/api/tags") || current_path.ends_with("/api/show"))
    {
        current_path
            .rsplit_once("/api/")
            .map(|(base, _)| base)
            .unwrap_or(&current_path)
    } else {
        &current_path
    };
    if current.rsplit('/').next() != Some(wanted.rsplit('/').next().unwrap_or(wanted)) {
        let next = if current.is_empty() || current == "/" {
            format!("/{wanted}")
        } else {
            format!("{current}/{wanted}")
        };
        url.set_path(&next);
    } else {
        url.set_path(if current.is_empty() { "/" } else { current });
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn request(
    p: &Provider,
    key: Option<&str>,
    cursor: Option<&str>,
) -> Result<reqwest::RequestBuilder> {
    let operation = if p.kind == "ollama" {
        "api/tags"
    } else {
        "models"
    };
    let mut req = models::client()?.get(endpoint(p, operation)?);
    if p.kind == "anthropic" {
        req = req
            .header("anthropic-version", "2023-06-01")
            .query(&[("limit", "100")]);
        if let Some(key) = key {
            req = req.header("x-api-key", key);
        }
        if let Some(cursor) = cursor {
            req = req.query(&[("after_id", cursor)]);
        }
    } else if p.kind == "gemini" {
        req = req.query(&[("pageSize", "1000")]);
        if let Some(key) = key {
            req = req.header("x-goog-api-key", key);
        }
        if let Some(cursor) = cursor {
            req = req.query(&[("pageToken", cursor)]);
        }
    } else if let Some(key) = key {
        req = req.bearer_auth(key);
    }
    Ok(req)
}
fn normalize(p: &Provider, row: &Value) -> Option<Value> {
    if p.kind == "gemini"
        && !row["supportedGenerationMethods"]
            .as_array()?
            .iter()
            .any(|m| m == "generateContent")
    {
        return None;
    }
    let raw_id = row["id"].as_str().or_else(|| row["name"].as_str())?;
    let id = if p.kind == "gemini" {
        raw_id.strip_prefix("models/").unwrap_or(raw_id)
    } else {
        raw_id
    };
    if id.is_empty() || id.len() > 256 {
        return None;
    }
    let host = url::Url::parse(&p.base_url).ok()?;
    let loopback = matches!(host.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    let remote = id.to_lowercase().contains("cloud")
        || row["remote_host"].as_str().is_some_and(|s| !s.is_empty())
        || !loopback
        || (!p.local_only && ["openai", "anthropic", "gemini"].contains(&p.kind.as_str()));
    let processing = if remote {
        "remote"
    } else if p.local_only {
        "local_configured"
    } else {
        "unknown"
    };
    let input_limit = row["inputTokenLimit"]
        .as_u64()
        .or_else(|| row["max_input_tokens"].as_u64());
    let output_limit = row["outputTokenLimit"]
        .as_u64()
        .or_else(|| row["max_tokens"].as_u64());
    let capabilities = json!({
        "text":true,
        "vision":row.pointer("/capabilities/input/image/supported").and_then(Value::as_bool).unwrap_or(false),
        "tools":row.pointer("/capabilities/tool_use/supported").and_then(Value::as_bool).unwrap_or(false),
        "structured_output":row.pointer("/capabilities/structured_outputs/supported").and_then(Value::as_bool).unwrap_or(false),
        "reasoning":row.pointer("/capabilities/thinking/supported").and_then(Value::as_bool).unwrap_or(false)||row.get("thinking").is_some()
    });
    Some(json!({"id":id,"remote":remote,"processing":processing,
        "display_name":row["displayName"].as_str().or_else(||row["display_name"].as_str()).unwrap_or(id),
        "input_token_limit":input_limit,"output_token_limit":output_limit,"capabilities":capabilities,
        "capabilities_source":"provider"}))
}
async fn enrich_ollama(p: &Provider, key: Option<&str>, item: &mut Value) -> Result<()> {
    let mut request = models::client()?
        .post(endpoint(p, "api/show")?)
        .json(&json!({"model":item["id"]}));
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let response = request.send().await?;
    ensure!(
        response.status().is_success(),
        "Ollama /api/show respondeu HTTP {}",
        response.status()
    );
    let value: Value = response.json().await?;
    let context = value["model_info"].as_object().and_then(|info| {
        info.iter()
            .find(|(key, _)| key.ends_with(".context_length"))
            .and_then(|(_, value)| value.as_u64())
    });
    if context.is_some() {
        item["input_token_limit"] = json!(context);
    }
    let capabilities = value["capabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let has = |name: &str| {
        capabilities
            .iter()
            .any(|value| value.as_str() == Some(name))
    };
    item["capabilities"] = json!({"text":true,"vision":has("vision"),"tools":has("tools"),"structured_output":false,"reasoning":has("thinking")});
    item["capabilities_source"] = json!("provider");
    Ok(())
}
pub async fn list(p: &Provider) -> Result<Value> {
    models::validate_provider(p)?;
    let key = models::secret(p)?;
    let mut cursor: Option<String> = None;
    let mut seen = HashSet::new();
    let mut ids = HashSet::new();
    let mut all = Vec::new();
    for _ in 0..20 {
        let mut response = request(p, key.as_deref(), cursor.as_deref())?
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "Provedor respondeu HTTP {}",
            response.status()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= 2_000_000,
                "Catálogo do provedor excede limite"
            );
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes)?;
        let rows = v["models"]
            .as_array()
            .or_else(|| v["data"].as_array())
            .ok_or_else(|| anyhow::anyhow!("Formato de catálogo inválido"))?;
        for row in rows {
            if let Some(item) = normalize(p, row) {
                if ids.insert(item["id"].as_str().unwrap().to_owned()) {
                    all.push(item);
                }
            }
        }
        ensure!(all.len() <= 2000, "Limite de 2000 modelos excedido");
        cursor = match p.kind.as_str() {
            "gemini" => v["nextPageToken"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            "anthropic" if v["has_more"] == true => Some(
                v["last_id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("Cursor Anthropic ausente"))?
                    .into(),
            ),
            _ => None,
        };
        let Some(next) = cursor.as_ref() else {
            if p.kind == "ollama" {
                for item in all.iter_mut().take(200) {
                    enrich_ollama(p, key.as_deref(), item).await?;
                }
            }
            return Ok(json!(all));
        };
        ensure!(
            next.len() <= 4096 && seen.insert(next.clone()),
            "Paginação do provedor repetiu cursor"
        );
    }
    anyhow::bail!("Paginação do provedor excedeu limite")
}
#[cfg(test)]
mod tests {
    use super::*;
    fn provider(kind: &str) -> Provider {
        Provider {
            id: "fixture".into(),
            name: "Fixture".into(),
            kind: kind.into(),
            base_url: "http://127.0.0.1:1/v1".into(),
            secret_ref: None,
            local_only: false,
        }
    }
    #[test]
    fn native_authentication_does_not_leak_keys_into_urls() {
        for (kind, header) in [
            ("anthropic", "x-api-key"),
            ("gemini", "x-goog-api-key"),
            ("openai", "authorization"),
        ] {
            let r = request(&provider(kind), Some("fixture-only-key"), Some("next-page"))
                .unwrap()
                .build()
                .unwrap();
            assert!(r.headers().contains_key(header));
            assert!(!r.url().as_str().contains("fixture-only-key"));
            if kind != "openai" {
                assert!(!r.headers().contains_key("authorization"));
            }
        }
    }
    #[test]
    fn catalog_endpoint_does_not_duplicate_suffixes() {
        let mut p = provider("openai-compatible");
        p.base_url = "https://api.example.test/v1/models/".into();
        assert_eq!(
            endpoint(&p, "models").unwrap().as_str(),
            "https://api.example.test/v1/models"
        );
        p.kind = "ollama".into();
        p.base_url = "http://127.0.0.1:11434/api/tags".into();
        assert_eq!(
            endpoint(&p, "api/tags").unwrap().as_str(),
            "http://127.0.0.1:11434/api/tags"
        );
        assert_eq!(
            endpoint(&p, "api/show").unwrap().as_str(),
            "http://127.0.0.1:11434/api/show"
        );
    }
    #[test]
    fn gemini_ids_and_capabilities_are_normalized() {
        let p = provider("gemini");
        let v=normalize(&p,&json!({"name":"models/gemini-test","supportedGenerationMethods":["generateContent"],"inputTokenLimit":8192})).unwrap();
        assert_eq!(v["id"], "gemini-test");
        assert_eq!(v["input_token_limit"], 8192);
        assert_eq!(v["processing"], "remote");
        assert!(normalize(
            &p,
            &json!({"name":"models/embed","supportedGenerationMethods":["embedContent"]})
        )
        .is_none());
    }
    #[test]
    fn loopback_does_not_imply_local_inference() {
        let mut p = provider("ollama");
        p.local_only = true;
        assert_eq!(
            normalize(
                &p,
                &json!({"name":"alias","remote_host":"https://remote.test"})
            )
            .unwrap()["processing"],
            "remote"
        );
        p.local_only = false;
        assert_eq!(
            normalize(&p, &json!({"name":"local-name"})).unwrap()["processing"],
            "unknown"
        );
    }
}
