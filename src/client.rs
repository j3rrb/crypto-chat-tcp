mod algos;
mod constants;
mod structs;
mod traits;
mod utils;

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use rand::{self, Rng};
use std::str::FromStr;
use std::sync::Arc;
use std::{env, u16};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{watch, Mutex};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use url::Url;
use uuid::Uuid;

use crate::constants::{BASE, PRIME};
use crate::structs::DiffieHellman;
use crate::traits::{Decryptor, Encryptor};
use crate::utils::mod_exp;

#[tokio::main]
async fn main() {
    // Declara a URL WebSocket do server
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:8080".to_string());

    let url = Url::parse(&url).expect("URL inválida");

    let (ws_stream, _) = connect_async(url.as_str()).await.expect("Erro ao conectar");

    println!("Conectado ao servidor: {}\n", url);

    let (ws_sender, ws_receiver) = ws_stream.split();

    let self_id = Arc::new(Mutex::new(Uuid::nil()));
    let self_private_key = Arc::new(Mutex::new(u16::MAX));
    let self_public_key = Arc::new(Mutex::new(u16::MAX));
    let (client_key_tx, _) = watch::channel::<u16>(u16::MAX);
    let (can_type_tx, can_type_rx) = watch::channel::<bool>(false);

    let client_key_rx1 = client_key_tx.subscribe();
    let client_key_rx2 = client_key_tx.subscribe();

    let ws_sdr_mutex = Arc::new(Mutex::new(ws_sender));
    let ws_rvr_mutex = Arc::new(Mutex::new(ws_receiver));

    tokio::spawn({
        let private_key = Arc::clone(&self_private_key);
        let public_key = Arc::clone(&self_public_key);
        let sender = Arc::clone(&ws_sdr_mutex);

        async move {
            let mut private_key = private_key.lock().await;
            let mut public_key = public_key.lock().await;
            send_round_key(&mut private_key, &mut public_key, &mut *sender.lock().await).await;
        }
    })
    .await
    .unwrap();

    println!("Handshook!");

    tokio::spawn({
        let private_key = Arc::clone(&self_private_key);
        let public_key = Arc::clone(&self_public_key);
        let sender = Arc::clone(&ws_sdr_mutex);
        let can_type_rx = can_type_rx.clone();
        let can_type_tx = can_type_tx.clone();

        async move {
            let mut reader = BufReader::new(tokio::io::stdin());
            let mut input = String::new();

            loop {
                let other_key = *client_key_rx1.borrow();

                if can_type_rx.has_changed().is_ok() && *can_type_rx.borrow() {
                    if other_key != u16::MAX {
                        println!("Digite sua mensagem: ");
                        // tokio::io::stdout().flush().await.unwrap();

                        if let Err(e) = reader.read_line(&mut input).await {
                            eprintln!("Erro ao ler entrada: {}", e);
                            break;
                        }

                        let trimmed_input = input.trim().to_string();

                        if let Some((encryptor, _)) = choose_algorithm(1).await {
                            let shared = mod_exp(other_key, *private_key.lock().await, PRIME);
                            println!("Chave compartilhada: {}", shared);

                            match encryptor.encrypt(&trimmed_input, &shared) {
                                Ok(encrypted) => {
                                    match sender.lock().await.send(Message::Text(encrypted)).await {
                                        Ok(_) => {
                                            send_round_key(
                                                &mut *private_key.lock().await,
                                                &mut *public_key.lock().await,
                                                &mut *sender.lock().await,
                                            )
                                            .await;

                                            println!(
                                                "Mensagem criptografada e chave pública enviadas!"
                                            );

                                            can_type_tx.send(false).unwrap();
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "Erro ao enviar mensagem criptografada! {}",
                                                e
                                            )
                                        }
                                    }
                                }
                                Err(e) => eprintln!("Erro ao criptografar mensagem! {}", e),
                            }
                        }

                        input.clear();
                    }
                }
            }
        }
    });

    while let Some(message) = ws_rvr_mutex.lock().await.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let other_key = *client_key_rx2.borrow();

                if let Ok(uuid) = Uuid::from_str(&text) {
                    let sender = Arc::clone(&ws_sdr_mutex);
                    println!("Seu ID é: {}", uuid);
                    *self_id.lock().await = uuid;

                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text("get_public_key".to_string()))
                        .await;

                    println!("Solicitado a chave pública...");

                    continue;
                }

                if text.starts_with("get_public_key:") {
                    if let Some(splitted) = text.split(":").nth(1) {
                        if let Ok(parsed) = splitted.parse::<u16>() {
                            let can_type_tx = can_type_tx.clone();
                            // tokio::io::stdout().flush().await.unwrap();
                            println!("Nova chave recebida! {}", parsed);

                            if client_key_tx.send(parsed).is_err() {
                                eprintln!("Erro ao alterar chave pública do outro client!")
                            }

                            can_type_tx.send(true).unwrap();
                        }
                    }

                    continue;
                }

                if other_key != u16::MAX {
                    let private_key = Arc::clone(&self_private_key);
                    let private_key = private_key.lock().await;

                    if let Some((_, decryptor)) = choose_algorithm(1).await {
                        let shared = mod_exp(other_key, *private_key, PRIME);
                        println!("Chave compartilhada: {}", shared);

                        match decryptor.decrypt(&text, &shared) {
                            Ok(decrypted) => {
                                println!("\nMensagem recebida! {}", decrypted);
                            }
                            Err(e) => eprintln!("Erro ao descriptografar mensagem! {}", e),
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => {
                println!("Conexão fechada!");
                break;
            }
            Ok(_) => {}
            Err(e) => eprintln!("Erro ao receber mensagem! {}", e),
        }
    }

    tokio::signal::ctrl_c()
        .await
        .expect("Falha ao aguardar sinal de interrupção");

    println!("Programa encerrado.");
}

fn generate_keys() -> (u16, u16) {
    let private_key: u16 = rand::thread_rng().gen_range(1..PRIME);
    let public_key: u16 = mod_exp(BASE, private_key, PRIME);

    (private_key, public_key)
}

fn generate_and_set_keys(private_key: &mut u16, public_key: &mut u16) -> (u16, u16) {
    let (pr, pb) = generate_keys();

    *private_key = pr;
    *public_key = pb;

    (pr, pb)
}

async fn send_round_key(
    private_key: &mut u16,
    public_key: &mut u16,
    ws_sender: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
) {
    let (_, pb) = generate_and_set_keys(private_key, public_key);

    let msg = Message::Text(format!("set_public_key:{}", pb));

    match ws_sender.send(msg).await {
        Ok(_) => println!("Nova chave pública enviada ao server!"),
        Err(e) => eprintln!("Erro ao enviar a nova chave pública ao server! {}", e),
    }
}

async fn choose_algorithm(option: u16) -> Option<(Box<dyn Encryptor>, Box<dyn Decryptor>)> {
    match option {
        1 => Some((
            Box::new(DiffieHellman::new()),
            Box::new(DiffieHellman::new()),
        )),
        _ => None,
    }
}
