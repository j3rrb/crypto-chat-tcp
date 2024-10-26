use futures_util::{SinkExt, StreamExt};
use std::env;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use url::Url;

#[tokio::main]
async fn main() {
    // Pega a URL do servidor a partir dos argumentos ou usa um padrão
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:8080".to_string());
    let url = Url::parse(&url).expect("URL inválida");

    // Conecta ao servidor WebSocket
    let (ws_stream, _) = connect_async(url.as_str()).await.expect("Erro ao conectar");

    println!("Conectado ao servidor: {}", url);

    // Divide o fluxo WebSocket em sender e receiver
    let (mut sender, mut receiver) = ws_stream.split();

    // Tarefa para enviar mensagens para o servidor
    tokio::spawn(async move {
        let mut input = String::new();
        loop {
            std::io::stdin()
                .read_line(&mut input)
                .expect("Falha ao ler entrada");
            let msg = Message::Text(input.trim().to_string());

            if sender.send(msg).await.is_err() {
                println!("Conexão fechada");
                break;
            }

            input.clear();
        }
    });

    // Tarefa para receber mensagens do servidor
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(message) => match message {
                Message::Text(text) => {
                    println!("{}\n", text);
                }
                Message::Close(_) => {
                    println!("Servidor fechou a conexão");
                    break;
                }
                _ => {}
            },
            Err(e) => {
                println!("Erro ao receber mensagem: {}", e);
                break;
            }
        }
    }
}
