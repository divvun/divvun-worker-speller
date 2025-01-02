use std::{path::Path, process::Stdio, sync::Arc};

// use divvun_runtime::{modules::Input, Bundle};
use poem::{
    handler,
    listener::TcpListener,
    middleware::Cors,
    post,
    web::{Data, Html, Json},
    EndpointExt, IntoResponse, Route, Server,
};
use subterm::{SubprocessHandler, SubprocessPool};
use tokio::io::AsyncWriteExt;

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
            return Json(serde_json::json!({
                "error": e.to_string()
            }))
            .into_response();
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
async fn process_get(
    Data(lang): Data<&Language>,
) -> impl IntoResponse {
    Html(PAGE.replace("%LANG%", &lang.0)).into_response()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Ok(run().await?)
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let file_path = match std::env::args().skip(1).next() {
        Some(file_path) => file_path,
        None => {
            tracing::error!("No bundle path provided");
            return Err(anyhow::anyhow!("No bundle path provided").into());
        }
    };

    let path = Path::new(&file_path).canonicalize().unwrap();
    let parent_path = path.parent().unwrap().to_path_buf();
    let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
    let lang = file_name.split('.').next().unwrap().to_string();

    tracing::info!("Parent path: {}", parent_path.display());
    tracing::info!("File name: {}", file_name);

    let pool = subterm::SubprocessPool::new(
        move || {
            let mut cmd = tokio::process::Command::new("docker");
            // docker run -it -v `pwd`:/data divvun-checker:latest divvun-checker -v -t -a /data/se.zcheck
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

    let port = match std::env::var("PORT").ok().map(|x| x.parse::<u16>()) {
        Some(Ok(port)) => port,
        Some(Err(e)) => {
            return Err(e.into());
        }
        None => 4000,
    };

    let host = match std::env::var("HOST").ok() {
        Some(host) => host,
        None => "127.0.0.1".to_string(),
    };

    let app = Route::new()
        .at("/", post(process).get(process_get))
        .data(pool)
        .data(Language(lang))
        .with(Cors::default());

    Server::new(TcpListener::bind((host, port)))
        .run(app)
        .await?;

    Ok(())
}
