mod algos;
mod constants;
mod structs;
mod traits;
mod utils;

use futures_util::{SinkExt, StreamExt};
use rand::{self, Rng};
use std::env;
use std::io::Write;
use std::sync::Arc;
use tokio::io::{stdin, AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use url::Url;

use crate::constants::{BASE, PRIME};
use crate::structs::DiffieHellman;
use crate::traits::{Decryptor, Encryptor};
use crate::utils::mod_exp;

fn generate_keys() -> (u32, u32) {
    // Gera um número aleatório de 1 até a constante representando o número primo
    // que é a chave privada
    let private_key: u32 = rand::thread_rng().gen_range(1..PRIME);

    // Gera a chave pública que é a constante que representa o módulo
    // elevando-a a chave privada e fazendo o módulo com a constante do número primo
    let public_key: u32 = mod_exp(BASE, private_key, PRIME);

    // Retorna uma tupla com os valores
    (private_key, public_key)
}

// Função que realiza a escolha do algoritmo a ser utilizado
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
    // Gera a chave pública e privada assim que o client é iniciado
    let (private_key, public_key) = generate_keys();

    // Exibe as chaves para o cliente conectado
    println!("Chave pública: {}", public_key);
    println!("Chave privada: {}", private_key);

    // Declara a URL WebSocket do server
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:8080".to_string());

    // Tenta realizar o parsing da url para um objeto do tipo Url
    let url = Url::parse(&url).expect("URL inválida");

    // Conecta ao servidor
    let (ws_stream, _) = connect_async(url.as_str()).await.expect("Erro ao conectar");

    println!("Conectado ao servidor: {}", url);

    // "Separa" a stream resultante em remetente e destinatário
    let (mut sender, receiver) = ws_stream.split();

    // Envia a chave pública do client ao outro client
    let msg = Message::Text(public_key.to_string());
    sender
        .send(msg)
        .await
        .expect("Erro ao enviar chave pública");

    // Cria um mutex para o destinatário
    let receiver = Arc::new(Mutex::new(receiver));

    // Recebe a chave pública do outro client e faz o parsing para u32
    let other_public_key = match receiver.lock().await.next().await {
        Some(Ok(Message::Text(text))) => text.parse::<u32>().unwrap(),
        _ => {
            eprintln!("Erro ao receber chave pública do outro cliente");
            return;
        }
    };

    // Faz o cálculo da chave compartilhada
    let shared_secret = mod_exp(other_public_key, private_key, PRIME);

    println!("Chave compartilhada: {}\n", shared_secret);

    // Cria um canal para envio das mensagens
    let (tx, mut rx) = mpsc::channel::<String>(32);

    // Cria uma task para o envio das mensagens
    tokio::spawn(async move {
        // Declara o IO do runtime assíncrono, leitura em buffer e uma string vazia
        let stdin = stdin();
        let mut reader = BufReader::new(stdin);
        let mut input = String::new();

        loop {
            print!("Digite sua mensagem: ");
            std::io::stdout().flush().unwrap(); // Limpa o terminal
            input.clear(); // Limpa o buffer da string

            // Lê a linha digitada pelo usuário
            if let Err(e) = reader.read_line(&mut input).await {
                eprintln!("Erro ao ler entrada: {}", e);
                break;
            }

            // Copia o que foi digitado com sanitização
            let input = input.trim().to_string();

            // Envia a mensagem
            if tx.send(input.clone()).await.is_err() {
                println!("Erro ao enviar mensagem para a tarefa de envio");
                break;
            }

            // Exibe o que foi digitado
            print!("\x1b[A\x1b[2K");
            println!("Você: {}\n", input);

            // Limpa o terminal
            std::io::stdout().flush().unwrap();
        }
    });

    // Cria uma tarefa para receber as mensagens
    tokio::spawn(async move {
        loop {
            if let Some(input) = rx.recv().await {
                if let Some((encryptor, _)) = choose_algorithm(1) {
                    // Criptografa a mensagem com a chave compartilhada
                    match encryptor.encrypt(&input, &shared_secret) {
                        Ok(encrypted) => {
                            let msg = Message::Text(encrypted);

                            // Envia a mensagem
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
            }
        }
    });

    while let Some(msg) = receiver.lock().await.next().await {
        // Tranca o mutex do destinatário
        match msg {
            Ok(message) => match message {
                Message::Text(text) => {
                    if let Some((_, decryptor)) = choose_algorithm(1) {
                        // Descriptografa a mensagem com a chave compartilhada
                        match decryptor.decrypt(&text, &shared_secret) {
                            Ok(decrypted) => {
                                // Exibe o que foi recebido
                                print!("\x1b[A\x1b[2K");
                                println!("\nMensagem recebida: {}\n", decrypted);

                                // Limpa o terminal
                                std::io::stdout().flush().unwrap();
                            }
                            Err(e) => {
                                println!("Erro ao descriptografar mensagem: {}", e);
                                return;
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
