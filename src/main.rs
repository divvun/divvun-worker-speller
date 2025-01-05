use clap::Parser;
use divvunspell::tokenizer::Tokenize;
use poem::{
    handler,
    http::StatusCode,
    listener::TcpListener,
    middleware::Cors,
    post,
    web::{Data, Html, Json},
    EndpointExt, IntoResponse, Route, Server,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use subterm::{SubprocessHandler as _, SubprocessPool};

#[derive(serde::Deserialize)]
struct ProcessInput {
    text: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HyphenationResponse {
    pub text: String,
    pub results: Vec<HyphenationResult>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HyphenationResult {
    pub word: String,
    pub hyphenations: Vec<HyphenationPattern>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HyphenationPattern {
    pub value: String,
    pub weight: f64,
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

    let words = body.text.word_indices().map(|x| x.1).collect::<Vec<&str>>();
    let mut results = vec![];

    for word in words {
        bundle.write_line(word).await.unwrap();
        bundle.flush().await.unwrap();

        let lines = bundle.read_until(b"\n\n").await.unwrap();

        let patterns = lines
            .trim()
            .lines()
            .filter_map(|line| {
                let components: Vec<&str> = line.split("\t").collect();
                if components.len() < 3 {
                    return None;
                }

                let weight = components[2].parse();
                if let Ok(weight) = weight {
                    return Some(HyphenationPattern {
                        value: components[1].to_owned(),
                        weight,
                    });
                } else {
                    None
                }
            })
            .collect::<Vec<HyphenationPattern>>();

        results.push(HyphenationResult {
            word: word.to_owned(),
            hyphenations: patterns,
        });
    }

    Json(HyphenationResponse {
        text: body.text,
        results,
    })
    .into_response()
}

const PAGE: &str = r#"
<!doctype html>
<html>
<head>
<title>Divvun Hyphenator</title>
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
Run hyphenator
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
                .args(["divvun-checker:latest"])
                .args(["hfst-lookup", "-n", "1", "-q"])
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
