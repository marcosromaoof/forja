use crate::{
    contracts::{Message, Provider, ToolCall, ToolDefinition},
    models::{Answer, Notify},
};
use anyhow::{ensure, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
fn wire(s: &str) -> String {
    s.replace('.', "__")
}
fn tool_name(history: &[Message], id: &str) -> String {
    history
        .iter()
        .flat_map(|m| &m.tool_calls)
        .find(|t| t.id == id)
        .map(|t| wire(&t.name))
        .unwrap_or_else(|| id.into())
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
pub fn request_body(
    kind: &str,
    model: &str,
    history: &[Message],
    tools: &[ToolDefinition],
) -> Value {
    let system = history
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    match kind {
        "openai" => {
            let mut input = Vec::new();
            for m in history.iter().filter(|m| m.role != "system") {
                if m.role == "tool" {
                    input.push(json!({"type":"function_call_output","call_id":m.tool_call_id,"output":m.content}));
                } else if m.role == "assistant" && m.provider_state.is_array() {
                    input.extend(m.provider_state.as_array().unwrap().clone());
                } else {
                    if !m.content.is_empty() || !images(m).is_empty() {
                        let image_parts = images(m);
                        if image_parts.is_empty() {
                            input.push(json!({"role":m.role,"content":m.content}));
                        } else {
                            let mut content = vec![json!({"type":"input_text","text":m.content})];
                            content.extend(image_parts.into_iter().map(|(media,data)|json!({"type":"input_image","image_url":format!("data:{media};base64,{data}")})));
                            input.push(json!({"role":m.role,"content":content}));
                        }
                    }
                    for c in &m.tool_calls {
                        input.push(json!({"type":"function_call","call_id":c.id,"name":wire(&c.name),"arguments":c.arguments.to_string()}));
                    }
                }
            }
            json!({"model":model,"instructions":system,"input":input,"stream":true,"store":false,"include":["reasoning.encrypted_content"],"tools":tools.iter().map(|t|json!({"type":"function","name":wire(&t.name),"description":t.description,"parameters":t.input_schema,"strict":false})).collect::<Vec<_>>()})
        }
        "anthropic" => {
            let mut messages: Vec<Value> = Vec::new();
            for m in history.iter().filter(|m| m.role != "system") {
                let (role, content) = if m.role == "tool" {
                    (
                        "user",
                        json!([{"type":"tool_result","tool_use_id":m.tool_call_id,"content":m.content}]),
                    )
                } else {
                    let mut parts = Vec::new();
                    if !m.content.is_empty() {
                        parts.push(json!({"type":"text","text":m.content}));
                    }
                    parts.extend(images(m).into_iter().map(|(media,data)|json!({"type":"image","source":{"type":"base64","media_type":media,"data":data}})));
                    for c in &m.tool_calls {
                        parts.push(json!({"type":"tool_use","id":c.id,"name":wire(&c.name),"input":c.arguments}));
                    }
                    (m.role.as_str(), json!(parts))
                };
                if let Some(last) = messages.last_mut().filter(|v| v["role"] == role) {
                    last["content"]
                        .as_array_mut()
                        .unwrap()
                        .extend(content.as_array().unwrap().clone())
                } else {
                    messages.push(json!({"role":role,"content":content}));
                }
            }
            json!({"model":model,"system":system,"messages":messages,"max_tokens":8192,"stream":true,"tools":tools.iter().map(|t|json!({"name":wire(&t.name),"description":t.description,"input_schema":t.input_schema})).collect::<Vec<_>>()})
        }
        "gemini" => {
            let mut contents = Vec::new();
            for m in history.iter().filter(|m| m.role != "system") {
                if m.role == "assistant" && !m.provider_state.is_null() {
                    contents.push(m.provider_state.clone());
                    continue;
                }
                let parts = if m.role == "tool" {
                    json!([{"functionResponse":{"id":m.tool_call_id,"name":tool_name(history,m.tool_call_id.as_deref().unwrap_or("")),"response":{"result":m.content}}}])
                } else {
                    let mut parts = Vec::new();
                    if !m.content.is_empty() {
                        parts.push(json!({"text":m.content}));
                    }
                    parts.extend(
                        images(m).into_iter().map(
                            |(media, data)| json!({"inlineData":{"mimeType":media,"data":data}}),
                        ),
                    );
                    for c in &m.tool_calls {
                        parts.push(json!({"functionCall":{"id":c.id,"name":wire(&c.name),"args":c.arguments}}));
                    }
                    json!(parts)
                };
                contents.push(
                    json!({"role":if m.role=="assistant"{"model"}else{"user"},"parts":parts}),
                );
            }
            let mut b =
                json!({"systemInstruction":{"parts":[{"text":system}]},"contents":contents});
            if !tools.is_empty() {
                b["tools"] = json!([{"functionDeclarations":tools.iter().map(|t|json!({"name":wire(&t.name),"description":t.description,"parameters":t.input_schema})).collect::<Vec<_>>()}]);
            }
            b
        }
        _ => json!({}),
    }
}
fn apply_reasoning(body: &mut Value, kind: &str, level: &str) {
    match kind {
        "openai" => body["reasoning"] = json!({"effort":level}),
        "anthropic" => {
            body["thinking"] = json!({"type":"adaptive"});
            body["output_config"] = json!({"effort":level});
        }
        "gemini" => {
            body["generationConfig"]["thinkingConfig"] =
                json!({"thinkingLevel":level.to_uppercase()})
        }
        _ => {}
    }
}
#[derive(Default)]
pub struct Collector {
    pub text: String,
    pub calls: std::collections::BTreeMap<usize, (String, String, String)>,
    pub usage: Value,
    pub state: Value,
    gemini_parts: Vec<Value>,
    completed: bool,
    stop_reason_valid: bool,
}
impl Collector {
    pub fn feed(&mut self, kind: &str, v: &Value) -> Result<String> {
        ensure!(
            v.get("error").is_none() && v["type"] != "error",
            "Provedor retornou erro no streaming"
        );
        let mut text = String::new();
        match kind {
            "openai" => match v["type"].as_str().unwrap_or("") {
                "response.output_text.delta" => {
                    text = v["delta"].as_str().unwrap_or("").into();
                }
                "response.output_item.added" if v["item"]["type"] == "function_call" => {
                    let c = &v["item"];
                    let i = v["output_index"].as_u64().unwrap_or(0) as usize;
                    self.calls.insert(
                        i,
                        (
                            c["call_id"].as_str().unwrap_or("").into(),
                            c["name"].as_str().unwrap_or("").into(),
                            c["arguments"].as_str().unwrap_or("").into(),
                        ),
                    );
                }
                "response.function_call_arguments.delta" => {
                    let i = v["output_index"].as_u64().unwrap_or(0) as usize;
                    if let Some(c) = self.calls.get_mut(&i) {
                        c.2.push_str(v["delta"].as_str().unwrap_or(""));
                    }
                }
                "response.completed" => {
                    ensure!(
                        v["response"]["status"] == "completed"
                            && v["response"]["output"].is_array(),
                        "Resposta final OpenAI inválida"
                    );
                    self.completed = true;
                    self.calls.clear();
                    for (i, item) in v["response"]["output"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .enumerate()
                    {
                        if item["type"] == "function_call" {
                            self.calls.insert(
                                i,
                                (
                                    item["call_id"].as_str().unwrap_or("").into(),
                                    item["name"].as_str().unwrap_or("").into(),
                                    item["arguments"].as_str().unwrap_or("").into(),
                                ),
                            );
                        }
                    }
                    self.usage = v["response"]["usage"].clone();
                    self.state = v["response"]["output"].clone();
                }
                "response.failed" | "response.incomplete" => {
                    anyhow::bail!("Resposta do provedor incompleta ou falhou")
                }
                _ => {}
            },
            "anthropic" => match v["type"].as_str().unwrap_or("") {
                "message_start" => {
                    self.usage = v["message"]["usage"].clone();
                }
                "content_block_start" if v["content_block"]["type"] == "tool_use" => {
                    let c = &v["content_block"];
                    let i = v["index"].as_u64().unwrap_or(0) as usize;
                    self.calls.insert(
                        i,
                        (
                            c["id"].as_str().unwrap_or("").into(),
                            c["name"].as_str().unwrap_or("").into(),
                            String::new(),
                        ),
                    );
                }
                "content_block_delta" => {
                    let d = &v["delta"];
                    if d["type"] == "text_delta" {
                        text = d["text"].as_str().unwrap_or("").into();
                    } else if d["type"] == "input_json_delta" {
                        let i = v["index"].as_u64().unwrap_or(0) as usize;
                        if let Some(c) = self.calls.get_mut(&i) {
                            c.2.push_str(d["partial_json"].as_str().unwrap_or(""));
                        }
                    }
                }
                "message_stop" => {
                    ensure!(
                        self.stop_reason_valid,
                        "Resposta Anthropic sem motivo de término válido"
                    );
                    self.completed = true;
                }
                "message_delta" => {
                    if let Some(reason) = v["delta"]["stop_reason"].as_str() {
                        ensure!(
                            ["end_turn", "tool_use", "stop_sequence"].contains(&reason),
                            "Resposta Anthropic interrompida: {reason}"
                        );
                        self.stop_reason_valid = true;
                    }
                    if let Some(o) = v["usage"].as_object() {
                        if !self.usage.is_object() {
                            self.usage = json!({})
                        }
                        for (k, val) in o {
                            self.usage[k] = val.clone();
                        }
                    }
                }
                _ => {}
            },
            "gemini" => {
                if let Some(parts) = v["candidates"][0]["content"]["parts"].as_array() {
                    for part in parts {
                        if part["thought"] == true {
                            continue;
                        }
                        if let Some(s) = part["text"].as_str() {
                            text.push_str(s);
                        }
                        if let Some(c) = part.get("functionCall") {
                            let i = self.calls.len();
                            self.calls.insert(
                                i,
                                (
                                    c["id"]
                                        .as_str()
                                        .map(str::to_owned)
                                        .unwrap_or_else(crate::id),
                                    c["name"].as_str().unwrap_or("").into(),
                                    c["args"].to_string(),
                                ),
                            );
                        }
                        self.gemini_parts.push(part.clone());
                    }
                    self.state = json!({"role":"model","parts":self.gemini_parts});
                }
                if v.get("usageMetadata").is_some() {
                    self.usage = v["usageMetadata"].clone();
                }
                if let Some(reason) = v["candidates"][0]["finishReason"].as_str() {
                    ensure!(reason == "STOP", "Resposta Gemini interrompida: {reason}");
                    self.completed = true;
                }
                ensure!(
                    v["promptFeedback"].get("blockReason").is_none(),
                    "Prompt recusado pelo provedor"
                );
            }
            _ => {}
        }
        self.text.push_str(&text);
        ensure!(
            self.text.len() < 2_000_000
                && self.calls.len() <= 16
                && self.calls.values().all(|(id, name, args)| id.len() <= 512
                    && name.len() <= 256
                    && args.len() <= 256_000)
                && self.state.to_string().len() <= 4_000_000,
            "Resposta excedeu limite"
        );
        Ok(text)
    }
    pub fn finish(self) -> Result<Answer> {
        ensure!(self.completed,"Streaming interrompido sem confirmação de término; nenhuma ferramenta desta resposta foi executada");
        let mut ids = std::collections::HashSet::new();
        let calls = self
            .calls
            .into_values()
            .map(|(id, name, args)| {
                ensure!(
                    !id.is_empty() && !name.is_empty() && ids.insert(id.clone()),
                    "Identificação de ferramenta inválida"
                );
                let parsed: Value =
                    serde_json::from_str(if args.is_empty() { "{}" } else { &args })?;
                ensure!(parsed.is_object(), "Argumentos precisam ser um objeto");
                Ok(ToolCall {
                    id,
                    name: name.replace("__", "."),
                    arguments: serde_json::from_str(if args.is_empty() { "{}" } else { &args })?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Answer {
            text: self.text,
            calls,
            usage: self.usage,
            provider_state: self.state,
        })
    }
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
    let key = crate::models::secret(p)?
        .ok_or_else(|| anyhow::anyhow!("Configure uma chave de API no cofre"))?;
    ensure!(
        model.len() < 200 && !model.contains(['?', '#', '/']),
        "Identificador de modelo inválido"
    );
    let base = p.base_url.trim_end_matches('/');
    let url = match p.kind.as_str() {
        "openai" => format!("{base}/responses"),
        "anthropic" => format!("{base}/messages"),
        "gemini" => format!("{base}/models/{model}:streamGenerateContent?alt=sse"),
        _ => anyhow::bail!("Provedor desconhecido"),
    };
    let mut body = request_body(&p.kind, model, history, tools);
    if let Some(level) = reasoning {
        apply_reasoning(&mut body, &p.kind, level);
    }
    let mut req = crate::models::client()?.post(url).json(&body);
    req = match p.kind.as_str() {
        "anthropic" => req
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01"),
        "gemini" => req.header("x-goog-api-key", &key),
        _ => req.bearer_auth(&key),
    };
    let response =
        tokio::select! {r=req.send()=>r?,_=cancel.cancelled()=>anyhow::bail!("Cancelado")};
    ensure!(
        response.status().is_success(),
        "Provedor respondeu HTTP {}",
        response.status()
    );
    let mut stream = response.bytes_stream();
    let mut decoder = crate::model_stream::Decoder::new(false);
    let mut collector = Collector::default();
    loop {
        let next =
            tokio::select! {n=stream.next()=>n,_=cancel.cancelled()=>anyhow::bail!("Cancelado")};
        let ended = next.is_none();
        let items = match next {
            Some(chunk) => decoder.push(&chunk?)?,
            None => decoder.finish()?,
        };
        for item in items {
            if let crate::model_stream::Item::Data(value) = item {
                let text = collector.feed(&p.kind, &value)?;
                if !text.is_empty() {
                    notify(text);
                }
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
    fn image_message() -> Message {
        Message {
            role: "user".into(),
            content: "Revise a tela".into(),
            tool_calls: vec![],
            tool_call_id: None,
            provider_state: json!({"forja_images":[{"media_type":"image/png","data":"cG5n"}]}),
        }
    }
    #[test]
    fn visual_context_is_encoded_for_each_native_protocol() {
        let history = vec![image_message()];
        let openai = request_body("openai", "model", &history, &[]);
        assert_eq!(openai["input"][0]["content"][1]["type"], "input_image");
        let anthropic = request_body("anthropic", "model", &history, &[]);
        assert_eq!(anthropic["messages"][0]["content"][1]["type"], "image");
        let gemini = request_body("gemini", "model", &history, &[]);
        assert_eq!(
            gemini["contents"][0]["parts"][1]["inlineData"]["mimeType"],
            "image/png"
        );
    }
    #[test]
    fn reasoning_is_mapped_only_to_each_native_protocol() {
        for (kind, pointer, expected) in [
            ("openai", "/reasoning/effort", "high"),
            ("anthropic", "/output_config/effort", "high"),
            (
                "gemini",
                "/generationConfig/thinkingConfig/thinkingLevel",
                "HIGH",
            ),
        ] {
            let mut body = json!({});
            apply_reasoning(&mut body, kind, "high");
            assert_eq!(
                body.pointer(pointer).and_then(Value::as_str),
                Some(expected)
            );
        }
        let mut body = json!({});
        apply_reasoning(&mut body, "unknown", "high");
        assert_eq!(body, json!({}));
    }
    #[test]
    fn incomplete_native_tool_calls_are_rejected() {
        for (kind, event) in [
            (
                "openai",
                json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call","name":"fs__list","arguments":"{}"}}),
            ),
            (
                "anthropic",
                json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"fs__list","input":{}}}),
            ),
            (
                "gemini",
                json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"fs__list","args":{}}}]}}]}),
            ),
        ] {
            let mut collector = Collector::default();
            collector.feed(kind, &event).unwrap();
            assert!(
                collector.finish().is_err(),
                "{kind} accepted a stream without its completion event"
            );
        }
        let mut anthropic = Collector::default();
        assert!(anthropic
            .feed("anthropic", &json!({"type":"message_stop"}))
            .is_err());
        let mut gemini = Collector::default();
        assert!(gemini
            .feed(
                "gemini",
                &json!({"candidates":[{"finishReason":"MAX_TOKENS"}]})
            )
            .is_err());
    }
    #[test]
    fn responses_preserves_call_ids() {
        let mut c = Collector::default();
        c.feed("openai",&json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_1","name":"fs__list","arguments":""}})).unwrap();
        c.feed("openai",&json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"path\":\".\"}"})).unwrap();
        c.feed("openai", &json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","call_id":"call_1","name":"fs__list","arguments":"{\"path\":\".\"}"}]}})).unwrap();
        let a = c.finish().unwrap();
        assert_eq!(a.calls[0].id, "call_1");
        assert_eq!(a.calls[0].name, "fs.list");
        assert_eq!(a.calls[0].arguments["path"], ".");
    }
    #[test]
    fn anthropic_json_chunks() {
        let mut c = Collector::default();
        for v in [
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t","name":"git__status"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{}"}}),
        ] {
            c.feed("anthropic", &v).unwrap();
        }
        c.feed(
            "anthropic",
            &json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
        )
        .unwrap();
        c.feed("anthropic", &json!({"type":"message_stop"}))
            .unwrap();
        assert_eq!(c.finish().unwrap().calls[0].name, "git.status");
    }
    #[test]
    fn gemini_preserves_opaque_signature() {
        let mut c = Collector::default();
        c.feed("gemini",&json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"fs__list","args":{"path":"."}},"thoughtSignature":"opaque-token"}]}}]})).unwrap();
        c.feed("gemini", &json!({"candidates":[{"finishReason":"STOP"}]}))
            .unwrap();
        assert_eq!(
            c.finish().unwrap().provider_state["parts"][0]["thoughtSignature"],
            "opaque-token"
        );
    }
}
