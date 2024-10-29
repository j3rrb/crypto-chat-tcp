mod algos;
mod constants;
mod structs;
mod traits;

use futures_util::{SinkExt, StreamExt};
use std::boxed::Box;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use traits::{Decryptor, Encryptor};
use uuid::Uuid;

use crate::structs::DiffieHellman;

type Tx = tokio::sync::mpsc::UnboundedSender<Message>;
type PeerList = Arc<Mutex<Vec<(Uuid, Tx)>>>;

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
    let mut option: u32;

    loop {
        println!("\nDigite qual algoritmo será utilizado:");
        println!("1 - Diffie-Hellmann + Ceasar");
        println!("0 - Sair");

        let mut input = String::new();

        match std::io::stdin().read_line(&mut input) {
            Ok(_) => {}
            Err(e) => {
                println!("Erro ao ler entrada: {}", e);
                continue;
            }
        }

        option = match input.trim().parse() {
            Ok(num) => num,
            Err(_) => {
                println!("Entrada inválida.");
                continue;
            }
        };

        match choose_algorithm(option) {
            Some(_) => break,
            None => {
                println!("\nOpção inválida!");
                continue;
            }
        }
    }

    let addr = "127.0.0.1:8080".to_string();
    let addr: SocketAddr = addr.parse().expect("Invalid address");

    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
    let peers = Arc::new(Mutex::new(Vec::new()));

    println!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let peers = Arc::clone(&peers);
        tokio::spawn(handle_connection(stream, peers, option));
    }
}

async fn handle_connection(stream: TcpStream, peers: PeerList, option: u32) {
    let client_id = Uuid::new_v4();
    println!("New client connected: {}", client_id);

    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            println!("Error during the websocket handshake: {}", e);
            return;
        }
    };

    let (mut sender, mut receiver) = ws_stream.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    {
        let mut peers = peers.lock().unwrap();
        peers.push((client_id, tx));
    }

    {
        let _ = sender
            .send(Message::Text(format!("Seu ID: {}", client_id)))
            .await;
    }

    {
        let _ = sender
            .send(Message::Text(format!("option: {}", option)))
            .await;
    }

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match choose_algorithm(option) {
                Some((encryptor, _)) => match encryptor.encrypt(&msg.to_string()) {
                    Ok(encrypted) => {
                        if let Err(e) = sender.send(Message::Text(encrypted)).await {
                            println!("Erro ao enviar mensagem: {:?}", e);
                            break;
                        }
                    }
                    Err(e) => println!("Erro ao criptografar: {}", e),
                },
                None => println!("Erro"),
            }
        }
    });

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => match choose_algorithm(option) {
                Some((_, decryptor)) => match decryptor.decrypt(&text.to_string()) {
                    Ok(decrypted) => {
                        let broadcast_msg = Message::Text(format!("{}: {}", client_id, decrypted));
                        let peers = peers.lock().unwrap();

                        for (id, peer) in peers.iter() {
                            if *id == client_id {
                                let _ = peer.send(Message::Text(decrypted.to_string()));
                            } else {
                                let _ = peer.send(broadcast_msg.clone());
                            }
                        }
                    }
                    Err(e) => println!("Erro ao descriptografar: {}", e),
                },
                None => println!(""),
            },
            Ok(Message::Close(_)) => break,
            Ok(_) => (),
            Err(e) => {
                println!("Error processing message: {}", e);
                break;
            }
        }
    }

    {
        let mut peers = peers.lock().unwrap();
        peers.retain(|(id, _)| *id != client_id);
    }

    println!("Client disconnected: {}", client_id);
}
