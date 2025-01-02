use clap::Parser;
use poem::{
    handler,
    http::StatusCode,
    listener::TcpListener,
    middleware::Cors,
    post,
    web::{Data, Html, Json},
    EndpointExt, IntoResponse, Route, Server,
};
use std::{path::Path, sync::Arc};
use subterm::{SubprocessHandler as _, SubprocessPool};

#[derive(serde::Deserialize)]
struct ProcessInput {
    text: String,
}

#[handler]
async fn process(
    Data(pool): Data<&Arc<SubprocessPool>>,
    Json(body): Json<ProcessInput>,
) -> impl IntoResponse {
    let mut bundle = match pool.acquire().await {
        Ok(bundle) => bundle,
        Err(e) => {
            tracing::error!("{:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    bundle.write_line(&body.text).await.unwrap();
    bundle.flush().await.unwrap();
    let line = bundle.read_line().await.unwrap();

    let json: serde_json::Value = serde_json::from_str(&line).unwrap();
    Json(json).into_response()
}

const PAGE: &str = r#"
<!doctype html>
<html>
<head>
<title>Divvun Grammar</title>
<meta charset="utf-8">
<style>
.container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
}

</style>
</head>
<body>
<div class="container">
<h2>Language: %LANG%</h2>
<textarea class="text"></textarea>
<div>
Result:
<pre class="result"></pre>
</div>
<button class="doit">
Run grammar
</button>
<script>
document.querySelector(".doit").addEventListener("click", () => {
    const text = document.querySelector(".text").value;
    fetch(location.href, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
        },
        body: JSON.stringify({ text }),
    }).then((r) => r.json()).then((r) => {
        document.querySelector(".result").textContent = JSON.stringify(r, null, 2);
    });
});
</script>
</div>
</body>
</html>
"#;

#[derive(Debug, Clone)]
struct Language(String);

#[handler]
async fn process_get(Data(lang): Data<&Language>) -> impl IntoResponse {
    Html(PAGE.replace("%LANG%", &lang.0)).into_response()
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the grammar bundle file
    #[arg(required = true)]
    bundle_path: String,

    /// Host to bind the server to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to run the server on
    #[arg(long, default_value_t = 4000)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    Ok(run(cli).await?)
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let path = Path::new(&cli.bundle_path).canonicalize().unwrap();
    let parent_path = path.parent().unwrap().to_path_buf();
    let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
    let lang = file_name.split('.').next().unwrap().to_string();

    tracing::info!("Parent path: {}", parent_path.display());
    tracing::info!("File name: {}", file_name);

    let pool = subterm::SubprocessPool::new(
        move || {
            let mut cmd = tokio::process::Command::new("docker");
            cmd.args(["run", "-i", "-v"])
                .arg(format!("{}:/data", parent_path.display()))
                .args(["divvun-checker:latest", "divvun-checker", "-a"])
                .arg(format!("/data/{}", &file_name));
            cmd
        },
        4,
    )
    .await
    .unwrap();

    let app = Route::new()
        .at("/", post(process).get(process_get))
        .data(pool)
        .data(Language(lang))
        .with(Cors::default());

    Server::new(TcpListener::bind((cli.host, cli.port)))
        .run(app)
        .await?;

    Ok(())
}
