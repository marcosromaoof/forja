use crate::{contracts::ToolCall, model_stream::Item, models::Answer};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Collector {
    ollama: bool,
    text: String,
    calls: BTreeMap<usize, (String, String, String)>,
    usage: Value,
    completed: bool,
}
impl Collector {
    pub fn new(ollama: bool) -> Self {
        Self {
            ollama,
            text: String::new(),
            calls: BTreeMap::new(),
            usage: json!({}),
            completed: false,
        }
    }
    pub fn feed(&mut self, item: Item) -> Result<String> {
        let Item::Data(v) = item else {
            return Ok(String::new());
        };
        ensure!(
            v.get("error").is_none(),
            "Provedor retornou erro no streaming"
        );
        let delta = if self.ollama {
            &v["message"]
        } else {
            &v["choices"][0]["delta"]
        };
        let text = delta["content"].as_str().unwrap_or("");
        ensure!(
            !self.completed || (text.is_empty() && delta.get("tool_calls").is_none()),
            "Conteúdo após o término do modelo"
        );
        self.text.push_str(text);
        if let Some(calls) = delta["tool_calls"].as_array() {
            for (i, c) in calls.iter().enumerate() {
                let index = c["index"].as_u64().unwrap_or(i as u64) as usize;
                let entry = self
                    .calls
                    .entry(index)
                    .or_insert_with(|| (crate::id(), String::new(), String::new()));
                if let Some(id) = c["id"].as_str() {
                    entry.0 = id.into();
                }
                if let Some(name) = c["function"]["name"].as_str() {
                    entry.1.push_str(name);
                }
                let args = &c["function"]["arguments"];
                if let Some(s) = args.as_str() {
                    entry.2.push_str(s);
                } else if args.is_object() {
                    entry.2 = args.to_string();
                }
                ensure!(
                    entry.0.len() <= 512 && entry.1.len() <= 256 && entry.2.len() <= 256_000,
                    "Chamada de ferramenta excede limite"
                );
            }
        }
        if self.ollama && v["done"] == true {
            let reason = v["done_reason"].as_str().unwrap_or("stop");
            ensure!(reason == "stop", "Resposta Ollama interrompida: {reason}");
            self.completed = true;
            self.usage =
                json!({"input_tokens":v["prompt_eval_count"],"output_tokens":v["eval_count"]});
        } else if !self.ollama {
            if let Some(reason) = v["choices"][0]["finish_reason"].as_str() {
                ensure!(
                    ["stop", "tool_calls"].contains(&reason),
                    "Resposta do modelo interrompida: {reason}"
                );
                self.completed = true;
            }
            if v["usage"].is_object() {
                self.usage = v["usage"].clone();
            }
        }
        ensure!(
            self.text.len() <= 2_000_000 && self.calls.len() <= 16,
            "Resposta do modelo excede limite"
        );
        Ok(text.into())
    }
    pub fn finish(self) -> Result<Answer> {
        ensure!(self.completed, "Streaming interrompido sem confirmação de término; nenhuma ferramenta desta resposta foi executada");
        let mut ids = std::collections::HashSet::new();
        let calls = self
            .calls
            .into_values()
            .map(|(id, name, args)| {
                ensure!(
                    !name.is_empty() && ids.insert(id.clone()),
                    "Identificação de ferramenta inválida"
                );
                let arguments: Value =
                    serde_json::from_str(if args.is_empty() { "{}" } else { &args })?;
                ensure!(
                    arguments.is_object(),
                    "Argumentos da ferramenta devem ser um objeto"
                );
                Ok(ToolCall {
                    id,
                    name: name.replace("__", "."),
                    arguments,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Answer {
            text: self.text,
            calls,
            usage: self.usage,
            provider_state: Value::Null,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_json_tool_is_not_enough_without_terminal_event() {
        let mut c = Collector::new(false);
        c.feed(Item::Data(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"fs__list","arguments":"{}"}}]}}]}))).unwrap();
        c.feed(Item::Done).unwrap();
        assert!(c.finish().is_err());
    }
    #[test]
    fn length_finish_does_not_release_tool_calls() {
        let mut c = Collector::new(false);
        assert!(c
            .feed(Item::Data(
                json!({"choices":[{"delta":{},"finish_reason":"length"}]})
            ))
            .is_err());
    }
}
