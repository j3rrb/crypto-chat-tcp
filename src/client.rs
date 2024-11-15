mod algos;
mod constants;
mod structs;
mod traits;
mod utils;

use futures_util::{SinkExt, StreamExt};
use rand::{self, Rng};
use std::str::FromStr;
use std::sync::Arc;
use std::{env, u16};
use tokio::sync::{watch, Mutex};
use tokio::task;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
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

    let client_id = Arc::new(Mutex::<Uuid>::new(Uuid::nil()));
    let private_key = Arc::new(Mutex::new(u16::MAX));
    let public_key = Arc::new(Mutex::new(u16::MAX));
    let ws_sender_mtx = Arc::new(Mutex::new(ws_sender));
    let ws_receiver_mtx = Arc::new(Mutex::new(ws_receiver));

    let (key_sender, key_receiver) = watch::channel(0u16);
    let key_sender_mtx = Arc::new(Mutex::new(key_sender));
    let key_receiver_mtx = Arc::new(Mutex::new(key_receiver));

    let _id_task = {
        let client_id_clone = Arc::clone(&client_id);
        let ws_receiver_clone = Arc::clone(&ws_receiver_mtx);

        task::spawn({
            async move {
                while let Some(msg) = ws_receiver_clone.lock().await.next().await {
                    match msg {
                        Ok(Message::Text(id)) => {
                            println!("Seu ID é: {}\n", id);
                            match Uuid::from_str(id.as_str()) {
                                Ok(uuid_str) => {
                                    *client_id_clone.lock().await = uuid_str;
                                    break;
                                }
                                Err(e) => eprintln!("Erro ao converter ID! {}", e),
                            }
                        }
                        Ok(Message::Close(_)) => {
                            println!("Server closed the connection");
                            break;
                        }
                        Err(e) => {
                            eprintln!("Error receiving message: {}", e);
                            break;
                        }
                        _ => (),
                    }
                }
            }
        })
    }.await.unwrap();

    let _send_key_task = {
        let ws_sender_clone = Arc::clone(&ws_sender_mtx);
        let pr_key_clone = Arc::clone(&private_key);
        let pb_key_clone = Arc::clone(&public_key);

        task::spawn({
            async move {
                generate_and_set_keys(
                    &mut *pr_key_clone.lock().await,
                    &mut *pb_key_clone.lock().await,
                );

                let msg = Message::Text(format!("set_public_key:{}", *pb_key_clone.lock().await));

                match ws_sender_clone.lock().await.send(msg).await {
                    Ok(_) => println!("Chave pública enviada ao server!"),
                    Err(e) => eprintln!("Erro ao enviar chave pública ao server! {}", e),
                }
            }
        })
    }.await.unwrap();

    let _receive_public_key_task = {
        let ws_receiver_clone = Arc::clone(&ws_receiver_mtx);
        let ws_sender_clone = Arc::clone(&ws_sender_mtx);
        let key_sender_clone = Arc::clone(&key_sender_mtx);

        task::spawn({
            async move {
                loop {
                    println!("Buscando chave pública...");

                    let msg = Message::Text("get_public_key".to_string());

                    match ws_sender_clone.lock().await.send(msg).await {
                        Ok(_) => println!("Solicitado a chave pública ao server!"),
                        Err(e) => eprintln!("Erro ao solicitar chave pública ao server! {}", e),
                    }

                    if let Some(msg) = ws_receiver_clone.lock().await.next().await {
                        match msg {
                            Ok(message) => match message {
                                Message::Text(text) => {
                                    if text.starts_with("get_public_key:") {
                                        if let Some(key) = text.split(':').nth(1) {
                                            if let Ok(parsed_key) = key.parse() {
                                                let _ =
                                                    key_sender_clone.lock().await.send(parsed_key);
                                                println!("Chave pública recebida! {}", parsed_key);
                                            }
                                        }
                                        break;
                                    }
                                }
                                Message::Close(_) => {
                                    println!("Servidor fechou a conexão");
                                    break;
                                }
                                _ => {}
                            },
                            Err(e) => {
                                println!("Erro ao receber chave pública: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        })
    }.await.unwrap();
}

fn generate_keys() -> (u16, u16) {
    let private_key: u16 = rand::thread_rng().gen_range(1..PRIME);
    let public_key: u16 = mod_exp(BASE, private_key, PRIME);

    (private_key, public_key)
}

fn generate_and_set_keys(private_key: &mut u16, public_key: &mut u16) {
    let (pr, pb) = generate_keys();

    *private_key = pr;
    *public_key = pb;
}

fn choose_algorithm(option: u16) -> Option<(Box<dyn Encryptor>, Box<dyn Decryptor>)> {
    match option {
        1 => Some((
            Box::new(DiffieHellman::new()),
            Box::new(DiffieHellman::new()),
        )),
        _ => None,
    }
}
