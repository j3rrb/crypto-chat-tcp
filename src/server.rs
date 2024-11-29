extern crate lazy_static;

use futures_util::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

type Tx = tokio::sync::mpsc::UnboundedSender<Message>;
type PeerList = Arc<RwLock<Vec<(Uuid, Tx)>>>;
type KEYRING = Arc<RwLock<Vec<(Uuid, Option<u16>)>>>;

lazy_static! {
    static ref Keyring: KEYRING = Arc::new(RwLock::new(vec![]));
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:8080".to_string();
    let addr: SocketAddr = addr.parse().expect("Invalid address");

    // Cria o servidor WebSocket
    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");

    // Declara um atômico para um vetor de mutexes, para os clients conectados
    let peers = Arc::new(RwLock::new(Vec::new()));

    println!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        // Quando houver alguma conexão de um client aceita
        // e cria uma task para manipular a conexão do usuário
        let peers = Arc::clone(&peers);
        tokio::spawn(handle_connection(stream, peers));
    }
}

async fn handle_connection(stream: TcpStream, peers: PeerList) {
    // Cria um UUIDv4 para o usuário conectado
    let client_id = Uuid::new_v4();
    println!("New client connected: {}", client_id);

    // Pega a stream para esse usuário
    let ws_stream = match accept_async(stream).await {
        Ok(mut ws) => {
            match ws.send(Message::Text(client_id.to_string())).await {
                Ok(_) => {
                    println!("ID enviado com sucesso!")
                }
                Err(e) => {
                    println!("Erro ao enviar o ID ao client! {}", e);
                    return;
                }
            }

            ws
        }
        Err(e) => {
            println!("Error during the websocket handshake: {}", e);
            return;
        }
    };

    // "Separa" entre remetente e destinatário
    let (sender, mut receiver) = ws_stream.split();

    // Cria um atômico de mutex para o remetente
    let sender = Arc::new(Mutex::new(sender));

    // Cria um canal para envio e recebimento de mensagens
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Trava o mutex dos clients e adiciona o cliente ao vetor
    {
        let tx = tx.clone();
        let mut peers = peers.write().await;
        peers.push((client_id, tx));

        let mut keyring = Keyring.write().await;
        keyring.push((client_id, None))
    }

    // Cria uma tarefa para envio das mensagens
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("Sending {} to {}", msg, client_id);
            if let Err(e) = sender.lock().await.send(msg).await {
                println!("Error sending message: {:?}", e);
                break;
            }
        }
    });

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let broadcast_msg = Message::Text(text.clone());

                if text.starts_with("set_public_key:") {
                    if let Some(key) = text.split(':').nth(1) {
                        if let Ok(parsed_key) = key.parse() {
                            update_keyring(client_id, parsed_key).await;
                        }
                    }
                    continue;
                }

                if text.starts_with("get_public_key") {
                    tokio::spawn(exchange_keys(client_id, peers.clone()));
                    continue;
                }

                broadcast_message(client_id, &peers, broadcast_msg).await;
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => (),
            Err(e) => {
                println!("Error processing message: {}", e);
                break;
            }
        }
    }

    // Se o cliente for desconectado, trava o mutext dos clientes para escrita
    // e remove o cliente que desconectou da lista
    {
        let mut peers = peers.write().await;
        peers.retain(|(id, _)| *id != client_id);

        let mut keyring = Keyring.write().await;
        keyring.retain(|(id, _)| *id != client_id)
    }

    println!("Client disconnected: {}", client_id);
}

async fn update_keyring(client_id: Uuid, key: u16) {
    let mut keyring = Keyring.write().await;
    if let Some((_, client_key)) = keyring.iter_mut().find(|(id, _)| *id == client_id) {
        *client_key = Some(key);
    }
    println!("Updated keyring for client {}: {:?}", client_id, key);
}

async fn exchange_keys(client_id: Uuid, peers: PeerList) {
    // Obtém as informações do cliente atual
    let (other_id, client_key) = match retrieve_key_from_other(client_id).await {
        Some(result) => result,
        None => {
            println!("No key found for client {}", client_id);
            return;
        }
    };

    // Obtém as informações do outro cliente
    let (_, other_key) = match retrieve_key_from_other(other_id).await {
        Some(result) => result,
        None => {
            println!("No key found for client {}", other_id);
            return;
        }
    };

    // Envia a chave do cliente atual para o outro cliente
    if let Some(client_sender) = {
        let peers = peers.read().await;
        peers
            .iter()
            .find(|(id, _)| *id != other_id)
            .map(|(_, sender)| sender.clone())
    } {
        if let Some(client_key) = client_key {
            let msg = Message::Text(format!("get_public_key:{}", client_key));
            if let Err(e) = client_sender.send(msg) {
                eprintln!(
                    "Error sending key from {} to {}: {:?}",
                    client_id, other_id, e
                );
            } else {
                println!("Key from {} sent to {}", client_id, other_id);
            }
        }
    }

    // Envia a chave do outro cliente para o cliente atual
    if let Some(client_sender) = {
        let peers = peers.read().await;
        peers
            .iter()
            .find(|(id, _)| *id != client_id)
            .map(|(_, sender)| sender.clone())
    } {
        if let Some(other_key) = other_key {
            let msg = Message::Text(format!("get_public_key:{}", other_key));
            if let Err(e) = client_sender.send(msg) {
                eprintln!(
                    "Error sending key from {} to {}: {:?}",
                    other_id, client_id, e
                );
            } else {
                println!("Key from {} sent to {}", other_id, client_id);
            }
        }
    }
}

async fn retrieve_key_from_other(client_id: Uuid) -> Option<(Uuid, Option<u16>)> {
    let keyring = Keyring.read().await;
    keyring.iter().find_map(|(id, key)| {
        if *id != client_id {
            Some((*id, *key))
        } else {
            None
        }
    })
}

async fn broadcast_message(client_id: Uuid, peers: &PeerList, message: Message) {
    let peers = peers.read().await;
    for (id, peer) in peers.iter() {
        if *id != client_id {
            let _ = peer.send(message.clone());
        }
    }
}
