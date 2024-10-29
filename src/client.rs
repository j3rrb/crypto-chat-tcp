#[macro_use]
extern crate lazy_static;

mod algos;
mod constants;
mod structs;
mod traits;

use futures_util::{SinkExt, StreamExt};
use std::env;
use tokio::sync::Mutex as AsyncMutex;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use url::Url;

use crate::structs::DiffieHellman;
use crate::traits::{Decryptor, Encryptor};

lazy_static! {
    static ref OPTION: AsyncMutex<u32> = AsyncMutex::new(0);
}

fn choose_algorithm(option: u32) -> Option<(Box<dyn Encryptor>, Box<dyn Decryptor>)> {
    match option {
        1 => Some((
            Box::new(DiffieHellman::new()),
            Box::new(DiffieHellman::new()),
        )),
        _ => None,
    }
}

#[tokio::main]
async fn main() {
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:8080".to_string());
    let url = Url::parse(&url).expect("URL inválida");

    let (ws_stream, _) = connect_async(url.as_str()).await.expect("Erro ao conectar");

    println!("Conectado ao servidor: {}", url);

    let (mut sender, mut receiver) = ws_stream.split();

    tokio::spawn(async move {
        let mut input = String::new();

        loop {
            let option = OPTION.lock().await;
            std::io::stdin()
                .read_line(&mut input)
                .expect("Falha ao ler entrada");

            if *option == 0 {
                panic!("Opção de algoritmo não selecionada!");
            }

            if let Some((encryptor, _)) = choose_algorithm(*option) {
                match encryptor.encrypt(input.trim()) {
                    Ok(encrypted) => {
                        let msg = Message::Text(encrypted);

                        if sender.send(msg).await.is_err() {
                            println!("Conexão fechada");
                            break;
                        }
                    }
                    Err(e) => {
                        println!("Erro ao criptografar mensagem: {}", e);
                        return;
                    }
                }
            }

            input.clear();
        }
    });

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(message) => match message {
                Message::Text(text) => {
                    {
                        let mut option = OPTION.lock().await;
                        
                        if text.contains("option") {
                            let parts: Vec<&str> = text.split(":").collect();

                            if let Some(last_part) = parts.last() {
                                match last_part.trim().parse() {
                                    Ok(val) => *option = val,
                                    Err(e) => {
                                        println!("Erro ao parsear variável: {}", e);
                                        return;
                                    }
                                };

                                println!("{:?}", *option);
                            } else {
                                println!("Nenhuma parte encontrada.");
                            }
                        }
                    }

                    {
                        let option = OPTION.lock().await;

                        if let Some((_, decryptor)) = choose_algorithm(*option) {
                            match decryptor.decrypt(&text) {
                                Ok(decrypted) => {
                                    println!("{}\n", decrypted);
                                }
                                Err(e) => {
                                    println!("Erro ao descriptografar mensagem: {}", e);
                                    return;
                                }
                            }
                        }
                    }
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
