use futures_util::{SinkExt, StreamExt};
use std::env;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

type Tx = tokio::sync::mpsc::UnboundedSender<Message>;
type PeerList = Arc<Mutex<Vec<(Uuid, Tx)>>>;

#[tokio::main]
async fn main() {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let addr: SocketAddr = addr.parse().expect("Invalid address");

    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
    let peers = Arc::new(Mutex::new(Vec::new()));

    println!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let peers = Arc::clone(&peers);
        tokio::spawn(handle_connection(stream, peers));
    }
}

async fn handle_connection(stream: TcpStream, peers: PeerList) {
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

    let _ = sender.send(Message::Text(format!("Seu ID: {}", client_id))).await;

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Mensagem a ser enviada apenas para outros clientes
                let broadcast_msg = Message::Text(format!("{}: {}", client_id, text));
                let peers = peers.lock().unwrap();
                for (id, peer) in peers.iter() {
                    if *id == client_id {
                        // Envia a mensagem "Você" apenas para o cliente que enviou
                        let _ = peer.send(Message::Text(format!("Você: {}", text)));
                    } else {
                        // Envia a mensagem com o ID do remetente para os outros
                        let _ = peer.send(broadcast_msg.clone());
                    }
                }
            }
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
