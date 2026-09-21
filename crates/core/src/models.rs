use crate::contracts::{Message, Provider, ToolCall, ToolDefinition};
use anyhow::{ensure, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub type Notify = Arc<dyn Fn(String) + Send + Sync>;
pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(180))
        .build()?)
}
pub fn validate_provider(p: &Provider) -> Result<()> {
    let u = url::Url::parse(&p.base_url)?;
    ensure!(
        u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none(),
        "URL deve conter apenas origem e caminho"
    );
    let local = matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    ensure!(
        u.scheme() == "https" || (u.scheme() == "http" && local),
        "Use HTTPS ou HTTP em loopback"
    );
    ensure!(
        !p.local_only || local,
        "Provedor local precisa usar loopback"
    );
    ensure!(
        [
            "ollama",
            "openai-compatible",
            "lmstudio",
            "openai",
            "anthropic",
            "gemini"
        ]
        .contains(&p.kind.as_str()),
        "Tipo de provedor desconhecido"
    );
    Ok(())
}
pub fn secret(p: &Provider) -> Result<Option<String>> {
    p.secret_ref
        .as_ref()
        .map(|r| Ok(keyring::Entry::new("app.forja.provider", r)?.get_password()?))
        .transpose()
}
pub fn save_secret(id: &str, key: &str) -> Result<()> {
    keyring::Entry::new("app.forja.provider", id)?.set_password(key)?;
    Ok(())
}
pub fn delete_secret(reference: &str) -> Result<()> {
    match keyring::Entry::new("app.forja.provider", reference)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
/// Exercises tool calling without executing any returned tool or touching a workspace.
pub async fn probe_tools(p: &Provider, model: &str, cancel: CancellationToken) -> Result<Value> {
    let nonce = crate::id();
    let tool = ToolDefinition {
        name: "forja.probe".into(),
        description: "Confirmar a capacidade de chamar ferramentas. Não possui efeitos.".into(),
        input_schema: json!({"type":"object","properties":{"value":{"type":"string","const":nonce}},"required":["value"],"additionalProperties":false}),
        risk: "read".into(),
    };
    let message = Message {
        role: "user".into(),
        content: format!(
            "Chame forja.probe uma vez com value igual a {nonce}. Não responda em texto."
        ),
        tool_calls: vec![],
        tool_call_id: None,
        provider_state: Value::Null,
    };
    let answer = generate(
        p,
        model,
        &[message],
        &[tool.clone()],
        cancel,
        Arc::new(|_| {}),
    )
    .await?;
    let valid = answer.calls.len() == 1
        && answer.calls[0].name == tool.name
        && jsonschema::validator_for(&tool.input_schema)?.is_valid(&answer.calls[0].arguments);
    Ok(
        json!({"status":if valid{"verified"}else{"not_observed"},"executed_tools":0,"usage":answer.usage}),
    )
}
pub async fn list_models(p: &Provider) -> Result<Value> {
    crate::model_catalog::list(p).await
}
pub async fn count_tokens(
    p: &Provider,
    model: &str,
    history: &[Message],
    tools: &[ToolDefinition],
) -> Result<Option<u64>> {
    if !["anthropic", "gemini"].contains(&p.kind.as_str()) {
        return Ok(None);
    }
    let key = secret(p)?.ok_or_else(|| anyhow::anyhow!("Configure uma chave de API no cofre"))?;
    let base = p.base_url.trim_end_matches('/');
    let mut body = crate::native_models::request_body(&p.kind, model, history, tools);
    if let Some(object) = body.as_object_mut() {
        object.remove("stream");
        object.remove("max_tokens");
    }
    let (url, request) = if p.kind == "anthropic" {
        let request = client()?
            .post(format!("{base}/messages/count_tokens"))
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01")
            .json(&body);
        (format!("{base}/messages/count_tokens"), request)
    } else {
        let url = format!("{base}/models/{model}:countTokens");
        let request = client()?
            .post(&url)
            .header("x-goog-api-key", &key)
            .json(&body);
        (url, request)
    };
    let response = request.send().await?;
    ensure!(
        response.status().is_success(),
        "Contagem de tokens respondeu HTTP {} em {}",
        response.status(),
        url
    );
    let value: Value = response.json().await?;
    Ok(value["input_tokens"]
        .as_u64()
        .or_else(|| value["totalTokens"].as_u64())
        .or_else(|| value["total_tokens"].as_u64()))
}
pub struct Answer {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub usage: Value,
    pub provider_state: Value,
}
fn wire_name(name: &str) -> String {
    name.replace('.', "__")
}
fn images(message: &Message) -> Vec<(String, String)> {
    message
        .provider_state
        .get("forja_images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                item["media_type"].as_str()?.to_owned(),
                item["data"].as_str()?.to_owned(),
            ))
        })
        .collect()
}
fn messages(history: &[Message], ollama: bool) -> Vec<Value> {
    history.iter().map(|m|{let image_parts=images(m);let mut v=json!({"role":m.role,"content":m.content});if !image_parts.is_empty(){if ollama{v["images"]=json!(image_parts.into_iter().map(|(_,data)|data).collect::<Vec<_>>())}else{let mut content=vec![json!({"type":"text","text":m.content})];content.extend(image_parts.into_iter().map(|(media,data)|json!({"type":"image_url","image_url":{"url":format!("data:{media};base64,{data}")}})));v["content"]=json!(content)}}if !m.tool_calls.is_empty(){v["tool_calls"]=json!(m.tool_calls.iter().map(|c|json!({"id":c.id,"type":"function","function":{"name":wire_name(&c.name),"arguments":if ollama{c.arguments.clone()}else{json!(c.arguments.to_string())}}})).collect::<Vec<_>>());}if let Some(id)=&m.tool_call_id{v["tool_call_id"]=json!(id);if ollama{v["tool_name"]=json!(id);}}v}).collect()
}
pub async fn generate(
    p: &Provider,
    model: &str,
    history: &[Message],
    tools: &[ToolDefinition],
    cancel: CancellationToken,
    notify: Notify,
) -> Result<Answer> {
    generate_with_reasoning(p, model, history, tools, None, cancel, notify).await
}
pub async fn generate_with_reasoning(
    p: &Provider,
    model: &str,
    history: &[Message],
    tools: &[ToolDefinition],
    reasoning: Option<&str>,
    cancel: CancellationToken,
    notify: Notify,
) -> Result<Answer> {
    validate_provider(p)?;
    ensure!(
        !p.local_only || !model.to_lowercase().contains("cloud"),
        "Modelo de nuvem incompatível com perfil local"
    );
    if ["openai", "anthropic", "gemini"].contains(&p.kind.as_str()) {
        return crate::native_models::generate_with_reasoning(
            p, model, history, tools, reasoning, cancel, notify,
        )
        .await;
    }
    let ollama = p.kind == "ollama";
    ensure!(
        ["ollama", "openai-compatible", "lmstudio"].contains(&p.kind.as_str()),
        "Adaptador nativo ainda não disponível nesta compilação"
    );
    let mut body = json!({"model":model,"messages":messages(history,ollama),"stream":true,"tools":tools.iter().map(|t|json!({"type":"function","function":{"name":wire_name(&t.name),"description":t.description,"parameters":t.input_schema}})).collect::<Vec<_>>()});
    if let Some(level) = reasoning {
        if ollama {
            body["think"] = json!(match level {
                "none" => Value::Bool(false),
                "low" | "medium" | "high" => Value::String(level.into()),
                _ => Value::Bool(true),
            });
        } else {
            body["reasoning_effort"] = json!(level);
        }
    }
    let url = format!(
        "{}{}",
        p.base_url.trim_end_matches('/'),
        if ollama {
            "/api/chat"
        } else {
            "/chat/completions"
        }
    );
    let mut req = client()?.post(url).json(&body);
    if let Some(key) = secret(p)? {
        req = req.bearer_auth(key);
    }
    let response =
        tokio::select! {r=req.send()=>r?,_=cancel.cancelled()=>anyhow::bail!("Cancelado")};
    ensure!(
        response.status().is_success(),
        "Modelo respondeu HTTP {}",
        response.status()
    );
    let mut stream = response.bytes_stream();
    let mut decoder = crate::model_stream::Decoder::new(ollama);
    let mut collector = crate::chat_stream::Collector::new(ollama);
    loop {
        let next =
            tokio::select! {n=stream.next()=>n,_=cancel.cancelled()=>anyhow::bail!("Cancelado")};
        let ended = next.is_none();
        let items = match next {
            Some(chunk) => decoder.push(&chunk?)?,
            None => decoder.finish()?,
        };
        for item in items {
            let text = collector.feed(item)?;
            if !text.is_empty() {
                notify(text);
            }
        }
        if ended {
            break;
        }
    }
    collector.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_context_is_encoded_for_compatible_and_ollama_protocols() {
        let history = vec![Message {
            role: "user".into(),
            content: "Revise a tela".into(),
            tool_calls: vec![],
            tool_call_id: None,
            provider_state: json!({"forja_images":[{"media_type":"image/png","data":"cG5n"}]}),
        }];
        let compatible = messages(&history, false);
        assert_eq!(compatible[0]["content"][1]["type"], "image_url");
        let ollama = messages(&history, true);
        assert_eq!(ollama[0]["images"][0], "cG5n");
    }
    #[test]
    fn endpoints_are_explicit() {
        let mut p = Provider {
            id: "p".into(),
            name: "p".into(),
            kind: "ollama".into(),
            base_url: "http://example.org".into(),
            secret_ref: None,
            local_only: false,
        };
        assert!(validate_provider(&p).is_err());
        p.base_url = "http://127.0.0.1:11434".into();
        assert!(validate_provider(&p).is_ok());
    }
}
