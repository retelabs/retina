use clickhouse::Client;
use query_api::build_app;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// `QUERY_API_KEYS_EXTRA`: comma-separated additional valid tokens, on top
/// of the required `QUERY_API_KEY` — same reasoning as
/// `crates/kernel/src/main.rs`'s `parse_extra_tokens`, duplicated rather
/// than shared (each binary stays self-contained, same posture already
/// applied to `auth.rs` itself). `None`/unset means "no extra tokens".
fn parse_extra_tokens(raw: Option<&str>) -> Vec<String> {
    raw.map(|config| {
        config
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// `QUERY_API_KEY` is required and must not be blank: `QUERY_API_KEY=` (set but empty, a
/// common `.env` slip) would otherwise start the process with the token
/// `""`, i.e. accept `authorization: Bearer ` — fail closed on it exactly
/// like on an unset key. Pure function, testable without the environment.
fn primary_api_key(raw: Option<String>) -> Result<String, String> {
    match raw {
        Some(key) if !key.trim().is_empty() => Ok(key),
        Some(_) => {
            Err("QUERY_API_KEY is set but blank — see docs/interfaces/kernel-auth.md".to_string())
        }
        None => Err("QUERY_API_KEY must be set — see docs/interfaces/kernel-auth.md".to_string()),
    }
}

#[tokio::main]
async fn main() {
    let url = env_or("CLICKHOUSE_URL", "http://localhost:8123");
    let user = env_or("CLICKHOUSE_USER", "dev");
    let password = env_or("CLICKHOUSE_PASSWORD", "dev");
    let database = env_or("CLICKHOUSE_DATABASE", "observability");
    let bind_addr = env_or("QUERY_API_BIND", "0.0.0.0:8080");
    // Fails closed (docs/interfaces/kernel-auth.md): no "auth disabled"
    // fallback, an unset key must stop the process rather than start it
    // unauthenticated.
    let api_key =
        primary_api_key(std::env::var("QUERY_API_KEY").ok()).unwrap_or_else(|e| panic!("{e}"));
    let mut api_keys = vec![api_key];
    api_keys.extend(parse_extra_tokens(
        std::env::var("QUERY_API_KEYS_EXTRA").ok().as_deref(),
    ));

    let client = Client::default()
        .with_url(url)
        .with_user(user)
        .with_password(password)
        .with_database(database);

    let app = build_app(client, api_keys);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind QUERY_API_BIND");
    eprintln!("query-api listening on {bind_addr}");
    axum::serve(listener, app).await.expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_api_key_refuses_unset_and_blank() {
        assert!(primary_api_key(None).is_err());
        assert!(primary_api_key(Some(String::new())).is_err());
        assert!(primary_api_key(Some("  ".to_string())).is_err());
        assert_eq!(primary_api_key(Some("k".to_string())).unwrap(), "k");
    }

    #[test]
    fn parse_extra_tokens_unset_is_empty() {
        assert!(parse_extra_tokens(None).is_empty());
    }

    #[test]
    fn parse_extra_tokens_explicit_empty_string_is_empty() {
        assert!(parse_extra_tokens(Some("")).is_empty());
    }

    #[test]
    fn parse_extra_tokens_splits_trims_and_drops_blanks() {
        let result = parse_extra_tokens(Some(" tok-a ,tok-b, ,tok-c "));
        assert_eq!(result, vec!["tok-a", "tok-b", "tok-c"]);
    }
}
