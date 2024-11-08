use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

type Tx = tokio::sync::mpsc::UnboundedSender<Message>;
type PeerList = Arc<RwLock<Vec<(Uuid, Tx, Option<u32>)>>>;

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
        Ok(ws) => ws,
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

    // Clona o remetente para um atômico
    let sender_clone = Arc::clone(&sender);

    // Trava o mutex dos clients e adiciona o cliente ao vetor
    {
        let mut peers = peers.write().await;
        peers.push((client_id, tx, None));
    }

    // Cria uma tarefa para envio das mensagens
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("Enviando mensagem: {}\n", msg);
            if let Err(e) = sender_clone.lock().await.send(msg).await {
                println!("Erro ao enviar mensagem: {:?}", e);
                break;
            }
        }
    });

    // Recebe a chave pública do outro client
    if let Some(Ok(Message::Text(public_key_text))) = receiver.next().await {
        if let Ok(public_key) = public_key_text.parse::<u32>() {
            println!(
                "Chave pública recebida do cliente {}: {}",
                client_id, public_key
            );

            // Tranca o mutex de clients para escrita e envia a chave publica ao client solicitado
            {
                let mut peers_guard = peers.write().await;
                if let Some(peer) = peers_guard.iter_mut().find(|(id, _, _)| *id == client_id) {
                    peer.2 = Some(public_key);
                }
            }

            // Tranca o mutex para leitura e envia a chave do client para outros clients
            // menos o próprio client
            let peers_guard = peers.read().await;
            for (id, peer_tx, peer_public_key) in peers_guard.iter() {
                if *id != client_id {
                    // Verifica se a chave pública recebida é igual a chave do cliente
                    if let Some(other_public_key) = peer_public_key {
                        let msg = Message::Text(other_public_key.to_string());

                        // Tranca o mutex do remetente e envia a chave pública
                        let _ = sender.lock().await.send(msg).await;
                    }

                    // Envia a chave pública ao cliente
                    let _ = peer_tx.send(Message::Text(public_key_text.clone()));
                }
            }
        }
    }

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let broadcast_msg = Message::Text(text.clone());
                let peers = peers.read().await;

                // Faz o broadcasting da mensagem para todos os outros clients conectados
                for (id, peer, _) in peers.iter() {
                    if *id != client_id {
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

    // Se o cliente for desconectado, trava o mutext dos clientes para escrita
    // e remove o cliente que desconectou da lista
    {
        let mut peers = peers.write().await;
        peers.retain(|(id, _, _)| *id != client_id);
    }

    println!("Client disconnected: {}", client_id);
}
