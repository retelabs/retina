use clickhouse::Client;
use query_api::build_app;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() {
    let url = env_or("CLICKHOUSE_URL", "http://localhost:8123");
    let user = env_or("CLICKHOUSE_USER", "dev");
    let password = env_or("CLICKHOUSE_PASSWORD", "dev");
    let database = env_or("CLICKHOUSE_DATABASE", "observability");
    let bind_addr = env_or("QUERY_API_BIND", "0.0.0.0:8080");

    let client = Client::default()
        .with_url(url)
        .with_user(user)
        .with_password(password)
        .with_database(database);

    let app = build_app(client);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind QUERY_API_BIND");
    eprintln!("query-api listening on {bind_addr}");
    axum::serve(listener, app).await.expect("server error");
}
