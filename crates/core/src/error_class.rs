//! Classificação estruturada de erros.
//!
//! Substitui a antiga detecção por substring espalhada pelo código
//! (`error.to_string().to_lowercase().contains(...)`) por tipos nomeados e
//! testáveis. A extração do status HTTP funciona para mensagens formatadas
//! como `"Provedor respondeu HTTP 429"` ou `"provider request failed: HTTP 503"`.

use std::fmt;

/// Categorias de erro reconhecidas pelo FORJA ao expor falhas na API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Credencial ausente, inválida ou expirada (HTTP 401).
    Unauthorized,
    /// Credencial válida, mas sem permissão (HTTP 403).
    Forbidden,
    /// Recurso ou modelo inexistente no provedor (HTTP 404).
    NotFound,
    /// Estado conflitante, ex.: registro duplicado (HTTP 409).
    Conflict,
    /// Resposta malformada ou JSON inválido do provedor.
    InvalidResponse,
    /// Configuração local incorreta (chave não configurada, URL relativa etc.).
    Misconfigured,
    /// Cota excedida no provedor (HTTP 429).
    RateLimited,
    /// Provedor devolveu 5xx.
    Upstream,
    /// Serviço de destino indisponível: conexão recusada, DNS, offline.
    Unavailable,
    /// Tempo limite excedido.
    Timeout,
    /// Cancelação iniciada pelo usuário.
    Cancelled,
    /// Requisição inválida (demais 4xx).
    InvalidRequest,
    /// Nenhuma das categorias anteriores.
    Other,
}

impl ErrorKind {
    /// Código estável usado no corpo JSON da API (`ApiError.code`).
    pub fn code(self) -> &'static str {
        match self {
            Self::Unauthorized => "provider_auth_failed",
            Self::Forbidden => "provider_forbidden",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::InvalidResponse => "provider_invalid_response",
            Self::Misconfigured => "provider_misconfigured",
            Self::RateLimited => "provider_rate_limited",
            Self::Upstream => "provider_upstream_error",
            Self::Unavailable => "provider_unavailable",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::InvalidRequest => "invalid_request",
            Self::Other => "request_failed",
        }
    }

    /// Se um novo retry tem chance razoável de sucesso sem intervenção humana.
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::Upstream
                | Self::Unavailable
                | Self::Timeout
                | Self::InvalidResponse
        )
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Extrai o código de status HTTP embutido em uma mensagem de erro, quando
/// presente. Reconhece tanto `"HTTP 429 Too Many Requests"` quanto
/// `"respondeu HTTP 401 Unauthorized"`. Retorna `None` se não houver um
/// número de três dígitos imediatamente após `"http "`.
pub fn http_status_from_message(message: &str) -> Option<u16> {
    let lower = message.to_ascii_lowercase();
    let (_, after) = lower.split_once("http ")?;
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() != 3 {
        return None;
    }
    digits.parse::<u16>().ok()
}

/// Erro com categoria explícita. Use [`Error::cancelled`] para fluxos
/// interrompidos pelo usuário e [`Error::classify`] para criar variantes a
/// partir de mensagens legadas via [`ErrorKind::from_message`].
#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ClassifiedError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorKind::Cancelled, "Cancelado")
    }

    /// Interpreta uma mensagem de erro (de qualquer fonte) em uma categoria.
    pub fn classify(message: impl Into<String>) -> Self {
        let message = message.into();
        let kind = ErrorKind::from_message(&message);
        Self { kind, message }
    }

    pub fn is_cancelled(&self) -> bool {
        self.kind == ErrorKind::Cancelled
    }
}

impl fmt::Display for ClassifiedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ClassifiedError {}

