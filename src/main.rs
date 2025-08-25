use clap::Parser;
use divvunspell::{speller::Speller, tokenizer::Tokenize};
use poem::{
    get, handler,
    listener::TcpListener,
    middleware::Cors,
    post,
    web::{Data, Html, Json},
    EndpointExt, IntoResponse, Route, Server,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(serde::Deserialize)]
struct ProcessInput {
    text: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SpellerResponse {
    pub text: String,
    pub results: Vec<SpellerResult>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SpellerResult {
    pub word: String,
    pub is_correct: bool,
    pub suggestions: Vec<SpellerSuggestion>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SpellerSuggestion {
    pub value: String,
    pub weight: f32,
}

#[handler]
async fn process(
    Data(speller): Data<&Arc<dyn Speller + Send + Sync>>,
    Json(body): Json<ProcessInput>,
) -> impl IntoResponse {
    let words = body.text.word_indices().map(|x| x.1).collect::<Vec<&str>>();
    let mut results = vec![];
    let speller = Arc::clone(&speller);

    for word in words {
        let word = word.to_string();
        let is_correct = speller.clone().is_correct(&word);
        let suggestions = speller.clone().suggest(&word);

        results.push(SpellerResult {
            word: word.to_owned(),
            is_correct,
            suggestions: suggestions
                .into_iter()
                .map(|s| SpellerSuggestion {
                    value: s.value().to_owned(),
                    weight: s.weight(),
                })
                .collect(),
        });
    }

    Json(SpellerResponse {
        text: body.text,
        results,
    })
    .into_response()
}

const PAGE: &str = r#"
<!doctype html>
<html>
<head>
<title>Divvun Speller</title>
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
Run speller
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

#[handler]
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "healthy"})).into_response()
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

    let archive = divvunspell::archive::open(path).unwrap();
    let speller = archive.speller();

    tracing::info!("Parent path: {}", parent_path.display());
    tracing::info!("File name: {}", file_name);

    let app = Route::new()
        .at("/", post(process).get(process_get))
        .at("/health", get(health))
        .data(speller)
        .data(Language(lang))
        .with(Cors::default());

    Server::new(TcpListener::bind((cli.host, cli.port)))
        .run(app)
        .await?;

    Ok(())
}
