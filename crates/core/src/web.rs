use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::IpAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchProvider {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub secret_ref: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub allow_local: bool,
    pub revision: u64,
}

pub fn validate_config(provider: &SearchProvider) -> Result<()> {
    ensure!(
        ["brave", "searxng", "declarative"].contains(&provider.kind.as_str()),
        "Adaptador de busca inválido"
    );
    ensure!(
        !provider.name.trim().is_empty() && provider.name.len() <= 120,
        "Nome do provedor de busca inválido"
    );
    let url = public_url(&provider.base_url, provider.allow_local)?;
    if provider.kind == "brave" {
        ensure!(url.scheme() == "https", "Brave Search exige HTTPS");
    }
    Ok(())
}

fn public_url(raw: &str, allow_local: bool) -> Result<url::Url> {
    let url = url::Url::parse(raw)?;
    ensure!(
        ["http", "https"].contains(&url.scheme())
            && url.username().is_empty()
            && url.password().is_none(),
        "URL web inválida"
    );
    let host = url.host_str().context("Host ausente")?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        ensure!(allow_local || !blocked_ip(ip), "Endereço privado bloqueado");
    }
    ensure!(
        host != "169.254.169.254" && !host.ends_with(".internal"),
        "Endpoint de metadados bloqueado"
    );
    Ok(url)
}

fn blocked_ip(ip: IpAddr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || match ip {
            IpAddr::V4(value) => {
                value.is_private()
                    || value.is_link_local()
                    || value.is_broadcast()
                    || value.octets()[0] == 0
            }
            IpAddr::V6(value) => value.is_unique_local() || value.is_unicast_link_local(),
        }
}

async fn validate_destination(url: &url::Url, allow_local: bool) -> Result<()> {
    let host = url.host_str().context("Host ausente")?;
    let port = url.port_or_known_default().context("Porta ausente")?;
    let addresses: Vec<_> = tokio::net::lookup_host((host, port)).await?.collect();
    ensure!(!addresses.is_empty(), "O host não pôde ser resolvido");
    ensure!(
        allow_local || addresses.iter().all(|address| !blocked_ip(address.ip())),
        "Destino privado ou reservado bloqueado"
    );
    Ok(())
}

pub async fn search(provider: &SearchProvider, query: &str, key: Option<&str>) -> Result<Value> {
    ensure!(
        provider.enabled && !query.trim().is_empty() && query.len() <= 600,
        "Busca inválida"
    );
    let base = public_url(&provider.base_url, provider.allow_local)?;
    validate_destination(&base, provider.allow_local).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let request = match provider.kind.as_str() {
        "brave" => client
            .get(base)
            .query(&[("q", query), ("count", "10")])
            .header("accept", "application/json")
            .header(
                "x-subscription-token",
                key.context("Chave Brave não configurada")?,
            ),
        "searxng" => client
            .get(base.join("search")?)
            .query(&[("q", query), ("format", "json")]),
        "declarative" => client
            .get(base)
            .query(&[("q", query)])
            .header("accept", "application/json"),
        _ => anyhow::bail!("Adaptador de busca desconhecido"),
    };
    let response = request.send().await?;
    ensure!(
        response.status().is_success(),
        "Busca respondeu HTTP {}",
        response.status()
    );
    let bytes = response.bytes().await?;
    ensure!(bytes.len() <= 2_000_000, "Resposta de busca excede limite");
    let value: Value = serde_json::from_slice(&bytes)?;
    let rows = if provider.kind == "brave" {
        value.pointer("/web/results").and_then(Value::as_array)
    } else {
        value["results"].as_array()
    }
    .cloned()
    .unwrap_or_default();
    Ok(json!(rows.into_iter().take(20).map(|row|json!({"title":row["title"],"url":row["url"],"snippet":row.get("description").or_else(||row.get("content")).cloned().unwrap_or(Value::Null)})).collect::<Vec<_>>()))
}

pub async fn fetch(raw: &str) -> Result<Value> {
    let mut url = public_url(raw, false)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(25))
        .user_agent("FORJA/0.1 web.fetch")
        .build()?;
    let mut redirects = Vec::new();
    let response = loop {
        validate_destination(&url, false).await?;
        let response = client
            .get(url.clone())
            .header(
                "accept",
                "text/html,text/plain,application/json;q=0.9,*/*;q=0.1",
            )
            .send()
            .await?;
        if response.status().is_redirection() {
            ensure!(redirects.len() < 5, "Redirecionamentos demais");
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .context("Redirecionamento sem destino")?;
            let next = public_url(url.join(location)?.as_str(), false)?;
            redirects.push(url.to_string());
            url = next;
            continue;
        }
        break response;
    };
    ensure!(
        response.status().is_success(),
        "A URL respondeu HTTP {}",
        response.status()
    );
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    ensure!(
        media_type.starts_with("text/")
            || media_type.contains("json")
            || media_type.contains("xml"),
        "Tipo de conteúdo não textual bloqueado"
    );
    if let Some(length) = response.content_length() {
        ensure!(length <= 2_000_000, "Conteúdo web excede 2 MB");
    }
    let bytes = response.bytes().await?;
    ensure!(bytes.len() <= 2_000_000, "Conteúdo web excede 2 MB");
    let mut content = String::from_utf8_lossy(&bytes).into_owned();
    if media_type.contains("html") {
        for element in ["script", "style", "noscript"] {
            let expression = format!(r"(?is)<{element}[^>]*>.*?</{element}>");
            content = regex::Regex::new(&expression)?
                .replace_all(&content, " ")
                .into_owned();
        }
        let tags = regex::Regex::new(r"(?is)<[^>]+>")?;
        content = tags.replace_all(&content, " ").into_owned();
        let whitespace = regex::Regex::new(r"[ \t\r\n]+")?;
        content = whitespace.replace_all(&content, " ").trim().to_owned();
    }
    if content.len() > 500_000 {
        content.truncate(500_000);
    }
    Ok(json!({
        "url": url.to_string(),
        "media_type": media_type,
        "content": content,
        "redirects": redirects,
        "untrusted": true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(kind: &str, base_url: &str, allow_local: bool) -> SearchProvider {
        SearchProvider {
            id: "search-test".into(),
            kind: kind.into(),
            name: "Busca de teste".into(),
            base_url: base_url.into(),
            secret_ref: None,
            enabled: true,
            allow_local,
            revision: 1,
        }
    }

    #[test]
    fn validates_search_provider_protocol_and_local_opt_in() {
        assert!(validate_config(&provider(
            "brave",
            "https://api.search.brave.com/res/v1/web/search",
            false
        ))
        .is_ok());
        assert!(validate_config(&provider("brave", "http://example.com/search", false)).is_err());
        assert!(validate_config(&provider("searxng", "http://127.0.0.1:8080/", false)).is_err());
        assert!(validate_config(&provider("searxng", "http://127.0.0.1:8080/", true)).is_ok());
        assert!(validate_config(&provider("unknown", "https://example.com/", false)).is_err());
    }
}