impl ErrorKind {
    /// Categoriza uma mensagem de erro livre. Prioriza o status HTTP numérico
    /// (fonte confiável) e só recorre a palavras-chave quando ele está
    /// ausente. Palavras-chave são intencionalmente estreitas para evitar
    /// falsos positivos (ex.: `"connection refused"` não é mais lido como
    /// erro de permissão).
    pub fn from_message(message: &str) -> Self {
        match http_status_from_message(message) {
            Some(401) => return Self::Unauthorized,
            Some(403) => return Self::Forbidden,
            Some(404) => return Self::NotFound,
            Some(409) => return Self::Conflict,
            Some(429) => return Self::RateLimited,
            Some(503) => return Self::Unavailable,
            Some(status @ 500..=599) => {
                // 502/504 e demais 5xx: upstream.
                let _ = status;
                return Self::Upstream;
            }
            Some(status @ 400..=499) => {
                let _ = status;
                return Self::InvalidRequest;
            }
            None => {}
        }
        let lower = message.to_lowercase();
        if lower.contains("cancelad") || lower.contains("cancel") {
            Self::Cancelled
        } else if lower.contains("timed out") || lower.contains("timeout") {
            Self::Timeout
        } else if lower.contains("not logged in")
            || lower.contains("missing api key")
            || lower.contains("configure uma chave")
            || lower.contains("unauthorized")
            || lower.contains("token expired")
            || lower.contains("credencial")
        {
            Self::Unauthorized
        } else if lower.contains("forbidden") || lower.contains("permission denied") {
            Self::Forbidden
        } else if lower.contains("formato de catálogo")
            || lower.contains("expected value at line")
            || lower.contains("invalid json")
        {
            Self::InvalidResponse
        } else if lower.contains("url relativa")
            || lower.contains("relative url")
            || lower.contains("tipo de provedor desconhecido")
            || lower.contains("url deve conter")
            || lower.contains("use https")
            || lower.contains("precisa usar loopback")
        {
            Self::Misconfigured
        } else if lower.contains("rate limit") || lower.contains("too many requests") {
            Self::RateLimited
        } else if lower.contains("not found") {
            Self::NotFound
        } else if lower.contains("already exists") {
            Self::Conflict
        } else if lower.contains("error sending request")
            || lower.contains("connection refused")
            || lower.contains("connection closed")
            || lower.contains("connection reset")
            || lower.contains("dns")
            || lower.contains("offline")
            || lower.contains("network")
        {
            Self::Unavailable
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_http_status_from_messages() {
        assert_eq!(
            http_status_from_message("Provedor respondeu HTTP 401 Unauthorized"),
            Some(401)
        );
        assert_eq!(
            http_status_from_message("provider request failed: HTTP 429 Too Many Requests"),
            Some(429)
        );
        assert_eq!(http_status_from_message("respondeu HTTP 503 em 3s"), Some(503));
        assert_eq!(http_status_from_message("sem status aqui"), None);
        assert_eq!(http_status_from_message("http 42"), None);
    }

    #[test]
    fn classifies_by_http_status_first() {
        assert_eq!(
            ErrorKind::from_message("Provedor respondeu HTTP 401 Unauthorized"),
            ErrorKind::Unauthorized
        );
        assert_eq!(
            ErrorKind::from_message("Modelo respondeu HTTP 404"),
            ErrorKind::NotFound
        );
        assert_eq!(
            ErrorKind::from_message("Provedor respondeu HTTP 503 Service Unavailable"),
            ErrorKind::Unavailable
        );
        assert_eq!(
            ErrorKind::from_message("Provedor respondeu HTTP 502 Bad Gateway"),
            ErrorKind::Upstream
        );
        assert_eq!(
            ErrorKind::from_message("Contagem de tokens respondeu HTTP 429"),
            ErrorKind::RateLimited
        );
    }

    #[test]
    fn classifies_network_failures_as_unavailable_not_permission() {
        // Regressão: a versão antiga casava "connection refused" com
        // permissão negada por causa de substrings soltas.
        assert_eq!(
            ErrorKind::from_message("error sending request: connection refused"),
            ErrorKind::Unavailable
        );
        assert_eq!(
            ErrorKind::from_message("failed to lookup address: Name or service not known (dns)"),
            ErrorKind::Unavailable
        );
    }

    #[test]
    fn classifies_cancellation() {
        assert_eq!(ErrorKind::from_message("Cancelado"), ErrorKind::Cancelled);
        assert_eq!(
            ErrorKind::from_message("Subagente cancelado"),
            ErrorKind::Cancelled
        );
        assert!(ClassifiedError::cancelled().is_cancelled());
    }

    #[test]
    fn retryability_matches_contract() {
        assert!(ErrorKind::RateLimited.is_retryable());
        assert!(ErrorKind::Unavailable.is_retryable());
        assert!(ErrorKind::Timeout.is_retryable());
        assert!(!ErrorKind::Unauthorized.is_retryable());
        assert!(!ErrorKind::Cancelled.is_retryable());
        assert!(!ErrorKind::Misconfigured.is_retryable());
        assert!(!ErrorKind::Other.is_retryable());
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(ErrorKind::Unauthorized.code(), "provider_auth_failed");
        assert_eq!(ErrorKind::Unavailable.code(), "provider_unavailable");
        assert_eq!(ErrorKind::RateLimited.code(), "provider_rate_limited");
        assert_eq!(ErrorKind::InvalidResponse.code(), "provider_invalid_response");
        assert_eq!(ErrorKind::Misconfigured.code(), "provider_misconfigured");
        assert_eq!(ErrorKind::Upstream.code(), "provider_upstream_error");
        assert_eq!(ErrorKind::Other.code(), "request_failed");
    }
}
